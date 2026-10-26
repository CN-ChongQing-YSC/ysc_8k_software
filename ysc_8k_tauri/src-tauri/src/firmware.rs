//! 在线固件服务（V3）：查询 YSC 固件服务器、下载并校验 APP 镜像。
//!
//! 只负责 HTTP / 缓存 / SHA-256；烧录仍交给 `ysc_host::iap2`。
//! 响应字段与 firmware_server 的 `FirmwareDto` 对齐（camelCase）。

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};
use ysc_core::util::json_escape;
use ysc_host::App;

use crate::state::TauriState;

const DEFAULT_BASE_URL: &str = "https://ysc-order-2026.top/fwapi";
const CACHE_DIR: &str = "firmware-cache";
const INSTALLED_FILE: &str = "installed.json";
const DOWNLOAD_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareInfo {
    #[serde(default)]
    pub id: Option<i64>,
    pub device_type: String,
    pub version: String,
    pub filename: String,
    #[serde(default)]
    pub original_filename: Option<String>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub release_notes: Option<String>,
    #[serde(default)]
    pub min_hardware: Option<String>,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub listed: bool,
    /// 以下由驱动补充，服务端不返回。
    #[serde(default)]
    pub download_url: String,
    #[serde(default)]
    pub is_newer: bool,
    #[serde(default)]
    pub is_latest: bool,
    #[serde(default)]
    pub is_installed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResult {
    pub device_type: String,
    pub version: String,
    pub filename: String,
    pub size: u64,
    pub sha256: String,
    pub path: String,
    pub info: String,
}

#[derive(Debug, Deserialize)]
struct ListResponse {
    #[serde(default)]
    firmwares: Vec<FirmwareInfo>,
}

fn base_url() -> String {
    std::env::var("YSC_FW_BASE_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))
}

fn valid_token(s: &str, what: &str) -> Result<(), String> {
    if s.is_empty()
        || !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err(format!("非法 {what}: {s}"));
    }
    Ok(())
}

fn valid_sha256(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn cache_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法定位应用数据目录: {e}"))?
        .join(CACHE_DIR);
    fs::create_dir_all(&dir).map_err(|e| format!("创建固件缓存目录失败: {e}"))?;
    Ok(dir)
}

fn installed_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(cache_dir(app)?.join(INSTALLED_FILE))
}

fn read_installed(app: &AppHandle) -> HashMap<String, String> {
    let Ok(path) = installed_path(app) else {
        return HashMap::new();
    };
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<HashMap<String, String>>(&s).ok())
        .unwrap_or_default()
}

fn write_installed(app: &AppHandle, map: &HashMap<String, String>) -> Result<(), String> {
    let path = installed_path(app)?;
    let body = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
    fs::write(path, body).map_err(|e| format!("写入 installed.json 失败: {e}"))
}

fn ver_segments(v: &str) -> Vec<u64> {
    let trimmed = v.trim();
    let cleaned = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    cleaned
        .split('.')
        .map(|s| {
            s.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u64>()
                .unwrap_or(0)
        })
        .collect()
}

pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let sa = ver_segments(a);
    let sb = ver_segments(b);
    let len = sa.len().max(sb.len());
    for i in 0..len {
        let va = sa.get(i).copied().unwrap_or(0);
        let vb = sb.get(i).copied().unwrap_or(0);
        match va.cmp(&vb) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
}

fn enrich(mut info: FirmwareInfo, base: &str, installed: &HashMap<String, String>) -> FirmwareInfo {
    info.download_url = format!(
        "{}/api/firmware/download/{}",
        base,
        info.filename // validated before use
    );
    let installed_ver = installed.get(&info.device_type);
    info.is_installed = installed_ver
        .map(|v| compare_versions(&info.version, v) == std::cmp::Ordering::Equal)
        .unwrap_or(false);
    info.is_newer = installed_ver
        .map(|v| compare_versions(&info.version, v) == std::cmp::Ordering::Greater)
        .unwrap_or(true);
    info
}

