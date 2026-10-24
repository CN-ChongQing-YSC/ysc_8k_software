//! 命令桥（对应 `command_bridge.cpp`）：按角色解析目标串口并构造固件 JSON。
//!
//! 所有命令码、字段名、日志文案都与 C++ 实现保持一致；发送是异步入队，
//! 返回值只表示"已写入串口队列"（与 C++ `SendJsonCommand` 语义相同）。

use std::sync::{Arc, Mutex};

use once_cell::sync::Lazy;

use ysc_core::consts::Role;
use ysc_core::serial::SerialPort;
use ysc_core::util::json_escape;

use crate::app::App;

static KB_PREV: Lazy<Mutex<Vec<u8>>> = Lazy::new(|| Mutex::new(Vec::new()));

impl App {
    /// 按角色解析目标串口。
    ///
    /// 优先级与 C++ 一致：
    ///   1) DeviceContext 里角色匹配且已连接的设备；
    ///   2) 只有一台已连接设备时任何角色都解析到它；
    ///   3) 一台都没连时退回旧单例 `g_serial`。
    pub fn resolve_serial(&self, role: Role) -> Option<Arc<SerialPort>> {
        self.resolve_serial_real(role)
    }

    fn tx_json(&self, role: Role, json: &str) -> bool {
        self.tx_json_impl(role, json, true)
    }

    fn tx_json_impl(&self, role: Role, json: &str, with_port_log: bool) -> bool {
        let Some(sp) = self.resolve_serial_real(role) else {
            return false;
        };
        if with_port_log {
            self.debug_window_log(&format!("TX serial: {}", json));
            self.log(&format!("TX[{}] {}", sp.port_name(), json));
        } else {
            self.debug_window_log(&format!("TX serial: {}", json));
            self.log(&format!("TX {}", json));
        }
        sp.send_json_command(json)
    }

    /// 与 [`Self::resolve_serial`] 相同，但能返回 legacy 单例的共享句柄。
    pub fn resolve_serial_real(&self, role: Role) -> Option<Arc<SerialPort>> {
        if let Some(sp) = self.serial_for_role(role) {
            if sp.is_connected() {
                return Some(sp);
            }
        }
        let mut connected = 0;
        for i in 0..2 {
            if self.slots[i].lock().unwrap().connected {
                connected += 1;
            }
        }
        // 与 C++ CommandBridge::Resolve 一致：只看句柄是否打开，不看 UI 的
        // serialConnected 标志（切换波特率失败后句柄仍可能开着）。
        if connected == 0 && self.legacy.is_connected() {
            return Some(Arc::clone(&self.legacy));
        }
        None
    }

    pub fn send_mouse_move(&self, x: i32, y: i32, steps: i32, role: Role) -> bool {
        let json = format!("{{\"cmd\":30,\"x\":{},\"y\":{},\"c\":{}}}", x, y, steps);
        self.tx_json(role, &json)
    }

    pub fn send_move_tow(&self, x: i32, y: i32, steps: i32, role: Role) -> bool {
        let json = format!("{{\"cmd\":31,\"x\":{},\"y\":{},\"c\":{}}}", x, y, steps);
        self.tx_json(role, &json)
    }

    #[allow(dead_code)]
    pub fn send_mouse_button(&self, mask: u8, pressed: bool, role: Role) -> bool {
        let json = format!(
            "{{\"cmd\":33,\"b\":{},\"s\":{}}}",
            mask,
            if pressed { 1 } else { 0 }
        );
        self.tx_json(role, &json)
    }

    pub fn send_mouse_button_mask(&self, mask: u8, role: Role) -> bool {
        let json = format!("{{\"cmd\":61,\"b\":{}}}", mask);
        self.tx_json(role, &json)
    }

    pub fn send_mouse_wheel(&self, wheel: i32, role: Role) -> bool {
        // 固件 cmd 42：一次性滚轮，int8 范围，0 不发。
        let w = wheel.clamp(-127, 127);
        if w == 0 {
            // C++ 先 Resolve 再返回 true；这里保持"未连接返回 false"。
            return self.resolve_serial_real(role).is_some();
        }
        let json = format!("{{\"cmd\":42,\"w\":{}}}", w);
        self.tx_json(role, &json)
    }

    pub fn send_button_release(&self, mask: u8, release: bool, role: Role) -> bool {
        let mask = mask & 0x1F;
        let json = format!(
            "{{\"cmd\":58,\"b\":{},\"s\":{}}}",
            mask,
            if release { 1 } else { 0 }
        );
        self.tx_json(role, &json)
    }

