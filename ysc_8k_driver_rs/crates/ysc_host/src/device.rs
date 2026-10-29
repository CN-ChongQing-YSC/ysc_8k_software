//! 多设备层（对应 `device_context.cpp`）：
//! 两个槽位、按角色路由、上游 HID 角色自动识别（cmd 715/711 + 数据流兜底）。

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::Value;

use ysc_core::consts::Role;
use ysc_core::enumerate::is_ysc_v3_port;
use ysc_core::family::{family_of_version, CdcFamily};
use ysc_core::serial::SerialPort;
use ysc_core::util::{json_escape, now_ms};

use crate::app::{App, Slot};

pub const DEV_MAX_SLOTS: usize = 2;
pub const PROBE_DEADLINE_MS: u64 = 5000;
/// YSC V3 CDC 没有真实波特率；主机句柄固定用这个速率打开，避免 Windows
/// 拒绝用户输入的任意 baud（实际 USB 吞吐不受影响）。
const CDC_SAFE_BAUD: u32 = 115_200;

/// 发 cmd132 确认固件族。必须在启动读线程之前调用。
fn version_is_v3(serial: &SerialPort) -> bool {
    serial
        .query_version(400)
        .map(|v| family_of_version(&v) == Some(CdcFamily::YscV3))
        .unwrap_or(false)
}

impl App {
    #[allow(dead_code)]
    pub fn slot_ref(&self, slot: usize) -> Option<&std::sync::Mutex<Slot>> {
        self.slots.get(slot)
    }

    /// 挂载一个槽位。返回实际波特率；失败返回 Err。
    ///
    /// 与 C++ `DeviceContext::Attach` 一致：显式波特率先握手，失败退回自动探测。
    /// YSC V3 USB CDC 例外：波特率只是逻辑标签，主机句柄固定用安全速率打开，
    /// 任意用户输入都只记录、不影响 USB 通讯。
    pub fn attach_device(self: &Arc<Self>, slot: usize, port: &str, baud: u32) -> Result<u32, ()> {
        if slot >= DEV_MAX_SLOTS || port.is_empty() {
            return Err(());
        }
        // 复用时先断开旧的（会触发 lost 回调，与 C++ 一致）。
        {
            let in_use = {
                let s = self.slots[slot].lock().unwrap();
                s.in_use()
            };
            if in_use {
                self.detach_device(slot);
            }
        }

        let v3_hint = is_ysc_v3_port(port);
        let mut actual_baud = baud;
        if actual_baud == 0 {
            actual_baud = SerialPort::detect_baudrate(port);
            if actual_baud == 0 {
                self.log(&format!(
                    "DEVICE attach slot={} {} 自动探测失败",
                    slot, port
                ));
                return Err(());
            }
        }
        let open_baud = if v3_hint {
            CDC_SAFE_BAUD
        } else {
            actual_baud
        };

        let serial = Arc::new(SerialPort::new());
        if serial.connect(port, open_baud).is_err() {
            return Err(());
        }

        if !serial.ping_once(400) {
            self.log(&format!(
                "DEVICE attach slot={} {} baud={} 无应答，改自动探测",
                slot, port, actual_baud
            ));
            serial.disconnect();
            drop(serial);

            let detected = SerialPort::detect_baudrate(port);
            if detected == 0 {
                self.log(&format!(
                    "DEVICE attach slot={} {} 自动探测也失败",
                    slot, port
                ));
                return Err(());
            }
            actual_baud = detected;
            let serial = Arc::new(SerialPort::new());
            let detected_open = if v3_hint { CDC_SAFE_BAUD } else { detected };
            if serial.connect(port, detected_open).is_err() {
                return Err(());
            }
            self.log(&format!(
                "DEVICE attach slot={} {} 实际速率={}（请求 {}）",
                slot, port, detected, baud
            ));
            let v3 = v3_hint || version_is_v3(&serial);
            self.finish_attach(slot, port, actual_baud, serial, v3);
            return Ok(actual_baud);
        }

        let v3 = v3_hint || version_is_v3(&serial);
        self.finish_attach(slot, port, actual_baud, serial, v3);
        Ok(actual_baud)
    }

