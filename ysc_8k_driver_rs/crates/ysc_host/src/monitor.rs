//! 鼠标 + 键盘物理状态汇聚（对应 `monitor_push.cpp`）。
//!
//! 固件上报：
//!   `{"code":200,"message":"mouse","data":"{\"b\":..,\"x\":..,\"y\":..,\"wh\":..}"}`
//!   `{"code":200,"message":"kbd","data":"{\"mod\":N,\"n\":M,\"ov\":K,\"k\":[..]}"}`
//!
//! 驱动把两路合成一个 kmboxnet monitor 包，并在状态变化时唤醒 UI 推送线程
//! （8ms 合并限速），避免 8K 帧率直接把 Electron 打满。

use serde_json::Value;

use crate::app::{App, KbdMon, MouseMon, KBD_MON_KEYS_MAX};

impl App {
    pub fn monitor_on_serial(&self, data: &[u8]) {
        let Ok(text) = std::str::from_utf8(data) else {
            return;
        };
        let Ok(v) = serde_json::from_str::<Value>(text) else {
            return;
        };
        let code = v.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
        if code != 200 {
            return;
        }
        let msg = v.get("message").and_then(|m| m.as_str()).unwrap_or("");
        let Some(inner) = v.get("data").and_then(|d| d.as_str()) else {
            return;
        };
        let Ok(inner_v) = serde_json::from_str::<Value>(inner) else {
            return;
        };

        if msg == "kbd" {
            let Some(mod_) = inner_v.get("mod").and_then(|x| x.as_i64()) else {
                return;
            };
            let n = inner_v.get("n").and_then(|x| x.as_i64()).unwrap_or(0);
            let ov = inner_v.get("ov").and_then(|x| x.as_i64()).unwrap_or(0);
            let mut kbd = KbdMon {
                mod_: (mod_ & 0xFF) as i32,
                overflow: ov as i32,
                ..Default::default()
            };
            let mut got = 0usize;
            if let Some(arr) = inner_v.get("k").and_then(|x| x.as_array()) {
                for item in arr {
                    if got >= KBD_MON_KEYS_MAX {
                        break;
                    }
                    if let Some(kc) = item.as_i64() {
                        kbd.key[got] = (kc & 0xFF) as u8;
                        got += 1;
                    }
                }
            }
            if n > 0 && (n as usize) < got {
                got = n as usize;
            }
            kbd.key_count = got as i32;
            kbd.valid = 1;
            self.monitor.lock().unwrap().kbd = kbd;
        } else {
            let Some(b) = inner_v.get("b").and_then(|x| x.as_i64()) else {
                return;
            };
            let Some(x) = inner_v.get("x").and_then(|x| x.as_i64()) else {
                return;
            };
            let Some(y) = inner_v.get("y").and_then(|x| x.as_i64()) else {
                return;
            };
            let wh = inner_v.get("wh").and_then(|x| x.as_i64()).unwrap_or(0);
            let mut m = self.monitor.lock().unwrap();
            m.mouse.buttons = b as i32;
            m.mouse.x = x as i32;
            m.mouse.y = y as i32;
            m.mouse.wheel = wh as i32;
            m.mouse.valid = 1;
            drop(m);
        }

        self.push_monitor_if_changed();
    }

    /// 变化检测 + 推 kmboxnet 客户端 + 唤醒 UI 推送线程。
    pub fn push_monitor_if_changed(&self) {
        let (changed, pkt) = {
            let mut m = self.monitor.lock().unwrap();
            // 与 C++ MouseEq/KbdEq 一致：鼠标变化只看 buttons/x/y/valid，不看 wheel。
            let mouse_changed = m.mouse.buttons != m.last_mouse.buttons
                || m.mouse.x != m.last_mouse.x
                || m.mouse.y != m.last_mouse.y
                || m.mouse.valid != m.last_mouse.valid;
            let kbd_changed = m.kbd.mod_ != m.last_kbd.mod_
                || m.kbd.key_count != m.last_kbd.key_count
                || m.kbd.overflow != m.last_kbd.overflow
                || m.kbd.valid != m.last_kbd.valid
                || m.kbd.key != m.last_kbd.key;
            let changed = !m.have_last || mouse_changed || kbd_changed;
            if changed {
                m.last_mouse = m.mouse;
                m.last_kbd = m.kbd;
                m.have_last = true;
            }
            (changed, build_monitor_packet(&m.mouse, &m.kbd))
        };
        if !changed {
            return;
        }
        if self.kmbox.has_monitor_client() {
            self.kmbox.send_monitor_data(&pkt);
        }
        self.wake_ui();
    }

    pub fn reset_monitor_side(&self, is_keyboard: i32) {
        let mut m = self.monitor.lock().unwrap();
        if is_keyboard != 0 {
            m.kbd = KbdMon::default();
        } else {
            m.mouse = MouseMon::default();
        }
        m.have_last = false;
    }

    pub fn serialize_mouse_json(&self) -> String {
        let m = self.monitor.lock().unwrap().mouse;
        format!(
            "\"mouse\":{{\"valid\":{},\"buttons\":{},\"x\":{},\"y\":{}}}",
            m.valid, m.buttons, m.x, m.y
        )
    }

