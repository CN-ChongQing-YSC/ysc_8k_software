//! 管道命令分发（对应 `main.cpp::OnPipeCommand`）。
//!
//! 命令在管道读线程上串行执行（与 C++ 相同）。所有事件字段、文案、默认值
//! 都按 C++ 实现逐条对齐。

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use base64::Engine;
use serde_json::Value;

use ysc_core::consts::{
    driver_version, is_supported_baud, Role, DEFAULT_MAC, KMBOXNET_DEFAULT_PORT,
};
use ysc_core::enumerate::{
    detect_ch343_driver_status, enum_com_ports, enum_towmcu_ports, is_ysc_v3_port,
};
use ysc_core::family::{family_of_version, CdcFamily};
use ysc_core::serial::SerialPort;
use ysc_core::util::json_escape;

use crate::app::App;

/// YSC V3 USB CDC 没有真实波特率；主机句柄固定用安全速率打开。
const CDC_SAFE_BAUD: u32 = 115_200;

// ---------------------------------------------------------------------------
// 极简 JSON 取值（容忍数字/字符串/布尔三种线上形态）
// ---------------------------------------------------------------------------

fn get_str(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

fn get_i64(v: &Value, key: &str) -> i64 {
    match v.get(key) {
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0),
        Some(Value::Bool(b)) => {
            if *b {
                1
            } else {
                0
            }
        }
        _ => 0,
    }
}

fn get_i32(v: &Value, key: &str) -> i32 {
    get_i64(v, key) as i32
}

fn get_bool(v: &Value, key: &str) -> bool {
    match v.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0) != 0,
        Some(Value::String(s)) => s == "true" || s == "1",
        _ => false,
    }
}

fn has(v: &Value, key: &str) -> bool {
    v.get(key).is_some()
}

/// V3 USB CDC 接受任意正波特率（只作显示）；普通串口仍受固件白名单限制。
fn baud_allowed(baud: u32, v3: bool) -> bool {
    baud > 0 && (v3 || is_supported_baud(baud))
}

fn err_result(app: &App, msg: &str) {
    app.send_event(
        "serial_error",
        Some(&format!("\"message\":\"{}\"", json_escape(msg))),
    );
}

fn send_result_ok(app: &App, cmd: &str) {
    app.send_event(
        "send_result",
        Some(&format!("\"ok\":true,\"cmd\":\"{}\"", cmd)),
    );
}

fn send_result_fail(app: &App, cmd: &str) {
    app.send_event(
        "send_result",
        Some(&format!("\"ok\":false,\"cmd\":\"{}\"", cmd)),
    );
}

fn require_serial(app: &App) -> bool {
    if !app.flags.lock().unwrap().serial_connected {
        app.send_event("send_result", Some("\"ok\":false,\"error\":\"未连接设备\""));
        false
    } else {
        true
    }
}

// ---------------------------------------------------------------------------
// 入口
// ---------------------------------------------------------------------------

