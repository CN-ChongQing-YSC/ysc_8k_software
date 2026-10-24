//! YSC 8K 纯 Rust Tauri 单 exe 宿主。

mod auto_flash;
mod commands;
mod host_loop;
mod notifier;
mod sink;
mod state;
mod tray;

use std::sync::mpsc;
use std::sync::Arc;

use tauri::{Manager, WindowEvent};
use windows::Win32::Media::timeBeginPeriod;
use windows::Win32::Networking::WinSock::{WSAStartup, WSADATA};

use notifier::TauriNotifier;
use sink::TauriSink;
use state::TauriState;

/// 自动烧录配置：
/// - `--auto-flash-v3=<path>` 显式指定镜像；
/// - `--auto-flash-v3` 使用 exe 同目录 `v3_firmware.bin`；
/// - 未传参数时，若 exe 同目录存在 `v3_firmware.bin` 也自动启用；
/// - `--no-auto-flash` 强制普通启动。
fn resolve_auto_flash() -> Option<String> {
    let mut explicit: Option<String> = None;
    let mut disabled = false;
    for arg in std::env::args().skip(1) {
        if let Some(v) = arg.strip_prefix("--auto-flash-v3=") {
            explicit = Some(v.to_string());
        } else if arg == "--auto-flash-v3" {
            if explicit.is_none() {
                explicit = Some(String::new());
            }
        } else if arg == "--no-auto-flash" {
            disabled = true;
        }
    }
    if disabled {
        return None;
    }
    if let Some(p) = explicit {
        if !p.is_empty() {
            return Some(p);
        }
    }

    let exe = std::env::current_exe().ok()?;
    let p = exe.parent()?.join("v3_firmware.bin");
    p.is_file().then(|| p.to_string_lossy().into_owned())
}

pub fn run() {
    unsafe {
        let _ = timeBeginPeriod(1);
        let mut wsa = WSADATA::default();
        let _ = WSAStartup(0x0202, &mut wsa);
    }

    let (tx, rx) = mpsc::channel();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let handle = app.handle().clone();

            ysc_host::debug_log::DEBUG.init();
            ysc_host::debug_log::install_core_log_hook();
            ysc_host::debug_log::DEBUG.log(&format!(
                "=== YSC 8K Desktop (Tauri) v{} ===",
                env!("CARGO_PKG_VERSION")
            ));

            let auto_flash = resolve_auto_flash();
            if let Some(path) = &auto_flash {
                ysc_host::debug_log::DEBUG.log(&format!("=== AUTO FLASH V3: {path} ==="));
            }

            let sink = Arc::new(TauriSink::new(handle.clone()));
            let notifier = Arc::new(TauriNotifier::new(tx));
            let host = ysc_host::App::new(sink, notifier);
            host.start();

            app.manage(TauriState {
                app: Arc::clone(&host),
                auto_flash,
            });

            host_loop::spawn(handle.clone(), host, rx);
            tray::build(&handle)?;

            if let Some(window) = app.get_webview_window("main") {
                // 先 show，再关装饰：Tauri/tao 在 show 时会把 tauri.conf 的默认
                // decorations 重新应用一遍；放在 show 之后才能压住 WS_CAPTION。
                let _ = window.show();
                let _ = window.set_decorations(false);
                #[cfg(windows)]
                {
                    if let Ok(hwnd) = window.hwnd() {
                        use windows::Win32::UI::WindowsAndMessaging::{
                            GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_STYLE,
                            SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
                            WS_CAPTION, WS_THICKFRAME,
                        };
                        let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) };
                        let style = style & !((WS_CAPTION.0 as isize) | (WS_THICKFRAME.0 as isize));
                        unsafe { SetWindowLongPtrW(hwnd, GWL_STYLE, style) };
                        unsafe {
                            let _ = SetWindowPos(
                                hwnd,
                                None,
                                0,
                                0,
                                0,
                                0,
                                SWP_FRAMECHANGED
                                    | SWP_NOMOVE
                                    | SWP_NOSIZE
                                    | SWP_NOZORDER
                                    | SWP_NOACTIVATE,
                            );
                        }
                    }
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ysc_get_version,
            commands::ysc_file_info,
            commands::ysc_driver_is_connected,
            commands::ysc_driver_send,
            commands::ysc_auto_flash_info,
            commands::ysc_auto_flash_start,
            commands::ysc_set_lang,
            commands::ysc_window_minimize,
            commands::ysc_window_toggle_maximize,
            commands::ysc_window_is_maximized,
            commands::ysc_window_close,
            commands::ysc_quit,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { .. } = event {
                window.app_handle().exit(0);
            }
        })
        .run(tauri::generate_context!())
        .expect("YSC 8K Tauri 启动失败");
}
