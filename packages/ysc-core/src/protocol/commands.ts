/**
 * 固件命令码。对应 ysc_8k_driver/command_bridge.cpp 与 main.cpp 的 cmd 路由。
 * JSON 控制帧形如 {"cmd":N,...}；IAP 二进制帧的 cmd 字节见 IAP_CMD_*。
 */

// ── 鼠标 ──
export const CMD_MOUSE_MOVE = 30;          // 绝对移动 {"cmd":30,"x","y","c"}
export const CMD_MOUSE_MOVE_TOW = 31;      // 相对移动
export const CMD_MOUSE_BUTTON = 33;        // 按键 {"cmd":33,"b","s"}
export const CMD_UPLOAD_STATUS = 34;       // 上报开关 {"cmd":34,"status":0/1}

// ── 宏 / 抖动 / 鼠标曲线（set/get/reset 三联） ──
export const CMD_MACRO_SET = 36;
export const CMD_MACRO_GET = 37;
export const CMD_MACRO_RESET = 38;
export const CMD_JITTER_SET = 39;
export const CMD_JITTER_GET = 40;
export const CMD_JITTER_RESET = 41;
export const CMD_MOUSE_CURVE_SET = 42;
export const CMD_MOUSE_CURVE_GET = 43;
export const CMD_MOUSE_CURVE_RESET = 44;

// ── 键盘 ──
export const CMD_KEYBOARD_KEY = 45;             // {"cmd":45,"kc":HID,"down":0/1}
export const CMD_KEYBOARD_RELEASE_ALL = 46;     // {"cmd":46}
export const CMD_KEYBOARD_TYPE_STRING = 47;     // {"cmd":47,"s":"..."} (≤128)
export const CMD_KEYBOARD_TYPE_STATUS = 48;     // SDK 用：输入状态查询

// ── IAP 跳转 ──
export const CMD_JUMP_IAP = 50;                 // {"cmd":50}

// ── 手柄映射 ──
export const CMD_GAMEPAD_SET = 100;             // {"cmd":100,"data":{...}}
export const CMD_GAMEPAD_GET = 101;             // {"cmd":101}
export const CMD_GAMEPAD_ENABLE = 102;          // {"cmd":102,"on":..}
export const CMD_GAMEPAD_PLUG_EVENT = 103;      // 设备异步上报：插拔事件
export const CMD_GAMEPAD_RESET = 104;           // {"cmd":104}

// ── 版本 / 波特率 ──
export const CMD_QUERY_VERSION = 132;           // {"cmd":132}
export const CMD_SWITCH_BAUDRATE = 133;         // {"cmd":133,"baud":N}

// ── 调试 / HID 抓包（DebugPanel） ──
export const CMD_DEBUG_ENTER = 700;
export const CMD_DEBUG_EXIT = 701;
export const CMD_DEBUG_STATUS = 702;
export const CMD_DEBUG_GET_DEV_DESCR = 710;
export const CMD_DEBUG_GET_CFG_DESCR = 711;
export const CMD_DEBUG_GET_REP_DESCR = 712;
export const CMD_DEBUG_GET_REPORT = 713;
export const CMD_DEBUG_CLEAR_REPORTS = 714;
export const CMD_DEBUG_GET_DEVICE_INFO = 715;

// ── IAP 二进制帧 cmd 字节（对应 iap_upgrader.cpp:13） ──
export const IAP_CMD_PROM = 0x80;     // 编程分片握手
export const IAP_CMD_ERASE = 0x81;    // 擦除（带 4 字节 rev）
export const IAP_CMD_VERIFY = 0x82;   // 校验分片（带 4 字节 rev）
export const IAP_CMD_END = 0x83;      // 结束（无响应）
export const IAP_CMD_GETVER = 0x84;   // IAP 版本（亦作 JSON cmd 132 用于 APP 模式）