fn latest_blocking(app: &AppHandle, device_type: &str) -> Result<Option<FirmwareInfo>, String> {
    valid_token(device_type, "deviceType")?;
    let base = base_url();
    let url = format!("{base}/api/firmware/{device_type}/latest");
    let resp = client()?
        .get(&url)
        .send()
        .map_err(|e| format!("查询最新固件失败: {e}"))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(format!("查询最新固件 HTTP {}", resp.status()));
    }
    let info = resp
        .json::<FirmwareInfo>()
        .map_err(|e| format!("解析最新固件 JSON 失败: {e}"))?;
    valid_token(&info.filename, "filename")?;
    let installed = read_installed(app);
    Ok(Some(enrich(info, &base, &installed)))
}

fn list_blocking(app: &AppHandle, device_type: &str) -> Result<Vec<FirmwareInfo>, String> {
    valid_token(device_type, "deviceType")?;
    let base = base_url();
    let url = format!("{base}/api/firmware/list/{device_type}");
    let resp = client()?
        .get(&url)
        .send()
        .map_err(|e| format!("查询固件列表失败: {e}"))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    if !resp.status().is_success() {
        return Err(format!("查询固件列表 HTTP {}", resp.status()));
    }
    let body = resp
        .json::<ListResponse>()
        .map_err(|e| format!("解析固件列表 JSON 失败: {e}"))?;
    let installed = read_installed(app);
    let base2 = base.clone();
    let mut list: Vec<FirmwareInfo> = body
        .firmwares
        .into_iter()
        .filter(|x| valid_token(&x.filename, "filename").is_ok())
        .map(|x| enrich(x, &base2, &installed))
        .collect();
    if let Some(first) = list.first_mut() {
        first.is_latest = true;
    }
    Ok(list)
}

fn human_size(n: u64) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{:.2} MB", n as f64 / 1024.0 / 1024.0)
    }
}

fn download_blocking(
    app: &AppHandle,
    host: &App,
    info: &FirmwareInfo,
) -> Result<DownloadResult, String> {
    valid_token(&info.device_type, "deviceType")?;
    valid_token(&info.filename, "filename")?;
    if !valid_sha256(&info.sha256) {
        return Err("服务端 sha256 缺失或格式非法".to_string());
    }

    let base = base_url();
    let url = format!("{base}/api/firmware/download/{}", info.filename);
    let resp = client()?
        .get(&url)
        .send()
        .map_err(|e| format!("下载失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载 HTTP {}", resp.status()));
    }

    let dir = cache_dir(app)?;
    let final_path = dir.join(&info.filename);
    let tmp_path = dir.join(format!("{}.part", info.filename));
    let expected_size = info.size;
    let mut file = File::create(&tmp_path).map_err(|e| format!("创建临时文件失败: {e}"))?;
    let mut hasher = Sha256::new();
    let mut received: u64 = 0;
    let mut last_percent: u64 = u64::MAX;
    let mut buf = vec![0u8; 64 * 1024];

    let result = (|| -> Result<(), String> {
        let mut reader = resp;
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|e| format!("读取下载流失败: {e}"))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])
                .map_err(|e| format!("写入固件失败: {e}"))?;
            hasher.update(&buf[..n]);
            received += n as u64;
            let total = expected_size.max(received);
            let percent = if total > 0 {
                ((received.saturating_mul(100)) / total).min(100)
            } else {
                0
            };
            if percent != last_percent {
                last_percent = percent;
                host.send_event(
                    "firmware:download_progress",
                    Some(&format!(
                        "\"deviceType\":\"{}\",\"received\":{},\"total\":{},\"percent\":{}",
                        json_escape(&info.device_type),
                        received,
                        total,
                        percent
                    )),
                );
            }
        }
        file.flush().map_err(|e| format!("刷新固件失败: {e}"))?;
        Ok(())
    })();

    if let Err(e) = result {
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }
    if expected_size > 0 && received != expected_size {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!(
            "下载大小不一致: 期望 {}，实际 {}",
            expected_size, received
        ));
    }
    let actual_sha = format!("{:x}", hasher.finalize());
    if actual_sha.to_lowercase() != info.sha256.to_lowercase() {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!(
            "sha256 校验失败: 期望 {}…，实际 {}…",
            &info.sha256[..12.min(info.sha256.len())],
            &actual_sha[..12]
        ));
    }
    if final_path.exists() {
        let _ = fs::remove_file(&final_path);
    }
    fs::rename(&tmp_path, &final_path).map_err(|e| format!("保存固件失败: {e}"))?;

    Ok(DownloadResult {
        device_type: info.device_type.clone(),
        version: info.version.clone(),
        filename: info.filename.clone(),
        size: received,
        sha256: actual_sha,
        path: final_path.to_string_lossy().into_owned(),
        info: format!("{} — {}", info.filename, human_size(received)),
    })
}

