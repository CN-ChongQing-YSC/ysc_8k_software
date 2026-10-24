//! `<START> <len BE u16> <payload> <END>` 帧编解码。
//!
//! 逐字节对齐 `ysc_8k_driver/serial_port.cpp`、`iap_upgrader.cpp`、
//! `towmcu_iap_upgrader.cpp`：
//!   * JSON 控制帧：total_len = 7 + 2 + json_len + 5
//!   * 二进制 IAP 帧：payload = cmd,len,[rev 4B],data,cksum LE u16
//!
//! 收帧状态机也在这里（[`FrameParser`]），供串口读线程复用。

use crate::consts::{
    END_MARKER, END_MARKER_LEN, FRAME_SZ, HEADER_LEN, START_MARKER, START_MARKER_LEN,
};

/// 构造 JSON 控制帧。JSON 过大（超出 [`FRAME_SZ`]）返回 `None`。
///
/// 对应用户态 `SerialPort::SendJsonCommand` 的防御：绝不写溢出栈缓冲。
pub fn build_json_frame(json: &str) -> Option<Vec<u8>> {
    let body = json.as_bytes();
    let total = HEADER_LEN + body.len() + END_MARKER_LEN;
    if body.is_empty() || total > FRAME_SZ {
        return None;
    }
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(START_MARKER);
    out.push((total >> 8) as u8);
    out.push((total & 0xff) as u8);
    out.extend_from_slice(body);
    out.extend_from_slice(END_MARKER);
    Some(out)
}

/// 构造 JSON 控制帧，返回 `Err(reason)`。
pub fn build_json_frame_checked(json: &str) -> Result<Vec<u8>, String> {
    build_json_frame(json).ok_or_else(|| format!("json too large ({} bytes)", json.len()))
}

/// 构造二进制 IAP 帧：
/// `<START><BE total_len><cmd><len>[rev 4B for ERASE/VERIFY]<data><LE cksum><END>`。
pub fn build_iap_frame(cmd: u8, data: &[u8]) -> Vec<u8> {
    // CMD_ERASE = 0x81, CMD_VERIFY = 0x82 带 4 字节 rev。
    let has_rev = cmd == 0x81 || cmd == 0x82;
    let mut payload = Vec::with_capacity(2 + 4 + data.len() + 2);
    payload.push(cmd);
    payload.push(data.len() as u8);
    if has_rev {
        payload.extend_from_slice(&[0, 0, 0, 0]);
    }
    payload.extend_from_slice(data);
    let mut cksum: u16 = 0;
    for &b in &payload {
        cksum = cksum.wrapping_add(b as u16);
    }
    payload.push((cksum & 0xff) as u8);
    payload.push((cksum >> 8) as u8);

    let total = START_MARKER_LEN + 2 + payload.len() + END_MARKER_LEN;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(START_MARKER);
    out.push((total >> 8) as u8);
    out.push((total & 0xff) as u8);
    out.extend_from_slice(&payload);
    out.extend_from_slice(END_MARKER);
    out
}

/// 构造任意 JSON 文本帧（`towmcu` 的 `{"cmd":132}` / `{"cmd":50}`）。
pub fn build_json_body_frame(json: &str) -> Vec<u8> {
    let body = json.as_bytes();
    let total = START_MARKER_LEN + 2 + body.len() + END_MARKER_LEN;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(START_MARKER);
    out.push((total >> 8) as u8);
    out.push((total & 0xff) as u8);
    out.extend_from_slice(body);
    out.extend_from_slice(END_MARKER);
    out
}

/// 版本查询帧 `{"cmd":132}`。
pub fn build_version_frame() -> Vec<u8> {
    build_json_frame("{\"cmd\":132}").expect("version frame fits")
}

/// 缓冲区里是否存在一条完整帧，且 payload 含 `"code":200`（容忍 `"code": 200`）。
///
/// 与 C++ `HasValidResponse` 等价。
pub fn has_ok_response(rx: &[u8]) -> bool {
    if rx.len() < 14 {
        return false;
    }
    let mut i = 0usize;
    while i + 14 <= rx.len() {
        if &rx[i..i + START_MARKER_LEN] != START_MARKER {
            i += 1;
            continue;
        }
        if i + 9 > rx.len() {
            break;
        }
        let total = ((rx[i + 7] as usize) << 8) | rx[i + 8] as usize;
        if total < 14 {
            i += 1;
            continue;
        }
        let end = i + total;
        if end > rx.len() {
            break;
        }
        if &rx[end - 5..end] != END_MARKER {
            i += 1;
            continue;
        }
        let payload = &rx[i + 9..end - 5];
        let text = String::from_utf8_lossy(payload);
        if text.contains("\"code\":200") || text.contains("\"code\": 200") {
            return true;
        }
        i = end;
    }
    false
}

/// 从缓冲区里剥出第一条完整帧的 payload（含长度前缀时剥离长度）。
///
/// 用于 SDK 的同步收发与 v2 的双帧式容忍：
///   * `<START><BE total_len><payload><END>`（IAP：total_len == content + 12）
///   * `<START><payload><END>`（APP 回应）
///
/// 返回 `(payload, consumed_bytes)`；未收全返回 `None`。
pub fn take_one_frame(buf: &[u8]) -> Option<(Vec<u8>, usize)> {
    let s = find_subslice(buf, START_MARKER)?;
    let mut e = s + START_MARKER_LEN;
    let end_rel = loop {
        if e + END_MARKER_LEN > buf.len() {
            return None;
        }
        if &buf[e..e + END_MARKER_LEN] == END_MARKER {
            break e;
        }
        e += 1;
    };
    let content_start = s + START_MARKER_LEN;
    let content_len = end_rel - content_start;
    let payload = if content_len >= 2 {
        let declared = ((buf[content_start] as usize) << 8) | buf[content_start + 1] as usize;
        if declared == content_len + 12 {
            buf[content_start + 2..end_rel].to_vec()
        } else {
            buf[content_start..end_rel].to_vec()
        }
    } else {
        buf[content_start..end_rel].to_vec()
    };
    Some((payload, end_rel + END_MARKER_LEN))
}

