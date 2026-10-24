//! 协议常量。数值必须与 C++ 驱动 / 固件保持一致，改一侧要同步另一侧。

/// 固件支持的串口档位（升序，与 main.h `SUPPORTED_BAUDS` 一致）。
pub const SUPPORTED_BAUDS: [u32; 9] = [
    115200, 230400, 460800, 921600, 1000000, 1500000, 2000000, 3000000, 4000000,
];

/// 自动探测顺序：从两端向中间收敛（4M → 115200 → 3M → 230400 → …）。
///
/// 与 C++ `SerialPort::DetectBaudrate` 的循环生成顺序完全一致：
/// `right--, left++, right--, left++, ...`
pub const PROBE_ORDER: [u32; 9] = [
    4_000_000, 115_200, 3_000_000, 230_400, 2_000_000, 460_800, 1_500_000, 921_600, 1_000_000,
];

/// 帧头/帧尾。
pub const START_MARKER: &[u8; 7] = b"<START>";
pub const END_MARKER: &[u8; 5] = b"<END>";
pub const START_MARKER_LEN: usize = 7;
pub const END_MARKER_LEN: usize = 5;
/// `<START>` + 2 字节大端总长度。
pub const HEADER_LEN: usize = START_MARKER_LEN + 2;
/// 帧缓冲上限（C++ `SerialPort::FRAME_SZ`）。手柄映射 24 条规则约 1.9KB。
pub const FRAME_SZ: usize = 4096;
/// 固件升级单块大小。
pub const IAP_CHUNK_SZ: usize = 60;
/// 默认 kmboxnet UDP 端口。
pub const KMBOXNET_DEFAULT_PORT: u16 = 5251;
/// kmboxnet 默认设备标识。
pub const DEFAULT_MAC: u32 = 0x0000_0001;

/// 设备角色。与 C++ `DeviceRole` 数值一致。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    None = 0,
    Mouse = 1,
    Kbd = 2,
}

impl Role {
    pub fn as_i32(self) -> i32 {
        self as i32
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Mouse => "mouse",
            Role::Kbd => "kbd",
            Role::None => "none",
        }
    }

    pub fn from_i32(v: i32) -> Role {
        match v {
            1 => Role::Mouse,
            2 => Role::Kbd,
            _ => Role::None,
        }
    }

    pub fn from_str(s: &str) -> Role {
        match s {
            "mouse" => Role::Mouse,
            "kbd" => Role::Kbd,
            _ => Role::None,
        }
    }
}

/// 驱动自报版本（发版时与 VERSION / package.json 同步）。
///
/// 编译期读取仓库根目录 `VERSION` 文件：`ysc_8k_driver_rs/../../VERSION`。
/// 这样版本号只有一处真源，不再需要像 C++ 那样手工改 main.cpp。
pub const DRIVER_VERSION: &str = include_str!("../../../../VERSION");

/// 返回去掉首尾空白的版本字符串。
pub fn driver_version() -> &'static str {
    DRIVER_VERSION.trim()
}

/// 是否在支持列表内。
pub fn is_supported_baud(baud: u32) -> bool {
    SUPPORTED_BAUDS.contains(&baud)
}
