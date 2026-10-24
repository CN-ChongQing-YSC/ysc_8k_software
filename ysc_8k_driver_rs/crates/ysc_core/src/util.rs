//! 小工具：UTF-16/UTF-8、错误码、时间、hex、JSON 转义。

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use windows::Win32::Foundation::{GetLastError, WIN32_ERROR};
use windows::Win32::System::SystemInformation::{GetLocalTime, GetTickCount64};

/// 全局日志钩子：核心层不依赖宿主，由驱动把日志接到 `DebugLogger`。
type LogHook = Arc<dyn Fn(&str) + Send + Sync>;
static LOG_HOOK: Mutex<Option<LogHook>> = Mutex::new(None);

/// 安装日志钩子。
pub fn set_log_hook(hook: LogHook) {
    *LOG_HOOK.lock().unwrap() = Some(hook);
}

/// 写一条核心层日志。未安装钩子时退化为调试输出（release 下不可见）。
pub fn log(msg: &str) {
    let hook = LOG_HOOK.lock().unwrap().clone();
    match hook {
        Some(h) => h(msg),
        None => {
            #[cfg(debug_assertions)]
            eprintln!("[ysc_core] {}", msg);
        }
    }
}

/// Rust 字符串 → 以 NUL 结尾的 UTF-16。
pub fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// UTF-16（NUL 结尾或定长）→ Rust 字符串。
pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// 读取 NUL 结尾 UTF-16 的字符串（用于 Win32 输出缓冲）。
pub fn wide_to_string(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
    }
}

/// Win32 错误码 → 可读文本（与 `std::io::Error` 一致）。
pub fn win_error_text(code: u32) -> String {
    std::io::Error::from_raw_os_error(code as i32).to_string()
}

/// 最近一次 Win32 错误。
pub fn last_error() -> u32 {
    unsafe { GetLastError().0 }
}

/// 当前 GetTickCount64（毫秒）。
#[inline]
pub fn now_ms() -> u64 {
    unsafe { GetTickCount64() }
}

/// 自某个起点（毫秒）以来的间隔，允许跨 0 回绕（GetTickCount64 实际不会）。
#[inline]
pub fn elapsed_ms(since: u64) -> u64 {
    now_ms().saturating_sub(since)
}

/// 本地时间（年,月,日,时,分,秒,毫秒）。
pub fn local_time() -> (u16, u16, u16, u16, u16, u16, u16) {
    unsafe {
        let st = GetLocalTime();
        (
            st.wYear,
            st.wMonth,
            st.wDay,
            st.wHour,
            st.wMinute,
            st.wSecond,
            st.wMilliseconds,
        )
    }
}

/// `[HH:MM:SS.mmm]`。
pub fn timestamp() -> String {
    let (_, _, _, h, m, s, ms) = local_time();
    format!("[{:02}:{:02}:{:02}.{:03}]", h, m, s, ms)
}

/// 十六进制转储：` AA BB ...`；超过 `max_bytes` 时追加 ` ...(共N字节)`。
pub fn hex_dump(data: &[u8], max_bytes: usize) -> String {
    let n = data.len().min(max_bytes);
    let mut s = String::with_capacity(n * 3 + 32);
    for b in &data[..n] {
        s.push_str(&format!(" {:02X}", b));
    }
    if data.len() > max_bytes {
        s.push_str(&format!(" ...(共{}字节)", data.len()));
    }
    s
}

/// JSON 字符串转义（等价于 C++ 手工转义：`"` `\` 换行/回车/制表及控制字符）。
pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// 把 JSON 片段包成一行事件：`{"type":"<type>",<body>}\n`。
///
/// `body` 为空或 `None` 时输出 `{"type":"<type>"}\n`。与 C++ `PipeServer::SendEvent`
/// 完全一致（含换行分隔）。
pub fn build_event_line(event_type: &str, body: Option<&str>) -> String {
    match body {
        Some(b) if !b.is_empty() => {
            format!("{{\"type\":\"{}\",{}}}\n", event_type, b)
        }
        _ => format!("{{\"type\":\"{}\"}}\n", event_type),
    }
}

/// 解析 `MM:SS.mmm` 不需要；保留通用 sleep 包装便于统一毫秒语义。
pub fn sleep_ms(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

/// 安全截断字符串到指定字节数（按 UTF-8 边界）。
pub fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// 由 Win32 错误（WIN32_ERROR）构造可读文本。
pub fn win32_error_text(err: WIN32_ERROR) -> String {
    win_error_text(err.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_matches_cpp() {
        assert_eq!(json_escape("a\"b\\c\n\t"), "a\\\"b\\\\c\\n\\t");
        assert_eq!(json_escape("\u{1}"), "\\u0001");
        assert_eq!(json_escape("中文"), "中文");
    }

    #[test]
    fn event_line_shape() {
        assert_eq!(
            build_event_line("version", Some("\"version\":\"1.17.0\"")),
            "{\"type\":\"version\",\"version\":\"1.17.0\"}\n"
        );
        assert_eq!(
            build_event_line("serial_disconnected", None),
            "{\"type\":\"serial_disconnected\"}\n"
        );
    }
}