    pub fn send_force_release(&self, mask: u8, duration_ms: u32, role: Role) -> bool {
        let mask = mask & 0x1F;
        if mask == 0 {
            return self.send_force_restore(role);
        }
        let json = if duration_ms > 0 {
            format!("{{\"cmd\":67,\"b\":{},\"t\":{}}}", mask, duration_ms)
        } else {
            format!("{{\"cmd\":67,\"b\":{}}}", mask)
        };
        self.tx_json(role, &json)
    }

    pub fn send_force_release_query(&self, role: Role) -> bool {
        self.tx_json(role, "{\"cmd\":67}")
    }

    pub fn send_force_restore(&self, role: Role) -> bool {
        self.tx_json(role, "{\"cmd\":67,\"b\":0}")
    }

    pub fn send_mouse_mask(&self, mask: u8, role: Role) -> bool {
        let json = format!("{{\"cmd\":60,\"m\":{}}}", mask);
        self.tx_json(role, &json)
    }

    pub fn send_reboot(&self, role: Role) -> bool {
        self.tx_json_impl(role, "{\"cmd\":57}", false)
    }

    /// kmboxnet 的整份键盘状态 → 固件 cmd 45 的单键按下/释放（状态差分）。
    pub fn send_keyboard(&self, ctrl: u8, keys: &[u8; 10]) -> bool {
        let mut cur: Vec<u8> = Vec::with_capacity(18);
        for bit in 0..8 {
            if ctrl & (1 << bit) != 0 {
                cur.push(0xE0 + bit as u8);
            }
        }
        for &k in keys {
            if k != 0 {
                cur.push(k);
            }
        }
        let prev = KB_PREV.lock().unwrap().clone();
        let mut any_ok = true;
        for &p in &prev {
            if !cur.contains(&p) && !self.send_keyboard_key(p, false, Role::Kbd) {
                any_ok = false;
            }
        }
        for &c in &cur {
            if !prev.contains(&c) && !self.send_keyboard_key(c, true, Role::Kbd) {
                any_ok = false;
            }
        }
        *KB_PREV.lock().unwrap() = cur;
        any_ok
    }

    pub fn send_keyboard_key(&self, kc: u8, down: bool, role: Role) -> bool {
        let json = format!(
            "{{\"cmd\":45,\"kc\":{},\"down\":{}}}",
            kc,
            if down { 1 } else { 0 }
        );
        self.tx_json(role, &json)
    }

    pub fn send_keyboard_release_all(&self, role: Role) -> bool {
        self.tx_json_impl(role, "{\"cmd\":46}", false)
    }

    pub fn send_keyboard_type_string(&self, s: &str, role: Role) -> bool {
        let n = s.len();
        if n == 0 || n > 128 {
            return false;
        }
        let json = format!("{{\"cmd\":47,\"s\":\"{}\"}}", json_escape(s));
        self.tx_json(role, &json)
    }

    pub fn send_upload_status(&self, enable: bool, role: Role) -> bool {
        let json = format!("{{\"cmd\":34,\"status\":{}}}", if enable { 1 } else { 0 });
        self.tx_json(role, &json)
    }

    pub fn send_jump_iap(&self, role: Role) -> bool {
        self.tx_json_impl(role, "{\"cmd\":50}", false)
    }

    pub fn send_raw_ysc(&self, json: &str, role: Role) -> bool {
        if let Some(sp) = self.resolve_serial_real(role) {
            self.debug_window_log(&format!("TX YSC: {}", json));
            self.log(&format!("TX YSC: {}", json));
            sp.send_json_command(json)
        } else {
            false
        }
    }

    pub fn send_raw_makcu(&self, text: &str, role: Role) -> bool {
        if let Some(sp) = self.resolve_serial_real(role) {
            self.debug_window_log(&format!("TX MAKCU: {}", text));
            self.log(&format!("TX MAKCU: {}", text));
            sp.send_raw(text.as_bytes())
        } else {
            false
        }
    }

    pub fn send_gamepad_config(&self, json: &str, role: Role) -> bool {
        let buf = format!("{{\"cmd\":100,\"data\":{}}}", json);
        if buf.len() + 14 > 4096 {
            return false;
        }
        if let Some(sp) = self.resolve_serial_real(role) {
            self.debug_window_log(&format!("TX GMAP set: {}", buf));
            self.log(&format!("TX GMAP set: {}", buf));
            sp.send_json_command(&buf)
        } else {
            false
        }
    }

    pub fn send_get_gamepad_config(&self, role: Role) -> bool {
        self.tx_json_impl(role, "{\"cmd\":101}", false)
    }

