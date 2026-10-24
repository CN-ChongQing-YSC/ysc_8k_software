//! kmboxnet UDP 服务（对应 `kmboxnet_server.cpp`）。
//!
//! 线上协议：16 字节头（mac/rand/indexpts/cmd，全小端）+ 负载。
//! 每条命令都回 ACK（原样头 + rand=0），未实现命令也 ACK。
//! 同时维护 UI「NET API 活动预览」统计与最近 10 条记录。

use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use ysc_core::consts::Role;
use ysc_core::util::now_ms;

use crate::app::App;

pub const CMD_CONNECT: u32 = 0xaf3c_2828;
pub const CMD_MOUSE_MOVE: u32 = 0xaede_7345;
pub const CMD_MOUSE_LEFT: u32 = 0x9823_ae8d;
pub const CMD_MOUSE_MIDDLE: u32 = 0x97a3_ae8d;
pub const CMD_MOUSE_RIGHT: u32 = 0x238d_8212;
pub const CMD_MOUSE_WHEEL: u32 = 0xffee_ad38;
pub const CMD_KEYBOARD_ALL: u32 = 0x123c_2c2f;
pub const CMD_MONITOR: u32 = 0x2738_8020;
pub const CMD_MOUSE_AUTOMOVE: u32 = 0xaede_7346;
pub const CMD_BAZER_MOVE: u32 = 0xa238_455a;
pub const CMD_REBOOT: u32 = 0xaa88_55aa;
pub const CMD_MASK_MOUSE: u32 = 0x2323_4343;
pub const CMD_UNMASK_ALL: u32 = 0x2334_4343;

const NET_RING_MAX: usize = 10;
const NET_PUSH_MIN_MS: u64 = 200;
const NET_CLIENT_ACTIVE_MS: u64 = 10_000;

#[derive(Clone, Copy)]
struct NetCmdRec {
    seq: u32,
    ms: u32,
    kind: u8,
    ok: u8,
    name: [u8; 12],
    detail: [u8; 40],
}

impl Default for NetCmdRec {
    fn default() -> Self {
        Self {
            seq: 0,
            ms: 0,
            kind: 0,
            ok: 0,
            name: [0; 12],
            detail: [0; 40],
        }
    }
}

impl NetCmdRec {
    fn set_name(&mut self, s: &str) {
        let b = s.as_bytes();
        let n = b.len().min(self.name.len() - 1);
        self.name[..n].copy_from_slice(&b[..n]);
        self.name[n] = 0;
    }
    fn set_detail(&mut self, s: &str) {
        let b = s.as_bytes();
        let n = b.len().min(self.detail.len() - 1);
        self.detail[..n].copy_from_slice(&b[..n]);
        self.detail[n] = 0;
    }
    fn name_str(&self) -> String {
        let end = self
            .name
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(self.name.len());
        String::from_utf8_lossy(&self.name[..end]).to_string()
    }
    fn detail_str(&self) -> String {
        let end = self
            .detail
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(self.detail.len());
        String::from_utf8_lossy(&self.detail[..end]).to_string()
    }
}

#[derive(Default)]
struct NetStats {
    total: u32,
    kind_cnt: [u32; 4],
    ring: [NetCmdRec; NET_RING_MAX],
    ring_count: usize,
    ring_head: usize,
    seq: u32,
    client_ip: String,
    client_port: u16,
    client_since: u64,
    client_last: u64,
    client_sessions: u32,
    last_push: u64,
    start_ms: u64,
}

