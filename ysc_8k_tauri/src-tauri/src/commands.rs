use serde::Serialize;
use serde_json::Value;
use std::sync::atomic::Ordering;
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

use crate::state::TauriState;

#[derive(Serialize)]
pub struct PickedFile {
    pub path: String,
    pub info: String,
}

/// 手柄测试器独立窗口的几何：优先放在主窗口右侧；空间不够时贴到当前显示器右缘。
fn gamepad_tester_geom(app: &AppHandle) -> (f64, f64, f64, f64) {
    const TW: f64 = 560.0;
    let Some(main) = app.get_webview_window("main") else {
        return (120.0, 120.0, TW, 800.0);
    };
    let scale = main.scale_factor().unwrap_or(1.0);
    let pos = main
        .outer_position()
        .unwrap_or(PhysicalPosition::new(120, 120));
    let size = main
        .outer_size()
        .unwrap_or(PhysicalSize::new(1280, 820));
    let mut x = (pos.x as f64 + size.width as f64) / scale + 8.0;
    let mut y = pos.y as f64 / scale;
    let mut h = size.height as f64 / scale;

    if let Ok(Some(mon)) = main.current_monitor() {
        let ms = mon.scale_factor();
        let mp = mon.position();
        let msz = mon.size();
        let left = mp.x as f64 / ms + 8.0;
        let top = mp.y as f64 / ms + 8.0;
        let right = (mp.x as f64 + msz.width as f64) / ms - 8.0;
        let bottom = (mp.y as f64 + msz.height as f64) / ms - 8.0;
        if x + TW > right {
            x = (right - TW).max(left);
        }
        if x < left {
            x = left;
        }
        if y < top {
            y = top;
        }
        if y + h > bottom {
            h = (bottom - y).max(560.0);
        }
    }
    (x, y, TW, h.max(560.0))
}

/// 打开/聚焦右侧的手柄测试器独立窗口。
///
/// 必须是 async 命令：同步命令跑在主线程上，而建窗/查询窗口几何都要回调主线程，
/// 会互相等待死锁（表现为 invoke 永不返回、窗口也出不来）。
#[tauri::command]
pub async fn ysc_gamepad_tester_open(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("gamepad-tester") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        return Ok(());
    }

    let (x, y, w, h) = gamepad_tester_geom(&app);
    let win = WebviewWindowBuilder::new(
        &app,
        "gamepad-tester",
        WebviewUrl::App("tester.html".into()),
    )
    .title("手柄测试器")
    .inner_size(w, h)
    .position(x, y)
    .resizable(true)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .build()
    .map_err(|e| e.to_string())?;
    let _ = win.show();
    Ok(())
}

/// 关闭手柄测试器窗口；同时停掉 XInput 推送。async 原因同上。
#[tauri::command]
pub async fn ysc_gamepad_tester_close(app: AppHandle) {
    if let Some(state) = app.try_state::<TauriState>() {
        state.app.gamepad_watch.store(false, Ordering::Release);
    }
    if let Some(w) = app.get_webview_window("gamepad-tester") {
        let _ = w.close();
    }
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
/// `path` 省略时用启动时解析到的默认镜像（exe 同目录 v3_firmware.bin）。
#[tauri::command]
pub fn ysc_auto_flash_start(state: State<'_, TauriState>, path: Option<String>) -> bool {
    // 优先级：UI 传进来的路径 > 启动时解析的路径 > exe 同目录 v3_firmware.bin。
    // 最后一条让"用 --no-auto-flash 启动后，再从开关打开"也能工作。
    let path = path.or_else(|| state.auto_flash.clone()).or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("v3_firmware.bin")))
            .filter(|p| p.is_file())
            .map(|p| p.to_string_lossy().into_owned())
    });
    let Some(path) = path else {
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

    state.auto_cancel.store(false, Ordering::Release);
    crate::auto_flash::spawn(
        std::sync::Arc::clone(&state.app),
        path,
        std::sync::Arc::clone(&state.auto_cancel),
        std::sync::Arc::clone(&state.auto_running),
    );
    true
}

/// 关掉"自动烧录"开关：让后台循环在下一轮退出。
#[tauri::command]
pub fn ysc_auto_flash_stop(state: State<'_, TauriState>) -> bool {
    state.auto_cancel.store(true, Ordering::Release);
    true
}

/// 自动烧录线程是否还在等设备（UI 用来显示"等待插入/已停止"）。
#[tauri::command]
pub fn ysc_auto_flash_status(state: State<'_, TauriState>) -> bool {
    state.auto_running.load(Ordering::Acquire)
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
