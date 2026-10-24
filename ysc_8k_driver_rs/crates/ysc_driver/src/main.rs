//! YSC 8K 驱动（Rust 移植版）入口。
//!
//! 单实例托盘程序，独占串口，向 Electron 暴露命名管道 `\\.\pipe\ysc_8k_driver`。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod pipe;
mod tray;

use std::sync::Arc;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HINSTANCE};
use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows::Win32::Networking::WinSock::{WSACleanup, WSAStartup, WSADATA};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONWARNING, MB_OK};

use ysc_host::app::App;
use ysc_host::debug_log::{install_core_log_hook, DEBUG};

use crate::pipe::{PipeServer, PipeSink};
use crate::tray::Win32Notifier;

fn main() {
    // 单实例（与 C++ 相同的互斥体名）。
    let mutex = unsafe { CreateMutexW(None, false, w!("Ysc8kDriver_SingleInstance")) };
    let already = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    if already {
        let text: Vec<u16> = "程序已在运行\0".encode_utf16().collect();
        let caption: Vec<u16> = "YSC 8K Driver\0".encode_utf16().collect();
        unsafe {
            MessageBoxW(
                None,
                PCWSTR(text.as_ptr()),
                PCWSTR(caption.as_ptr()),
                MB_OK | MB_ICONWARNING,
            );
        }
        return;
    }

    unsafe {
        let _ = timeBeginPeriod(1);
        let mut wsa = WSADATA::default();
        let _ = WSAStartup(0x0202, &mut wsa);
    }

    DEBUG.init();
    install_core_log_hook();
    DEBUG.log(&format!(
        "=== YSC 8K Driver (Rust) v{} ===",
        ysc_core::consts::driver_version()
    ));

    let pipe = Arc::new(PipeServer::new());
    let sink = Arc::new(PipeSink::new(Arc::clone(&pipe)));
    let notifier = Arc::new(Win32Notifier::new());
    let app: Arc<App> = App::new(sink, notifier.clone());
    app.start();

    let weak = Arc::downgrade(&app);
    pipe.set_command_callback(Arc::new(move |line: &str| {
        if let Some(app) = weak.upgrade() {
            ysc_host::commands::dispatch(&app, line);
        }
    }));
    pipe.start();

    let hinstance = HINSTANCE(
        unsafe { GetModuleHandleW(None) }
            .map(|m| m.0)
            .unwrap_or(std::ptr::null_mut()),
    );
    let code = tray::run(app, notifier, hinstance);

    pipe.stop();
    unsafe {
        let _ = timeEndPeriod(1);
        let _ = WSACleanup();
    }
    if let Ok(h) = mutex {
        unsafe {
            let _ = CloseHandle(h);
        }
    }
    std::process::exit(code);
}
