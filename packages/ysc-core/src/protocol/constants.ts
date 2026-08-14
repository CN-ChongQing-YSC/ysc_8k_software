/**
 * 协议常量。逐字节对应 ysc_8k_driver/serial_port.h:108 与 iap_upgrader.cpp:12。
 */
const enc = new TextEncoder();

/** 帧起始标记 "<START>"（7 字节）。 */
export const START_MARKER = '<START>';
/** 帧结束标记 "<END>"（5 字节）。 */
export const END_MARKER = '<END>';

export const START_MARKER_LEN = 7;
export const END_MARKER_LEN = 5;
/** 头长度 = <START>(7) + 2 字节 BE 总长 = 9。 */
export const HEADER_LEN = START_MARKER_LEN + 2;

/** 缓存的 marker 字节，避免每次编码。 */
export const START_BYTES: Readonly<Uint8Array> = enc.encode(START_MARKER);
export const END_BYTES: Readonly<Uint8Array> = enc.encode(END_MARKER);

/** IAP 编程/校验分片大小（字节）。 */
export const CHUNK_SZ = 60;

/** IAP 自动波特率探测表（与 serial_port.cpp / SDK 一致）。 */
export const IAP_BAUDS = [
  115200, 230400, 460800, 921600,
  1000000, 1500000, 2000000, 3000000, 4000000,
] as const;

/** IAP 帧容错（对应 iap_upgrader.cpp:31）。 */
export const FRAME_RETRIES = 3;
/** VERIFY 单帧超时（不写 Flash，500ms 足够）。 */
export const FRAME_TIMEOUT_FAST_MS = 500;
/** ERASE / PROGRAM 单帧超时（Flash 擦写会让设备短暂无响应）。 */
export const FRAME_TIMEOUT_SLOW_MS = 3000;
/** 重试前给设备的喘息时间。 */
export const RETRY_BREATHE_MS = 100;

/** YSC 设备 USB 标识（CDC 串口）。 */
export const YSC_USB_VENDOR = 0x1a86;
export const YSC_USB_PRODUCT = 0xfe0c;

/** 接收缓冲上限（payload 最大字节数，防御性）。 */
export const MAX_PAYLOAD = 4096;
