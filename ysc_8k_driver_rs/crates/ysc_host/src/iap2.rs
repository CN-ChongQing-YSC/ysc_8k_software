//! USB-CDC 双 MCU 固件升级（V2 towmcu + V3 YSC-V3）。
//!
//! 两条流程共用 ERASE/PROGRAM/VERIFY/END：
//!   * V2 towmcu：APP=`ysc-towmcu-*`、IAP=`YSC-IAP`；cmd50 后按序列号
//!     `TOWMCUIAP` 重新找端口。
//!   * V3 YSC-V3：APP=`YSC-V3-APP <date>`、IAP=`YSC-V3-IAP`；IAP/APP 共用
//!     序列号 `YSCV3-CDC`，cmd50 后按版本串重新确认端口。END 后必须等回
//!     `YSC-V3-APP` 才算成功（能抓住“密钥不匹配但 PROM/VERIFY 通过”的镜像）。
//!
//! 事件前缀 `iap2_*`，与 v1 的 `iap_*` 完全隔离；命令仍复用 `towmcu_*`，
//! 前端无需区分 V2/V3。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::Value;
use windows::Win32::Foundation::HANDLE;

use ysc_core::consts::IAP_CHUNK_SZ;
use ysc_core::enumerate::enum_towmcu_ports;
use ysc_core::family::{family_of_version, is_iap_version, CdcFamily};
use ysc_core::frame::{build_iap_frame, build_json_body_frame};
use ysc_core::serial::{close_sync, open_cdc, sync_read, sync_write};
use ysc_core::util::{hex_dump, json_escape, now_ms};

use crate::app::App;

const CMD_PROM: u8 = 0x80;
const CMD_ERASE: u8 = 0x81;
const CMD_VERIFY: u8 = 0x82;
const CMD_END: u8 = 0x83;

const CDC_BAUD: u32 = 115_200;
const FRAME_RETRIES: i32 = 3;
const FRAME_TIMEOUT_FAST_MS: u32 = 500;
const FRAME_TIMEOUT_SLOW_MS: u32 = 3000;
const RETRY_BREATHE_MS: u64 = 100;
const VERSION_TIMEOUT_MS: u32 = 1500;
const V3_SCAN_TIMEOUT_MS: u32 = 1200;
const IAP_RESCAN_TIMEOUT_MS: u64 = 30_000;
const V3_APP_WAIT_MS: u64 = 15_000;
const IAP_RESCAN_POLL_MS: u64 = 400;
const IAP_ENTER_WAIT_MS: u64 = 2500;
const VERIFY_CHUNK_DELAY_MS: u64 = 5;

fn send_log(app: &App, msg: &str, cls: &str) {
    app.send_event(
        "iap2_log",
        Some(&format!(
            "\"message\":\"{}\",\"cls\":\"{}\"",
            json_escape(msg),
            cls
        )),
    );
}

fn send_progress(app: &App, cur: i32, total: i32, status: &str) {
    app.send_event(
        "iap2_progress",
        Some(&format!(
            "\"current\":{},\"total\":{},\"status\":\"{}\"",
            cur,
            total,
            json_escape(status)
        )),
    );
}

fn send_done(app: &App, ok: bool, err: &str, code: Option<&str>) {
    let body = match code {
        Some(c) if !c.is_empty() => format!(
            "\"success\":{},\"error\":\"{}\",\"code\":\"{}\"",
            ok,
            json_escape(err),
            c
        ),
        _ => format!("\"success\":{},\"error\":\"{}\"", ok, json_escape(err)),
    };
    app.send_event("iap2_done", Some(&body));
}

pub fn start_file(app: Arc<App>, port: String, path: String) {
    if app.iap2_running.load(Ordering::Acquire) {
        return;
    }
    match std::fs::read(&path) {
        Ok(fw) if !fw.is_empty() => start_worker(app, port, fw),
        Ok(_) => {
            send_log(&app, "固件数据为空", "err");
            send_done(&app, false, "固件数据为空", None);
        }
        Err(_) => {
            let msg = format!("无法打开固件文件: {}", path);
            send_log(&app, &msg, "err");
            send_done(&app, false, &msg, None);
        }
    }
}

pub fn start_mem(app: Arc<App>, port: String, firmware: Vec<u8>) {
    if app.iap2_running.load(Ordering::Acquire) {
        return;
    }
    if firmware.is_empty() {
        send_log(&app, "固件数据为空", "err");
        send_done(&app, false, "固件数据为空", None);
        return;
    }
    start_worker(app, port, firmware);
}

fn start_worker(app: Arc<App>, port: String, firmware: Vec<u8>) {
    app.iap2_running.store(true, Ordering::Release);
    app.iap2_cancel.store(false, Ordering::Release);
    let _ = thread::Builder::new()
        .name("ysc-iap-v2".into())
        .spawn(move || worker(app, port, firmware));
}