#[tauri::command]
pub async fn ysc_firmware_latest(
    app: AppHandle,
    device_type: String,
) -> Result<Option<FirmwareInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || latest_blocking(&app, &device_type))
        .await
        .map_err(|e| format!("查询线程失败: {e}"))?
}

#[tauri::command]
pub async fn ysc_firmware_list(
    app: AppHandle,
    device_type: String,
) -> Result<Vec<FirmwareInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || list_blocking(&app, &device_type))
        .await
        .map_err(|e| format!("查询线程失败: {e}"))?
}

#[tauri::command]
pub async fn ysc_firmware_download(
    state: State<'_, TauriState>,
    app: AppHandle,
    info: FirmwareInfo,
) -> Result<DownloadResult, String> {
    let host_for_task = Arc::clone(&state.app);
    let info_for_task = info.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        download_blocking(&app, &host_for_task, &info_for_task)
    })
    .await
    .map_err(|e| format!("下载线程失败: {e}"))?;
    result
}

/// 读取已下载并校验的缓存文件，交给宿主做 in-memory 烧录。
/// 只允许访问 firmware-cache 目录，防止前端传入任意路径。
#[tauri::command]
pub fn ysc_firmware_read_base64(app: AppHandle, path: String) -> Result<String, String> {
    let dir = cache_dir(&app)?;
    let candidate = PathBuf::from(&path);
    let canonical = candidate
        .canonicalize()
        .map_err(|e| format!("固件缓存文件不存在: {e}"))?;
    let dir_canonical = dir
        .canonicalize()
        .map_err(|e| format!("固件缓存目录不可用: {e}"))?;
    if !canonical.starts_with(&dir_canonical) {
        return Err("拒绝读取固件缓存目录之外的文件".to_string());
    }
    let bytes = fs::read(&canonical).map_err(|e| format!("读取固件缓存失败: {e}"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[tauri::command]
pub fn ysc_firmware_record_installed(
    app: AppHandle,
    device_type: String,
    version: String,
) -> Result<(), String> {
    valid_token(&device_type, "deviceType")?;
    let mut map = read_installed(&app);
    map.insert(device_type, version);
    write_installed(&app, &map)
}

#[tauri::command]
pub fn ysc_firmware_installed(app: AppHandle, device_type: String) -> Option<String> {
    read_installed(&app).remove(&device_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_semver() {
        assert_eq!(
            compare_versions("1.2.1", "1.2.0"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(compare_versions("v1.2", "1.2.0"), std::cmp::Ordering::Equal);
        assert_eq!(compare_versions("1.10", "1.9"), std::cmp::Ordering::Greater);
        assert_eq!(compare_versions("2", "10"), std::cmp::Ordering::Less);
    }

    #[test]
    fn validates_tokens() {
        assert!(valid_token("ysc_v3_8k", "deviceType").is_ok());
        assert!(valid_token("ysc_v3_8k_v1.2.1.bin", "filename").is_ok());
        assert!(valid_token("../evil", "filename").is_err());
        assert!(valid_token("a/b", "filename").is_err());
        assert!(valid_token("a b", "filename").is_err());
    }

    #[test]
    fn validates_sha() {
        assert!(valid_sha256(
            "090dfa5a599b827720c00629245ff1c92c55cc9502c272f8d693b712e275a2f7"
        ));
        assert!(!valid_sha256("xyz"));
        assert!(!valid_sha256(""));
    }
}
