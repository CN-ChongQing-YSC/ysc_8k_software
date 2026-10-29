//! v1 单 UART MCU 固件升级（对应 `iap_upgrader.cpp`）。
//!
//! 流程：ERASE → PROGRAM(60B) → VERIFY(60B) → END；每帧最多 3 次重试，
//! ERASE/PROGRAM 3s 超时、VERIFY 500ms。事件前缀 `iap_*`。

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::HANDLE;

use ysc_core::consts::{is_supported_baud, IAP_CHUNK_SZ, SUPPORTED_BAUDS};
use ysc_core::frame::{build_iap_frame, build_version_frame};
use ysc_core::serial::{close_sync, open_raw, purge_rx_tx, raw_read, raw_write};
use ysc_core::util::{hex_dump, json_escape, now_ms};

use crate::app::App;

const CMD_PROM: u8 = 0x80;
const CMD_ERASE: u8 = 0x81;
const CMD_VERIFY: u8 = 0x82;
const CMD_END: u8 = 0x83;

const FRAME_RETRIES: i32 = 3;
const FRAME_TIMEOUT_FAST_MS: u32 = 500;
const FRAME_TIMEOUT_SLOW_MS: u32 = 3000;
const RETRY_BREATHE_MS: u64 = 100;

fn send_log(app: &App, msg: &str, cls: &str) {
    app.send_event(
        "iap_log",
        Some(&format!(
            "\"message\":\"{}\",\"cls\":\"{}\"",
            json_escape(msg),
            cls
        )),
    );
}

