//! `ysc_sdk.dll` — YSC 8K 驱动 C ABI（Rust 移植版）。
//!
//! 与 `ysc_8k_driver/ysc_sdk.cpp` 的导出面、命令码、返回值和错误语义一致：
//!   * 同步串口（非 overlapped），每个句柄内部串行化；
//!   * `Ysc_SendCommand` 发帧后等到一条完整响应，返回真实负载长度；
//!   * `NoWait` 系列只清发送缓冲，不丢弃异步回包。

use std::cell::RefCell;
use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use windows::Win32::Foundation::HANDLE;

use ysc_core::consts::{is_supported_baud, SUPPORTED_BAUDS};
use ysc_core::enumerate::is_ysc_v3_port;
use ysc_core::enumerate::{enum_all_com_ports, enum_towmcu_ports};
use ysc_core::frame::{build_json_frame, build_version_frame, has_ok_response};
use ysc_core::serial::{
    close_sync, open_sync_code, purge_rx_tx, purge_tx, recv_frame_sync, sync_write,
};
use ysc_core::util::json_escape;

const SDK_VERSION: &[u8] = b"1.1.0\0";

thread_local! {
    static LAST_ERR: RefCell<CString> = RefCell::new(CString::new("").unwrap());
}

fn set_err(msg: impl AsRef<str>) {
    let s = msg.as_ref().replace('\0', " ");
    LAST_ERR.with(|e| {
        *e.borrow_mut() = CString::new(s).unwrap_or_else(|_| CString::new("").unwrap());
    });
}

/// 安全取 C 字符串（非 UTF-8 时返回 None）。
unsafe fn cstr<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        None
    } else {
        CStr::from_ptr(p).to_str().ok()
    }
}

/// `snprintf` 风格：返回 payload 应有长度；调用方缓冲不足时截断并 NUL 结尾。
fn write_out(out: *mut c_char, size: i32, text: &str) -> i32 {
    let need = text.len() as i32;
    if !out.is_null() && size > 0 {
        let cap = (size - 1) as usize;
        let n = cap.min(text.len());
        unsafe {
            std::ptr::copy_nonoverlapping(text.as_ptr(), out as *mut u8, n);
            *out.add(n) = 0;
        }
    }
    need
}

fn safe_copy(out: *mut c_char, size: i32, text: &str) {
    if !out.is_null() && size > 0 {
        let n = ((size - 1) as usize).min(text.len());
        unsafe {
            std::ptr::copy_nonoverlapping(text.as_ptr(), out as *mut u8, n);
            *out.add(n) = 0;
        }
    }
}

fn h(v: isize) -> HANDLE {
    HANDLE(v as *mut c_void)
}

// ---------------------------------------------------------------------------
// 连接句柄
// ---------------------------------------------------------------------------

struct DevInner {
    handle: isize,
    baud: u32,
    port: String,
}

/// 不透明连接句柄（一次 `Ysc_Connect` 对应一个串口）。
pub struct YscDevice {
    inner: Mutex<DevInner>,
}

fn dev_guard<'a>(dev: *mut YscDevice) -> Option<MutexGuard<'a, DevInner>> {
    if dev.is_null() {
        None
    } else {
        unsafe { (*dev).inner.lock().ok() }
    }
}

fn open_com(port: &str, baud: u32, read_total: u32) -> Result<HANDLE, u32> {
    let handle = open_sync_code(port, baud, read_total, 200)?;
    purge_rx_tx(handle);
    Ok(handle)
}

// ---------------------------------------------------------------------------
// 0. 信息
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "system" fn Ysc_SdkVersion() -> *const c_char {
    SDK_VERSION.as_ptr() as *const c_char
}

#[no_mangle]
pub extern "system" fn Ysc_LastError() -> *const c_char {
    LAST_ERR.with(|e| e.borrow().as_ptr())
}

#[no_mangle]
pub extern "system" fn Ysc_SupportedBaudrates(count: *mut i32) -> *const u32 {
    if !count.is_null() {
        unsafe {
            *count = SUPPORTED_BAUDS.len() as i32;
        }
    }
    SUPPORTED_BAUDS.as_ptr()
}