/// kmboxnet UDP 服务。
pub struct KmboxServer {
    inner: Arc<Inner>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

struct Inner {
    running: AtomicBool,
    port: AtomicU16,
    socket: Mutex<Option<Arc<UdpSocket>>>,
    monitor_client: Mutex<Option<SocketAddr>>,
    app: Mutex<Weak<App>>,
    stats: Mutex<NetStats>,
    btn_mask: Mutex<(u8, bool)>,
}

impl KmboxServer {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                running: AtomicBool::new(false),
                port: AtomicU16::new(0),
                socket: Mutex::new(None),
                monitor_client: Mutex::new(None),
                app: Mutex::new(Weak::<App>::new()),
                stats: Mutex::new(NetStats::default()),
                btn_mask: Mutex::new((0, false)),
            }),
            thread: Mutex::new(None),
        }
    }

    pub fn set_app(&self, app: Weak<App>) {
        *self.inner.app.lock().unwrap() = app;
    }

    pub fn is_running(&self) -> bool {
        self.inner.running.load(Ordering::Acquire)
    }

    pub fn start(&self, port: u16) -> bool {
        if self.is_running() {
            return false;
        }
        let sock = match UdpSocket::bind(("0.0.0.0", port)) {
            Ok(s) => s,
            Err(_) => return false,
        };
        // 短超时轮询：让 stop() 能在一个周期内 join，不必等 3s。
        let _ = sock.set_read_timeout(Some(Duration::from_millis(100)));
        let sock = Arc::new(sock);
        *self.inner.socket.lock().unwrap() = Some(Arc::clone(&sock));
        self.inner.port.store(port, Ordering::Relaxed);
        self.inner.running.store(true, Ordering::Release);
        *self.inner.btn_mask.lock().unwrap() = (0, false);
        {
            let mut st = self.inner.stats.lock().unwrap();
            *st = NetStats {
                start_ms: now_ms(),
                ..Default::default()
            };
        }

        let inner = Arc::clone(&self.inner);
        match thread::Builder::new()
            .name("ysc-kmnet".into())
            .spawn(move || recv_loop(inner, sock))
        {
            Ok(j) => {
                *self.thread.lock().unwrap() = Some(j);
                true
            }
            Err(_) => {
                self.inner.running.store(false, Ordering::Release);
                *self.inner.socket.lock().unwrap() = None;
                false
            }
        }
    }

    pub fn stop(&self) {
        self.inner.running.store(false, Ordering::Release);
        // 缩短读超时唤醒 recv_from，再等线程退出。
        if let Some(sock) = self.inner.socket.lock().unwrap().clone() {
            let _ = sock.set_read_timeout(Some(Duration::from_millis(10)));
        }
        if let Some(j) = self.thread.lock().unwrap().take() {
            let _ = j.join();
        }
        *self.inner.socket.lock().unwrap() = None;
        *self.inner.monitor_client.lock().unwrap() = None;
        *self.inner.btn_mask.lock().unwrap() = (0, false);
        {
            let mut st = self.inner.stats.lock().unwrap();
            *st = NetStats {
                start_ms: now_ms(),
                ..Default::default()
            };
        }
        if let Some(app) = self.inner.app.lock().unwrap().upgrade() {
            let body = self.get_stat_json();
            if !body.is_empty() && app.ui_attached() {
                app.emit("kmnet_stat", Some(&body));
            }
        }
    }

    pub fn has_monitor_client(&self) -> bool {
        self.inner.monitor_client.lock().unwrap().is_some()
    }

    pub fn send_monitor_data(&self, pkt: &[u8]) {
        let dest = *self.inner.monitor_client.lock().unwrap();
        let Some(dest) = dest else { return };
        let sock = self.inner.socket.lock().unwrap().clone();
        if let Some(sock) = sock {
            let _ = sock.send_to(pkt, dest);
        }
    }

    pub fn get_stat_json(&self) -> String {
        let st = self.inner.stats.lock().unwrap();
        build_stat_locked(&st)
    }
}

impl Default for KmboxServer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for KmboxServer {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Inner {
    fn app(&self) -> Option<Arc<App>> {
        self.app.lock().unwrap().upgrade()
    }

    fn send_stat(&self, body: &str) {
        if let Some(app) = self.app() {
            if app.ui_attached() {
                app.emit("kmnet_stat", Some(body));
            }
        }
    }

