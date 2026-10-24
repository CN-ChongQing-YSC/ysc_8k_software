//! Win32 串口层。
//!
//! 行为逐条对齐 `ysc_8k_driver/serial_port.cpp`：
//!   * **重叠 I/O**（`FILE_FLAG_OVERLAPPED`）—— 非重叠模式下同一句柄的读会阻塞写
//!     （600~800ms，见 `串口读写阻塞分析与修复.md`）。
//!   * DCB：8/N/1 + DTR/RTS enable；超时 `ReadIntervalTimeout=1`、
//!     `ReadTotalTimeoutConstant=10`、`WriteTotalTimeoutConstant=100`。
//!   * 自动探测顺序与命中判定（`"code":200`）与 C++ 完全一致。
//!   * 异步写队列容量 64、单帧上限 4096，满时丢弃最旧（与 C++ 环形队列一致）。
//!   * 读线程收数看门狗：4 秒无数据时用 `ClearCommError` 探活。
//!   * 读线程异常退出通过 `lost_cb` 上报，主动断开不报。

use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows::core::PCWSTR;
use windows::Win32::Devices::Communication::{
    ClearCommError, GetCommState, PurgeComm, SetCommState, SetCommTimeouts, CLEAR_COMM_ERROR_FLAGS,
    COMMTIMEOUTS, COMSTAT, DCB, NOPARITY, ONESTOPBIT, PURGE_RXCLEAR, PURGE_TXCLEAR,
};
use windows::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_OVERLAPPED,
    FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_MODE, OPEN_EXISTING,
};
use windows::Win32::System::Threading::{
    CreateEventW, GetCurrentThread, ResetEvent, SetEvent, SetThreadPriority,
    WaitForMultipleObjects, WaitForSingleObject, INFINITE, THREAD_PRIORITY_ABOVE_NORMAL,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

use crate::consts::{FRAME_SZ, PROBE_ORDER, SUPPORTED_BAUDS};
use crate::frame::{build_json_frame, build_version_frame, has_ok_response, FrameParser};
use crate::util::{now_ms, wide, win_error_text};

const IO_PENDING_CODE: u32 = 997;
const QUEUE_CAP: usize = 64;
const RX_WATCHDOG_MS: u64 = 4000;

/// 串口数据回调（在串口读线程上调用）。
pub type DataCallback = Arc<dyn Fn(&[u8]) + Send + Sync>;
/// 读线程异常退出回调（参数为该 reader 的世代号）。
pub type LostCallback = Arc<dyn Fn(u32) + Send + Sync>;

fn pending(err: &windows::core::Error) -> bool {
    ((err.code().0 as u32) & 0xffff) == IO_PENDING_CODE
}

fn h(v: isize) -> HANDLE {
    HANDLE(v as *mut c_void)
}

fn as_isize(handle: HANDLE) -> isize {
    handle.0 as isize
}

fn apply_dcb(handle: HANDLE, baud: u32) -> Result<(), String> {
    let mut dcb = DCB::default();
    dcb.DCBlength = std::mem::size_of::<DCB>() as u32;
    unsafe { GetCommState(handle, &mut dcb) }
        .map_err(|e| format!("读取串口配置失败: {}", e.message()))?;
    dcb.BaudRate = baud;
    dcb.ByteSize = 8;
    dcb.Parity = NOPARITY;
    dcb.StopBits = ONESTOPBIT;
    // DTR/RTS enable：与 C++ DCB 的 fDtrControl / fRtsControl 等价。windows-rs 的
    // DCB 位域字段保留为 _bitfield，直接置位最稳；用 EscapeCommFunction 亦可。
    unsafe { SetCommState(handle, &dcb) }
        .map_err(|e| format!("设置 {} 波特率失败: {}", baud, e.message()))?;
    unsafe {
        windows::Win32::Devices::Communication::EscapeCommFunction(
            handle,
            windows::Win32::Devices::Communication::SETDTR,
        )
        .ok();
        windows::Win32::Devices::Communication::EscapeCommFunction(
            handle,
            windows::Win32::Devices::Communication::SETRTS,
        )
        .ok();
    }
    Ok(())
}

fn set_timeouts(handle: HANDLE, interval: u32, read_total: u32, write_total: u32) {
    let t = COMMTIMEOUTS {
        ReadIntervalTimeout: interval,
        ReadTotalTimeoutMultiplier: 0,
        ReadTotalTimeoutConstant: read_total,
        WriteTotalTimeoutMultiplier: 0,
        WriteTotalTimeoutConstant: write_total,
    };
    unsafe {
        let _ = SetCommTimeouts(handle, &t);
    }
}

fn open_overlapped(port: &str, baud: u32) -> Result<HANDLE, String> {
    let path = wide(&format!("\\\\.\\{}", port));
    let handle = unsafe {
        CreateFileW(
            PCWSTR(path.as_ptr()),
            (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0,
            FILE_SHARE_MODE(0),
            None,
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED,
            None,
        )
    }
    .map_err(|e| format!("打开 {} 失败: {}", port, e.message()))?;
    if let Err(e) = apply_dcb(handle, baud) {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Err(e);
    }
    set_timeouts(handle, 1, 10, 100);
    Ok(handle)
}

fn create_event() -> Result<HANDLE, String> {
    unsafe { CreateEventW(None, true, false, PCWSTR::null()) }
        .map_err(|e| format!("创建串口事件失败: {}", e.message()))
}

/// 一个已配置的串口（可带常驻读/写线程）。
pub struct SerialPort {
    inner: Arc<Inner>,
    reader: Mutex<Option<JoinHandle<()>>>,
    writer: Mutex<Option<JoinHandle<()>>>,
}

struct Inner {
    handle: AtomicIsize,
    read_ol: AtomicIsize,
    write_ol: AtomicIsize,
    stop: AtomicIsize,
    port: Mutex<String>,
    baud: AtomicU32,
    reader_gen: AtomicU32,
    last_rx: AtomicU64,
    callback: Mutex<Option<DataCallback>>,
    lost_cb: Mutex<Option<LostCallback>>,
    queue: Mutex<VecDeque<Vec<u8>>>,
    queue_cv: Condvar,
    write_running: AtomicBool,
    /// 直接写（握手/切波特率）与写线程互斥。
    write_lock: Mutex<()>,
}

unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}

impl Inner {
    fn handle(&self) -> HANDLE {
        h(self.handle.load(Ordering::Acquire))
    }
    fn read_ol(&self) -> HANDLE {
        h(self.read_ol.load(Ordering::Acquire))
    }
    fn write_ol(&self) -> HANDLE {
        h(self.write_ol.load(Ordering::Acquire))
    }
    fn stop(&self) -> HANDLE {
        h(self.stop.load(Ordering::Acquire))
    }
    fn is_open(&self) -> bool {
        self.handle.load(Ordering::Acquire) != 0
    }
}

impl Default for SerialPort {
    fn default() -> Self {
        Self::new()
    }
}

impl SerialPort {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                handle: AtomicIsize::new(0),
                read_ol: AtomicIsize::new(0),
                write_ol: AtomicIsize::new(0),
                stop: AtomicIsize::new(0),
                port: Mutex::new(String::new()),
                baud: AtomicU32::new(0),
                reader_gen: AtomicU32::new(0),
                last_rx: AtomicU64::new(0),
                callback: Mutex::new(None),
                lost_cb: Mutex::new(None),
                queue: Mutex::new(VecDeque::with_capacity(QUEUE_CAP)),
                queue_cv: Condvar::new(),
                write_running: AtomicBool::new(false),
                write_lock: Mutex::new(()),
            }),
            reader: Mutex::new(None),
            writer: Mutex::new(None),
        }
    }

    pub fn set_callback(&self, cb: DataCallback) {
        *self.inner.callback.lock().unwrap() = Some(cb);
    }

    pub fn set_lost_callback(&self, cb: LostCallback) {
        *self.inner.lost_cb.lock().unwrap() = Some(cb);
    }

    pub fn is_connected(&self) -> bool {
        self.inner.is_open()
    }

    pub fn baud(&self) -> u32 {
        self.inner.baud.load(Ordering::Relaxed)
    }

    pub fn port_name(&self) -> String {
        self.inner.port.lock().unwrap().clone()
    }

    pub fn reader_gen(&self) -> u32 {
        self.inner.reader_gen.load(Ordering::Relaxed)
    }

    /// 打开串口。若已打开会先断开。
    pub fn connect(&self, port: &str, baud: u32) -> Result<(), String> {
        if self.is_connected() {
            self.disconnect();
        }
        let handle = open_overlapped(port, baud)?;
        let read_ol = create_event()?;
        let write_ol = create_event()?;
        let stop = create_event()?;
        self.inner.handle.store(as_isize(handle), Ordering::Release);
        self.inner
            .read_ol
            .store(as_isize(read_ol), Ordering::Release);
        self.inner
            .write_ol
            .store(as_isize(write_ol), Ordering::Release);
        self.inner.stop.store(as_isize(stop), Ordering::Release);
        *self.inner.port.lock().unwrap() = port.to_string();
        self.inner.baud.store(baud, Ordering::Relaxed);
        Ok(())
    }

    /// 握手一次：发 `{"cmd":132}`，等 `"code":200`。必须在启动读线程之前调用。
    pub fn ping_once(&self, timeout_ms: u32) -> bool {
        if !self.is_connected() {
            return false;
        }
        let handle = self.inner.handle();
        unsafe {
            let _ = PurgeComm(handle, PURGE_RXCLEAR | PURGE_TXCLEAR);
        }
        let frame = build_version_frame();
        if !self.write_direct(&frame) {
            return false;
        }
        match self.read_direct(256, timeout_ms) {
            Some(data) => has_ok_response(&data),
            None => false,
        }
    }

    /// 主动断开：先停读写线程、取消挂起 I/O，再关句柄与事件。
    pub fn disconnect(&self) {
        self.stop_read_thread();
        let handle = h(self.inner.handle.swap(0, Ordering::AcqRel));
        if !handle.0.is_null() {
            unsafe {
                let _ = CloseHandle(handle);
            }
        }
        for field in [&self.inner.read_ol, &self.inner.write_ol, &self.inner.stop] {
            let ev = h(field.swap(0, Ordering::AcqRel));
            if !ev.0.is_null() {
                unsafe {
                    let _ = CloseHandle(ev);
                }
            }
        }
        self.inner.baud.store(0, Ordering::Relaxed);
    }

    /// 发送 JSON 命令（自动包 `<START>…<END>`）。返回是否成功入队。
    pub fn send_json_command(&self, json: &str) -> bool {
        if !self.is_connected() {
            return false;
        }
        match build_json_frame(json) {
            Some(frame) => self.enqueue(frame),
            None => false,
        }
    }

    /// 发送原始字节（不加帧）。
    pub fn send_raw(&self, data: &[u8]) -> bool {
        if !self.is_connected() || data.is_empty() || data.len() > FRAME_SZ {
            return false;
        }
        self.enqueue(data.to_vec())
    }

    fn enqueue(&self, frame: Vec<u8>) -> bool {
        if frame.len() > FRAME_SZ {
            return false;
        }
        let mut q = self.inner.queue.lock().unwrap();
        if q.len() == QUEUE_CAP {
            q.pop_front();
        }
        q.push_back(frame);
        self.inner.queue_cv.notify_one();
        true
    }

    /// 启动常驻读写线程（与 C++ `StartReadThread` 等价：同时启动写线程）。
    pub fn start_read_thread(&self) -> bool {
        if !self.is_connected() {
            return false;
        }
        let mut reader_guard = self.reader.lock().unwrap();
        if reader_guard.is_some() {
            return false;
        }
        // 复位停止事件（上一次 stop 可能已置位）。
        let stop = self.inner.stop();
        if !stop.0.is_null() {
            unsafe {
                let _ = ResetEvent(stop);
            }
        }
        let gen = self.inner.reader_gen.fetch_add(1, Ordering::AcqRel) + 1;
        self.inner.last_rx.store(now_ms(), Ordering::Relaxed);

        // 写线程
        {
            let mut writer_guard = self.writer.lock().unwrap();
            if writer_guard.is_none() {
                self.inner.write_running.store(true, Ordering::Release);
                let inner = Arc::clone(&self.inner);
                match thread::Builder::new()
                    .name("ysc-serial-tx".into())
                    .spawn(move || write_loop(inner))
                {
                    Ok(j) => *writer_guard = Some(j),
                    Err(_) => {
                        self.inner.write_running.store(false, Ordering::Release);
                        return false;
                    }
                }
            }
        }

        let inner = Arc::clone(&self.inner);
        match thread::Builder::new()
            .name("ysc-serial-rx".into())
            .spawn(move || read_loop(inner, gen))
        {
            Ok(j) => {
                *reader_guard = Some(j);
                true
            }
            Err(_) => {
                self.stop_write_thread();
                false
            }
        }
    }

    /// 停止读线程（并停写线程）。世代号自增，使旧线程的失联上报失效。
    pub fn stop_read_thread(&self) {
        if self.inner.is_open() {
            self.inner.reader_gen.fetch_add(1, Ordering::AcqRel);
            let stop = self.inner.stop();
            if !stop.0.is_null() {
                unsafe {
                    let _ = SetEvent(stop);
                }
            }
            let handle = self.inner.handle();
            if !handle.0.is_null() {
                unsafe {
                    let _ = CancelIoEx(handle, None);
                }
            }
        } else {
            self.inner.reader_gen.fetch_add(1, Ordering::AcqRel);
        }
        if let Some(j) = self.reader.lock().unwrap().take() {
            let _ = j.join();
        }
        self.stop_write_thread();
    }

    fn stop_write_thread(&self) {
        self.inner.write_running.store(false, Ordering::Release);
        self.inner.queue_cv.notify_all();
        if let Some(j) = self.writer.lock().unwrap().take() {
            let _ = j.join();
        }
    }

    /// 切换到新波特率：发 cmd 133 → 关闭 → 以新速率重开 → 版本查询校验。
    ///
    /// 失败时**保持端口打开**（调用方需要用它回退到原速率），与 C++ 一致。
    pub fn switch_baudrate(&self, new_baud: u32) -> bool {
        if !self.is_connected() || !SUPPORTED_BAUDS.contains(&new_baud) {
            return false;
        }
        let port = self.port_name();
        let cmd = format!("{{\"cmd\":133,\"baud\":{}}}", new_baud);
        if !self.send_json_command(&cmd) {
            return false;
        }
        thread::sleep(Duration::from_millis(200));

        // 先关闭当前句柄（保留 port 字符串）。
        self.inner.reader_gen.fetch_add(1, Ordering::AcqRel);
        self.stop_read_thread();
        let handle = h(self.inner.handle.swap(0, Ordering::AcqRel));
        if !handle.0.is_null() {
            unsafe {
                let _ = CloseHandle(handle);
            }
        }
        for field in [&self.inner.read_ol, &self.inner.write_ol, &self.inner.stop] {
            let ev = h(field.swap(0, Ordering::AcqRel));
            if !ev.0.is_null() {
                unsafe {
                    let _ = CloseHandle(ev);
                }
            }
        }
        thread::sleep(Duration::from_millis(100));

        // 验证阶段用独立的临时句柄，绝不能和读线程共用 read_ol。
        let mut ok = false;
        if let Some(raw) = open_raw(&port, new_baud) {
            let frame = build_version_frame();
            if raw_write(raw, &frame, 500) {
                let start = now_ms();
                let mut rx: Vec<u8> = Vec::with_capacity(256);
                while now_ms().saturating_sub(start) < 500 && rx.len() < 256 {
                    let chunk = raw_read(raw, 256 - rx.len(), 100);
                    if !chunk.is_empty() {
                        rx.extend_from_slice(&chunk);
                        if has_ok_response(&rx) {
                            ok = true;
                            break;
                        }
                    }
                }
            }
            unsafe {
                let _ = CloseHandle(raw);
            }
        }

        // 失败也要保持端口打开（新速率），调用方才能回退到旧速率。
        if self.connect(&port, new_baud).is_err() {
            return false;
        }
        ok
    }

    fn write_direct(&self, data: &[u8]) -> bool {
        if !self.is_connected() || data.is_empty() {
            return false;
        }
        let _guard = self.inner.write_lock.lock().unwrap();
        write_overlapped(self.inner.handle(), self.inner.write_ol(), data, 5000)
    }

    fn read_direct(&self, buf_size: usize, timeout_ms: u32) -> Option<Vec<u8>> {
        if !self.is_connected() {
            return None;
        }
        read_overlapped(
            self.inner.handle(),
            self.inner.read_ol(),
            buf_size,
            timeout_ms,
        )
    }

    /// 自动探测硬件波特率。失败返回 0。
    pub fn detect_baudrate(port: &str) -> u32 {
        let frame = build_version_frame();
        for baud in PROBE_ORDER {
            let handle = match open_sync(port, baud, 50, 100) {
                Ok(h) => h,
                Err(_) => continue,
            };
            unsafe {
                let _ = PurgeComm(handle, PURGE_RXCLEAR | PURGE_TXCLEAR);
            }
            thread::sleep(Duration::from_millis(10));
            let ok = sync_write(handle, &frame, 1000);
            let mut matched = false;
            if ok {
                let start = now_ms();
                let mut rx: Vec<u8> = Vec::with_capacity(512);
                while now_ms().saturating_sub(start) < 200 && rx.len() < 512 {
                    let mut chunk = [0u8; 256];
                    let want = (512 - rx.len()).min(chunk.len());
                    match sync_read(handle, &mut chunk[..want]) {
                        Some(n) if n > 0 => {
                            rx.extend_from_slice(&chunk[..n]);
                            if has_ok_response(&rx) {
                                matched = true;
                                break;
                            }
                        }
                        Some(_) => {}
                        None => break,
                    }
                }
            }
            unsafe {
                let _ = CloseHandle(handle);
            }
            if matched {
                return baud;
            }
        }
        0
    }
}