// ---------------------------------------------------------------------------
// 1. 串口枚举
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "system" fn Ysc_ListPorts(out: *mut c_char, size: i32) -> i32 {
    let ports = enum_towmcu_ports();
    let mut s = String::from("[");
    for (i, p) in ports.iter().enumerate() {
        s.push_str(&format!(
            "{{\"port\":\"{}\",\"serial\":\"{}\",\"side\":\"{}\",\"desc\":\"{}\"}}",
            json_escape(&p.port),
            json_escape(&p.serial),
            json_escape(&p.side),
            json_escape(&p.description)
        ));
        if i + 1 < ports.len() {
            s.push(',');
        }
    }
    s.push(']');
    write_out(out, size, &s)
}

#[no_mangle]
pub extern "system" fn Ysc_ListAllComPorts(out: *mut c_char, size: i32) -> i32 {
    let ports = enum_all_com_ports();
    let mut s = String::from("[");
    for (i, p) in ports.iter().enumerate() {
        s.push_str(&format!(
            "{{\"port\":\"{}\",\"desc\":\"{}\"}}",
            json_escape(&p.port),
            json_escape(&p.description)
        ));
        if i + 1 < ports.len() {
            s.push(',');
        }
    }
    s.push(']');
    write_out(out, size, &s)
}