    fn finish_attach(
        self: &Arc<Self>,
        slot: usize,
        port: &str,
        baud: u32,
        serial: Arc<SerialPort>,
        v3: bool,
    ) {
        let weak = Arc::downgrade(self);
        serial.set_callback(Arc::new(move |data: &[u8]| {
            if let Some(app) = weak.upgrade() {
                app.on_slot_data(slot, data);
            }
        }));
        let weak = Arc::downgrade(self);
        serial.set_lost_callback(Arc::new(move |gen: u32| {
            if let Some(app) = weak.upgrade() {
                app.notifier.slot_lost(slot, gen);
            }
        }));
        let _ = serial.start_read_thread();

        let mut s = self.slots[slot].lock().unwrap();
        s.serial = Some(serial);
        s.port = port.to_string();
        s.baud = baud;
        s.connected = true;
        s.v3_cdc = v3;
        s.role = Role::None;
        s.auto_detected = false;
        s.role_manual = false;
        s.probe_ambiguous = false;
        s.probe_state = 0;
        s.probe_done = false;
        s.probe_got_info = false;
        s.probe_got_cfg = false;
        s.probe_deadline = now_ms() + PROBE_DEADLINE_MS;
        s.iface_mask = 0;
        s.vid = -1;
        s.pid = -1;
        s.product.clear();
        s.probe_stage = 0;
        s.saw_mouse_data = false;
        s.saw_kbd_data = false;
    }

    pub fn detach_device(&self, slot: usize) {
        if slot >= DEV_MAX_SLOTS {
            return;
        }
        let (serial, was_in_use) = {
            let mut s = self.slots[slot].lock().unwrap();
            let was = s.in_use();
            (s.serial.take(), was)
        };
        if let Some(serial) = serial {
            serial.stop_read_thread();
            serial.disconnect();
        }
        {
            let mut s = self.slots[slot].lock().unwrap();
            s.connected = false;
            s.port.clear();
            s.baud = 0;
            s.v3_cdc = false;
            s.probe_state = 0;
            s.iface_mask = 0;
            s.probe_stage = 0;
        }
        if was_in_use {
            self.on_device_lost(slot);
        }
    }

    #[allow(dead_code)]
    pub fn detach_all(&self) {
        for i in 0..DEV_MAX_SLOTS {
            self.detach_device(i);
        }
    }

    /// 按角色取槽位；单设备兼容/未指定角色的规则与 C++ 完全一致。
    pub fn slot_by_role(&self, role: Role) -> Option<usize> {
        if role != Role::None {
            for i in 0..DEV_MAX_SLOTS {
                let s = self.slots[i].lock().unwrap();
                if s.connected && s.role == role {
                    return Some(i);
                }
            }
        }
        let mut only = None;
        let mut n = 0;
        for i in 0..DEV_MAX_SLOTS {
            let s = self.slots[i].lock().unwrap();
            if s.connected {
                n += 1;
                only = Some(i);
            }
        }
        if role == Role::None {
            return only;
        }
        if n == 1 {
            only
        } else {
            None
        }
    }

    pub fn serial_for_role(&self, role: Role) -> Option<Arc<SerialPort>> {
        let slot = self.slot_by_role(role)?;
        let s = self.slots[slot].lock().unwrap();
        s.serial.clone()
    }

    #[allow(dead_code)]
    pub fn serial_for_slot(&self, slot: usize) -> Option<Arc<SerialPort>> {
        self.slots.get(slot)?.lock().unwrap().serial.clone()
    }