fn send_progress(app: &App, cur: i32, total: i32, status: &str) {
    app.send_event(
        "iap_progress",
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
    app.send_event("iap_done", Some(&body));
}

pub fn start_file(app: Arc<App>, path: String, target_baud: u32) {
    if app.iap_running.load(Ordering::Acquire) {
        return;
    }
    match std::fs::read(&path) {
        Ok(fw) if !fw.is_empty() => start_worker(app, fw, target_baud),
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

pub fn start_mem(app: Arc<App>, firmware: Vec<u8>, target_baud: u32) {
    if app.iap_running.load(Ordering::Acquire) {
        return;
    }
    if firmware.is_empty() {
        send_log(&app, "固件数据为空", "err");
        send_done(&app, false, "固件数据为空", None);
        return;
    }
    start_worker(app, firmware, target_baud);
}

fn start_worker(app: Arc<App>, firmware: Vec<u8>, target_baud: u32) {
    app.iap_running.store(true, Ordering::Release);
    app.iap_cancel.store(false, Ordering::Release);
    let _ = thread::Builder::new()
        .name("ysc-iap-v1".into())
        .spawn(move || worker(app, firmware, target_baud));
}

struct RunningGuard(Arc<App>);
impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.0.iap_running.store(false, Ordering::Release);
    }
}

fn worker(app: Arc<App>, firmware: Vec<u8>, target_baud: u32) {
    let _guard = RunningGuard(Arc::clone(&app));
    let fsize = firmware.len() as i64;
    let total_chunks = ((firmware.len() + IAP_CHUNK_SZ - 1) / IAP_CHUNK_SZ) as i32;

    send_log(
        &app,
        &format!("固件大小: {} 字节 ({:.1} KB)", fsize, fsize as f64 / 1024.0),
        "",
    );

    let (port, known_baud, was_connected) = {
        let f = app.flags.lock().unwrap();
        (f.serial_port.clone(), f.serial_baud, f.serial_connected)
    };
    if port.is_empty() {
        send_log(&app, "未指定串口端口", "err");
        send_done(&app, false, "未指定串口端口", None);
        return;
    }

    // 独占串口：先停掉常驻读写线程并关闭句柄。
    if was_connected {
        app.legacy.stop_read_thread();
        app.legacy.disconnect();
        app.flags.lock().unwrap().serial_connected = false;
    }

    let mut handle: Option<HANDLE> = None;
    let mut baud = 0u32;
    if known_baud > 0 {
        send_log(&app, &format!("打开 {} @ {} ...", port, known_baud), "");
        if let Some(h) = open_raw(&port, known_baud) {
            handle = Some(h);
            baud = known_baud;
        }
    }
    if handle.is_none() {
        send_log(&app, "探测波特率...", "");
        let (b, h) = detect_iap_baud(&port, known_baud);
        baud = b;
        handle = h;
    }
    let Some(mut h) = handle else {
        send_log(&app, "无法连接设备", "err");
        send_done(&app, false, "无法连接设备", None);
        return;
    };
    if baud == 0 {
        close_sync(h);
        send_log(&app, "无法连接设备", "err");
        send_done(&app, false, "无法连接设备", None);
        return;
    }
    send_log(&app, &format!("已连接 @ {}", baud), "");

    if target_baud == 0 {
        send_log(&app, &format!("保持当前波特率 {} (不切换)", baud), "");
    } else if baud != target_baud {
        send_log(
            &app,
            &format!("切换波特率 {} -> {} ...", baud, target_baud),
            "",
        );
        match switch_baud(h, &port, baud, target_baud) {
            Some(nh) => {
                h = nh;
                baud = target_baud;
                send_log(&app, "波特率切换成功", "");
            }
            None => {
                send_log(&app, "波特率切换失败", "err");
                send_done(&app, false, "波特率切换失败", None);
                return;
            }
        }
    }

    // ---- ERASE ----
    send_log(&app, "发送擦除命令...", "");
    send_progress(&app, 0, total_chunks, "擦除");
    let frame = build_iap_frame(CMD_ERASE, &[]);
    {
        let start = now_ms();
        let r = send_recv_retry(h, &frame, FRAME_TIMEOUT_SLOW_MS);
        let erase_ms = now_ms().saturating_sub(start);
        if !r.ok {
            let mut detail = format!(
                "擦除超时: 尝试 {}/{} 次 (单次上限 {}ms, 最坏 {}ms), 残留 {} 字节:",
                r.attempts,
                FRAME_RETRIES,
                FRAME_TIMEOUT_SLOW_MS,
                r.worst_ms,
                r.last_residual.len()
            );
            detail.push_str(&hex_dump(&r.last_residual, 64));
            send_log(&app, &detail, "err");
            close_sync(h);
            send_log(&app, "擦除: 响应超时", "err");
            send_done(&app, false, "擦除: 响应超时", None);
            return;
        }
        if r.status != 0 {
            let msg = format!("擦除失败 (错误码: 0x{:02X})", r.status);
            close_sync(h);
            send_log(&app, &msg, "err");
            send_done(&app, false, &msg, None);
            return;
        }
        send_log(
            &app,
            &format!(
                "擦除成功 ({}ms, 尝试 {} 次, 最坏单次 {}ms)",
                erase_ms, r.attempts, r.worst_ms
            ),
            "",
        );
    }

    // ---- PROGRAM ----
    send_log(
        &app,
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

        for i in 0..total_chunks {
            if app.iap_cancel.load(Ordering::Acquire) {
                close_sync(h);
                send_log(&app, "用户取消", "err");
                send_done(&app, false, "用户取消", None);
                return;
            }
            let offset = i as usize * IAP_CHUNK_SZ;
            let chunk_len = if i == total_chunks - 1 {
                firmware.len() - offset
            } else {
                IAP_CHUNK_SZ
            };
            let frame = build_iap_frame(CMD_PROM, &firmware[offset..offset + chunk_len]);
            let block_start = now_ms();
            let r = send_recv_retry(h, &frame, FRAME_TIMEOUT_SLOW_MS);
            let block_ms = now_ms().saturating_sub(block_start);
            if r.attempts > 1 {
                retry_blocks += 1;
                total_retries += r.attempts - 1;
                last_retry_block = i + 1;
                last_retry_attempts = r.attempts;
                last_retry_ms = r.worst_ms;
                send_log(
                    &app,
                    &format!(
                        "编程块 {}/{} 触发重试: 第 {}/{} 次成功, 最坏 {}ms, 块耗时 {}ms",
                        i + 1,
                        total_chunks,
                        r.attempts,
                        FRAME_RETRIES,
                        r.worst_ms,
                        block_ms
                    ),
                    "warn",
                );
                let mut detail = format!("  ↳ 残留 {} 字节:", r.last_residual.len());
                detail.push_str(&hex_dump(&r.last_residual, 64));
                send_log(&app, &detail, "warn");
            }
            if !r.ok {
                let mut detail = format!(
                    "编程块 {}/{} 全部超时: {} 次尝试, 最坏 {}ms, 残留 {} 字节:",
                    i + 1,
                    total_chunks,
                    r.attempts,
                    r.worst_ms,
                    r.last_residual.len()
                );
                detail.push_str(&hex_dump(&r.last_residual, 64));
                close_sync(h);
                send_log(&app, &detail, "err");
                send_log(&app, &detail, "err");
                send_done(&app, false, &detail, None);
                return;
            }
            if r.status != 0 {
                let msg = format!(
                    "编程块 {}/{} 失败 (0x{:02X})",
                    i + 1,
                    total_chunks,
                    r.status
                );
                close_sync(h);
                send_log(&app, &msg, "err");
                send_done(&app, false, &msg, None);
                return;
            }
            max_chunk_ms = max_chunk_ms.max(r.last_ms);
            sum_chunk_ms += r.last_ms as u64;
            if i % 10 == 0 || i == total_chunks - 1 {
                send_progress(
                    &app,
                    i + 1,
                    total_chunks,
                    &format!("编程 {}/{}", i + 1, total_chunks),
                );
            }
        }
        let prog_ms = now_ms().saturating_sub(t1);
        let kbps = if prog_ms > 0 {
            (fsize as f64 / (prog_ms as f64 / 1000.0) / 1024.0) as f64
        } else {
            0.0
        };
        send_log(
            &app,
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

    // ---- VERIFY ----
    send_log(&app, "开始校验...", "");
    send_progress(&app, 0, total_chunks, "校验");
    {
        let t2 = now_ms();
        let mut max_chunk_ms = 0u32;
        let mut sum_chunk_ms = 0u64;
        let mut retry_blocks = 0i32;
        let mut total_retries = 0i32;
        let mut last_retry_block = -1i32;
        let mut last_retry_attempts = 0i32;
        let mut last_retry_ms = 0u32;

        for i in 0..total_chunks {
            if app.iap_cancel.load(Ordering::Acquire) {
                close_sync(h);
                send_log(&app, "用户取消", "err");
                send_done(&app, false, "用户取消", None);
                return;
            }
            let offset = i as usize * IAP_CHUNK_SZ;
            let chunk_len = if i == total_chunks - 1 {
                firmware.len() - offset
            } else {
                IAP_CHUNK_SZ
            };
            let frame = build_iap_frame(CMD_VERIFY, &firmware[offset..offset + chunk_len]);
            let block_start = now_ms();
            let r = send_recv_retry(h, &frame, FRAME_TIMEOUT_FAST_MS);
            let block_ms = now_ms().saturating_sub(block_start);
            if r.attempts > 1 {
                retry_blocks += 1;
                total_retries += r.attempts - 1;
                last_retry_block = i + 1;
                last_retry_attempts = r.attempts;
                last_retry_ms = r.worst_ms;
                send_log(
                    &app,
                    &format!(
                        "校验块 {}/{} 触发重试: 第 {}/{} 次成功, 最坏 {}ms, 块耗时 {}ms",
                        i + 1,
                        total_chunks,
                        r.attempts,
                        FRAME_RETRIES,
                        r.worst_ms,
                        block_ms
                    ),
                    "warn",
                );
                let mut detail = format!("  ↳ 残留 {} 字节:", r.last_residual.len());
                detail.push_str(&hex_dump(&r.last_residual, 64));
                send_log(&app, &detail, "warn");
            }
            if !r.ok {
                let mut detail = format!(
                    "校验块 {}/{} 全部超时: {} 次尝试, 最坏 {}ms, 残留 {} 字节:",
                    i + 1,
                    total_chunks,
                    r.attempts,
                    r.worst_ms,
                    r.last_residual.len()
                );
                detail.push_str(&hex_dump(&r.last_residual, 64));
                close_sync(h);
                send_log(&app, &detail, "err");
                send_log(&app, &detail, "err");
                send_done(&app, false, &detail, None);
                return;
            }
            if r.status != 0 {
                let msg = format!("校验块 {}/{} 失败 — 固件不匹配", i + 1, total_chunks);
                close_sync(h);
                send_log(&app, &msg, "err");
                send_log(&app, &msg, "err");
                send_done(&app, false, &msg, Some("FIRMWARE_MISMATCH"));
                return;
            }
            max_chunk_ms = max_chunk_ms.max(r.last_ms);
            sum_chunk_ms += r.last_ms as u64;
            if i % 10 == 0 || i == total_chunks - 1 {
                send_progress(
                    &app,
                    i + 1,
                    total_chunks,
                    &format!("校验 {}/{}", i + 1, total_chunks),
                );
            }
        }
        let ver_ms = now_ms().saturating_sub(t2);
        send_log(
            &app,
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

    // ---- END ----
    send_log(&app, "发送结束命令...", "");
    let end = build_iap_frame(CMD_END, &[]);
    let _ = raw_write(h, &end, 1000);
    send_progress(&app, total_chunks, total_chunks, "完成");
    send_log(&app, "固件升级完成! 设备将重启进入 APP", "");
    send_done(&app, true, "", None);
    close_sync(h);

    send_log(&app, "等待设备重启...", "");
    thread::sleep(Duration::from_millis(2000));
    send_log(&app, "重新连接设备...", "");
    let (reconn_baud, reconn_handle) = detect_iap_baud(&port, baud);
    if reconn_baud > 0 {
        if let Some(rh) = reconn_handle {
            close_sync(rh);
        }
        if app.legacy.connect(&port, reconn_baud).is_ok() {
            {
                let mut f = app.flags.lock().unwrap();
                f.serial_connected = true;
                f.serial_baud = reconn_baud;
                f.serial_v3_cdc = false;
                f.serial_port = port.clone();
            }
            let _ = app.legacy.start_read_thread();
            app.send_event(
                "serial_connected",
                Some(&format!(
                    "\"port\":\"{}\",\"baud\":{},\"v3\":false,\"baudFree\":false",
                    json_escape(&port),
                    reconn_baud
                )),
            );
            send_log(&app, &format!("已重连 @ {}", reconn_baud), "info");
        }
    } else {
        send_log(&app, "自动重连失败，请手动连接", "warn");
    }

    let _ = is_supported_baud;
    let _ = SUPPORTED_BAUDS;
}

struct FrameResult {
    ok: bool,
    status: u8,
    attempts: i32,
    last_ms: u32,
    worst_ms: u32,
    last_residual: Vec<u8>,
}

fn send_recv_retry(handle: HANDLE, frame: &[u8], timeout_ms: u32) -> FrameResult {
    let mut r = FrameResult {
        ok: false,
        status: 0,
        attempts: 0,
        last_ms: 0,
        worst_ms: 0,
        last_residual: Vec::new(),
    };
    for attempt in 0..FRAME_RETRIES {
        r.attempts = attempt + 1;
        purge_rx_tx(handle);
        let t0 = now_ms();
        let write_ok = raw_write(handle, frame, 2000);
        let mut elapsed = now_ms().saturating_sub(t0);
        let mut buf: Vec<u8> = Vec::new();
        let mut found = false;

        if write_ok {
            let deadline = t0 + timeout_ms as u64;
            while now_ms() < deadline {
                let remaining = 256usize.saturating_sub(buf.len());
                if remaining == 0 {
                    break;
                }
                let chunk = raw_read(handle, remaining, 50);
                if !chunk.is_empty() {
                    buf.extend_from_slice(&chunk);
                }
                let total = buf.len();
                let mut i = 0usize;
                while i + 14 <= total {
                    if &buf[i..i + 7] != b"<START>" {
                        i += 1;
                        continue;
                    }
                    if i + 9 > total {
                        break;
                    }
                    let frame_len = ((buf[i + 7] as usize) << 8) | buf[i + 8] as usize;
                    let end = i + frame_len;
                    if end > total {
                        break;
                    }
                    if &buf[end - 5..end] != b"<END>" {
                        i += 1;
                        continue;
                    }
                    let payload_len = frame_len.saturating_sub(14);
                    if payload_len >= 2 {
                        r.status = buf[i + 10];
                        found = true;
                        break;
                    }
                    i += 1;
                }
                if found {
                    break;
                }
            }
            elapsed = now_ms().saturating_sub(t0);
        }

        r.last_ms = elapsed as u32;
        if elapsed as u32 > r.worst_ms {
            r.worst_ms = elapsed as u32;
        }
        if found {
            r.ok = true;
            return r;
        }
        r.last_residual = buf;
        if attempt == 0 {
            thread::sleep(Duration::from_millis(RETRY_BREATHE_MS));
        }
    }
    r
}

fn detect_iap_baud(port: &str, hint: u32) -> (u32, Option<HANDLE>) {
    let ver = build_version_frame();
    let mut order: Vec<u32> = Vec::with_capacity(SUPPORTED_BAUDS.len() + 1);
    if hint > 0 {
        order.push(hint);
    }
    for &b in &SUPPORTED_BAUDS {
        if b != hint {
            order.push(b);
        }
    }

    for round in 0..3 {
        for &baud in &order {
            let Some(h) = open_raw(port, baud) else {
                continue;
            };
            purge_rx_tx(h);
            thread::sleep(Duration::from_millis(20));
            let _ = raw_write(h, &ver, 2000);
            let buf = raw_read(h, 128, 500);
            close_sync(h);
            if buf.len() >= 9 && &buf[..7] == b"<START>" {
                let text = String::from_utf8_lossy(&buf);
                if text.contains("\"code\":200") || text.contains("\"code\": 200") {
                    if let Some(h2) = open_raw(port, baud) {
                        return (baud, Some(h2));
                    }
                }
            }
        }
        if round < 2 {
            thread::sleep(Duration::from_millis(300));
        }
    }
    (0, None)
}

fn switch_baud(handle: HANDLE, port: &str, cur: u32, new: u32) -> Option<HANDLE> {
    if cur == new {
        return Some(handle);
    }
    let json = format!("{{\"cmd\":133,\"baud\":{}}}", new);
    let frame = ysc_core::frame::build_json_body_frame(&json);
    let _ = raw_write(handle, &frame, 2000);
    let _ = raw_read(handle, 64, 500);
    thread::sleep(Duration::from_millis(100));
    close_sync(handle);
    let h = open_raw(port, new)?;
    let ver = build_version_frame();
    let _ = raw_write(h, &ver, 2000);
    let n = raw_read(h, 128, 1000);
    if n.is_empty() {
        close_sync(h);
        return None;
    }
    Some(h)
}