    pub fn serialize_kbd_json(&self) -> String {
        let k = self.monitor.lock().unwrap().kbd;
        let mut keys = String::new();
        for i in 0..k.key_count.max(0) as usize {
            if i >= KBD_MON_KEYS_MAX {
                break;
            }
            if i > 0 {
                keys.push(',');
            }
            keys.push_str(&k.key[i].to_string());
        }
        format!(
            "\"kbd\":{{\"valid\":{},\"mod\":{},\"count\":{},\"overflow\":{},\"keys\":[{}]}}",
            k.valid, k.mod_, k.key_count, k.overflow, keys
        )
    }

    pub fn serialize_ui_json(&self) -> String {
        let m = self.monitor.lock().unwrap().mouse;
        let kbd = self.serialize_kbd_json();
        format!(
            "\"buttons\":{},\"x\":{},\"y\":{},\"wheel\":{},{},\"mouseValid\":{}",
            m.buttons, m.x, m.y, m.wheel, kbd, m.valid
        )
    }

    /// 固件响应转发（对应 `ForwardDebugResponse`）：调试抓包 700..718、
    /// 手柄映射 100..104、以及 code==200 且 message 非空的控制回包。
    pub fn forward_debug_response(&self, data: &[u8]) {
        if data.len() < 8 || data.len() > 8000 {
            return;
        }
        let Ok(text) = std::str::from_utf8(data) else {
            return;
        };
        if !text.starts_with('{') || !text.ends_with('}') {
            return;
        }
        let Ok(v) = serde_json::from_str::<Value>(text) else {
            return;
        };
        let code = v.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
        let msg = v.get("message").and_then(|m| m.as_str()).unwrap_or("");
        let is_debug = (700..=718).contains(&code);
        let is_gmap = (100..=104).contains(&code);
        let is_ctrl = code == 200 && !msg.is_empty();
        if !is_debug && !is_gmap && !is_ctrl {
            return;
        }
        let body = &text[1..text.len() - 1];
        self.send_event("debug_response", Some(body));
    }

    #[cfg(test)]
    pub fn monitor_snapshot(&self) -> (MouseMon, KbdMon) {
        let m = self.monitor.lock().unwrap();
        (m.mouse, m.kbd)
    }
}

/// kmboxnet 监控包：mouse_report(8B) + keyboard_report(12B) = 20B。
pub fn build_monitor_packet(m: &MouseMon, k: &KbdMon) -> [u8; 20] {
    let mut pkt = [0u8; 20];
    pkt[0] = 0x01;
    pkt[1] = m.buttons as u8;
    pkt[2..4].copy_from_slice(&(m.x as i16).to_le_bytes());
    pkt[4..6].copy_from_slice(&(m.y as i16).to_le_bytes());
    pkt[6..8].copy_from_slice(&(m.wheel as i16).to_le_bytes());

    pkt[8] = 0x02;
    pkt[9] = k.mod_ as u8;
    if k.valid != 0 {
        let n = (k.key_count.max(0) as usize).min(10);
        for i in 0..n {
            if i >= KBD_MON_KEYS_MAX {
                break;
            }
            pkt[10 + i] = k.key[i];
        }
    }
    pkt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;

    #[test]
    fn parse_mouse_and_keyboard_frames() {
        let app = App::test_app();
        app.monitor_on_serial(
            br#"{"code":200,"message":"mouse","data":"{\"b\":3,\"x\":7,\"y\":-2,\"wh\":1}"}"#,
        );
        let (m, _) = app.monitor_snapshot();
        assert_eq!((m.valid, m.buttons, m.x, m.y, m.wheel), (1, 3, 7, -2, 1));

        app.monitor_on_serial(
            br#"{"code":200,"message":"kbd","data":"{\"mod\":2,\"n\":2,\"ov\":1,\"k\":[4,5]}"}"#,
        );
        let (_, k) = app.monitor_snapshot();
        assert_eq!((k.valid, k.mod_, k.key_count, k.overflow), (1, 2, 2, 1));
        assert_eq!(&k.key[..2], &[4, 5]);

        let ui = app.serialize_ui_json();
        assert!(ui.contains("\"buttons\":3"));
        assert!(ui.contains("\"mouseValid\":1"));
        assert!(ui.contains("\"keys\":[4,5]"));
    }

    #[test]
    fn monitor_packet_is_20_bytes() {
        let m = MouseMon {
            buttons: 5,
            x: 300,
            y: -300,
            wheel: -1,
            valid: 1,
        };
        let mut k = KbdMon::default();
        k.valid = 1;
        k.mod_ = 2;
        k.key_count = 1;
        k.key[0] = 4;
        let p = build_monitor_packet(&m, &k);
        assert_eq!(p.len(), 20);
        assert_eq!(p[0], 0x01);
        assert_eq!(p[1], 5);
        assert_eq!(&p[2..4], &300i16.to_le_bytes());
        assert_eq!(&p[4..6], &(-300i16).to_le_bytes());
        assert_eq!(&p[6..8], &(-1i16).to_le_bytes());
        assert_eq!(p[8], 0x02);
        assert_eq!(p[9], 2);
        assert_eq!(p[10], 4);
    }

    #[test]
    fn reset_side_clears_one_side() {
        let app = App::test_app();
        app.monitor_on_serial(
            br#"{"code":200,"message":"mouse","data":"{\"b\":3,\"x\":7,\"y\":-2}"}"#,
        );
        app.reset_monitor_side(0);
        let (m, _) = app.monitor_snapshot();
        assert_eq!(m.valid, 0);
    }
}