    pub fn set_role(&self, slot: usize, role: Role) -> bool {
        let Some(m) = self.slots.get(slot) else {
            return false;
        };
        let mut s = m.lock().unwrap();
        if role == Role::None {
            s.role = Role::None;
            s.auto_detected = false;
            s.role_manual = false;
            s.probe_ambiguous = false;
            s.probe_done = false;
            s.probe_state = 0;
            s.probe_got_info = false;
            s.probe_got_cfg = false;
            s.probe_deadline = now_ms() + PROBE_DEADLINE_MS;
            s.probe_stage = 0;
            return true;
        }
        s.role = role;
        s.auto_detected = false;
        s.role_manual = true;
        s.probe_ambiguous = false;
        s.probe_state = 2;
        s.probe_done = true;
        true
    }

    /// 已连接设备切换波特率（两端都改；失败回退）。
    pub fn switch_device_baud(&self, slot: usize, baud: u32) -> bool {
        if slot >= DEV_MAX_SLOTS {
            return false;
        }
        let (serial, old_baud, connected, v3) = {
            let s = self.slots[slot].lock().unwrap();
            (s.serial.clone(), s.baud, s.connected, s.v3_cdc)
        };
        let Some(serial) = serial else { return false };
        if !connected || baud == 0 {
            return false;
        }
        if old_baud == baud {
            return true;
        }

        // YSC V3 USB CDC：没有真实波特率，切换只是改驱动侧显示值。
        // 仍发一条 cmd133 让固件保持 ACK 语义，但绝不关端口/重开（重开会让
        // Windows 拒绝任意 baud，且对 USB 吞吐毫无影响）。
        if v3 {
            let _ = serial.send_json_command(&format!("{{\"cmd\":133,\"baud\":{}}}", baud));
            let mut s = self.slots[slot].lock().unwrap();
            s.baud = baud;
            s.connected = true;
            s.v3_cdc = true;
            return true;
        }

        if !serial.switch_baudrate(baud) {
            // 固件可能没切成：把两端都拉回原速率，否则只能断电恢复。
            if serial.switch_baudrate(old_baud) {
                let _ = serial.start_read_thread();
                let mut s = self.slots[slot].lock().unwrap();
                s.baud = old_baud;
                s.connected = true;
                s.probe_state = 0;
                s.probe_done = false;
                // C++ 回退路径刻意保留 probeGotInfo/probeGotCfg：旧探测结果仍有效。
                s.probe_deadline = now_ms() + PROBE_DEADLINE_MS;
                s.probe_stage = 0;
                drop(s);
                self.log(&format!(
                    "DEVICE slot={} baud {} unsupported, reverted to {}",
                    slot, baud, old_baud
                ));
            } else {
                let mut s = self.slots[slot].lock().unwrap();
                s.connected = false;
            }
            return false;
        }

        let _ = serial.start_read_thread();
        let mut s = self.slots[slot].lock().unwrap();
        s.baud = baud;
        s.connected = true;
        s.probe_state = 0;
        s.probe_done = false;
        s.probe_got_info = false;
        s.probe_got_cfg = false;
        s.probe_deadline = now_ms() + PROBE_DEADLINE_MS;
        s.probe_stage = 0;
        true
    }

    /// 数据流兜底判角色（不依赖固件支持 715/711）。
    pub fn note_data_kind(&self, slot: usize, kind: i32) {
        if slot >= DEV_MAX_SLOTS {
            return;
        }
        let mut s = self.slots[slot].lock().unwrap();
        if !s.connected {
            return;
        }
        match kind {
            0 => s.saw_mouse_data = true,
            1 => s.saw_kbd_data = true,
            _ => return,
        }
        if s.role_manual {
            return;
        }
        s.role = if kind == 0 { Role::Mouse } else { Role::Kbd };
        s.probe_ambiguous = false;
        s.auto_detected = true;
        s.probe_done = true;
        s.probe_state = 2;
        drop(s);
        self.log(&format!(
            "PROBE role by data slot={} kind={} role={}",
            slot,
            kind,
            if kind == 0 { 1 } else { 2 }
        ));
    }