    fn send_response(&self, dest: SocketAddr, req: &[u8; 16]) {
        let mut resp = *req;
        resp[4..8].copy_from_slice(&0u32.to_le_bytes()); // rand = 0 = success
        let sock = self.socket.lock().unwrap().clone();
        if let Some(sock) = sock {
            let _ = sock.send_to(&resp, dest);
        }
    }

    fn touch_client(&self, ip: &str, port: u16) {
        let (body, changed) = {
            let mut st = self.stats.lock().unwrap();
            let changed = st.client_ip != ip || st.client_port != port;
            if changed {
                st.client_ip = ip.to_string();
                st.client_port = port;
                st.client_since = now_ms();
                st.client_sessions += 1;
            }
            st.client_last = now_ms();
            let body = build_stat_locked(&st);
            (body, changed)
        };
        if changed {
            self.send_stat(&body);
            self.stats.lock().unwrap().last_push = now_ms();
        }
    }

    fn record(&self, name: &str, kind: u8, ok: bool, detail: &str, need_push: bool) {
        let (body, allow) = {
            let mut st = self.stats.lock().unwrap();
            st.total += 1;
            if (kind as usize) < 4 {
                st.kind_cnt[kind as usize] += 1;
            }
            st.seq += 1;
            let seq = st.seq;
            let ms = now_ms().saturating_sub(st.start_ms) as u32;
            let mut rec = NetCmdRec {
                seq,
                ms,
                kind,
                ok: ok as u8,
                ..Default::default()
            };
            rec.set_name(name);
            rec.set_detail(detail);
            let head = st.ring_head;
            st.ring[head] = rec;
            st.ring_head = (head + 1) % NET_RING_MAX;
            if st.ring_count < NET_RING_MAX {
                st.ring_count += 1;
            }
            let now = now_ms();
            let allow = need_push || now.saturating_sub(st.last_push) >= NET_PUSH_MIN_MS;
            let body = if allow {
                build_stat_locked(&st)
            } else {
                String::new()
            };
            if allow {
                st.last_push = now;
            }
            (body, allow)
        };
        if allow && !body.is_empty() {
            self.send_stat(&body);
        }
    }

