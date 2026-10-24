use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State, WebviewWindow};

use crate::state::TauriState;

#[derive(Serialize)]
pub struct PickedFile {
    pub path: String,
    pub info: String,
}

#[tauri::command]
pub fn ysc_file_info(path: String) -> Result<PickedFile, String> {
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    let name = std::path::Path::new(&path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.clone());
    let size = if meta.len() >= 1024 {
        format!("{:.1} KB", meta.len() as f64 / 1024.0)
    } else {
        format!("{} B", meta.len())
    };
    Ok(PickedFile {
        path: path.clone(),
        info: format!("{name} — {size}"),
    })
}

#[tauri::command]
pub fn ysc_get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
pub fn ysc_driver_is_connected(state: State<'_, TauriState>) -> bool {
    state.app.serial_snapshot().0
}

/// 通用命令桥：`payload = { type, ...params }`，与旧管道逐行 JSON 同义。
#[tauri::command]
pub fn ysc_driver_send(state: State<'_, TauriState>, payload: Value) {
    let line = payload.to_string();
    ysc_host::commands::dispatch(&state.app, &line);
}

/// 普通启动返回 None；自动烧录模式返回 V3 镜像路径。
#[tauri::command]
pub fn ysc_auto_flash_info(state: State<'_, TauriState>) -> Option<String> {
    state.auto_flash.clone()
}

/// 前端已切到 V3 页并装好事件监听后调用；后台等待 V3 端口并自动烧录。
#[tauri::command]
pub fn ysc_auto_flash_start(state: State<'_, TauriState>) -> bool {
    let Some(path) = state.auto_flash.clone() else {
        return false;
    };

    let name = std::path::Path::new(&path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("v3_firmware.bin");
    let info = match std::fs::metadata(&path) {
        Ok(m) => format!("{} — {:.1} KB", name, m.len() as f64 / 1024.0),
        Err(_) => format!("{} — 文件不存在", name),
    };
    state.app.send_event(
        "auto_flash_file",
        Some(&format!(
            "\"path\":\"{}\",\"info\":\"{}\"",
            ysc_core::util::json_escape(&path),
            ysc_core::util::json_escape(&info)
        )),
    );

    crate::auto_flash::spawn(std::sync::Arc::clone(&state.app), path);
    true
}

#[tauri::command]
pub fn ysc_set_lang(_lang: String) {}

#[tauri::command]
pub fn ysc_window_minimize(window: WebviewWindow) -> Result<(), String> {
    window.minimize().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn ysc_window_toggle_maximize(window: WebviewWindow) -> Result<bool, String> {
    let was_max = window.is_maximized().map_err(|e| e.to_string())?;
    if was_max {
        window.unmaximize().map_err(|e| e.to_string())?;
    } else {
        window.maximize().map_err(|e| e.to_string())?;
    }
    Ok(!was_max)
}

#[tauri::command]
pub fn ysc_window_is_maximized(window: WebviewWindow) -> Result<bool, String> {
    window.is_maximized().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn ysc_window_close(window: WebviewWindow) {
    let _ = window.close();
}

#[tauri::command]
pub fn ysc_quit(app: AppHandle) {
    app.exit(0);
}