    /// 串口槽位数据入口：先给 715/711 探测消费，再转设备数据通路。
    pub fn on_slot_data(&self, slot: usize, data: &[u8]) {
        if data.len() < 1024 {
            if let Ok(text) = std::str::from_utf8(data) {
                if self.probe_consume(slot, text) {
                    return;
                }
            }
        }
        self.on_device_data(slot, data);
    }

    /// 设备数据通路（对照 C++ `OnDeviceData`）：不经过探测消费，
    /// debug_inject_slot 也直接走这里。
    pub fn on_device_data(&self, slot: usize, data: &[u8]) {
        if data.len() > 0 && data.len() < 512 {
            if let Ok(text) = std::str::from_utf8(data) {
                if text.contains("\"message\":\"mouse\"") {
                    self.note_data_kind(slot, 0);
                } else if text.contains("\"message\":\"kbd\"") {
                    self.note_data_kind(slot, 1);
                }
            }
        }
        self.on_serial_response_combo(data);
    }

    /// 角色探测消费；返回 true 表示该帧被探测流程吃掉。
    fn probe_consume(&self, slot: usize, json: &str) -> bool {
        let mut s = self.slots[slot].lock().unwrap();
        if s.probe_done {
            return false;
        }
        let is715 = json.contains("\"code\":715");
        let is711 = json.contains("\"code\":711");
        if is715 {
            self.debug_window_log(&format!("PROBE got715 raw={}", trunc(json, 180)));
        }
        if is711 {
            self.debug_window_log(&format!("PROBE got711 raw={}", trunc(json, 120)));
        }
        if !is715 && !is711 {
            return false;
        }

        if is715 {
            if let Some((vid, pid, product)) = parse_vid_pid_product(json) {
                s.vid = vid;
                s.pid = pid;
                s.product = product;
                s.probe_got_info = true;
            }
        } else if let Some(mask) = parse_cfg_iface_mask(json) {
            s.iface_mask = mask;
            s.probe_got_cfg = true;
        }

        if s.probe_got_info && s.probe_got_cfg {
            let has_kbd = s.iface_mask & 0x1 != 0;
            let has_mouse = s.iface_mask & 0x2 != 0;
            if has_kbd && has_mouse {
                s.role = Role::None;
                s.probe_ambiguous = true;
            } else if has_kbd {
                s.role = Role::Kbd;
            } else if has_mouse {
                s.role = Role::Mouse;
            } else {
                s.role = Role::None;
            }
            s.auto_detected = true;
            s.probe_state = 2;
            s.probe_done = true;
            s.probe_stage = 2;
            let (mask, vid, pid, role, ambig) = (
                s.iface_mask,
                s.vid,
                s.pid,
                s.role.as_i32(),
                s.probe_ambiguous,
            );
            drop(s);
            self.log(&format!(
                "PROBE done slot={} mask={} vid={:04X} pid={:04X} role={} ambig={}",
                slot, mask, vid, pid, role, ambig as i32
            ));
        }
        true
    }