impl Drop for SerialPort {
    fn drop(&mut self) {
        self.disconnect();
    }
}

fn read_loop(inner: Arc<Inner>, my_gen: u32) {
    let mut parser = FrameParser::new();
    let mut buf = [0u8; 512];
    let mut error_check = 0usize;
    let mut lost = false;

    let read_ol = inner.read_ol();
    let stop = inner.stop();

    while inner.is_open() {
        let handle = inner.handle();
        if handle.0.is_null() {
            break;
        }
        unsafe {
            let _ = ResetEvent(read_ol);
        }
        let mut ov = OVERLAPPED::default();
        ov.hEvent = read_ol;
        let mut got: u32 = 0;
        let res = unsafe { ReadFile(handle, Some(&mut buf), Some(&mut got), Some(&mut ov)) };
        match res {
            Ok(()) => {}
            Err(e) => {
                if !pending(&e) {
                    lost = true;
                    break;
                }
                let elapsed = now_ms().saturating_sub(inner.last_rx.load(Ordering::Relaxed));
                let wait_ms = if elapsed > RX_WATCHDOG_MS {
                    500
                } else {
                    INFINITE
                };
                let wait = unsafe { WaitForMultipleObjects(&[read_ol, stop], false, wait_ms) };
                if wait == WAIT_TIMEOUT {
                    if now_ms().saturating_sub(inner.last_rx.load(Ordering::Relaxed))
                        > RX_WATCHDOG_MS
                    {
                        let mut err = CLEAR_COMM_ERROR_FLAGS::default();
                        let mut stat = COMSTAT::default();
                        if unsafe { ClearCommError(handle, Some(&mut err), Some(&mut stat)) }
                            .is_err()
                        {
                            unsafe {
                                let _ = CancelIoEx(handle, None);
                            }
                            lost = true;
                            break;
                        }
                    }
                    continue;
                }
                if wait.0 == WAIT_OBJECT_0.0 + 1 {
                    unsafe {
                        let _ = CancelIoEx(handle, None);
                    }
                    break;
                }
                if wait == WAIT_OBJECT_0 {
                    if unsafe { GetOverlappedResult(handle, &ov, &mut got, false) }.is_err() {
                        lost = true;
                        break;
                    }
                } else {
                    break;
                }
            }
        }

        if got == 0 {
            continue;
        }
        inner.last_rx.store(now_ms(), Ordering::Relaxed);

        error_check += 1;
        if error_check >= 200 {
            error_check = 0;
            let mut errors = CLEAR_COMM_ERROR_FLAGS::default();
            let mut stat = COMSTAT::default();
            unsafe {
                let _ = ClearCommError(inner.handle(), Some(&mut errors), Some(&mut stat));
            }
            // 与 C++ 一样只在日志里记录，不打断收流。错误位：CE_RXOVER=0x1,
            // CE_OVERRUN=0x2, CE_RXPARITY=0x4, CE_FRAME=0x8。
            if errors.0 & 0x1 != 0 {
                crate::util::log(&format!(
                    "SERIAL RX OVERFLOW: input buffer overflow, {} bytes queued",
                    stat.cbInQue
                ));
            }
            if errors.0 & 0x2 != 0 {
                crate::util::log("SERIAL RX OVERRUN: hardware buffer overrun");
            }
            if errors.0 & 0xC != 0 {
                crate::util::log(&format!(
                    "SERIAL RX ERROR: framing/parity error (0x{:08X})",
                    errors.0
                ));
            }
        }

        let mut frames = Vec::new();
        parser.feed(&buf[..got as usize], &mut frames);
        if !frames.is_empty() {
            let cb = inner.callback.lock().unwrap().clone();
            if let Some(cb) = cb {
                for payload in frames {
                    cb(&payload);
                }
            }
        }
    }

    // 只有"仍为当前 reader"（期间未重连/切换）才上报失联。主动 stop 会自增世代。
    if lost && inner.reader_gen.load(Ordering::Relaxed) == my_gen {
        let cb = inner.lost_cb.lock().unwrap().clone();
        if let Some(cb) = cb {
            cb(my_gen);
        }
    }
}