struct RunningGuard(Arc<App>);
impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.0.iap2_running.store(false, Ordering::Release);
    }
}

fn worker(app: Arc<App>, port: String, firmware: Vec<u8>) {
    let _guard = RunningGuard(Arc::clone(&app));
    let fsize = firmware.len() as i64;
    let total_chunks = ((firmware.len() + IAP_CHUNK_SZ - 1) / IAP_CHUNK_SZ) as i32;
    send_log(
        &app,
        &format!("固件大小: {} 字节 ({:.1} KB)", fsize, fsize as f64 / 1024.0),
        "",
    );

    // 仅当旧单例正持有同一个串口时才关闭。
    let mut tore_down = false;
    {
        let f = app.flags.lock().unwrap();
        if f.serial_connected && f.serial_port == port {
            drop(f);
            app.legacy.stop_read_thread();
            app.legacy.disconnect();
            app.flags.lock().unwrap().serial_connected = false;
            tore_down = true;
        }
    }

    // 确认设备模式。
    send_log(&app, "确认设备模式 ...", "");
    let version = query_version_on_port(&port);
    if version.is_empty() {
        send_log(&app, "无法与设备通信 — 请检查端口/连线", "err");
        send_done(&app, false, "无法与设备通信 — 请检查端口/连线", None);
        return;
    }

    let family = family_of_version(&version);
    let is_v3 = family == Some(CdcFamily::YscV3);
    let mode = if is_iap_version(&version) {
        "IAP"
    } else {
        "APP"
    };
    send_log(
        &app,
        &format!("当前: [{} / {}] {}", mode, family_name(family), version),
        "info",
    );

    let Some(iap_port) = prepare_iap_port(&app, &port, &version, family) else {
        send_done(&app, false, "未找到 IAP 端口 — 设备未进入下载模式", None);
        return;
    };

    // V3 不在序列结束时立刻报成功：必须等设备重新枚举回 YSC-V3-APP。
    let ok = run_iap_sequence(&app, &iap_port, &firmware, !is_v3);
    if !ok {
        return;
    }

    let mut final_port = port.clone();
    if is_v3 {
        send_log(&app, "等待设备重新枚举回 APP ...", "");
        let app_port = find_v3_port(&iap_port, false, V3_APP_WAIT_MS);
        if app_port.is_empty() {
            let msg = "升级后未回到 APP（可能密钥不匹配/固件不兼容）";
            send_log(&app, msg, "err");
            send_done(&app, false, msg, Some("APP_NOT_BOOTED"));
            return;
        }
        final_port = app_port;
        let v = query_version_on_port(&final_port);
        if !v.is_empty() && !v.contains("-APP") {
            let msg = format!("重新枚举后版本异常: {}", v);
            send_log(&app, &msg, "err");
            send_done(&app, false, &msg, Some("APP_NOT_BOOTED"));
            return;
        }
        if v.is_empty() {
            send_log(&app, "端口已重新出现，但版本查询超时（按成功处理）", "warn");
        } else {
            send_log(&app, &format!("设备已回到 APP: {}", v), "info");
        }
        send_progress(&app, total_chunks, total_chunks, "完成");
        send_log(&app, "固件升级完成! 设备已回到 APP", "");
        send_done(&app, true, "", None);
    }

    if tore_down {
        send_log(&app, "等待设备重启 ...", "");
        thread::sleep(Duration::from_millis(2000));
        if app.legacy.connect(&final_port, CDC_BAUD).is_ok() {
            {
                let mut f = app.flags.lock().unwrap();
                f.serial_connected = true;
                f.serial_baud = CDC_BAUD;
                f.serial_v3_cdc = is_v3;
                f.serial_port = final_port.clone();
            }
            let _ = app.legacy.start_read_thread();
            send_log(&app, "已重新连接串口", "info");
        } else {
            send_log(&app, "串口自动重连失败，请手动连接", "warn");
        }
    }
}

fn family_name(family: Option<CdcFamily>) -> &'static str {
    match family {
        Some(CdcFamily::YscV3) => "V3",
        Some(CdcFamily::Towmcu) => "V2",
        None => "?",
    }
}