    /// 主循环 100ms 心跳：推进探测状态机。
    pub fn probe_tick(&self) {
        let now = now_ms();
        let last = self.last_probe_tick.load(Ordering::Relaxed);
        if now.saturating_sub(last) < 100 {
            return;
        }
        self.last_probe_tick.store(now, Ordering::Relaxed);

        for i in 0..DEV_MAX_SLOTS {
            let (serial, stage, need_timeout, deadline, got_cfg, mask) = {
                let s = self.slots[i].lock().unwrap();
                if s.serial.is_none() || !s.connected || s.probe_done || s.auto_detected {
                    continue;
                }
                (
                    s.serial.clone(),
                    s.probe_stage,
                    now > s.probe_deadline,
                    s.probe_deadline,
                    s.probe_got_cfg,
                    s.iface_mask,
                )
            };
            if need_timeout {
                let mut s = self.slots[i].lock().unwrap();
                if s.probe_got_cfg {
                    if s.iface_mask & 0x1 != 0 {
                        s.role = Role::Kbd;
                    } else if s.iface_mask & 0x2 != 0 {
                        s.role = Role::Mouse;
                    }
                    s.auto_detected = true;
                }
                s.probe_done = true;
                s.probe_state = 2;
                let (gi, gc, m, role) = (
                    s.probe_got_info,
                    s.probe_got_cfg,
                    s.iface_mask,
                    s.role.as_i32(),
                );
                drop(s);
                self.log(&format!(
                    "PROBE timeout slot={} gotInfo={} gotCfg={} mask={} role={}",
                    i, gi as i32, gc as i32, m, role
                ));
                continue;
            }
            let _ = (deadline, got_cfg, mask);
            let Some(serial) = serial else { continue };
            if stage == 0 {
                {
                    let mut s = self.slots[i].lock().unwrap();
                    s.probe_stage = 1;
                }
                let _ = serial.send_json_command("{\"cmd\":715}");
            } else if stage == 1 {
                {
                    let mut s = self.slots[i].lock().unwrap();
                    s.probe_stage = 2;
                }
                let _ = serial.send_json_command("{\"cmd\":711}");
            }
        }
    }

    /// 槽位读线程异常退出：清理并通知前端。
    pub fn handle_slot_lost(&self, slot: usize, gen: u32) {
        if slot >= DEV_MAX_SLOTS {
            return;
        }
        let still_current = {
            let s = self.slots[slot].lock().unwrap();
            s.serial
                .as_ref()
                .map(|ser| ser.reader_gen() == gen && s.connected)
                .unwrap_or(false)
        };
        if !still_current {
            return;
        }
        self.log("SERIAL unexpected disconnect (physical/IO), cleaning up");
        self.detach_device(slot);
        self.send_device_list();
    }

    /// 扇出 `device_list` 事件。
    pub fn send_device_list(&self) {
        let mut body = String::from("\"devices\":[");
        for i in 0..DEV_MAX_SLOTS {
            if i > 0 {
                body.push(',');
            }
            let s = self.slots[i].lock().unwrap();
            body.push_str(&format!(
                "{{\"dev\":{},\"role\":\"{}\",\"auto\":{},\"connected\":{},\"port\":\"{}\",\"baud\":{},\"vid\":{},\"pid\":{},\"product\":\"{}\",\"ifaceMask\":{},\"probeDone\":{},\"v3\":{},\"baudFree\":{}}}",
                i,
                s.role.as_str(),
                s.auto_detected,
                s.connected,
                json_escape(&s.port),
                s.baud,
                s.vid,
                s.pid,
                json_escape(&s.product),
                s.iface_mask,
                if s.probe_done { 1 } else { 0 },
                s.v3_cdc,
                s.v3_cdc
            ));
        }
        body.push(']');
        self.send_event("device_list", Some(&body));
    }

    /// 设备断开副作用（对应 C++ `OnDeviceLost`）。
    pub fn on_device_lost(&self, slot: usize) {
        let (role, role_str) = {
            let s = self.slots[slot].lock().unwrap();
            (s.role, s.role.as_str())
        };
        self.reset_monitor_side(if role == Role::Kbd { 1 } else { 0 });
        if role != Role::Kbd {
            self.guard_abort("device_lost");
        }
        let body = format!(
            "\"dev\":{},\"role\":\"{}\",\"message\":\"设备已断开\"",
            slot, role_str
        );
        self.send_event("device_lost", Some(&body));
        self.log(&format!("DEVICE slot={} role={} lost", slot, role_str));
    }
}