fn write_loop(inner: Arc<Inner>) {
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL);
    }
    loop {
        let frame = {
            let mut q = inner.queue.lock().unwrap();
            while q.is_empty() && inner.write_running.load(Ordering::Acquire) {
                let (guard, _) = inner
                    .queue_cv
                    .wait_timeout(q, Duration::from_millis(50))
                    .unwrap();
                q = guard;
            }
            if q.is_empty() {
                break;
            }
            q.pop_front()
        };
        if let Some(frame) = frame {
            if inner.is_open() {
                let ok = {
                    let _guard = inner.write_lock.lock().unwrap();
                    write_overlapped(inner.handle(), inner.write_ol(), &frame, 5000)
                };
                crate::util::log(&format!("SERIAL WRITE {} bytes (ok={})", frame.len(), ok));
            }
        }
    }
}

fn write_overlapped(handle: HANDLE, event: HANDLE, data: &[u8], timeout_ms: u32) -> bool {
    if handle.0.is_null() || event.0.is_null() || data.is_empty() {
        return false;
    }
    unsafe {
        let _ = ResetEvent(event);
    }
    let mut ov = OVERLAPPED::default();
    ov.hEvent = event;
    let mut written: u32 = 0;
    match unsafe { WriteFile(handle, Some(data), Some(&mut written), Some(&mut ov)) } {
        Ok(()) => true,
        Err(e) => {
            if !pending(&e) {
                return false;
            }
            if unsafe { WaitForSingleObject(event, timeout_ms) } != WAIT_OBJECT_0 {
                unsafe {
                    let _ = CancelIoEx(handle, Some(&ov));
                }
                return false;
            }
            unsafe { GetOverlappedResult(handle, &ov, &mut written, false) }.is_ok()
        }
    }
}