    fn apply_button_mask(&self, want: u8) -> bool {
        let want = want & 0x1F;
        {
            let (mask, valid) = *self.btn_mask.lock().unwrap();
            if valid && want == mask {
                return true;
            }
        }
        if let Some(app) = self.app() {
            if app.send_mouse_button_mask(want, Role::Mouse) {
                *self.btn_mask.lock().unwrap() = (want, true);
                return true;
            }
        }
        false
    }
}

fn recv_loop(inner: Arc<Inner>, sock: Arc<UdpSocket>) {
    unsafe {
        let _ = windows::Win32::System::Threading::SetThreadPriority(
            windows::Win32::System::Threading::GetCurrentThread(),
            windows::Win32::System::Threading::THREAD_PRIORITY_ABOVE_NORMAL,
        );
    }
    let mut buf = [0u8; 1080];
    while inner.running.load(Ordering::Acquire) {
        let (n, from) = match sock.recv_from(&mut buf) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if n < 16 {
            continue;
        }
        handle_packet(&inner, &buf[..n], from);
    }
}

fn read_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
fn read_i32(b: &[u8], off: usize) -> i32 {
    i32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

fn handle_packet(inner: &Arc<Inner>, data: &[u8], from: SocketAddr) {
    let mut hdr = [0u8; 16];
    hdr.copy_from_slice(&data[..16]);
    let mac = read_u32(data, 0);
    let rand = read_u32(data, 4);
    let idx = read_u32(data, 8);
    let cmd = read_u32(data, 12);
    let ip = from.ip().to_string();
    let port = from.port();
    {
        let mut log = String::new();
        log.push_str(&format!(
            "UDP RECV cmd=0x{:08X} idx={} from={}:{} len={}",
            cmd,
            idx,
            ip,
            port,
            data.len()
        ));
        if let Some(app) = inner.app() {
            app.log(&log);
            app.debug_window_log(&format!(
                "UDP recv cmd=0x{:08X} mac=0x{:08X} idx={} from={}:{} len={}",
                cmd,
                mac,
                idx,
                ip,
                port,
                data.len()
            ));
        }
    }
    inner.touch_client(&ip, port);

    match cmd {
        CMD_CONNECT => {
            inner.record("connect", 2, true, "ACK", true);
            inner.send_response(from, &hdr);
        }
        CMD_MOUSE_MOVE => {
            if data.len() < 16 + 56 {
                return;
            }
            let x = read_i32(data, 16 + 4);
            let y = read_i32(data, 16 + 8);
            let ok = match inner.app() {
                Some(app) => app.send_mouse_move(x, y, 1, Role::Mouse),
                None => false,
            };
            inner.record("move", 0, ok, &format!("x={} y={}", x, y), false);
            inner.send_response(from, &hdr);
        }
        CMD_MOUSE_AUTOMOVE | CMD_BAZER_MOVE => {
            if data.len() < 16 + 56 {
                return;
            }
            let x = read_i32(data, 16 + 4);
            let y = read_i32(data, 16 + 8);
            let mut steps = if rand > 0 { (rand / 16) as i32 } else { 1 };
            steps = steps.clamp(1, 1000);
            let (ok, name) = match inner.app() {
                Some(app) => {
                    if cmd == CMD_MOUSE_AUTOMOVE {
                        (app.send_mouse_move(x, y, steps, Role::Mouse), "automove")
                    } else {
                        (app.send_move_tow(x, y, steps, Role::Mouse), "beizer")
                    }
                }
                None => (false, "?"),
            };
            inner.record(name, 0, ok, &format!("x={} y={} c={}", x, y, steps), false);
            inner.send_response(from, &hdr);
        }
        CMD_MOUSE_LEFT | CMD_MOUSE_RIGHT | CMD_MOUSE_MIDDLE => {
            if data.len() < 16 + 56 {
                return;
            }
            let button = read_u32(data, 16);
            let btn_name = match cmd {
                CMD_MOUSE_LEFT => "LEFT",
                CMD_MOUSE_RIGHT => "RIGHT",
                _ => "MIDDLE",
            };
            let ok = inner.apply_button_mask(button as u8);
            inner.record(
                "button",
                0,
                ok,
                &format!("{} b=0x{:02X}", btn_name, button & 0x1F),
                false,
            );
            inner.send_response(from, &hdr);
        }
        CMD_MOUSE_WHEEL => {
            if data.len() < 16 + 56 {
                return;
            }
            let button = read_u32(data, 16);
            let x = read_i32(data, 16 + 4);
            let y = read_i32(data, 16 + 8);
            let wheel = read_i32(data, 16 + 12);
            let mut ok = inner.apply_button_mask(button as u8);
            let Some(app) = inner.app() else {
                return;
            };
            if x != 0 || y != 0 {
                ok = app.send_mouse_move(x, y, 1, Role::Mouse) && ok;
            }
            if wheel != 0 {
                ok = app.send_mouse_wheel(wheel, Role::Mouse) && ok;
            }
            let name = if x != 0 || y != 0 { "all" } else { "wheel" };
            inner.record(
                name,
                0,
                ok,
                &format!("wh={} b=0x{:02X}", wheel, button & 0x1F),
                false,
            );
            inner.send_response(from, &hdr);
        }
        CMD_MASK_MOUSE | CMD_UNMASK_ALL => {
            let m = (rand & 0xFF) as u8;
            let ok = match inner.app() {
                Some(app) => app.send_mouse_mask(m, Role::Mouse),
                None => false,
            };
            let name = if cmd == CMD_UNMASK_ALL {
                "unmask_all"
            } else {
                "mask_mouse"
            };
            inner.record(name, 2, ok, &format!("m=0x{:02X}", m), true);
            inner.send_response(from, &hdr);
        }
        CMD_REBOOT => {
            let ok = match inner.app() {
                Some(app) => {
                    let a = app.send_reboot(Role::Mouse);
                    let b = app.send_reboot(Role::Kbd);
                    a || b
                }
                None => false,
            };
            inner.record("reboot", 2, ok, "mouse+kbd", true);
            inner.send_response(from, &hdr);
        }
        CMD_KEYBOARD_ALL => {
            if data.len() < 16 + 12 {
                return;
            }
            let ctrl = data[16];
            let mut keys = [0u8; 10];
            keys.copy_from_slice(&data[18..28]);
            let nkeys = keys.iter().filter(|&&k| k != 0).count();
            let ok = match inner.app() {
                Some(app) => app.send_keyboard(ctrl, &keys),
                None => false,
            };
            inner.record(
                "keyboard",
                1,
                ok,
                &format!("ctrl=0x{:02X} n={}", ctrl, nkeys),
                false,
            );
            inner.send_response(from, &hdr);
        }
        CMD_MONITOR => {
            let client_port = (rand & 0xFFFF) as u16;
            if client_port == 0 {
                *inner.monitor_client.lock().unwrap() = None;
                if let Some(app) = inner.app() {
                    app.send_upload_status(false, Role::Mouse);
                }
            } else {
                *inner.monitor_client.lock().unwrap() =
                    Some(SocketAddr::new(from.ip(), client_port));
                if let Some(app) = inner.app() {
                    app.send_upload_status(true, Role::Mouse);
                }
            }
            inner.record(
                if client_port != 0 {
                    "monitor_on"
                } else {
                    "monitor_off"
                },
                2,
                true,
                &format!("port={}", client_port),
                true,
            );
            inner.send_response(from, &hdr);
        }
        _ => {
            inner.record("unsupported", 3, false, &format!("cmd=0x{:08X}", cmd), true);
            if let Some(app) = inner.app() {
                app.log(&format!("UDP unsupported cmd=0x{:08X} (ACK only)", cmd));
            }
            inner.send_response(from, &hdr);
        }
    }
}

fn build_stat_locked(st: &NetStats) -> String {
    let now = now_ms();
    let seen = if st.client_last != 0 {
        now.saturating_sub(st.client_last) as u32
    } else {
        0
    };
    let active = st.client_last != 0 && now.saturating_sub(st.client_last) < NET_CLIENT_ACTIVE_MS;
    let mut s = format!(
        "\"total\":{},\"c\":{{\"mouse\":{},\"kbd\":{},\"ctrl\":{},\"other\":{}}},\"client\":{{\"ip\":\"{}\",\"port\":{},\"sessions\":{},\"seenMs\":{},\"active\":{}}},\"ring\":[",
        st.total,
        st.kind_cnt[0],
        st.kind_cnt[1],
        st.kind_cnt[2],
        st.kind_cnt[3],
        st.client_ip,
        st.client_port,
        st.client_sessions,
        seen,
        active as i32
    );
    for i in 0..st.ring_count {
        let idx = (st.ring_head + NET_RING_MAX * 2 - 1 - i) % NET_RING_MAX;
        let r = &st.ring[idx];
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "{{\"i\":{},\"ms\":{},\"k\":{},\"ok\":{},\"n\":\"{}\",\"d\":\"{}\"}}",
            r.seq,
            r.ms,
            r.kind,
            r.ok,
            r.name_str(),
            r.detail_str()
        ));
    }
    s.push(']');
    s
}

/// 本地 IPv4（显示用）。等价 C++ `GetLocalIP`：优先非回环地址，失败回 127.0.0.1。
pub fn get_local_ip() -> String {
    if let Some(ip) = crate::netutil::first_non_loopback_ipv4() {
        return ip;
    }
    "127.0.0.1".to_string()
}
