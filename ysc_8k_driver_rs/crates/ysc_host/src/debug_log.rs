//! 文件日志 + 调试窗口输出。
//!
//! 对齐 `debug_logger.cpp`：
//!   * 日志文件在 exe 同目录 `ysc_driver_debug.log`，共享读（运行中可外部查看）。
//!   * 每 50000 行轮转一次（截断重写）。
//!   * 每行 `[HH:MM:SS.mmm] [DBG_OPEN|DBG_CLOSED] …`，≤500ms 时批量 flush。
//!   * 调试窗口打开时，`DebugLog` 额外把行 Post 到窗口（`WM_DEBUG_LOG`）。

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use ysc_core::util::{now_ms, timestamp};

/// 调试窗口消息（与 `main.h` 一致）。
pub const WM_DEBUG_LOG: u32 = 0x0400 + 2; // WM_USER + 2

const MAX_LINES: u32 = 50_000;
const FLUSH_INTERVAL_MS: u64 = 500;

pub static DEBUG: Lazy<DebugLogger> = Lazy::new(DebugLogger::new);

pub struct DebugLogger {
    inner: Mutex<Inner>,
    hwnd: AtomicIsize,
    window_open: AtomicBool,
    pending: Mutex<VecDeque<String>>,
}

struct Inner {
    path: PathBuf,
    file: Option<BufWriter<File>>,
    line_count: u32,
    last_flush: u64,
}

impl DebugLogger {
    fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                path: PathBuf::from("ysc_driver_debug.log"),
                file: None,
                line_count: 0,
                last_flush: 0,
            }),
            hwnd: AtomicIsize::new(0),
            window_open: AtomicBool::new(false),
            pending: Mutex::new(VecDeque::new()),
        }
    }

    /// 打开日志文件（启动时一次）。
    pub fn init(&self) {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("ysc_8k_driver.exe"));
        let path = exe
            .parent()
            .map(|p| p.join("ysc_driver_debug.log"))
            .unwrap_or_else(|| PathBuf::from("ysc_driver_debug.log"));

        let mut inner = self.inner.lock().unwrap();
        inner.path = path.clone();
        let file = open_shared(&path, true).ok();
        inner.file = file.map(BufWriter::new);
        inner.line_count = 2;
        inner.last_flush = now_ms();
        drop(inner);

        let (y, mo, d, h, mi, s, _) = ysc_core::util::local_time();
        self.raw(&format!(
            "=== YSC 8K Driver Debug Log Started {:04}-{:02}-{:02} {:02}:{:02}:{:02} ===\r\n",
            y, mo, d, h, mi, s
        ));
        self.raw("=== Tags: [DBG_OPEN] = debug window visible, [DBG_CLOSED] = debug window hidden ===\r\n\r\n");
    }

    pub fn set_hwnd(&self, hwnd: HWND) {
        self.hwnd.store(hwnd.0 as isize, Ordering::Release);
    }

    pub fn set_window_open(&self, open: bool) {
        let was = self.window_open.swap(open, Ordering::AcqRel);
        if was != open {
            self.log(&format!(
                "[{}] Debug window state changed to: {}",
                if open { "DBG_OPEN" } else { "DBG_CLOSED" },
                if open { "VISIBLE" } else { "HIDDEN" }
            ));
        }
    }

    pub fn is_window_open(&self) -> bool {
        self.window_open.load(Ordering::Acquire)
    }

    /// 文件日志（等价 C++ `DebugLogger::Log`）。
    pub fn log(&self, msg: &str) {
        let mut inner = self.inner.lock().unwrap();
        if inner.file.is_none() {
            return;
        }
        if inner.line_count >= MAX_LINES {
            inner.file = None;
            if let Ok(f) = open_shared(&inner.path, true) {
                let mut w = BufWriter::new(f);
                let (_, _, _, h, m, s, _) = ysc_core::util::local_time();
                let _ = writeln!(w, "=== Log Rotated at {:02}:{:02}:{:02} ===", h, m, s);
                inner.file = Some(w);
            }
            inner.line_count = 1;
        }

        let tag = if self.window_open.load(Ordering::Relaxed) {
            "DBG_OPEN"
        } else {
            "DBG_CLOSED"
        };
        let now = now_ms();
        let mut flush_now = false;
        let last_flush = inner.last_flush;
        if let Some(file) = inner.file.as_mut() {
            let _ = write!(file, "{} [{}] {}\r\n", timestamp(), tag, msg);
            if now.saturating_sub(last_flush) >= FLUSH_INTERVAL_MS {
                let _ = file.flush();
                flush_now = true;
            }
        }
        inner.line_count += 1;
        if flush_now {
            inner.last_flush = now;
        }
    }

    /// 调试窗口输出（等价 C++ `DebugLog`）：只在窗口可见时生效。
    pub fn debug_window_log(&self, msg: &str) {
        if !self.is_window_open() {
            return;
        }
        let hwnd = self.hwnd.load(Ordering::Acquire);
        if hwnd == 0 {
            return;
        }
        let line = format!("{} {}\r\n", timestamp(), msg);
        {
            let mut q = self.pending.lock().unwrap();
            // 窗口线程若一时来不及消费，限制积压，避免无界内存。
            if q.len() >= 4096 {
                q.pop_front();
            }
            q.push_back(line);
        }
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd as *mut core::ffi::c_void)),
                WM_DEBUG_LOG,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }

    /// 窗口线程取走待显示的日志行。
    pub fn drain_pending(&self) -> Vec<String> {
        let mut q = self.pending.lock().unwrap();
        q.drain(..).collect()
    }

    pub fn clear_pending(&self) {
        self.pending.lock().unwrap().clear();
    }

    #[allow(dead_code)]
    pub fn log_hex(&self, label: &str, data: &[u8]) {
        let mut line = format!(
            "{} [{}] {} ({} bytes):",
            timestamp(),
            if self.is_window_open() {
                "DBG_OPEN"
            } else {
                "DBG_CLOSED"
            },
            label,
            data.len()
        );
        let n = data.len().min(64);
        for b in &data[..n] {
            line.push_str(&format!(" {:02X}", b));
        }
        if data.len() > 64 {
            line.push_str(" ...");
        }
        self.log(&line);
    }

    fn raw(&self, text: &str) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(file) = inner.file.as_mut() {
            let _ = file.write_all(text.as_bytes());
            let _ = file.flush();
        }
    }

    pub fn shutdown(&self) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(mut file) = inner.file.take() {
            let _ = file.write_all(b"\r\n=== Log Ended ===\r\n");
            let _ = file.flush();
        }
        drop(inner);
    }
}

fn open_shared(path: &PathBuf, truncate: bool) -> std::io::Result<File> {
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(truncate)
        .share_mode(0x7) // FILE_SHARE_READ | WRITE | DELETE
        .open(path)
}

/// 安装核心层日志钩子（串口溢出等诊断写入同一文件）。
pub fn install_core_log_hook() {
    ysc_core::util::set_log_hook(std::sync::Arc::new(|msg: &str| {
        DEBUG.log(msg);
    }));
}