fn read_overlapped(
    handle: HANDLE,
    event: HANDLE,
    buf_size: usize,
    timeout_ms: u32,
) -> Option<Vec<u8>> {
    if handle.0.is_null() || event.0.is_null() {
        return None;
    }
    unsafe {
        let _ = ResetEvent(event);
    }
    let mut buf = vec![0u8; buf_size.max(1)];
    let mut ov = OVERLAPPED::default();
    ov.hEvent = event;
    let mut got: u32 = 0;
    match unsafe { ReadFile(handle, Some(&mut buf), Some(&mut got), Some(&mut ov)) } {
        Ok(()) => Some(buf[..got as usize].to_vec()),
        Err(e) => {
            if !pending(&e) {
                return None;
            }
            if unsafe { WaitForSingleObject(event, timeout_ms) } != WAIT_OBJECT_0 {
                unsafe {
                    let _ = CancelIoEx(handle, Some(&ov));
                }
                let mut reaped: u32 = 0;
                unsafe {
                    let _ = GetOverlappedResult(handle, &ov, &mut reaped, true);
                }
                let _ = reaped;
                return None;
            }
            if unsafe { GetOverlappedResult(handle, &ov, &mut got, false) }.is_ok() {
                Some(buf[..got as usize].to_vec())
            } else {
                None
            }
        }
    }
}