// ---------------------------------------------------------------------------
// 2. 连接生命周期
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "system" fn Ysc_Connect(
    port_name: *const c_char,
    baud_rate: u32,
    err_buf: *mut c_char,
    err_size: i32,
) -> *mut YscDevice {
    if !err_buf.is_null() && err_size > 0 {
        *err_buf = 0;
    }
    let Some(port) = cstr(port_name) else {
        set_err("null portName");
        safe_copy(err_buf, err_size, "null portName");
        return std::ptr::null_mut();
    };

    let mut baud = baud_rate;
    if baud == 0 {
        baud = Ysc_DetectBaudrate(port_name);
        if baud == 0 {
            LAST_ERR.with(|e| {
                safe_copy(
                    err_buf,
                    err_size,
                    e.borrow().to_str().unwrap_or("detect baudrate failed"),
                );
            });
            return std::ptr::null_mut();
        }
    }

    // YSC V3 CDC 没有真实波特率：主机句柄固定用安全速率打开，逻辑 baud 仍
    // 按调用方传入值保存/回读。普通串口保持原样。
    let v3 = is_ysc_v3_port(&port);
    let open_baud = if v3 { 115_200 } else { baud };
    let handle = match open_com(port, open_baud, 20) {
        Ok(h) => h,
        Err(code) => {
            let msg = format!("open {} @ {} failed (err={})", port, open_baud, code);
            set_err(format!("open {} @ {} failed, err={}", port, open_baud, code));
            safe_copy(err_buf, err_size, &msg);
            return std::ptr::null_mut();
        }
    };

    let dev = Box::new(YscDevice {
        inner: Mutex::new(DevInner {
            handle: handle.0 as isize,
            baud,
            port: port.to_string(),
        }),
    });
    Box::into_raw(dev)
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_Disconnect(dev: *mut YscDevice) {
    if dev.is_null() {
        return;
    }
    let boxed = Box::from_raw(dev);
    let mut inner = boxed.inner.lock().unwrap();
    if inner.handle != 0 {
        close_sync(h(inner.handle));
        inner.handle = 0;
    }
    inner.baud = 0;
    drop(inner);
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_DetectBaudrate(port_name: *const c_char) -> u32 {
    let Some(port) = cstr(port_name) else {
        set_err("null portName");
        return 0;
    };
    let baud = ysc_core::serial::SerialPort::detect_baudrate(port);
    if baud == 0 {
        set_err("no baudrate responded to {\"cmd\":132}");
    }
    baud
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_IsConnected(dev: *mut YscDevice) -> i32 {
    match dev_guard(dev) {
        Some(inner) => (inner.handle != 0) as i32,
        None => 0,
    }
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_GetBaudrate(dev: *mut YscDevice) -> u32 {
    match dev_guard(dev) {
        Some(inner) => inner.baud,
        None => 0,
    }
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_GetPortName(
    dev: *mut YscDevice,
    out: *mut c_char,
    size: i32,
) -> i32 {
    let Some(inner) = dev_guard(dev) else {
        set_err("null dev");
        return -1;
    };
    write_out(out, size, &inner.port)
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_SwitchBaudrate(dev: *mut YscDevice, new_baud: u32) -> i32 {
    let Some(mut inner) = dev_guard(dev) else {
        set_err("not connected");
        return 0;
    };
    if inner.handle == 0 {
        set_err("not connected");
        return 0;
    }
    let port = inner.port.clone();
    let v3 = is_ysc_v3_port(&port);
    if !v3 && !is_supported_baud(new_baud) {
        set_err(format!("unsupported baud {}", new_baud));
        return 0;
    }
    if v3 {
        // V3 USB CDC：只更新逻辑 baud，不关端口/不重开。
        let json = format!("{{\"cmd\":133,\"baud\":{}}}", new_baud);
        if let Some(frame) = build_json_frame(&json) {
            let old = h(inner.handle);
            purge_rx_tx(old);
            let _ = sync_write(old, &frame, 200);
        }
        inner.baud = new_baud;
        return 1;
    }
    let old = h(inner.handle);

    // 1) 通知固件切换
    let json = format!("{{\"cmd\":133,\"baud\":{}}}", new_baud);
    if let Some(frame) = build_json_frame(&json) {
        purge_rx_tx(old);
        let _ = sync_write(old, &frame, 200);
    }
    std::thread::sleep(Duration::from_millis(200));

    // 2) 关闭旧端口
    close_sync(old);
    inner.handle = 0;
    std::thread::sleep(Duration::from_millis(100));

    // 3) 新波特率重开
    let new_handle = match open_com(&port, new_baud, 200) {
        Ok(h) => h,
        Err(code) => {
            set_err(format!(
                "reopen {} @ {} failed, err={}",
                port, new_baud, code
            ));
            return 0;
        }
    };
    inner.handle = new_handle.0 as isize;

    // 4) 版本查询校验
    let frame = build_version_frame();
    purge_rx_tx(new_handle);
    let _ = sync_write(new_handle, &frame, 200);
    let start = std::time::Instant::now();
    let mut rx: Vec<u8> = Vec::with_capacity(256);
    let mut ok = false;
    while start.elapsed() < Duration::from_millis(500) && rx.len() < 256 {
        let mut chunk = [0u8; 256];
        match ysc_core::serial::sync_read(new_handle, &mut chunk) {
            Some(n) if n > 0 => {
                rx.extend_from_slice(&chunk[..n]);
                if has_ok_response(&rx) {
                    ok = true;
                    break;
                }
            }
            _ => std::thread::sleep(Duration::from_millis(1)),
        }
    }
    if !ok {
        set_err("baudrate switch verify failed");
        return 0;
    }
    inner.baud = new_baud;
    1
}

// ---------------------------------------------------------------------------
// 3. 核心发送
// ---------------------------------------------------------------------------

fn send_and_wait(
    inner: &mut DevInner,
    json: &str,
    timeout_ms: i32,
    out: *mut c_char,
    size: i32,
) -> i32 {
    if inner.handle == 0 {
        set_err("not connected");
        return -1;
    }
    let Some(frame) = build_json_frame(json) else {
        set_err("json too long");
        return -1;
    };
    let handle = h(inner.handle);
    purge_rx_tx(handle);
    if !sync_write(handle, &frame, 200) {
        set_err("WriteFile failed");
        return -1;
    }
    let timeout = if timeout_ms < 0 {
        1000
    } else {
        timeout_ms as u32
    };
    let Some(payload) = recv_frame_sync(handle, timeout) else {
        set_err(format!("timeout / no response within {} ms", timeout));
        return -2;
    };
    write_out(out, size, &String::from_utf8_lossy(&payload))
}

fn send_no_wait(inner: &mut DevInner, json: &str) -> i32 {
    if inner.handle == 0 {
        set_err("not connected");
        return 0;
    }
    let Some(frame) = build_json_frame(json) else {
        set_err("json too long");
        return 0;
    };
    let handle = h(inner.handle);
    purge_tx(handle);
    if sync_write(handle, &frame, 200) {
        1
    } else {
        set_err("WriteFile failed");
        0
    }
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_SendCommand(
    dev: *mut YscDevice,
    json: *const c_char,
    timeout_ms: i32,
    out: *mut c_char,
    size: i32,
) -> i32 {
    let Some(mut inner) = dev_guard(dev) else {
        set_err("not connected");
        return -1;
    };
    let Some(json) = cstr(json) else {
        set_err("null json");
        return -1;
    };
    send_and_wait(&mut inner, json, timeout_ms, out, size)
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_SendCommandNoWait(
    dev: *mut YscDevice,
    json: *const c_char,
) -> i32 {
    let Some(mut inner) = dev_guard(dev) else {
        set_err("not connected");
        return 0;
    };
    let Some(json) = cstr(json) else {
        set_err("null json");
        return 0;
    };
    send_no_wait(&mut inner, json)
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_SendRaw(dev: *mut YscDevice, data: *const u8, len: i32) -> i32 {
    let Some(inner) = dev_guard(dev) else {
        set_err("not connected");
        return 0;
    };
    if inner.handle == 0 {
        set_err("not connected");
        return 0;
    }
    if data.is_null() || len <= 0 {
        set_err("bad data");
        return 0;
    }
    let slice = std::slice::from_raw_parts(data, len as usize);
    if sync_write(h(inner.handle), slice, 200) {
        1
    } else {
        set_err("WriteFile failed");
        0
    }
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_QueryVersion(
    dev: *mut YscDevice,
    timeout_ms: i32,
    out: *mut c_char,
    size: i32,
) -> i32 {
    Ysc_SendCommand(
        dev,
        b"{\"cmd\":132}\0".as_ptr() as *const c_char,
        timeout_ms,
        out,
        size,
    )
}

// ---------------------------------------------------------------------------
// 4. 便捷命令
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "system" fn Ysc_MouseMove(
    dev: *mut YscDevice,
    x: i32,
    y: i32,
    steps: i32,
) -> i32 {
    let j = format!("{{\"cmd\":30,\"x\":{},\"y\":{},\"c\":{}}}", x, y, steps);
    Ysc_SendCommandNoWait(dev, j.as_ptr() as *const c_char)
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_MouseMoveTow(
    dev: *mut YscDevice,
    x: i32,
    y: i32,
    steps: i32,
) -> i32 {
    let j = format!("{{\"cmd\":31,\"x\":{},\"y\":{},\"c\":{}}}", x, y, steps);
    Ysc_SendCommandNoWait(dev, j.as_ptr() as *const c_char)
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_MouseButton(
    dev: *mut YscDevice,
    button_mask: u8,
    pressed: i32,
) -> i32 {
    let j = format!(
        "{{\"cmd\":33,\"b\":{},\"s\":{}}}",
        button_mask,
        if pressed != 0 { 1 } else { 0 }
    );
    Ysc_SendCommandNoWait(dev, j.as_ptr() as *const c_char)
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_KeyboardKey(dev: *mut YscDevice, keycode: u8, down: i32) -> i32 {
    let j = format!(
        "{{\"cmd\":45,\"kc\":{},\"down\":{}}}",
        keycode,
        if down != 0 { 1 } else { 0 }
    );
    Ysc_SendCommandNoWait(dev, j.as_ptr() as *const c_char)
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_KeyboardReleaseAll(dev: *mut YscDevice) -> i32 {
    Ysc_SendCommandNoWait(dev, b"{\"cmd\":46}\0".as_ptr() as *const c_char)
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_KeyboardTypeString(dev: *mut YscDevice, s: *const c_char) -> i32 {
    let Some(s) = cstr(s) else {
        set_err("empty string");
        return 0;
    };
    if s.is_empty() {
        set_err("empty string");
        return 0;
    }
    if s.len() > 128 {
        set_err("string too long (max 128)");
        return 0;
    }
    let j = format!("{{\"cmd\":47,\"s\":\"{}\"}}", json_escape(s));
    Ysc_SendCommandNoWait(dev, j.as_ptr() as *const c_char)
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_KeyboardTypeStatus(
    dev: *mut YscDevice,
    timeout_ms: i32,
    out: *mut c_char,
    size: i32,
) -> i32 {
    Ysc_SendCommand(
        dev,
        b"{\"cmd\":48}\0".as_ptr() as *const c_char,
        timeout_ms,
        out,
        size,
    )
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_UploadStatus(dev: *mut YscDevice, enable: i32) -> i32 {
    let j = format!(
        "{{\"cmd\":34,\"status\":{}}}",
        if enable != 0 { 1 } else { 0 }
    );
    Ysc_SendCommandNoWait(dev, j.as_ptr() as *const c_char)
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_JumpIAP(dev: *mut YscDevice) -> i32 {
    Ysc_SendCommandNoWait(dev, b"{\"cmd\":50}\0".as_ptr() as *const c_char)
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_GamepadGetConfig(
    dev: *mut YscDevice,
    timeout_ms: i32,
    out: *mut c_char,
    size: i32,
) -> i32 {
    Ysc_SendCommand(
        dev,
        b"{\"cmd\":101}\0".as_ptr() as *const c_char,
        timeout_ms,
        out,
        size,
    )
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_GamepadSetConfig(
    dev: *mut YscDevice,
    config_json: *const c_char,
) -> i32 {
    let Some(cfg) = cstr(config_json) else {
        set_err("null configJson");
        return 0;
    };
    let j = format!("{{\"cmd\":100,\"data\":{}}}", cfg);
    if j.len() >= 4096 {
        set_err("config too long");
        return 0;
    }
    Ysc_SendCommandNoWait(dev, j.as_ptr() as *const c_char)
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_GamepadEnable(dev: *mut YscDevice, on: i32) -> i32 {
    let j = format!("{{\"cmd\":102,\"on\":{}}}", if on != 0 { 1 } else { 0 });
    Ysc_SendCommandNoWait(dev, j.as_ptr() as *const c_char)
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_GamepadReset(dev: *mut YscDevice) -> i32 {
    Ysc_SendCommandNoWait(dev, b"{\"cmd\":104}\0".as_ptr() as *const c_char)
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_InterpGet(
    dev: *mut YscDevice,
    timeout_ms: i32,
    out: *mut c_char,
    size: i32,
) -> i32 {
    Ysc_SendCommand(
        dev,
        b"{\"cmd\":52}\0".as_ptr() as *const c_char,
        timeout_ms,
        out,
        size,
    )
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_InterpSet(
    dev: *mut YscDevice,
    enable: i32,
    profile: i32,
    window_ms: i32,
    max_window_ms: i32,
) -> i32 {
    let j = format!(
        "{{\"cmd\":51,\"e\":{},\"p\":{},\"w\":{},\"mx\":{}}}",
        if enable != 0 { 1 } else { 0 },
        profile,
        window_ms,
        max_window_ms
    );
    Ysc_SendCommandNoWait(dev, j.as_ptr() as *const c_char)
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_InterpReset(dev: *mut YscDevice) -> i32 {
    Ysc_SendCommandNoWait(dev, b"{\"cmd\":53}\0".as_ptr() as *const c_char)
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_ReleaseButton(
    dev: *mut YscDevice,
    button_mask: u8,
    release: i32,
) -> i32 {
    let mask = button_mask & 0x1F;
    let j = format!(
        "{{\"cmd\":58,\"b\":{},\"s\":{}}}",
        mask,
        if release != 0 { 1 } else { 0 }
    );
    Ysc_SendCommandNoWait(dev, j.as_ptr() as *const c_char)
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_ForceRelease(
    dev: *mut YscDevice,
    button_mask: u8,
    duration_ms: u32,
    timeout_ms: i32,
    out: *mut c_char,
    size: i32,
) -> i32 {
    let mask = button_mask & 0x1F;
    if mask == 0 {
        return Ysc_ForceRestore(dev, timeout_ms, out, size);
    }
    let j = if duration_ms > 0 {
        format!("{{\"cmd\":67,\"b\":{},\"t\":{}}}", mask, duration_ms)
    } else {
        format!("{{\"cmd\":67,\"b\":{}}}", mask)
    };
    Ysc_SendCommand(dev, j.as_ptr() as *const c_char, timeout_ms, out, size)
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_ForceReleaseQuery(
    dev: *mut YscDevice,
    timeout_ms: i32,
    out: *mut c_char,
    size: i32,
) -> i32 {
    Ysc_SendCommand(
        dev,
        b"{\"cmd\":67}\0".as_ptr() as *const c_char,
        timeout_ms,
        out,
        size,
    )
}

#[no_mangle]
pub unsafe extern "system" fn Ysc_ForceRestore(
    dev: *mut YscDevice,
    timeout_ms: i32,
    out: *mut c_char,
    size: i32,
) -> i32 {
    Ysc_SendCommand(
        dev,
        b"{\"cmd\":67,\"b\":0}\0".as_ptr() as *const c_char,
        timeout_ms,
        out,
        size,
    )
}
