//! 本机 IPv4 查询（显示用）。
//!
//! 严格对齐 C++ `GetLocalIP()`：
//!   gethostname → getaddrinfo(AF_INET, SOCK_DGRAM) → 第一个非回环 IPv4；
//!   失败/全回环时退回 127.0.0.1（或第一条结果）。
//!
//! 不用 GetAdaptersAddresses：它的适配器顺序与 getaddrinfo 不同，会把 Hyper-V
//! 虚拟网卡排在前面，导致 NET 面板显示的地址与参照驱动不一致。

use std::ffi::CString;
use std::net::Ipv4Addr;

use windows::core::PCSTR;
use windows::Win32::Networking::WinSock::{
    freeaddrinfo, getaddrinfo, gethostname, ADDRINFOA, AF_INET, SOCKADDR_IN, SOCK_DGRAM,
};

pub fn first_non_loopback_ipv4() -> Option<String> {
    let mut host = [0u8; 256];
    if unsafe { gethostname(&mut host) } != 0 {
        return None;
    }
    let end = host.iter().position(|&c| c == 0).unwrap_or(host.len());
    let name = match CString::new(&host[..end]) {
        Ok(s) => s,
        Err(_) => return None,
    };

    let mut hints = ADDRINFOA::default();
    hints.ai_family = AF_INET.0 as i32;
    hints.ai_socktype = SOCK_DGRAM.0;
    let mut res: *mut ADDRINFOA = std::ptr::null_mut();
    let rc = unsafe {
        getaddrinfo(
            PCSTR(name.as_ptr() as *const u8),
            PCSTR::null(),
            Some(&hints),
            &mut res,
        )
    };
    if rc != 0 || res.is_null() {
        return None;
    }

    let mut first: Option<String> = None;
    let mut chosen: Option<String> = None;
    let mut p = res;
    unsafe {
        while !p.is_null() {
            let ai = &*p;
            if !ai.ai_addr.is_null() {
                let sa = &*(ai.ai_addr as *const SOCKADDR_IN);
                if sa.sin_family == AF_INET {
                    let raw = sa.sin_addr.S_un.S_addr;
                    let ip = Ipv4Addr::from(raw.to_ne_bytes()).to_string();
                    if first.is_none() {
                        first = Some(ip.clone());
                    }
                    if !Ipv4Addr::from(raw.to_ne_bytes()).is_loopback() {
                        chosen = Some(ip);
                        break;
                    }
                }
            }
            p = ai.ai_next;
        }
        freeaddrinfo(Some(res));
    }
    chosen.or(first)
}