/// 在字节流里找子串（返回首次出现的下标）。
pub fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// 收帧状态机。与 C++ `SerialPort::ReadLoop` 的 RX 状态机逐分支一致。
#[derive(Debug)]
pub struct FrameParser {
    state: RxState,
    start_match_pos: usize,
    buf: Vec<u8>,
    expected_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RxState {
    Idle,
    Header,
    Payload,
}

impl Default for FrameParser {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameParser {
    pub fn new() -> Self {
        Self {
            state: RxState::Idle,
            start_match_pos: 0,
            buf: vec![0u8; FRAME_SZ],
            expected_len: 0,
        }
    }

    /// 清空状态（重连/切换波特率后调用）。
    pub fn reset(&mut self) {
        self.state = RxState::Idle;
        self.start_match_pos = 0;
        self.expected_len = 0;
    }

    /// 喂一段字节，把每个完整 payload 追加到 `out`。
    pub fn feed(&mut self, data: &[u8], out: &mut Vec<Vec<u8>>) {
        for &b in data {
            match self.state {
                RxState::Idle => {
                    if b == START_MARKER[self.start_match_pos] {
                        self.start_match_pos += 1;
                        if self.start_match_pos == START_MARKER_LEN {
                            self.start_match_pos = 0;
                            self.state = RxState::Header;
                            self.buf.clear();
                        }
                    } else if self.start_match_pos > 0 {
                        self.start_match_pos = if b == b'<' { 1 } else { 0 };
                    }
                }
                RxState::Header => {
                    if self.buf.len() < 2 {
                        self.buf.push(b);
                    }
                    if self.buf.len() == 2 {
                        self.expected_len = ((self.buf[0] as usize) << 8) | self.buf[1] as usize;
                        let payload_len = self.expected_len as isize
                            - HEADER_LEN as isize
                            - END_MARKER_LEN as isize;
                        if payload_len < 0 || payload_len as usize > FRAME_SZ - END_MARKER_LEN {
                            self.state = RxState::Idle;
                            self.buf.clear();
                        } else {
                            self.buf.clear();
                            self.state = RxState::Payload;
                        }
                    }
                }
                RxState::Payload => {
                    self.buf.push(b);
                    let payload_len = self.expected_len - HEADER_LEN - END_MARKER_LEN;
                    if self.buf.len() == payload_len + END_MARKER_LEN {
                        let end_ok = &self.buf[payload_len..] == END_MARKER;
                        if end_ok && payload_len > 0 {
                            out.push(self.buf[..payload_len].to_vec());
                        }
                        self.state = RxState::Idle;
                        self.buf.clear();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_frame_matches_cpp() {
        let f = build_json_frame("{\"cmd\":132}").unwrap();
        assert_eq!(&f[..7], b"<START>");
        assert_eq!(&f[7..9], &[0, 25]); // 14 + 11 = 25
        assert_eq!(&f[9..20], b"{\"cmd\":132}");
        assert_eq!(&f[20..], b"<END>");
        assert_eq!(f.len(), 25);
    }

    #[test]
    fn iap_frames_match_cpp() {
        let erase = build_iap_frame(0x81, &[]);
        // total = 7+2+ (cmd,len,rev4,cksum2) +5 = 18? payload=8 -> 7+2+8+5=22
        assert_eq!(erase.len(), 22);
        // cmd=0x81, len=0, rev=00000000, cksum=0x0081 LE -> 81 00
        assert_eq!(&erase[9..17], &[0x81, 0x00, 0, 0, 0, 0, 0x81, 0x00]);

        let data = [0xAAu8; 60];
        let prom = build_iap_frame(0x80, &data);
        assert_eq!(&prom[9..11], &[0x80, 0x3C]);
        // 校验和 = 0x80 + 0x3c + 60*0xaa
        let sum: u16 = 0x80u16 + 0x3c + 60u16 * 0xaa;
        assert_eq!(
            &prom[prom.len() - 7..prom.len() - 5],
            &[(sum & 0xff) as u8, (sum >> 8) as u8]
        );
    }

    #[test]
    fn parser_handles_split_and_noise() {
        let mut p = FrameParser::new();
        let mut out = Vec::new();
        let frame = build_json_frame("{\"code\":200,\"message\":null}").unwrap();
        let mut noisy = b"junk<<STA".to_vec();
        noisy.extend_from_slice(&frame);
        for chunk in noisy.chunks(3) {
            p.feed(chunk, &mut out);
        }
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], b"{\"code\":200,\"message\":null}");
    }

    #[test]
    fn take_one_frame_both_styles() {
        let iap = build_iap_frame(0x80, &[1, 2, 3]);
        let (p, n) = take_one_frame(&iap).unwrap();
        assert_eq!(n, iap.len());
        assert_eq!(&p[..2], &[0x80, 0x03]);

        let app = b"<START>{\"code\":200}<END>".to_vec();
        let (p2, n2) = take_one_frame(&app).unwrap();
        assert_eq!(n2, app.len());
        assert_eq!(p2, b"{\"code\":200}");
    }
}