    pub fn send_gamepad_enable(&self, on: bool, role: Role) -> bool {
        let json = format!("{{\"cmd\":102,\"on\":{}}}", if on { 1 } else { 0 });
        if let Some(sp) = self.resolve_serial_real(role) {
            self.debug_window_log(&format!("TX GMAP enable: {}", json));
            self.log(&format!("TX GMAP enable: {}", json));
            sp.send_json_command(&json)
        } else {
            false
        }
    }

    pub fn send_gamepad_reset(&self, role: Role) -> bool {
        self.tx_json_impl(role, "{\"cmd\":104}", false)
    }

    pub fn send_dev_mode_get(&self, role: Role) -> bool {
        self.tx_json_impl(role, "{\"cmd\":62}", false)
    }

    pub fn send_dev_mode_set(&self, mode: i32, role: Role) -> bool {
        let json = format!("{{\"cmd\":63,\"m\":{}}}", if mode != 0 { 1 } else { 0 });
        self.tx_json(role, &json)
    }

    pub fn send_interp_set(
        &self,
        enable: i32,
        profile: i32,
        window: i32,
        maxw: i32,
        role: Role,
    ) -> bool {
        let json = format!(
            "{{\"cmd\":51,\"e\":{},\"p\":{},\"w\":{},\"mx\":{}}}",
            if enable != 0 { 1 } else { 0 },
            profile,
            window,
            maxw
        );
        self.tx_json(role, &json)
    }

    pub fn send_interp_get(&self, role: Role) -> bool {
        self.tx_json_impl(role, "{\"cmd\":52}", false)
    }

    pub fn send_interp_reset(&self, role: Role) -> bool {
        self.tx_json_impl(role, "{\"cmd\":53}", false)
    }

    pub fn send_guard_set(&self, enable: i32, wx: i32, wy: i32, role: Role) -> bool {
        let json = format!(
            "{{\"cmd\":68,\"e\":{},\"wx\":{},\"wy\":{}}}",
            if enable != 0 { 1 } else { 0 },
            wx,
            wy
        );
        self.tx_json(role, &json)
    }

    pub fn send_guard_get(&self, role: Role) -> bool {
        self.tx_json_impl(role, "{\"cmd\":69}", false)
    }

    pub fn send_guard_reset(&self, role: Role) -> bool {
        self.tx_json_impl(role, "{\"cmd\":70}", false)
    }

    pub fn debug_enter(&self, role: Role) -> bool {
        self.tx_json_impl(role, "{\"cmd\":700}", false)
    }
    pub fn debug_exit(&self, role: Role) -> bool {
        self.tx_json_impl(role, "{\"cmd\":701}", false)
    }
    pub fn debug_status(&self, role: Role) -> bool {
        self.tx_json_impl(role, "{\"cmd\":702}", false)
    }
    pub fn debug_get_dev_descr(&self, role: Role) -> bool {
        self.tx_json_impl(role, "{\"cmd\":710}", false)
    }
    pub fn debug_get_cfg_descr(&self, role: Role) -> bool {
        self.tx_json_impl(role, "{\"cmd\":711}", false)
    }
    pub fn debug_get_report_descr(&self, offset: u16, max: u16, role: Role) -> bool {
        let json = format!("{{\"cmd\":712,\"offset\":{},\"max\":{}}}", offset, max);
        self.tx_json_impl(role, &json, false)
    }
    pub fn debug_get_report(&self, idx: u8, role: Role) -> bool {
        let json = format!("{{\"cmd\":713,\"idx\":{}}}", idx);
        self.tx_json_impl(role, &json, false)
    }
    pub fn debug_clear_reports(&self, role: Role) -> bool {
        self.tx_json_impl(role, "{\"cmd\":714}", false)
    }
    pub fn debug_get_device_info(&self, role: Role) -> bool {
        self.tx_json_impl(role, "{\"cmd\":715}", false)
    }
    pub fn debug_set_mode(&self, mode: u8, role: Role) -> bool {
        let json = format!("{{\"cmd\":716,\"mode\":{}}}", mode);
        self.tx_json_impl(role, &json, false)
    }
    pub fn debug_get_diag(&self, role: Role) -> bool {
        self.tx_json_impl(role, "{\"cmd\":717}", false)
    }
    pub fn debug_get_events(&self, from: u16, n: u16, role: Role) -> bool {
        let json = format!("{{\"cmd\":718,\"from\":{},\"n\":{}}}", from, n);
        self.tx_json_impl(role, &json, false)
    }
}