pub fn dispatch(app: &Arc<App>, line: &str) {
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return,
    };
    let ty = get_str(&v, "type");
    if ty.is_empty() {
        return;
    }

    match ty.as_str() {
        "enum_ports" => {
            let ports = enum_com_ports();
            let mut body = String::from("\"ports\":[");
            for (i, p) in ports.iter().enumerate() {
                if i > 0 {
                    body.push(',');
                }
                body.push_str(&format!("\"{}\"", json_escape(p)));
            }
            body.push(']');
            app.send_event("ports_list", Some(&body));
        }
        "device_list" => app.send_device_list(),
        "device_attach" => cmd_device_attach(app, &v),
        "device_detach" => {
            let mut dev = get_i32(&v, "dev");
            if !(0..2).contains(&dev) {
                dev = 0;
            }
            app.detach_device(dev as usize);
            app.send_event("device_detached", Some(&format!("\"dev\":{}", dev)));
            app.send_device_list();
        }
        "device_set_role" => {
            let mut dev = get_i32(&v, "dev");
            if !(0..2).contains(&dev) {
                dev = 0;
            }
            let role = Role::from_str(&get_str(&v, "role"));
            app.set_role(dev as usize, role);
            app.send_device_list();
        }
        "device_set_baud" => cmd_device_set_baud(app, &v),
        "get_kbd_state" => {
            let body = app.serialize_kbd_json();
            app.send_event("kbd_state", Some(&body));
        }
        "debug_inject_slot" => {
            if get_i32(&v, "test") == 1 {
                let dev = get_i32(&v, "dev");
                let hex = get_str(&v, "rawHex");
                let payload = decode_hex(&hex);
                if (0..2).contains(&dev) && !payload.is_empty() {
                    app.on_device_data(dev as usize, &payload);
                    app.send_event("inject_result", Some("\"ok\":true"));
                } else {
                    app.send_event("inject_result", Some("\"ok\":false"));
                }
            }
        }
        "kbd_key" => {
            let kc = get_i32(&v, "kc");
            let down = get_i32(&v, "down");
            let ok = app.send_keyboard_key(kc as u8, down != 0, Role::Kbd);
            app.send_event(
                "kbd_result",
                Some(&format!("\"ok\":{},\"kc\":{},\"down\":{}", ok, kc, down)),
            );
        }
        "kbd_type_string" => {
            let s = get_str(&v, "s");
            let ok = app.send_keyboard_type_string(&s, Role::Kbd);
            app.send_event("kbd_result", Some(&format!("\"ok\":{}", ok)));
        }
        "kbd_release_all" => {
            let ok = app.send_keyboard_release_all(Role::Kbd);
            app.send_event("kbd_result", Some(&format!("\"ok\":{}", ok)));
        }
        "serial_connect" => cmd_serial_connect(app, &v),
        "gamepad_watch" => {
            let on = get_bool(&v, "on");
            app.gamepad_watch.store(on, Ordering::Release);
            app.send_event(
                "gamepad_watch",
                Some(if on { "\"on\":true" } else { "\"on\":false" }),
            );
        }
        "gamepad_vibrate" => {
            let index = get_i32(&v, "index").clamp(0, 3) as u32;
            let left = get_i32(&v, "left").clamp(0, 65535) as u16;
            let right = get_i32(&v, "right").clamp(0, 65535) as u16;
            let ok = crate::gamepad::vibrate(index, left, right);
            app.send_event(
                "gamepad_vibrate_result",
                Some(&format!("\"ok\":{},\"index\":{}", ok, index)),
            );
        }
        "serial_disconnect" => {
            app.guard_abort("device_lost");
            app.legacy.disconnect();
            app.flags.lock().unwrap().serial_connected = false;
            app.log("SERIAL DISCONNECTED");
            app.update_tray_tip();
            app.send_event("serial_disconnected", None);
        }
        "switch_baudrate" => cmd_switch_baudrate(app, &v),
        "get_kmnet_stat" => {
            let body = app.kmbox.get_stat_json();
            if !body.is_empty() {
                app.send_event("kmnet_stat", Some(&body));
            }
        }
        "kmnet_start" => cmd_kmnet_start(app, &v),
        "kmnet_stop" => {
            app.debug_window_log("Net service STOP");
            app.log("NET SERVICE STOP");
            app.kmbox.stop();
            app.flags.lock().unwrap().net_running = false;
            app.update_tray_tip();
            app.send_event("kmnet_stopped", None);
        }
        "upload_enable" => {
            let enable = get_bool(&v, "enable");
            app.send_upload_status(enable, Role::Mouse);
            app.send_event("upload_status", Some(&format!("\"enable\":{}", enable)));
        }
        "jump_iap" => {
            app.send_jump_iap(Role::None);
        }
        "get_local_ip" => {
            let ip = crate::kmbox::get_local_ip();
            app.send_event(
                "local_ip",
                Some(&format!("\"ip\":\"{}\"", json_escape(&ip))),
            );
        }
        "get_version" => {
            app.send_event(
                "version",
                Some(&format!("\"version\":\"{}\"", driver_version())),
            );
        }
        "get_monitor" => {
            let body = app.serialize_ui_json();
            app.send_event("monitor_data", Some(&body));
        }
        "get_state" => cmd_get_state(app),
        "send_ysc" => cmd_send_ysc(app, &v),
        "release_button" => cmd_release_button(app, &v),
        "force_release" => cmd_force_release(app, &v),
        "force_release_query" => {
            if require_serial(app) {
                let ok = app.send_force_release_query(Role::Mouse);
                if ok {
                    send_result_ok(app, "force_release_query");
                } else {
                    send_result_fail(app, "force_release_query");
                }
            }
        }
        "force_restore" => {
            if require_serial(app) {
                let ok = app.send_force_restore(Role::Mouse);
                if ok {
                    send_result_ok(app, "force_restore");
                } else {
                    send_result_fail(app, "force_restore");
                }
            }
        }
        "set_gamepad_config" => {
            let cfg = get_str(&v, "data");
            if !require_serial(app) {
            } else if cfg.is_empty() {
                app.send_event("send_result", Some("\"ok\":false,\"error\":\"empty data\""));
            } else {
                let ok = app.send_gamepad_config(&cfg, Role::Mouse);
                if ok {
                    send_result_ok(app, "gmap_set");
                } else {
                    app.send_event("send_result", Some("\"ok\":false"));
                }
            }
        }
        "get_gamepad_config" => {
            if require_serial(app) {
                let ok = app.send_get_gamepad_config(Role::Mouse);
                if ok {
                    send_result_ok(app, "gmap_get");
                } else {
                    send_result_fail(app, "gmap_get");
                }
            }
        }
        "set_gamepad_enable" => {
            if require_serial(app) {
                let on = get_i32(&v, "on") != 0;
                let ok = app.send_gamepad_enable(on, Role::Mouse);
                if ok {
                    send_result_ok(app, "gmap_enable");
                } else {
                    send_result_fail(app, "gmap_enable");
                }
            }
        }
        "reset_gamepad_config" => {
            if require_serial(app) {
                let ok = app.send_gamepad_reset(Role::Mouse);
                if ok {
                    send_result_ok(app, "gmap_reset");
                } else {
                    send_result_fail(app, "gmap_reset");
                }
            }
        }
        "set_dev_mode" => {
            if require_serial(app) {
                let mode = get_i32(&v, "mode");
                let ok = app.send_dev_mode_set(mode, Role::Mouse);
                if ok {
                    send_result_ok(app, "devmode_set");
                } else {
                    send_result_fail(app, "devmode_set");
                }
            }
        }
        "get_dev_mode" => {
            if require_serial(app) {
                let ok = app.send_dev_mode_get(Role::Mouse);
                if ok {
                    send_result_ok(app, "devmode_get");
                } else {
                    send_result_fail(app, "devmode_get");
                }
            }
        }
        "set_interp_config" => {
            if require_serial(app) {
                let enable = get_i32(&v, "enable");
                let profile = get_i32(&v, "profile");
                let window = get_i32(&v, "window");
                let maxw = get_i32(&v, "maxw");
                let ok = app.send_interp_set(enable, profile, window, maxw, Role::Mouse);
                if ok {
                    send_result_ok(app, "interp_set");
                } else {
                    send_result_fail(app, "interp_set");
                }
            }
        }
        "get_interp_config" => {
            if require_serial(app) {
                let ok = app.send_interp_get(Role::Mouse);
                if ok {
                    send_result_ok(app, "interp_get");
                } else {
                    send_result_fail(app, "interp_get");
                }
            }
        }
        "reset_interp_config" => {
            if require_serial(app) {
                let ok = app.send_interp_reset(Role::Mouse);
                if ok {
                    send_result_ok(app, "interp_reset");
                } else {
                    send_result_fail(app, "interp_reset");
                }
            }
        }
        "human_guard_begin" => {
            let axis = get_str(&v, "axis");
            let mut duration = get_i32(&v, "duration");
            if duration <= 0 {
                duration = 5000;
            }
            let ok = app.guard_begin(&axis, duration);
            if ok {
                send_result_ok(app, "human_guard_begin");
            } else {
                send_result_fail(app, "human_guard_begin");
            }
        }
        "human_guard_stop" => {
            app.guard_stop();
            send_result_ok(app, "human_guard_stop");
        }
        "human_guard_status" => app.guard_status(),
        "set_guard_config" => {
            if app.resolve_serial(Role::Mouse).is_none() {
                app.send_event(
                    "send_result",
                    Some("\"ok\":false,\"error\":\"未连接鼠标设备\",\"cmd\":\"guard_set\""),
                );
            } else {
                let enable = get_i32(&v, "enable");
                let wx = get_i32(&v, "wx");
                let wy = get_i32(&v, "wy");
                let ok = app.send_guard_set(enable, wx, wy, Role::Mouse);
                if ok {
                    send_result_ok(app, "guard_set");
                } else {
                    send_result_fail(app, "guard_set");
                }
            }
        }
        "get_guard_config" => {
            if app.resolve_serial(Role::Mouse).is_none() {
                app.send_event(
                    "send_result",
                    Some("\"ok\":false,\"error\":\"未连接鼠标设备\",\"cmd\":\"guard_get\""),
                );
            } else {
                let ok = app.send_guard_get(Role::Mouse);
                if ok {
                    send_result_ok(app, "guard_get");
                } else {
                    send_result_fail(app, "guard_get");
                }
            }
        }
        "reset_guard_config" => {
            if app.resolve_serial(Role::Mouse).is_none() {
                app.send_event(
                    "send_result",
                    Some("\"ok\":false,\"error\":\"未连接鼠标设备\",\"cmd\":\"guard_reset\""),
                );
            } else {
                let ok = app.send_guard_reset(Role::Mouse);
                if ok {
                    send_result_ok(app, "guard_reset");
                } else {
                    send_result_fail(app, "guard_reset");
                }
            }
        }
        "send_makcu" => cmd_send_makcu(app, &v),
        "iap_enter" => {
            if !app.flags.lock().unwrap().serial_connected {
                app.send_event(
                    "iap_log",
                    Some("\"message\":\"设备未连接\",\"cls\":\"err\""),
                );
                return;
            }
            app.send_jump_iap(Role::None);
            thread::sleep(Duration::from_millis(200));
            app.send_event(
                "iap_log",
                Some("\"message\":\"已进入下载模式\",\"cls\":\"info\""),
            );
        }
        "iap_start" => {
            if app.iap_running.load(std::sync::atomic::Ordering::Acquire) {
                app.send_event(
                    "iap_log",
                    Some("\"message\":\"升级正在进行中\",\"cls\":\"warn\""),
                );
                return;
            }
            let path = get_str(&v, "path");
            if path.is_empty() {
                app.send_event(
                    "iap_log",
                    Some("\"message\":\"未选择固件文件\",\"cls\":\"err\""),
                );
                return;
            }
            let baud = get_i32(&v, "baud") as u32;
            crate::iap::start_file(Arc::clone(app), path, baud);
        }
        "iap_start_mem" => {
            if app.iap_running.load(std::sync::atomic::Ordering::Acquire) {
                app.send_event(
                    "iap_log",
                    Some("\"message\":\"升级正在进行中\",\"cls\":\"warn\""),
                );
                return;
            }
            let b64 = get_str(&v, "data");
            if b64.is_empty() {
                app.send_event(
                    "iap_log",
                    Some("\"message\":\"未收到固件数据\",\"cls\":\"err\""),
                );
                return;
            }
            let fw = match base64::engine::general_purpose::STANDARD.decode(b64.as_bytes()) {
                Ok(fw) if !fw.is_empty() => fw,
                _ => {
                    app.send_event(
                        "iap_log",
                        Some("\"message\":\"固件 Base64 解码失败\",\"cls\":\"err\""),
                    );
                    return;
                }
            };
            let baud = get_i32(&v, "baud") as u32;
            crate::iap::start_mem(Arc::clone(app), fw, baud);
        }
        "iap_cancel" => {
            app.iap_cancel
                .store(true, std::sync::atomic::Ordering::Release);
        }
        "towmcu_list_ports" => cmd_towmcu_list_ports(app),
        "check_ch343_driver" => cmd_check_ch343(app),
        "towmcu_query_version" => {
            let port = get_str(&v, "port");
            if port.is_empty() {
                app.send_event(
                    "iap2_log",
                    Some("\"message\":\"No port specified\",\"cls\":\"err\""),
                );
                return;
            }
            crate::iap2::query_version(Arc::clone(app), port);
        }
        "towmcu_enter_iap" => {
            let port = get_str(&v, "port");
            if port.is_empty() {
                app.send_event(
                    "iap2_log",
                    Some("\"message\":\"No port specified\",\"cls\":\"err\""),
                );
                return;
            }
            crate::iap2::enter_iap(Arc::clone(app), port);
        }
        "towmcu_start" => {
            if app.iap2_running.load(std::sync::atomic::Ordering::Acquire) {
                app.send_event(
                    "iap2_log",
                    Some("\"message\":\"升级正在进行中\",\"cls\":\"warn\""),
                );
                return;
            }
            let port = get_str(&v, "port");
            let path = get_str(&v, "path");
            if port.is_empty() || path.is_empty() {
                app.send_event(
                    "iap2_log",
                    Some("\"message\":\"未指定端口或固件\",\"cls\":\"err\""),
                );
                return;
            }
            crate::iap2::start_file(Arc::clone(app), port, path);
        }
        "towmcu_start_mem" => {
            if app.iap2_running.load(std::sync::atomic::Ordering::Acquire) {
                app.send_event(
                    "iap2_log",
                    Some("\"message\":\"升级正在进行中\",\"cls\":\"warn\""),
                );
                return;
            }
            let port = get_str(&v, "port");
            let b64 = get_str(&v, "data");
            if port.is_empty() || b64.is_empty() {
                app.send_event(
                    "iap2_log",
                    Some("\"message\":\"未指定端口或固件数据\",\"cls\":\"err\""),
                );
                return;
            }
            let fw = match base64::engine::general_purpose::STANDARD.decode(b64.as_bytes()) {
                Ok(fw) if !fw.is_empty() => fw,
                _ => {
                    app.send_event(
                        "iap2_log",
                        Some("\"message\":\"固件 Base64 解码失败\",\"cls\":\"err\""),
                    );
                    return;
                }
            };
            crate::iap2::start_mem(Arc::clone(app), port, fw);
        }
        "towmcu_cancel" => {
            app.iap2_cancel
                .store(true, std::sync::atomic::Ordering::Release);
        }
        "debug_enter" => {
            let ok = app.flags.lock().unwrap().serial_connected && app.debug_enter(Role::Mouse);
            app.send_event(
                "debug_cmd_result",
                Some(if ok {
                    "\"cmd\":\"enter\",\"ok\":true"
                } else {
                    "\"cmd\":\"enter\",\"ok\":false,\"error\":\"未连接设备\""
                }),
            );
        }
        "debug_exit" => {
            let ok = app.flags.lock().unwrap().serial_connected && app.debug_exit(Role::Mouse);
            app.send_event(
                "debug_cmd_result",
                Some(if ok {
                    "\"cmd\":\"exit\",\"ok\":true"
                } else {
                    "\"cmd\":\"exit\",\"ok\":false,\"error\":\"未连接设备\""
                }),
            );
        }
        "debug_status" => {
            let ok = app.flags.lock().unwrap().serial_connected && app.debug_status(Role::Mouse);
            if !ok {
                app.send_event(
                    "debug_cmd_result",
                    Some("\"cmd\":\"status\",\"ok\":false,\"error\":\"未连接设备\""),
                );
            }
        }
        "debug_get_dev_descr" => {
            let ok =
                app.flags.lock().unwrap().serial_connected && app.debug_get_dev_descr(Role::Mouse);
            if !ok {
                app.send_event(
                    "debug_cmd_result",
                    Some("\"cmd\":\"dev_descr\",\"ok\":false,\"error\":\"未连接设备\""),
                );
            }
        }
        "debug_get_cfg_descr" => {
            let ok =
                app.flags.lock().unwrap().serial_connected && app.debug_get_cfg_descr(Role::Mouse);
            if !ok {
                app.send_event(
                    "debug_cmd_result",
                    Some("\"cmd\":\"cfg_descr\",\"ok\":false,\"error\":\"未连接设备\""),
                );
            }
        }
        "debug_get_rep_descr" => {
            let offset = get_i32(&v, "offset") as u16;
            let mut max = get_i32(&v, "max");
            if max <= 0 {
                max = 200;
            }
            let ok = app.flags.lock().unwrap().serial_connected
                && app.debug_get_report_descr(offset, max as u16, Role::Mouse);
            if !ok {
                app.send_event(
                    "debug_cmd_result",
                    Some("\"cmd\":\"rep_descr\",\"ok\":false,\"error\":\"未连接设备\""),
                );
            }
        }
        "debug_get_report" => {
            let idx = get_i32(&v, "idx") as u8;
            let ok = app.flags.lock().unwrap().serial_connected
                && app.debug_get_report(idx, Role::Mouse);
            if !ok {
                app.send_event(
                    "debug_cmd_result",
                    Some("\"cmd\":\"report\",\"ok\":false,\"error\":\"未连接设备\""),
                );
            }
        }
        "debug_clear_reports" => {
            let ok =
                app.flags.lock().unwrap().serial_connected && app.debug_clear_reports(Role::Mouse);
            app.send_event(
                "debug_cmd_result",
                Some(if ok {
                    "\"cmd\":\"clear\",\"ok\":true"
                } else {
                    "\"cmd\":\"clear\",\"ok\":false,\"error\":\"未连接设备\""
                }),
            );
        }
        "debug_get_device_info" => {
            let ok = app.flags.lock().unwrap().serial_connected
                && app.debug_get_device_info(Role::Mouse);
            if !ok {
                app.send_event(
                    "debug_cmd_result",
                    Some("\"cmd\":\"device_info\",\"ok\":false,\"error\":\"未连接设备\""),
                );
            }
        }
        "debug_set_mode" => {
            let mut mode = get_i32(&v, "mode");
            if !(0..=2).contains(&mode) {
                mode = 1;
            }
            let ok = app.flags.lock().unwrap().serial_connected
                && app.debug_set_mode(mode as u8, Role::Mouse);
            if !ok {
                app.send_event(
                    "debug_cmd_result",
                    Some("\"cmd\":\"set_mode\",\"ok\":false,\"error\":\"未连接设备\""),
                );
            }
        }
        "debug_get_diag" => {
            let ok = app.flags.lock().unwrap().serial_connected && app.debug_get_diag(Role::Mouse);
            if !ok {
                app.send_event(
                    "debug_cmd_result",
                    Some("\"cmd\":\"diag\",\"ok\":false,\"error\":\"未连接设备\""),
                );
            }
        }
        "debug_get_events" => {
            let mut from = get_i32(&v, "from");
            let mut n = get_i32(&v, "n");
            if from < 0 {
                from = 0;
            }
            if n <= 0 {
                n = 16;
            }
            let ok = app.flags.lock().unwrap().serial_connected
                && app.debug_get_events(from as u16, n as u16, Role::Mouse);
            if !ok {
                app.send_event(
                    "debug_cmd_result",
                    Some("\"cmd\":\"events\",\"ok\":false,\"error\":\"未连接设备\""),
                );
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// 各命令实现
// ---------------------------------------------------------------------------

fn cmd_device_attach(app: &Arc<App>, v: &Value) {
    let port = get_str(v, "port");
    let mut dev = get_i32(v, "dev");
    let baud_req = get_i32(v, "baud");
    let role_str = get_str(v, "role");
    if port.is_empty() {
        err_result(app, "No port specified");
        return;
    }
    if !(0..2).contains(&dev) {
        dev = 0;
    }

    for i in 0..2usize {
        if i as i32 != dev {
            let s = app.slots[i].lock().unwrap();
            if s.connected && s.port == port {
                err_result(app, "该串口已挂在另一个设备槽位");
                return;
            }
        }
    }

    let v3_hint = is_ysc_v3_port(&port);
    let baud = if baud_req > 0 {
        let b = baud_req as u32;
        if !baud_allowed(b, v3_hint) {
            err_result(app, "Invalid baudrate");
            return;
        }
        b
    } else {
        let b = SerialPort::detect_baudrate(&port);
        if b == 0 {
            err_result(app, "无法检测到设备");
            return;
        }
        b
    };

    app.detach_device(dev as usize);
    let actual = match app.attach_device(dev as usize, &port, baud) {
        Ok(b) => b,
        Err(_) => {
            err_result(app, "无法打开串口");
            return;
        }
    };
    if role_str == "mouse" {
        app.set_role(dev as usize, Role::Mouse);
    } else if role_str == "kbd" {
        app.set_role(dev as usize, Role::Kbd);
    }

    let v3 = app.slots[dev as usize].lock().unwrap().v3_cdc;
    app.send_event(
        "device_attached",
        Some(&format!(
            "\"dev\":{},\"port\":\"{}\",\"baud\":{},\"v3\":{},\"baudFree\":{}",
            dev,
            json_escape(&port),
            actual,
            v3,
            v3
        )),
    );
    app.log(&format!(
        "DEVICE attach slot={} port={} baud={}",
        dev, port, actual
    ));
    app.send_device_list();
}

fn cmd_device_set_baud(app: &App, v: &Value) {
    let mut dev = get_i32(v, "dev");
    let baud = get_i32(v, "baud") as u32;
    if !(0..2).contains(&dev) {
        dev = 0;
    }
    let (connected, v3) = {
        let s = app.slots[dev as usize].lock().unwrap();
        (s.serial.is_some() && s.connected, s.v3_cdc)
    };
    if !connected {
        app.send_event(
            "baudrate_failed",
            Some("\"dev\":0,\"message\":\"device not connected\""),
        );
        return;
    }
    if !baud_allowed(baud, v3) {
        app.send_event("baudrate_failed", Some("\"message\":\"invalid baudrate\""));
        return;
    }
    let cur = app.slots[dev as usize].lock().unwrap().baud;
    if baud == cur {
        app.send_event(
            "baudrate_switched",
            Some(&format!(
                "\"dev\":{},\"baud\":{},\"v3\":{},\"baudFree\":{}",
                dev, baud, v3, v3
            )),
        );
        return;
    }
    app.log(&format!(
        "DEVICE slot={} switch baud {} -> {} (v3={})",
        dev, cur, baud, v3
    ));
    if app.switch_device_baud(dev as usize, baud) {
        app.send_event(
            "baudrate_switched",
            Some(&format!(
                "\"dev\":{},\"baud\":{},\"v3\":{},\"baudFree\":{}",
                dev, baud, v3, v3
            )),
        );
        app.send_device_list();
    } else {
        app.send_event(
            "baudrate_failed",
            Some(&format!("\"dev\":{},\"v3\":{}", dev, v3)),
        );
        app.send_device_list();
    }
}

fn cmd_serial_connect(app: &Arc<App>, v: &Value) {
    let port = get_str(v, "port");
    let baud_req = get_i32(v, "baud");
    if port.is_empty() {
        err_result(app, "No port specified");
        return;
    }

    // 异常残留状态：先释放旧句柄，否则 DetectBaudrate 全部访问被拒。
    if app.legacy.is_connected() {
        app.legacy.disconnect();
        app.flags.lock().unwrap().serial_connected = false;
        app.update_tray_tip();
    }

    let v3_hint = is_ysc_v3_port(&port);
    let baud = if baud_req > 0 {
        let b = baud_req as u32;
        if !baud_allowed(b, v3_hint) {
            err_result(app, "Invalid baudrate");
            return;
        }
        b
    } else {
        SerialPort::detect_baudrate(&port)
    };
    if baud == 0 {
        err_result(app, "无法检测到设备");
        return;
    }

    // YSC V3 CDC 的 DCB 速率对 USB 吞吐没有影响；用固定安全速率打开，
    // 这样用户传任意 baud（例如 1234567）也不会被 Windows 拒绝。
    let open_baud = if v3_hint { CDC_SAFE_BAUD } else { baud };
    match app.legacy.connect(&port, open_baud) {
        Ok(()) => {
            // 序列号没枚举出来时，再用 cmd132 兜底确认是不是 V3。
            let v3 = v3_hint
                || app
                    .legacy
                    .query_version(400)
                    .map(|v| family_of_version(&v) == Some(CdcFamily::YscV3))
                    .unwrap_or(false);
            app.debug_window_log(&format!(
                "Serial CONNECTED {} @ {} (v3={})",
                port, baud, v3
            ));
            app.log(&format!(
                "SERIAL CONNECTED {} @ {} (v3={})",
                port, baud, v3
            ));
            {
                let mut f = app.flags.lock().unwrap();
                f.serial_port = port.clone();
                f.serial_baud = baud;
                f.serial_v3_cdc = v3;
                f.serial_connected = true;
            }
            let _ = app.legacy.start_read_thread();
            // 旧接口也登记进多设备层（槽 0）。串口独占，通常这里会失败，
            // C++ 同样忽略返回值；单设备命令随后走 g_serial 兜底。
            app.detach_device(0);
            let _ = app.attach_device(0, &port, baud);
            app.update_tray_tip();
            app.send_event(
                "serial_connected",
                Some(&format!(
                    "\"port\":\"{}\",\"baud\":{},\"v3\":{},\"baudFree\":{}",
                    json_escape(&port),
                    baud,
                    v3,
                    v3
                )),
            );
            app.send_device_list();
        }
        Err(e) => err_result(app, &format!("无法打开串口: {}", e)),
    }
}

fn cmd_switch_baudrate(app: &App, v: &Value) {
    let baud_req = get_i32(v, "baud");
    let (connected, cur, v3) = {
        let f = app.flags.lock().unwrap();
        (f.serial_connected, f.serial_baud, f.serial_v3_cdc)
    };
    if baud_req <= 0 {
        app.send_event("baudrate_failed", Some("\"message\":\"invalid baudrate\""));
        return;
    }
    if !connected {
        app.send_event(
            "baudrate_failed",
            Some("\"message\":\"device not connected\""),
        );
        return;
    }
    let new_baud = baud_req as u32;
    if !baud_allowed(new_baud, v3) {
        app.send_event("baudrate_failed", Some("\"message\":\"invalid baudrate\""));
        return;
    }
    if new_baud == cur {
        app.send_event(
            "baudrate_switched",
            Some(&format!(
                "\"baud\":{},\"v3\":{},\"baudFree\":{}",
                new_baud, v3, v3
            )),
        );
        return;
    }

    // YSC V3 USB CDC：波特率只是显示值，USB 始终全速/高速传输。
    // 不关端口、不重开（重开会把任意 baud 交给 Windows，很容易被拒绝）。
    if v3 {
        let _ = app
            .legacy
            .send_json_command(&format!("{{\"cmd\":133,\"baud\":{}}}", new_baud));
        {
            let mut f = app.flags.lock().unwrap();
            f.serial_baud = new_baud;
            f.serial_v3_cdc = true;
            f.serial_connected = true;
        }
        app.update_tray_tip();
        app.log(&format!(
            "SERIAL baud {} -> {} (V3 USB CDC, logical only)",
            cur, new_baud
        ));
        app.send_event(
            "baudrate_switched",
            Some(&format!(
                "\"baud\":{},\"v3\":true,\"baudFree\":true",
                new_baud
            )),
        );
        return;
    }

    if app.legacy.switch_baudrate(new_baud) {
        {
            let mut f = app.flags.lock().unwrap();
            f.serial_baud = new_baud;
            f.serial_connected = true;
        }
        let _ = app.legacy.start_read_thread();
        app.update_tray_tip();
        app.send_event(
            "baudrate_switched",
            Some(&format!(
                "\"baud\":{},\"v3\":false,\"baudFree\":false",
                new_baud
            )),
        );
    } else {
        app.flags.lock().unwrap().serial_connected = false;
        app.update_tray_tip();
        app.send_event(
            "baudrate_failed",
            Some("\"message\":\"切换失败，设备可能已断开\",\"v3\":false"),
        );
    }
}

fn cmd_kmnet_start(app: &App, v: &Value) {
    let mut port = get_i32(v, "port");
    if port <= 0 {
        port = KMBOXNET_DEFAULT_PORT as i32;
    }
    let port = port as u16;
    if app.kmbox.start(port) {
        {
            let mut f = app.flags.lock().unwrap();
            f.net_running = true;
            f.net_port = port;
        }
        app.debug_window_log(&format!("Net service START on port {}", port));
        app.log(&format!("NET SERVICE START on port {}", port));
        app.update_tray_tip();
        let ip = crate::kmbox::get_local_ip();
        *app.net_ip.lock().unwrap() = ip.clone();
        app.send_event(
            "kmnet_started",
            Some(&format!(
                "\"port\":{},\"ip\":\"{}\",\"mac\":\"{:08X}\"",
                port,
                json_escape(&ip),
                DEFAULT_MAC
            )),
        );
    } else {
        app.send_event("kmnet_error", Some("\"message\":\"Net 服务启动失败\""));
    }
}

fn cmd_get_state(app: &App) {
    let (connected, port, baud, v3) = app.serial_snapshot();
    // C++ 只在连接时把端口名写进 state；断开后端口名显示为空。
    let port = if connected { port } else { String::new() };
    let (net_running, net_port) = {
        let f = app.flags.lock().unwrap();
        (f.net_running, f.net_port)
    };
    let net_ip = if net_running {
        crate::kmbox::get_local_ip()
    } else {
        String::new()
    };
    let mouse = app.serialize_mouse_json();
    let kbd = app.serialize_kbd_json();
    let body = format!(
        "\"serialConnected\":{},\"serialPort\":\"{}\",\"serialBaud\":{},\"serialV3\":{},\"serialBaudFree\":{},\"netRunning\":{},\"netPort\":{},\"netIp\":\"{}\",{},{}",
        connected,
        json_escape(&port),
        baud,
        v3,
        v3,
        net_running,
        net_port,
        json_escape(&net_ip),
        mouse,
        kbd
    );
    app.send_event("state", Some(&body));
}

fn cmd_send_ysc(app: &App, v: &Value) {
    let cmd = get_str(v, "cmd");
    if cmd.is_empty() {
        app.send_event("send_result", Some("\"ok\":false,\"error\":\"empty cmd\""));
        return;
    }
    if !app.flags.lock().unwrap().serial_connected {
        app.send_event("send_result", Some("\"ok\":false,\"error\":\"未连接设备\""));
        return;
    }
    let ok = app.send_raw_ysc(&cmd, Role::None);
    app.send_event(
        "send_result",
        Some(&format!(
            "\"ok\":{},\"dir\":\"tx\",\"proto\":\"ysc\",\"cmd\":\"{}\"",
            ok,
            json_escape(&cmd)
        )),
    );
}

fn cmd_release_button(app: &App, v: &Value) {
    if !require_serial(app) {
        return;
    }
    let mut b = get_i32(v, "b");
    b = b.clamp(0, 0x1F);
    let s = if has(v, "s") { get_i32(v, "s") } else { 1 };
    let ok = app.send_button_release(b as u8, s != 0, Role::Mouse);
    app.send_event(
        "send_result",
        Some(&format!(
            "\"ok\":{},\"cmd\":\"release_button\",\"b\":{},\"s\":{}",
            ok,
            b,
            if s != 0 { 1 } else { 0 }
        )),
    );
}

fn cmd_force_release(app: &App, v: &Value) {
    if !require_serial(app) {
        return;
    }
    let mut b = get_i32(v, "b");
    let mut t = get_i32(v, "t");
    b = b.clamp(0, 0x1F);
    if t < 0 {
        t = 0;
    }
    let ok = app.send_force_release(b as u8, t as u32, Role::Mouse);
    app.send_event(
        "send_result",
        Some(&format!(
            "\"ok\":{},\"cmd\":\"force_release\",\"b\":{},\"t\":{}",
            ok, b, t
        )),
    );
}

fn cmd_send_makcu(app: &App, v: &Value) {
    let cmd = get_str(v, "cmd");
    if cmd.is_empty() {
        app.send_event("send_result", Some("\"ok\":false,\"error\":\"empty cmd\""));
        return;
    }
    if !app.flags.lock().unwrap().serial_connected {
        app.send_event("send_result", Some("\"ok\":false,\"error\":\"未连接设备\""));
        return;
    }
    let ok = app.send_raw_makcu(&cmd, Role::None);
    app.send_event(
        "send_result",
        Some(&format!(
            "\"ok\":{},\"dir\":\"tx\",\"proto\":\"makcu\",\"cmd\":\"{}\"",
            ok,
            json_escape(&cmd)
        )),
    );
}

fn cmd_towmcu_list_ports(app: &App) {
    if app.iap2_running.load(std::sync::atomic::Ordering::Acquire) {
        app.send_event("towmcu_ports", Some("\"ports\":[]"));
        return;
    }
    let ports = enum_towmcu_ports();
    let mut body = String::from("\"ports\":[");
    for (i, p) in ports.iter().enumerate() {
        if i > 0 {
            body.push(',');
        }
        body.push_str(&format!(
            "{{\"port\":\"{}\",\"serial\":\"{}\",\"side\":\"{}\",\"desc\":\"{}\"}}",
            json_escape(&p.port),
            json_escape(&p.serial),
            json_escape(&p.side),
            json_escape(&p.description)
        ));
    }
    body.push(']');
    app.send_event("towmcu_ports", Some(&body));
}

fn cmd_check_ch343(app: &App) {
    let st = detect_ch343_driver_status();
    let mut body = format!(
        "\"present\":{},\"problem\":{},\"problems\":[",
        st.any_present, st.any_problem
    );
    for (i, p) in st.problems.iter().enumerate() {
        if i > 0 {
            body.push(',');
        }
        body.push_str(&format!(
            "{{\"pid\":\"{}\",\"name\":\"{}\",\"code\":{}}}",
            json_escape(&p.pid_hex),
            json_escape(&p.friendly_name),
            p.problem_code
        ));
    }
    body.push(']');
    app.send_event("ch343_driver_status", Some(&body));
}

fn decode_hex(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    if b.len() % 2 != 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(b.len() / 2);
    let mut i = 0;
    while i + 1 < b.len() {
        let hv = |c: u8| -> Option<u8> {
            match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                b'A'..=b'F' => Some(c - b'A' + 10),
                _ => None,
            }
        };
        let (Some(h), Some(l)) = (hv(b[i]), hv(b[i + 1])) else {
            return Vec::new();
        };
        out.push((h << 4) | l);
        i += 2;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v3_accepts_any_positive_baud() {
        assert!(baud_allowed(1, true));
        assert!(baud_allowed(1_234_567, true));
        assert!(baud_allowed(8_000_000, true));
        assert!(!baud_allowed(0, true));
    }

    #[test]
    fn non_v3_keeps_firmware_whitelist() {
        assert!(baud_allowed(115_200, false));
        assert!(baud_allowed(4_000_000, false));
        assert!(!baud_allowed(1_234_567, false));
        assert!(!baud_allowed(0, false));
    }
}