/// 确保设备停在 IAP，返回 IAP 端口。
///
/// 已是 IAP 时直接返回；APP 时发 cmd50 并按固件族等待重新枚举。
fn prepare_iap_port(
    app: &App,
    port: &str,
    version: &str,
    family: Option<CdcFamily>,
) -> Option<String> {
    if is_iap_version(version) {
        return Some(port.to_string());
    }

    send_log(app, "设备在 APP 模式 — 发送进入下载模式 (cmd 50) ...", "");
    if !send_jump_iap(port) {
        send_log(app, "进入下载模式失败 — 无法打开端口", "err");
        return None;
    }
    thread::sleep(Duration::from_millis(IAP_ENTER_WAIT_MS));

    match family {
        Some(CdcFamily::Towmcu) => {
            send_log(app, "寻找 IAP 端口 (TOWMCUIAP) ...", "");
            let iap = find_iap_port(IAP_RESCAN_TIMEOUT_MS);
            if iap.is_empty() {
                send_log(app, "未找到 IAP 端口 — 设备未进入下载模式", "err");
                None
            } else {
                send_log(app, &format!("IAP 端口: {} (YSC-IAP)", iap), "info");
                Some(iap)
            }
        }
        Some(CdcFamily::YscV3) => {
            send_log(app, "寻找 V3 IAP 端口 (YSCV3-CDC) ...", "");
            let iap = find_v3_port(port, true, IAP_RESCAN_TIMEOUT_MS);
            if iap.is_empty() {
                send_log(app, "未找到 V3 IAP 端口 — 设备未进入下载模式", "err");
                None
            } else {
                send_log(app, &format!("IAP 端口: {} (YSC-V3-IAP)", iap), "info");
                Some(iap)
            }
        }
        None => {
            send_log(app, &format!("无法识别的固件族: {}", version), "err");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// CDC 一次性操作
// ---------------------------------------------------------------------------

fn query_version_on_port(port: &str) -> String {
    // 单发一枪容易被"设备刚复位 / CDC 刚枚举 / 驱动还没就绪"吃掉。
    // 2026-10-24 真机教训：固件应答帧正好是 64B 整数倍时主机读请求永不完成，
    // 表现为"查版本必然失败"，而上层据此连 cmd50 都不发 —— 一次丢帧被放大
    // 成"永远进不了下载模式"。这里给第二次机会，仍失败才返回空串。
    for attempt in 0..2 {
        let v = query_version_on_port_timeout(port, VERSION_TIMEOUT_MS);
        if !v.is_empty() {
            return v;
        }
        if attempt == 0 {
            thread::sleep(Duration::from_millis(200));
        }
    }
    String::new()
}

/// 自动烧录/诊断用：查询一次端口版本，空串表示端口未就绪或无响应。
/// 会完整打开/关闭端口，不在调用方留下句柄。
pub fn probe_version(port: &str) -> String {
    query_version_on_port(port)
}

fn query_version_on_port_timeout(port: &str, timeout_ms: u32) -> String {
    let Some(h) = open_cdc(port) else {
        return String::new();
    };
    let mut reader = PollingReader::start(h);
    let frame = build_json_body_frame("{\"cmd\":132}");
    let _ = sync_write(h, &frame, 2000);
    let payload = reader.wait_for_frame(timeout_ms);
    reader.stop();
    close_sync(h);
    let Some(payload) = payload else {
        return String::new();
    };
    let text = String::from_utf8_lossy(&payload);
    if !text.contains("\"code\":200") && !text.contains("\"code\": 200") {
        return String::new();
    }
    if let Ok(v) = serde_json::from_str::<Value>(&text) {
        if let Some(s) = v.get("data").and_then(|x| x.as_str()) {
            if !s.is_empty() {
                return s.to_string();
            }
        }
        if let Some(s) = v.get("message").and_then(|x| x.as_str()) {
            return s.to_string();
        }
    }
    String::new()
}

fn send_jump_iap(port: &str) -> bool {
    let Some(h) = open_cdc(port) else {
        return false;
    };
    let frame = build_json_body_frame("{\"cmd\":50}");
    let ok = sync_write(h, &frame, 2000);
    close_sync(h);
    ok
}

fn find_iap_port(timeout_ms: u64) -> String {
    let start = now_ms();
    while now_ms().saturating_sub(start) < timeout_ms {
        for p in enum_towmcu_ports() {
            if p.side == "IAP" {
                return p.port;
            }
        }
        thread::sleep(Duration::from_millis(IAP_RESCAN_POLL_MS));
    }
    String::new()
}

/// V3 的 IAP/APP 共用序列号，不能靠 side 判断模式；必须打开端口问 cmd132。
///
/// 先试原端口（COM 号通常不变），再枚举所有 `[V3]` 端口（COM 号变了也能找到）。
fn find_v3_port(original: &str, want_iap: bool, timeout_ms: u64) -> String {
    let start = now_ms();
    loop {
        let mut candidates: Vec<String> = Vec::new();
        if !original.is_empty() {
            candidates.push(original.to_string());
        }
        for p in enum_towmcu_ports() {
            if p.side == "V3" && p.port != original {
                candidates.push(p.port);
            }
        }
        candidates.sort();
        candidates.dedup();

        for p in &candidates {
            let v = query_version_on_port_timeout(p, V3_SCAN_TIMEOUT_MS);
            if v3_version_matches(&v, want_iap) {
                return p.clone();
            }
        }

        if now_ms().saturating_sub(start) >= timeout_ms {
            return String::new();
        }
        thread::sleep(Duration::from_millis(IAP_RESCAN_POLL_MS));
    }
}

/// V3 端口探测的版本判定（纯函数，便于单测）。
fn v3_version_matches(version: &str, want_iap: bool) -> bool {
    if version.is_empty() {
        return false;
    }
    if want_iap {
        is_iap_version(version)
    } else {
        version.contains("-APP")
    }
}

// ---------------------------------------------------------------------------
// 轮询读线程
// ---------------------------------------------------------------------------

struct ReaderState {
    buf: Mutex<Vec<u8>>,
    cv: Condvar,
    running: AtomicBool,
    last_rx: AtomicU64,
    total_rx: AtomicU64,
}

struct PollingReader {
    state: Arc<ReaderState>,
    thread: Option<JoinHandle<()>>,
}

impl PollingReader {
    fn start(handle: HANDLE) -> Self {
        // HANDLE 是裸指针，跨线程需要显式转为整数再还原。
        let raw = handle.0 as isize;
        let state = Arc::new(ReaderState {
            buf: Mutex::new(Vec::with_capacity(4096)),
            cv: Condvar::new(),
            running: AtomicBool::new(true),
            last_rx: AtomicU64::new(0),
            total_rx: AtomicU64::new(0),
        });
        let st = Arc::clone(&state);
        let thread = thread::Builder::new()
            .name("ysc-cdc-rx".into())
            .spawn(move || {
                let handle = HANDLE(raw as *mut std::ffi::c_void);
                let mut tmp = [0u8; 1024];
                while st.running.load(Ordering::Acquire) {
                    match sync_read(handle, &mut tmp) {
                        Some(n) if n > 0 => {
                            let mut buf = st.buf.lock().unwrap();
                            buf.extend_from_slice(&tmp[..n]);
                            st.total_rx.fetch_add(n as u64, Ordering::Relaxed);
                            st.last_rx.store(now_ms(), Ordering::Relaxed);
                            drop(buf);
                            st.cv.notify_all();
                        }
                        _ => thread::sleep(Duration::from_millis(1)),
                    }
                }
            })
            .ok();
        Self { state, thread }
    }

    fn stop(&mut self) {
        self.state.running.store(false, Ordering::Release);
        self.state.cv.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    fn drain(&self) {
        self.state.buf.lock().unwrap().clear();
    }

    fn snapshot(&self) -> Vec<u8> {
        self.state.buf.lock().unwrap().clone()
    }

    fn ms_since_last_rx(&self) -> u32 {
        let t = self.state.last_rx.load(Ordering::Relaxed);
        if t == 0 {
            0xFFFF_FFFF
        } else {
            now_ms().saturating_sub(t) as u32
        }
    }

    fn total_rx_bytes(&self) -> u64 {
        self.state.total_rx.load(Ordering::Relaxed)
    }

    fn wait_for_frame(&self, timeout_ms: u32) -> Option<Vec<u8>> {
        let start = now_ms();
        while now_ms().saturating_sub(start) < timeout_ms as u64 {
            if let Some(frame) = self.try_extract() {
                return Some(frame);
            }
            let guard = self.state.buf.lock().unwrap();
            let _ = self
                .state
                .cv
                .wait_timeout(guard, Duration::from_millis(10))
                .unwrap();
        }
        None
    }

    fn try_extract(&self) -> Option<Vec<u8>> {
        let mut buf = self.state.buf.lock().unwrap();
        if buf.len() < 13 {
            return None;
        }
        let start = match find_start(&buf) {
            Some(s) => s,
            None => {
                if buf.len() > 6 {
                    let keep = buf.len() - 6;
                    buf.drain(..keep);
                }
                return None;
            }
        };
        if start > 0 {
            buf.drain(..start);
        }
        let content_start = 7usize;
        let end = match find_end(&buf, content_start) {
            Some(e) => e,
            None => {
                if buf.len() > 4096 {
                    buf.clear();
                }
                return None;
            }
        };
        let content_len = end - content_start;
        let payload = if content_len >= 2 {
            let declared = ((buf[content_start] as usize) << 8) | buf[content_start + 1] as usize;
            if declared == content_len + 12 {
                buf[content_start + 2..end].to_vec()
            } else {
                buf[content_start..end].to_vec()
            }
        } else {
            buf[content_start..end].to_vec()
        };
        buf.drain(..end + 5);
        Some(payload)
    }
}

impl Drop for PollingReader {
    fn drop(&mut self) {
        self.state.running.store(false, Ordering::Release);
        self.state.cv.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn find_start(buf: &[u8]) -> Option<usize> {
    buf.windows(7).position(|w| w == b"<START>")
}

fn find_end(buf: &[u8], from: usize) -> Option<usize> {
    if from + 5 > buf.len() {
        return None;
    }
    let mut i = from;
    while i + 5 <= buf.len() {
        if &buf[i..i + 5] == b"<END>" {
            return Some(i);
        }
        i += 1;
    }
    None
}

// ---------------------------------------------------------------------------
// IAP 序列
// ---------------------------------------------------------------------------

struct FrameResult {
    ok: bool,
    status: u8,
    attempts: i32,
    last_attempt_ms: u32,
    worst_attempt_ms: u32,
    last_timeout_buffer: Vec<u8>,
    ms_since_rx_at_timeout: u32,
    total_rx_at_timeout: u64,
}

fn send_recv_retry(
    h: HANDLE,
    reader: &PollingReader,
    frame: &[u8],
    timeout_ms: u32,
) -> FrameResult {
    let mut r = FrameResult {
        ok: false,
        status: 0,
        attempts: 0,
        last_attempt_ms: 0,
        worst_attempt_ms: 0,
        last_timeout_buffer: Vec::new(),
        ms_since_rx_at_timeout: 0,
        total_rx_at_timeout: 0,
    };
    for attempt in 0..FRAME_RETRIES {
        r.attempts = attempt + 1;
        reader.drain();
        let start = now_ms();
        if !sync_write(h, frame, 2000) {
            let el = now_ms().saturating_sub(start) as u32;
            r.last_attempt_ms = el;
            r.worst_attempt_ms = r.worst_attempt_ms.max(el);
            r.last_timeout_buffer = reader.snapshot();
            r.ms_since_rx_at_timeout = reader.ms_since_last_rx();
            r.total_rx_at_timeout = reader.total_rx_bytes();
            continue;
        }
        let payload = reader.wait_for_frame(timeout_ms);
        let el = now_ms().saturating_sub(start) as u32;
        r.last_attempt_ms = el;
        r.worst_attempt_ms = r.worst_attempt_ms.max(el);
        if let Some(p) = payload {
            if p.len() >= 2 {
                r.ok = true;
                r.status = p[1];
                return r;
            }
        }
        r.last_timeout_buffer = reader.snapshot();
        r.ms_since_rx_at_timeout = reader.ms_since_last_rx();
        r.total_rx_at_timeout = reader.total_rx_bytes();
        if attempt == 0 {
            thread::sleep(Duration::from_millis(RETRY_BREATHE_MS));
        }
    }
    r
}

fn run_iap_sequence(app: &App, iap_port: &str, firmware: &[u8], notify_success: bool) -> bool {
    let fsize = firmware.len() as i64;
    let total_chunks = ((firmware.len() + IAP_CHUNK_SZ - 1) / IAP_CHUNK_SZ) as i32;

    let Some(h) = open_cdc(iap_port) else {
        send_log(app, "无法打开 IAP 端口", "err");
        send_done(app, false, "无法打开 IAP 端口", None);
        return false;
    };
    let mut reader = PollingReader::start(h);

    // ERASE
    send_log(app, "发送擦除命令 ...", "");
    send_progress(app, 0, total_chunks, "擦除");
    {
        let frame = build_iap_frame(CMD_ERASE, &[]);
        let start = now_ms();
        let r = send_recv_retry(h, &reader, &frame, FRAME_TIMEOUT_SLOW_MS);
        let erase_ms = now_ms().saturating_sub(start);
        if !r.ok {
            let mut detail = format!(
                "擦除超时: 尝试 {}/{} 次 (单次上限 {}ms, 最坏 {}ms), RX 累计 {}B, 距上次RX {}ms, buffer 残留 {} 字节:",
                r.attempts,
                FRAME_RETRIES,
                FRAME_TIMEOUT_SLOW_MS,
                r.worst_attempt_ms,
                r.total_rx_at_timeout,
                r.ms_since_rx_at_timeout,
                r.last_timeout_buffer.len()
            );
            detail.push_str(&hex_dump(&r.last_timeout_buffer, 64));
            send_log(app, &detail, "err");
            reader.stop();
            close_sync(h);
            send_log(app, "擦除: 响应超时", "err");
            send_done(app, false, "擦除: 响应超时", None);
            return false;
        }
        if r.status != 0 {
            let msg = format!("擦除失败 (错误码: 0x{:02X})", r.status);
            reader.stop();
            close_sync(h);
            send_log(app, &msg, "err");
            send_done(app, false, &msg, None);
            return false;
        }
        send_log(
            app,
            &format!(
                "擦除成功 ({}ms, 尝试 {} 次, 最坏单次 {}ms, RX累计 {}B)",
                erase_ms,
                r.attempts,
                r.last_attempt_ms,
                reader.total_rx_bytes()
            ),
            "",
        );
    }

    // PROGRAM
    send_log(
        app,
        &format!(
            "开始编程 ({} 块, 单块 {} ms 超时) ...",
            total_chunks, FRAME_TIMEOUT_SLOW_MS
        ),
        "",
    );
    {
        let t1 = now_ms();
        let mut max_chunk_ms = 0u32;
        let mut sum_chunk_ms = 0u64;
        let mut retry_blocks = 0i32;
        let mut total_retries = 0i32;
        let mut last_retry_block = -1i32;
        let mut last_retry_attempts = 0i32;
        let mut last_retry_ms = 0u32;
        let mut rx_before = reader.total_rx_bytes();
        for i in 0..total_chunks {
            if app.iap2_cancel.load(Ordering::Acquire) {
                reader.stop();
                close_sync(h);
                send_log(app, "用户取消", "err");
                send_done(app, false, "用户取消", None);
                return false;
            }
            let offset = i as usize * IAP_CHUNK_SZ;
            let chunk_len = if i == total_chunks - 1 {
                firmware.len() - offset
            } else {
                IAP_CHUNK_SZ
            };
            let frame = build_iap_frame(CMD_PROM, &firmware[offset..offset + chunk_len]);
            let block_start = now_ms();
            let r = send_recv_retry(h, &reader, &frame, FRAME_TIMEOUT_SLOW_MS);
            let block_ms = now_ms().saturating_sub(block_start);
            if r.attempts > 1 {
                retry_blocks += 1;
                total_retries += r.attempts - 1;
                last_retry_block = i + 1;
                last_retry_attempts = r.attempts;
                last_retry_ms = r.worst_attempt_ms;
                send_log(
                    app,
                    &format!(
                        "编程块 {}/{} 触发重试: 第 {}/{} 次成功, 最坏 {}ms, 块耗时 {}ms",
                        i + 1,
                        total_chunks,
                        r.attempts,
                        FRAME_RETRIES,
                        r.worst_attempt_ms,
                        block_ms
                    ),
                    "warn",
                );
                let mut detail = format!(
                    "  ↳ 超时现场: RX累计 {}B (本块 +B{}), 距上次RX {}ms, buffer残留 {}字节:",
                    r.total_rx_at_timeout,
                    r.total_rx_at_timeout.saturating_sub(rx_before),
                    r.ms_since_rx_at_timeout,
                    r.last_timeout_buffer.len()
                );
                detail.push_str(&hex_dump(&r.last_timeout_buffer, 64));
                send_log(app, &detail, "warn");
            }
            if !r.ok {
                let mut detail = format!(
                    "编程块 {}/{} 全部超时: {} 次尝试, 最坏 {}ms, RX累计 {}B, 距上次RX {}ms, buffer残留 {}字节:",
                    i + 1,
                    total_chunks,
                    r.attempts,
                    r.worst_attempt_ms,
                    r.total_rx_at_timeout,
                    r.ms_since_rx_at_timeout,
                    r.last_timeout_buffer.len()
                );
                detail.push_str(&hex_dump(&r.last_timeout_buffer, 64));
                reader.stop();
                close_sync(h);
                send_log(app, &detail, "err");
                send_log(app, &detail, "err");
                send_done(app, false, &detail, None);
                return false;
            }
            if r.status != 0 {
                let msg = format!(
                    "编程块 {}/{} 失败 (0x{:02X})",
                    i + 1,
                    total_chunks,
                    r.status
                );
                reader.stop();
                close_sync(h);
                send_log(app, &msg, "err");
                send_done(app, false, &msg, None);
                return false;
            }
            rx_before = reader.total_rx_bytes();
            max_chunk_ms = max_chunk_ms.max(r.last_attempt_ms);
            sum_chunk_ms += r.last_attempt_ms as u64;
            if i % 10 == 0 || i == total_chunks - 1 {
                send_progress(
                    app,
                    i + 1,
                    total_chunks,
                    &format!("编程 {}/{}", i + 1, total_chunks),
                );
            }
        }
        let prog_ms = now_ms().saturating_sub(t1);
        let kbps = if prog_ms > 0 {
            fsize as f64 / (prog_ms as f64 / 1000.0) / 1024.0
        } else {
            0.0
        };
        send_log(
            app,
            &format!(
                "编程完成: 总耗时 {:.2}s ({:.1} KB/s), 单块最坏 {}ms / 平均 {}ms, 重试块 {}/{}, 重试次数 {} (最后: 块 {} 第 {} 次 {}ms)",
                prog_ms as f64 / 1000.0,
                kbps,
                max_chunk_ms,
                sum_chunk_ms / total_chunks.max(1) as u64,
                retry_blocks,
                total_chunks,
                total_retries,
                last_retry_block,
                last_retry_attempts,
                last_retry_ms
            ),
            "",
        );
    }

    // VERIFY
    send_log(app, "开始校验 ...", "");
    send_progress(app, 0, total_chunks, "校验");
    {
        let t2 = now_ms();
        let mut max_chunk_ms = 0u32;
        let mut sum_chunk_ms = 0u64;
        let mut retry_blocks = 0i32;
        let mut total_retries = 0i32;
        let mut last_retry_block = -1i32;
        let mut last_retry_attempts = 0i32;
        let mut last_retry_ms = 0u32;
        let mut rx_before = reader.total_rx_bytes();
        for i in 0..total_chunks {
            if app.iap2_cancel.load(Ordering::Acquire) {
                reader.stop();
                close_sync(h);
                send_log(app, "用户取消", "err");
                send_done(app, false, "用户取消", None);
                return false;
            }
            let offset = i as usize * IAP_CHUNK_SZ;
            let chunk_len = if i == total_chunks - 1 {
                firmware.len() - offset
            } else {
                IAP_CHUNK_SZ
            };
            let frame = build_iap_frame(CMD_VERIFY, &firmware[offset..offset + chunk_len]);
            let block_start = now_ms();
            let r = send_recv_retry(h, &reader, &frame, FRAME_TIMEOUT_FAST_MS);
            let block_ms = now_ms().saturating_sub(block_start);
            if r.attempts > 1 {
                retry_blocks += 1;
                total_retries += r.attempts - 1;
                last_retry_block = i + 1;
                last_retry_attempts = r.attempts;
                last_retry_ms = r.worst_attempt_ms;
                send_log(
                    app,
                    &format!(
                        "校验块 {}/{} 触发重试: 第 {}/{} 次成功, 最坏 {}ms, 块耗时 {}ms",
                        i + 1,
                        total_chunks,
                        r.attempts,
                        FRAME_RETRIES,
                        r.worst_attempt_ms,
                        block_ms
                    ),
                    "warn",
                );
                let mut detail = format!(
                    "  ↳ 超时现场: RX累计 {}B (本块 +B{}), 距上次RX {}ms, buffer残留 {}字节:",
                    r.total_rx_at_timeout,
                    r.total_rx_at_timeout.saturating_sub(rx_before),
                    r.ms_since_rx_at_timeout,
                    r.last_timeout_buffer.len()
                );
                detail.push_str(&hex_dump(&r.last_timeout_buffer, 64));
                send_log(app, &detail, "warn");
            }
            if !r.ok {
                let mut detail = format!(
                    "校验块 {}/{} 全部超时: {} 次尝试, 最坏 {}ms, RX累计 {}B, 距上次RX {}ms, buffer残留 {}字节:",
                    i + 1,
                    total_chunks,
                    r.attempts,
                    r.worst_attempt_ms,
                    r.total_rx_at_timeout,
                    r.ms_since_rx_at_timeout,
                    r.last_timeout_buffer.len()
                );
                detail.push_str(&hex_dump(&r.last_timeout_buffer, 64));
                reader.stop();
                close_sync(h);
                send_log(app, &detail, "err");
                send_log(app, &detail, "err");
                send_done(app, false, &detail, None);
                return false;
            }
            if r.status != 0 {
                let msg = format!("校验块 {}/{} 失败 — 固件不匹配", i + 1, total_chunks);
                reader.stop();
                close_sync(h);
                send_log(app, &msg, "err");
                send_log(app, &msg, "err");
                send_done(app, false, &msg, Some("FIRMWARE_MISMATCH"));
                return false;
            }
            rx_before = reader.total_rx_bytes();
            max_chunk_ms = max_chunk_ms.max(r.last_attempt_ms);
            sum_chunk_ms += r.last_attempt_ms as u64;
            if i % 10 == 0 || i == total_chunks - 1 {
                send_progress(
                    app,
                    i + 1,
                    total_chunks,
                    &format!("校验 {}/{}", i + 1, total_chunks),
                );
            }
            thread::sleep(Duration::from_millis(VERIFY_CHUNK_DELAY_MS));
        }
        let ver_ms = now_ms().saturating_sub(t2);
        send_log(
            app,
            &format!(
                "校验通过: 总耗时 {:.2}s, 单块最坏 {}ms / 平均 {}ms, 重试块 {}/{}, 重试次数 {} (最后: 块 {} 第 {} 次 {}ms)",
                ver_ms as f64 / 1000.0,
                max_chunk_ms,
                sum_chunk_ms / total_chunks.max(1) as u64,
                retry_blocks,
                total_chunks,
                total_retries,
                last_retry_block,
                last_retry_attempts,
                last_retry_ms
            ),
            "",
        );
    }

    // END（无应答）——**必须重发**：IAP 收到 END 才做双片联合判决跳 APP，
    // 丢一帧就永远停在 IAP。2026-10-24 真机：同一状态/同一镜像，python 脚本
    // 发一次就跳（0.7s），驱动只发一次却不跳 → 设备留在 IAP，自动烧录于是
    // 反复重烧。这里连发 3 次、间隔 300ms；设备跳走后后续写会失败，无所谓。
    send_log(app, "发送结束命令 ...", "");
    let end = build_iap_frame(CMD_END, &[]);
    for attempt in 0..3 {
        let _ = sync_write(h, &end, 2000);
        if attempt < 2 {
            thread::sleep(Duration::from_millis(300));
        }
    }

    reader.stop();
    close_sync(h);
    if notify_success {
        send_progress(app, total_chunks, total_chunks, "完成");
        send_log(app, "固件升级完成! 设备将跳转到 APP ...", "");
        send_done(app, true, "", None);
    }
    true
}

// ---------------------------------------------------------------------------
// 一次性查询 / 进入 IAP
// ---------------------------------------------------------------------------

pub fn query_version(app: Arc<App>, port: String) {
    if app.iap2_running.load(Ordering::Acquire) {
        send_log(&app, "升级正在进行中，无法查询版本", "warn");
        return;
    }
    let _ = thread::Builder::new()
        .name("ysc-towmcu-ver".into())
        .spawn(move || {
            let ver = query_version_on_port(&port);
            let family = family_of_version(&ver);
            let mode = if is_iap_version(&ver) { "IAP" } else { "APP" };
            app.send_event(
                "towmcu_version",
                Some(&format!(
                    "\"port\":\"{}\",\"version\":\"{}\",\"mode\":\"{}\"",
                    json_escape(&port),
                    json_escape(&ver),
                    mode
                )),
            );
            if ver.is_empty() {
                send_log(&app, "查询版本失败 — 设备无响应", "err");
            } else {
                send_log(
                    &app,
                    &format!("[{} / {}] {}", mode, family_name(family), ver),
                    "info",
                );
            }
        });
}

pub fn enter_iap(app: Arc<App>, port: String) {
    if app.iap2_running.load(Ordering::Acquire) {
        send_log(&app, "升级正在进行中", "warn");
        return;
    }
    let _ = thread::Builder::new()
        .name("ysc-towmcu-iap".into())
        .spawn(move || {
            let ver = query_version_on_port(&port);
            if ver.is_empty() {
                // 版本查不到 ≠ 设备不在。可能只是这一枪丢了（固件应答是 64B
                // 整数倍、CDC 刚枚举、驱动缓冲没就绪）。继续发 cmd50：它是幂等
                // 的 —— 设备已在 IAP 时 cmd50 无人应答，后面按端口扫描仍能找到。
                send_log(
                    &app,
                    "查询版本无响应 — 仍尝试发送 cmd50（查不到版本不等于进不去）",
                    "warn",
                );
            }
            // 端口来自 V3 面板（side=="V3"）；拿不到版本时按 YSC V3 处理，
            // 否则 family=None 会让 prepare_iap_port 直接判"无法识别的固件族"。
            let family = if ver.is_empty() {
                Some(CdcFamily::YscV3)
            } else {
                family_of_version(&ver)
            };
            if !ver.is_empty() && is_iap_version(&ver) {
                send_log(
                    &app,
                    &format!("设备已在 IAP: {} ({})", ver, family_name(family)),
                    "info",
                );
                app.send_event(
                    "iap_entered",
                    Some(&format!("\"port\":\"{}\"", json_escape(&port))),
                );
                return;
            }

            let Some(iap) = prepare_iap_port(&app, &port, &ver, family) else {
                app.send_event("iap_entered", Some("\"port\":\"\""));
                return;
            };
            send_log(
                &app,
                &format!("已进入 IAP: {} ({})", iap, family_name(family)),
                "info",
            );
            app.send_event(
                "iap_entered",
                Some(&format!("\"port\":\"{}\"", json_escape(&iap))),
            );
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v3_port_version_match() {
        assert!(v3_version_matches("YSC-V3-IAP", true));
        assert!(!v3_version_matches("YSC-V3-IAP", false));
        assert!(v3_version_matches("YSC-V3-APP Oct 21 2026", false));
        assert!(!v3_version_matches("YSC-V3-APP Oct 21 2026", true));
        assert!(v3_version_matches(
            r#"{"v":"1.1.0","raw":"YSC-V3-APP Oct 21 2026"}"#,
            false
        ));
        assert!(!v3_version_matches("", true));
        assert!(!v3_version_matches("ysc-towmcu-L v1.0", true));
    }

    #[test]
    fn family_labels() {
        assert_eq!(family_name(Some(CdcFamily::YscV3)), "V3");
        assert_eq!(family_name(Some(CdcFamily::Towmcu)), "V2");
        assert_eq!(family_name(None), "?");
    }
}