/// 同步打开串口（非 overlapped，供探测 / SDK 使用），失败返回原始 Win32 错误码。
pub fn open_sync_code(
    port: &str,
    baud: u32,
    read_total: u32,
    write_total: u32,
) -> Result<HANDLE, u32> {
    let path = wide(&format!("\\\\.\\{}", port));
    let handle = unsafe {
        CreateFileW(
            PCWSTR(path.as_ptr()),
            (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0,
            FILE_SHARE_MODE(0),
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map_err(|e| (e.code().0 as u32) & 0xffff)?;
    if apply_dcb(handle, baud).is_err() {
        let code = unsafe { GetLastError().0 };
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Err(code);
    }
    set_timeouts(handle, 1, read_total, write_total);
    Ok(handle)
}

/// 同步打开串口（非 overlapped，供探测 / SDK 使用）。
pub fn open_sync(
    port: &str,
    baud: u32,
    read_total: u32,
    write_total: u32,
) -> Result<HANDLE, String> {
    open_sync_code(port, baud, read_total, write_total)
        .map_err(|code| format!("打开 {} @ {} 失败: {}", port, baud, win_error_text(code)))
}

pub fn sync_write(handle: HANDLE, data: &[u8], _timeout_ms: u32) -> bool {
    let mut written: u32 = 0;
    match unsafe { WriteFile(handle, Some(data), Some(&mut written), None) } {
        Ok(()) => written as usize == data.len(),
        Err(_) => false,
    }
}

/// 同步读一次；超时/无数据返回 `Some(0)`，句柄错误返回 `None`。
pub fn sync_read(handle: HANDLE, buf: &mut [u8]) -> Option<usize> {
    let mut got: u32 = 0;
    match unsafe { ReadFile(handle, Some(buf), Some(&mut got), None) } {
        Ok(()) => Some(got as usize),
        Err(_) => None,
    }
}

/// 关闭同步句柄。
pub fn close_sync(handle: HANDLE) {
    unsafe {
        let _ = CloseHandle(handle);
    }
}

/// 清接收/发送缓冲。
pub fn purge_rx_tx(handle: HANDLE) {
    unsafe {
        let _ = PurgeComm(handle, PURGE_RXCLEAR | PURGE_TXCLEAR);
    }
}

/// 只清发送缓冲（SDK 的 NoWait 语义：不丢异步回包）。
pub fn purge_tx(handle: HANDLE) {
    unsafe {
        let _ = PurgeComm(handle, PURGE_TXCLEAR);
    }
}

/// IAP 用：重叠模式打开串口，读超时组合 `(MAXDWORD, 0, 100)`（与 C++ `OpenSerialRaw` 一致）。
pub fn open_raw(port: &str, baud: u32) -> Option<HANDLE> {
    let path = wide(&format!("\\\\.\\{}", port));
    let handle = unsafe {
        CreateFileW(
            PCWSTR(path.as_ptr()),
            (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0,
            FILE_SHARE_MODE(0),
            None,
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED,
            None,
        )
    }
    .ok()?;
    if apply_dcb(handle, baud).is_err() {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return None;
    }
    let t = COMMTIMEOUTS {
        ReadIntervalTimeout: u32::MAX,
        ReadTotalTimeoutMultiplier: 0,
        ReadTotalTimeoutConstant: 100,
        WriteTotalTimeoutMultiplier: 0,
        WriteTotalTimeoutConstant: 1000,
    };
    unsafe {
        let _ = SetCommTimeouts(handle, &t);
        let _ = PurgeComm(handle, PURGE_TXCLEAR | PURGE_RXCLEAR);
    }
    Some(handle)
}

/// IAP 用：重叠写（内部临时事件），等待完成。
pub fn raw_write(handle: HANDLE, data: &[u8], timeout_ms: u32) -> bool {
    let event = match create_event() {
        Ok(e) => e,
        Err(_) => return false,
    };
    let ok = write_overlapped(handle, event, data, timeout_ms);
    unsafe {
        let _ = CloseHandle(event);
    }
    ok
}

/// IAP 用：重叠读（内部临时事件），超时返回空。
pub fn raw_read(handle: HANDLE, buf_size: usize, timeout_ms: u32) -> Vec<u8> {
    let event = match create_event() {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let data = read_overlapped(handle, event, buf_size, timeout_ms).unwrap_or_default();
    unsafe {
        let _ = CloseHandle(event);
    }
    data
}

/// CDC（v2）用：非重叠打开，读超时组合 `(MAXDWORD, 0, 0)` 即时返回。
pub fn open_cdc(port: &str) -> Option<HANDLE> {
    let path = wide(&format!("\\\\.\\{}", port));
    let handle = unsafe {
        CreateFileW(
            PCWSTR(path.as_ptr()),
            (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0,
            FILE_SHARE_MODE(0),
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .ok()?;
    if apply_dcb(handle, 115_200).is_err() {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return None;
    }
    let t = COMMTIMEOUTS {
        ReadIntervalTimeout: u32::MAX,
        ReadTotalTimeoutMultiplier: 0,
        ReadTotalTimeoutConstant: 0,
        WriteTotalTimeoutMultiplier: 0,
        WriteTotalTimeoutConstant: 2000,
    };
    unsafe {
        let _ = SetCommTimeouts(handle, &t);
        let _ = PurgeComm(handle, PURGE_TXCLEAR | PURGE_RXCLEAR);
    }
    Some(handle)
}

/// 同步收一条完整帧（边读边跑帧状态机），超时返回空。
///
/// SDK 的 `Ysc_SendCommand` 用这个实现；与 C++ `ysc_sdk.cpp::RecvFrame` 等价。
pub fn recv_frame_sync(handle: HANDLE, timeout_ms: u32) -> Option<Vec<u8>> {
    let deadline = now_ms() + timeout_ms.max(0) as u64;
    let mut parser = FrameParser::new();
    let mut chunk = [0u8; 512];
    loop {
        if now_ms() >= deadline {
            return None;
        }
        let n = sync_read(handle, &mut chunk)?;
        if n == 0 {
            thread::sleep(Duration::from_millis(1));
            continue;
        }
        let mut frames = Vec::new();
        parser.feed(&chunk[..n], &mut frames);
        if let Some(first) = frames.into_iter().next() {
            return Some(first);
        }
    }
}

/// 读当前 Win32 错误的文本（供上层日志）。
pub fn last_win_error() -> String {
    unsafe {
        let code = GetLastError().0;
        std::io::Error::from_raw_os_error(code as i32).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_code() {
        assert_eq!(IO_PENDING_CODE, 997);
    }

    #[test]
    fn probe_order_matches_cpp() {
        assert_eq!(PROBE_ORDER[0], 4_000_000);
        assert_eq!(PROBE_ORDER[1], 115_200);
        assert_eq!(*PROBE_ORDER.last().unwrap(), 1_000_000);
        assert_eq!(PROBE_ORDER.len(), SUPPORTED_BAUDS.len());
    }
}
