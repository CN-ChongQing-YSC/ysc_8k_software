//! ysc_core — YSC 8K 驱动公共核心。
//!
//! 本 crate 只包含与 UI/宿主无关的底层能力：
//!   * [`frame`]   — `<START>…<END>` 帧编解码与接收状态机（与 C++ 驱动逐字节一致）
//!   * [`serial`]  — Win32 重叠（overlapped）串口 I/O、波特率探测/切换、异步读写线程
//!   * [`enumerate`] — 注册表 COM 口枚举、SetupAPI CDC 枚举、WCH/CH34x 驱动健康检测
//!   * [`family`]  — USB-CDC 固件族（V2 towmcu / V3 YSC-V3）识别
//!   * [`consts`]  — 设备角色、波特率表、帧常量
//!   * [`util`]    — UTF-16/UTF-8、错误码、时间、hex 等小工具

pub mod consts;
pub mod enumerate;
pub mod family;
pub mod frame;
pub mod serial;
pub mod util;