fn trunc(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// 固件 715/711 的 data 可能是嵌套 JSON 字符串（内层引号在线上是 `\"`）。
fn nested_value(json: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(json).ok()?;
    if let Some(data) = v.get("data").and_then(|d| d.as_str()) {
        if let Ok(inner) = serde_json::from_str::<Value>(data) {
            return Some(inner);
        }
    }
    Some(v)
}

fn json_int_after_key(json: &str, key: &str) -> Option<i32> {
    for pat in [format!("{}\\\"", key), format!("\"{}\":", key)] {
        if let Some(mut p) = json.find(&pat) {
            p += pat.len();
            let rest = &json[p..];
            let mut chars = rest.char_indices().peekable();
            // 跳到冒号之后（兼容 `\"vid\":` / `"vid":`）。
            let mut idx = 0usize;
            for (i, c) in chars.by_ref() {
                if c == ':' {
                    idx = i + 1;
                    break;
                }
            }
            let tail = &rest[idx..];
            let tail = tail.trim_start_matches([' ', '\\', '"']);
            let mut num = String::new();
            for c in tail.chars() {
                if c == '-' || c.is_ascii_digit() {
                    num.push(c);
                } else {
                    break;
                }
            }
            if let Ok(v) = num.parse() {
                return Some(v);
            }
        }
    }
    None
}

fn parse_vid_pid_product(json: &str) -> Option<(i32, i32, String)> {
    if let Some(v) = nested_value(json) {
        let vid = v.get("vid").and_then(|x| x.as_i64());
        let pid = v.get("pid").and_then(|x| x.as_i64());
        if let (Some(vid), Some(pid)) = (vid, pid) {
            let product = v
                .get("product")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            return Some((vid as i32, pid as i32, product));
        }
    }
    let vid = json_int_after_key(json, "vid")?;
    let pid = json_int_after_key(json, "pid")?;
    let product = extract_product(json).unwrap_or_default();
    Some((vid, pid, product))
}

fn extract_product(json: &str) -> Option<String> {
    if let Some(v) = nested_value(json) {
        if let Some(s) = v.get("product").and_then(|x| x.as_str()) {
            return Some(s.to_string());
        }
    }
    let p = json.find("product\\")?;
    let rest = &json[p..];
    let colon = rest.find(':')?;
    let mut tail = &rest[colon + 1..];
    tail = tail.trim_start_matches([' ', '\\', '"']);
    let mut out = String::new();
    for c in tail.chars() {
        if c == '"' || c == '\\' {
            break;
        }
        out.push(c);
    }
    Some(out)
}

fn parse_cfg_iface_mask(json: &str) -> Option<i32> {
    let hex = extract_hex(json)?;
    let buf = decode_hex(&hex)?;
    if buf.len() < 9 || buf[1] != 0x02 {
        return None;
    }
    let mut total = buf[2] as usize | ((buf[3] as usize) << 8);
    if total > buf.len() {
        total = buf.len();
    }
    let mut mask = 0i32;
    let mut i = 0usize;
    while i + 8 < total {
        let blen = buf[i] as usize;
        let btype = buf[i + 1];
        if blen < 2 {
            break;
        }
        if btype == 0x04 && blen >= 9 {
            let cls = buf[i + 5];
            let proto = buf[i + 7];
            if cls == 0x03 {
                if proto == 0x01 {
                    mask |= 0x1;
                } else if proto == 0x02 {
                    mask |= 0x2;
                }
            }
        }
        i += blen;
    }
    Some(mask)
}

fn extract_hex(json: &str) -> Option<String> {
    if let Some(v) = nested_value(json) {
        if let Some(h) = v.get("hex").and_then(|x| x.as_str()) {
            return Some(h.to_string());
        }
    }
    let p = json.find("hex")?;
    let rest = &json[p + 3..];
    let colon = rest.find(':')?;
    let mut tail = rest[colon + 1..].trim_start_matches([' ', '\\', '"']);
    let mut out = String::new();
    while let Some(c) = tail.chars().next() {
        if !c.is_ascii_hexdigit() {
            break;
        }
        out.push(c);
        tail = &tail[c.len_utf8()..];
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    let b = s.as_bytes();
    if b.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(b.len() / 2);
    let mut i = 0;
    while i + 1 < b.len() {
        let hi = (b[i] as char).to_digit(16)?;
        let lo = (b[i + 1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
        i += 2;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_json(hex: &str) -> String {
        let inner = format!(
            "{{\"total\":34,\"offset\":0,\"len\":34,\"hex\":\"{}\"}}",
            hex
        );
        format!(
            "{{\"code\":711,\"message\":\"ok\",\"data\":{}}}",
            serde_json::to_string(&inner).unwrap()
        )
    }

    fn info_json() -> String {
        let inner = "{\"vid\":13652,\"pid\":65036,\"product\":\"YSC Box\"}";
        format!(
            "{{\"code\":715,\"message\":\"ok\",\"data\":{}}}",
            serde_json::to_string(inner).unwrap()
        )
    }

    #[test]
    fn parse_identity_and_descriptor() {
        let (vid, pid, product) = parse_vid_pid_product(&info_json()).unwrap();
        assert_eq!((vid, pid), (0x3554, 0xFE0C));
        assert_eq!(product, "YSC Box");

        // CONFIG(9) + KBD INTERFACE(9) + EP(7) + MOUSE INTERFACE(9)
        let mut cfg = Vec::new();
        cfg.extend_from_slice(&[9, 0x02, 34, 0, 1, 0, 0, 0x80, 50]);
        cfg.extend_from_slice(&[9, 0x04, 0, 0, 1, 0x03, 1, 0x01, 0]);
        cfg.extend_from_slice(&[7, 0x05, 0x81, 0x03, 8, 0, 10]);
        cfg.extend_from_slice(&[9, 0x04, 1, 0, 1, 0x03, 1, 0x02, 0]);
        let hex: String = cfg.iter().map(|b| format!("{:02X}", b)).collect();
        assert_eq!(parse_cfg_iface_mask(&cfg_json(&hex)), Some(3));

        // 只保留键盘接口
        let mut kbd_only = cfg[..25].to_vec();
        kbd_only[4] = 0; // numInterfaces
        kbd_only[2] = 25;
        kbd_only[3] = 0;
        let hex: String = kbd_only.iter().map(|b| format!("{:02X}", b)).collect();
        assert_eq!(parse_cfg_iface_mask(&cfg_json(&hex)), Some(1));
    }

    #[test]
    fn probe_consumes_and_decides_role() {
        let app = App::test_app();
        {
            let mut s = app.slots[0].lock().unwrap();
            s.connected = true;
            s.probe_deadline = now_ms() + 5000;
        }
        let info = info_json();
        assert!(app.probe_consume(0, &info));
        {
            let s = app.slots[0].lock().unwrap();
            assert!(s.probe_got_info);
            assert_eq!(s.vid, 0x3554);
        }
        let mut cfg = Vec::new();
        cfg.extend_from_slice(&[9, 0x02, 34, 0, 1, 0, 0, 0x80, 50]);
        cfg.extend_from_slice(&[9, 0x04, 0, 0, 1, 0x03, 1, 0x01, 0]);
        cfg.extend_from_slice(&[7, 0x05, 0x81, 0x03, 8, 0, 10]);
        let hex: String = cfg.iter().map(|b| format!("{:02X}", b)).collect();
        assert!(app.probe_consume(0, &cfg_json(&hex)));
        let s = app.slots[0].lock().unwrap();
        assert!(s.probe_done);
        assert_eq!(s.role, Role::Kbd);
        assert!(s.auto_detected);
    }

    #[test]
    fn data_kind_fallback_sets_role() {
        let app = App::test_app();
        {
            let mut s = app.slots[0].lock().unwrap();
            s.connected = true;
        }
        app.note_data_kind(0, 0);
        assert_eq!(app.slots[0].lock().unwrap().role, Role::Mouse);
        app.note_data_kind(0, 1);
        assert_eq!(app.slots[0].lock().unwrap().role, Role::Kbd);
    }
}
