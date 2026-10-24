//! 命名管道服务 `\\.\pipe\ysc_8k_driver`。
//!
//! 与 `pipe_server.cpp` 一致：单客户端、字节流、逐行 JSON、重叠 I/O。
//! 客户端断开后重新创建实例等待下一次连接；**不调用** `FlushFileBuffers`
//! （客户端被杀时它会永久阻塞，C++ 侧踩过坑）。

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    CloseHandle, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, HANDLE, WAIT_OBJECT_0,
};
use windows::Win32::Storage::FileSystem::{
    ReadFile, WriteFile, FILE_FLAG_OVERLAPPED, PIPE_ACCESS_DUPLEX,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
    PIPE_WAIT,
};
use windows::Win32::System::Threading::{
    CreateEventW, ResetEvent, SetEvent, WaitForMultipleObjects, WaitForSingleObject, INFINITE,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

use ysc_core::util::{build_event_line, wide};
use ysc_host::HostSink;

pub const PIPE_NAME: &str = "\\\\.\\pipe\\ysc_8k_driver";
pub const PIPE_BUF_SIZE: usize = 4096;

fn h(v: isize) -> HANDLE {
    HANDLE(v as *mut c_void)
}

fn pending(err: &windows::core::Error) -> bool {
    ((err.code().0 as u32) & 0xffff) == ERROR_IO_PENDING.0
}

type CommandCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// 命名管道服务。
pub struct PipeServer {
    inner: Arc<Inner>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

struct Inner {
    running: AtomicBool,
    has_client: AtomicBool,
    handle: AtomicIsize,
    stop: AtomicIsize,
    write_lock: Mutex<()>,
    callback: Mutex<Option<CommandCallback>>,
}

unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}

impl PipeServer {
    pub fn new() -> Self {
        let stop = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }
            .expect("创建管道停止事件失败");
        Self {
            inner: Arc::new(Inner {
                running: AtomicBool::new(false),
                has_client: AtomicBool::new(false),
                handle: AtomicIsize::new(0),
                stop: AtomicIsize::new(stop.0 as isize),
                write_lock: Mutex::new(()),
                callback: Mutex::new(None),
            }),
            thread: Mutex::new(None),
        }
    }

    pub fn set_command_callback(&self, cb: CommandCallback) {
        *self.inner.callback.lock().unwrap() = Some(cb);
    }

    pub fn has_client(&self) -> bool {
        self.inner.has_client.load(Ordering::Acquire)
    }

    pub fn start(&self) {
        if self.inner.running.swap(true, Ordering::AcqRel) {
            return;
        }
        let inner = Arc::clone(&self.inner);
        let handle = thread::Builder::new()
            .name("ysc-pipe-server".into())
            .spawn(move || server_loop(inner));
        if let Ok(j) = handle {
            *self.thread.lock().unwrap() = Some(j);
        }
    }

    pub fn stop(&self) {
        self.inner.running.store(false, Ordering::Release);
        let stop = h(self.inner.stop.load(Ordering::Acquire));
        if !stop.0.is_null() {
            unsafe {
                let _ = SetEvent(stop);
            }
        }
        let pipe = h(self.inner.handle.load(Ordering::Acquire));
        if !pipe.0.is_null() {
            unsafe {
                let _ = CancelIoEx(pipe, None);
            }
        }
        if let Some(j) = self.thread.lock().unwrap().take() {
            let _ = j.join();
        }
    }

    /// 向 Electron 发一行事件。`body` 是不含外层大括号的 JSON 片段。
    pub fn send_event(&self, event_type: &str, body: Option<&str>) {
        if !self.has_client() {
            return;
        }
        let line = build_event_line(event_type, body);
        if line.len() >= PIPE_BUF_SIZE {
            return;
        }
        let _ = self.write_all(line.as_bytes());
    }

    fn write_all(&self, data: &[u8]) -> bool {
        let _guard = self.inner.write_lock.lock().unwrap();
        let pipe = h(self.inner.handle.load(Ordering::Acquire));
        if pipe.0.is_null() {
            return false;
        }
        let event = match unsafe { CreateEventW(None, true, false, PCWSTR::null()) } {
            Ok(e) => e,
            Err(_) => return false,
        };
        let ok = {
            unsafe {
                let _ = ResetEvent(event);
            }
            let mut ov = OVERLAPPED::default();
            ov.hEvent = event;
            let mut written: u32 = 0;
            let res = unsafe { WriteFile(pipe, Some(data), Some(&mut written), Some(&mut ov)) };
            match res {
                Ok(()) => true,
                Err(e) => {
                    if !pending(&e) {
                        ysc_core::util::log(&format!("PIPE write error code=0x{:08X}", e.code().0));
                        false
                    } else if unsafe { WaitForSingleObject(event, 1000) } != WAIT_OBJECT_0 {
                        unsafe {
                            let _ = CancelIoEx(pipe, Some(&ov));
                        }
                        false
                    } else {
                        unsafe { GetOverlappedResult(pipe, &ov, &mut written, false) }.is_ok()
                    }
                }
            }
        };
        unsafe {
            let _ = CloseHandle(event);
        }
        ok
    }
}

impl Default for PipeServer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PipeServer {
    fn drop(&mut self) {
        self.stop();
        let stop = h(self.inner.stop.swap(0, Ordering::AcqRel));
        if !stop.0.is_null() {
            unsafe {
                let _ = CloseHandle(stop);
            }
        }
    }
}

