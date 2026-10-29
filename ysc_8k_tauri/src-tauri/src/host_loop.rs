use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tauri::AppHandle;
use ysc_core::enumerate::enum_com_ports;
use ysc_core::util::now_ms;
use ysc_host::App;

use crate::notifier::HostMsg;

/// 宿主循环：替代旧 Win32 托盘的 `WM_TIMER` / `WM_DEVICECHANGE`。
///
/// - 100ms：推进设备探测状态机（`probe_tick`）；
/// - 1s：比对 COM 列表，变化时广播 `ports_changed` / `ports_list`；
/// - 处理串口/槽位失联、托盘提示等宿主消息。
pub fn spawn(app_handle: AppHandle, app: Arc<App>, rx: Receiver<HostMsg>) {
    // PC 手柄测试器：WebView2 没有 Gamepad API，由宿主用 XInput 轮询，
    // 仅当测试器通过 gamepad_watch 打开时推送。
    let gamepad_weak = Arc::downgrade(&app);
    let _ = thread::Builder::new()
        .name("ysc-gamepad-poll".into())
        .spawn(move || loop {
            thread::sleep(Duration::from_millis(16));
            let Some(app) = gamepad_weak.upgrade() else { break };
            if app.shutting_down.load(Ordering::Acquire) {
                break;
            }
            if app.gamepad_watch.load(Ordering::Acquire) {
                let body = ysc_host::gamepad::poll_body();
                app.emit("gamepad_state", Some(&body));
            }
        });

    let _ = thread::Builder::new()
        .name("ysc-tauri-host-loop".into())
        .spawn(move || {
            let mut last_ports: Vec<String> = enum_com_ports();
            let mut last_port_check = now_ms();
            app.emit_ports();
            app.update_tray_tip();

            loop {
                if app.shutting_down.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }

                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(HostMsg::SerialLost(gen)) => app.handle_legacy_lost(gen),
                    Ok(HostMsg::SlotLost(slot, gen)) => app.handle_slot_lost(slot, gen),
                    Ok(HostMsg::TrayTip(text)) => {
                        if let Some(tray) = app_handle.tray_by_id(crate::tray::TRAY_ID) {
                            let _ = tray.set_tooltip(Some(&text));
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }

                app.probe_tick();

                if now_ms().saturating_sub(last_port_check) >= 1000 {
                    last_port_check = now_ms();
                    let ports = enum_com_ports();
                    if ports != last_ports {
                        last_ports = ports;
                        app.emit_ports();
                        app.update_tray_tip();
                    }
                }
            }
        });
}