fn server_loop(inner: Arc<Inner>) {
    let name = wide(PIPE_NAME);
    let stop = h(inner.stop.load(Ordering::Acquire));

    while inner.running.load(Ordering::Acquire) {
        let pipe = unsafe {
            CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                1,
                PIPE_BUF_SIZE as u32,
                PIPE_BUF_SIZE as u32,
                0,
                None,
            )
        };
        if pipe.is_invalid() {
            thread::sleep(std::time::Duration::from_millis(1000));
            continue;
        }

        let conn_event = match unsafe { CreateEventW(None, true, false, PCWSTR::null()) } {
            Ok(e) => e,
            Err(_) => {
                unsafe {
                    let _ = CloseHandle(pipe);
                }
                continue;
            }
        };

        let mut ov = OVERLAPPED::default();
        ov.hEvent = conn_event;
        let res = unsafe { ConnectNamedPipe(pipe, Some(&mut ov)) };
        let mut proceed = res.is_ok();
        if !proceed {
            let code = res
                .as_ref()
                .err()
                .map(|e| (e.code().0 as u32) & 0xffff)
                .unwrap_or(0);
            if code == (ERROR_PIPE_CONNECTED.0 & 0xffff) {
                proceed = true;
            } else {
                let wait = unsafe { WaitForMultipleObjects(&[conn_event, stop], false, INFINITE) };
                if wait == WAIT_OBJECT_0 {
                    proceed = true;
                } else if wait.0 == WAIT_OBJECT_0.0 + 1 {
                    unsafe {
                        let _ = CloseHandle(conn_event);
                        let _ = CloseHandle(pipe);
                    }
                    break;
                }
            }
        }
        unsafe {
            let _ = CloseHandle(conn_event);
        }
        if !proceed {
            unsafe {
                let _ = CloseHandle(pipe);
            }
            continue;
        }
        if !inner.running.load(Ordering::Acquire) {
            unsafe {
                let _ = CloseHandle(pipe);
            }
            break;
        }

        inner.handle.store(pipe.0 as isize, Ordering::Release);
        inner.has_client.store(true, Ordering::Release);

        read_client(&inner, pipe, stop);

        inner.has_client.store(false, Ordering::Release);
        {
            // 与写线程互斥：先让 handle 归零，再关闭，避免写线程拿到已关闭的句柄。
            let _g = inner.write_lock.lock().unwrap();
            inner.handle.store(0, Ordering::Release);
        }
        unsafe {
            let _ = DisconnectNamedPipe(pipe);
            let _ = CloseHandle(pipe);
        }
    }
}

fn read_client(inner: &Arc<Inner>, pipe: HANDLE, stop: HANDLE) {
    let mut buf = [0u8; PIPE_BUF_SIZE];
    let mut line: Vec<u8> = Vec::with_capacity(512);

    while inner.running.load(Ordering::Acquire) {
        let event = match unsafe { CreateEventW(None, true, false, PCWSTR::null()) } {
            Ok(e) => e,
            Err(_) => break,
        };
        unsafe {
            let _ = ResetEvent(event);
        }
        let mut ov = OVERLAPPED::default();
        ov.hEvent = event;
        let mut got: u32 = 0;
        let res = unsafe {
            ReadFile(
                pipe,
                Some(&mut buf[..PIPE_BUF_SIZE - 1]),
                Some(&mut got),
                Some(&mut ov),
            )
        };
        let mut disconnected = false;
        match res {
            Ok(()) => {}
            Err(e) => {
                if !pending(&e) {
                    ysc_core::util::log(&format!("PIPE read error code=0x{:08X}", e.code().0));
                    disconnected = true;
                } else {
                    let wait = unsafe { WaitForMultipleObjects(&[event, stop], false, INFINITE) };
                    if wait == WAIT_OBJECT_0 {
                        if unsafe { GetOverlappedResult(pipe, &ov, &mut got, false) }.is_err() {
                            disconnected = true;
                        }
                    } else {
                        unsafe {
                            let _ = CancelIoEx(pipe, Some(&ov));
                        }
                        disconnected = true;
                    }
                }
            }
        }
        unsafe {
            let _ = CloseHandle(event);
        }
        if disconnected {
            ysc_core::util::log("PIPE read disconnected");
            break;
        }
        if got == 0 {
            ysc_core::util::log("PIPE read returned 0 bytes");
            break;
        }

        for &b in &buf[..got as usize] {
            if b == b'\n' {
                if !line.is_empty() {
                    let text = String::from_utf8_lossy(&line).to_string();
                    line.clear();
                    if !text.trim().is_empty() {
                        let cb = inner.callback.lock().unwrap().clone();
                        if let Some(cb) = cb {
                            cb(text.trim_end_matches('\r'));
                        }
                    }
                }
            } else {
                line.push(b);
                if line.len() > 1024 * 1024 {
                    line.clear();
                }
            }
        }
    }
}

/// 旧管道宿主的 `HostSink`：事件写回命名管道。
pub struct PipeSink {
    pipe: Arc<PipeServer>,
}

impl PipeSink {
    pub fn new(pipe: Arc<PipeServer>) -> Self {
        Self { pipe }
    }
}

impl HostSink for PipeSink {
    fn emit(&self, event_type: &str, body: Option<&str>) {
        self.pipe.send_event(event_type, body);
    }

    fn ui_attached(&self) -> bool {
        self.pipe.has_client()
    }
}
