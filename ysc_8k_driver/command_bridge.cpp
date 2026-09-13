#include "command_bridge.h"
#include "serial_port.h"
#include "device_context.h"
#include "main.h"
#include "debug_logger.h"
#include <cstdio>
#include <cstring>
#include <string>

SerialPort *CommandBridge::s_serial = nullptr;

void CommandBridge::Init(SerialPort *serial) {
    s_serial = serial;
}

/* 按"命令属于哪台设备"解析目标串口。优先级：
 *   1) DeviceContext 里按角色匹配且已连接的设备 —— 双设备场景的正解
 *      （鼠标命令发鼠标机、键盘命令发键盘机，调用方无需知道设备存在）；
 *   2) DeviceContext 中第一个已连接设备 —— 单设备/角色未知时的兜底；
 *   3) s_serial 遗留单例 —— 尚未走 DeviceContext 的旧路径。
 * 这样单设备用法行为完全不变，双设备时自动分流。 */
SerialPort *CommandBridge::Resolve(DeviceRole role) {
    SerialPort *sp = DeviceContext::SerialForRole((int)role);
    if (sp && sp->IsConnected()) return sp;
    if (s_serial && s_serial->IsConnected()) return s_serial;
    return nullptr;
}

bool CommandBridge::SendMouseMove(int32_t x, int32_t y, int steps, DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    char json[128];
    snprintf(json, sizeof(json), "{\"cmd\":30,\"x\":%d,\"y\":%d,\"c\":%d}",
             (int)x, (int)y, steps);
    DebugLog("TX serial: %s", json);
    DebugLogger::Log("TX serial: %s", json);
    return sp->SendJsonCommand(json);
}

bool CommandBridge::SendMoveTow(int32_t x, int32_t y, int steps, DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    char json[128];
    snprintf(json, sizeof(json), "{\"cmd\":31,\"x\":%d,\"y\":%d,\"c\":%d}",
             (int)x, (int)y, steps);
    DebugLog("TX serial: %s", json);
    DebugLogger::Log("TX serial: %s", json);
    return sp->SendJsonCommand(json);
}

bool CommandBridge::SendMouseButton(uint8_t button_mask, bool pressed, DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    char json[64];
    snprintf(json, sizeof(json), "{\"cmd\":33,\"b\":%u,\"s\":%u}",
             (unsigned)button_mask, pressed ? 1 : 0);
    DebugLog("TX serial: %s", json);
    DebugLogger::Log("TX serial: %s", json);
    return sp->SendJsonCommand(json);
}

bool CommandBridge::SendMouseWheel(int32_t wheel) {
    // Firmware has no dedicated wheel command; acknowledge silently
    (void)wheel;
    return true;
}

bool CommandBridge::SendKeyboard(uint8_t ctrl, const uint8_t keys[10]) {
    /* kmboxnet 的 CMD_KEYBOARD_ALL 是"整份键盘状态"语义（ctrl 修饰位图 + 10 个
     * 键码槽），而固件 cmd 45 是"单键按下/释放"语义。这里做状态差分：
     * 与上一次下发的集合比较，掉出的键发 down=0、新增的键发 down=1。
     *
     * 用函数内静态缓存保存上一次集合：DriveKmboxnet 的键盘命令在 UDP 收包线程
     * 里串行处理，不会有并发调用。 */
    uint8_t cur[18];      // 8 个修饰键 + 10 个普通键
    int curN = 0;
    for (int bit = 0; bit < 8; bit++)
        if (ctrl & (1u << bit)) cur[curN++] = (uint8_t)(0xE0 + bit);
    for (int i = 0; i < 10; i++)
        if (keys[i] != 0) cur[curN++] = keys[i];

    static uint8_t prev[18];
    static int prevN = 0;

    bool anyOk = true;
    for (int i = 0; i < prevN; i++) {
        bool still = false;
        for (int j = 0; j < curN; j++) if (cur[j] == prev[i]) { still = true; break; }
        if (!still && !SendKeyboardKey(prev[i], false, (DeviceRole)DEV_ROLE_KBD)) anyOk = false;
    }
    for (int i = 0; i < curN; i++) {
        bool had = false;
        for (int j = 0; j < prevN; j++) if (prev[j] == cur[i]) { had = true; break; }
        if (!had && !SendKeyboardKey(cur[i], true, (DeviceRole)DEV_ROLE_KBD)) anyOk = false;
    }

    memcpy(prev, cur, sizeof(cur));
    prevN = curN;
    return anyOk;
}

bool CommandBridge::SendKeyboardKey(uint8_t kc, bool down, DeviceRole __role) {
    /* Firmware cmd 45: inject one keyboard press/release on the impersonated
     * device's real keyboard endpoint. kc = HID keycode (0x04-0xE7; modifiers
     * 0xE0-0xE7). Device replies 404 no_keyboard if it has no keyboard iface. */
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    char json[64];
    snprintf(json, sizeof(json), "{\"cmd\":45,\"kc\":%u,\"down\":%u}",
             (unsigned)kc, down ? 1 : 0);
    DebugLog("TX serial: %s", json);
    DebugLogger::Log("TX serial: %s", json);
    return sp->SendJsonCommand(json);
}

bool CommandBridge::SendKeyboardReleaseAll(DeviceRole __role) {
    /* Firmware cmd 46: release every injected key. */
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    DebugLog("TX serial: {\"cmd\":46}");
    DebugLogger::Log("TX serial: {\"cmd\":46}");
    return sp->SendJsonCommand("{\"cmd\":46}");
}

bool CommandBridge::SendKeyboardTypeString(const char *s, DeviceRole __role) {
    /* Firmware cmd 47: type a mixed-case ASCII string char-by-char. The firmware
     * owns case (CapsLock-aware via the HID LED Output report) and auto-presses
     * LeftShift per char. s must be non-empty and <= 128 bytes (KBD_TYPE_MAX on
     * the device). The string is JSON-escaped before framing. */
    SerialPort *sp = Resolve(__role); if (!sp || !s) return false;
    size_t n = strlen(s);
    if (n == 0 || n > 128) return false;
    std::string esc;
    esc.reserve(n + 8);
    for (const char *p = s; *p; ++p) {
        unsigned char c = (unsigned char)*p;
        switch (c) {
            case '"':  esc += "\\\""; break;
            case '\\': esc += "\\\\"; break;
            case '\n': esc += "\\n"; break;
            case '\r': esc += "\\r"; break;
            case '\t': esc += "\\t"; break;
            default:
                if (c < 0x20) { char b[8]; snprintf(b, sizeof(b), "\\u%04x", (unsigned)c); esc += b; }
                else esc += (char)c;
        }
    }
    std::string json = "{\"cmd\":47,\"s\":\"" + esc + "\"}";
    DebugLog("TX serial: %s", json.c_str());
    DebugLogger::Log("TX serial: %s", json.c_str());
    return sp->SendJsonCommand(json.c_str());
}

bool CommandBridge::SendUploadStatus(bool enable, DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    char json[64];
    snprintf(json, sizeof(json), "{\"cmd\":34,\"status\":%d}", enable ? 1 : 0);
    DebugLog("TX serial: %s", json);
    DebugLogger::Log("TX serial: %s", json);
    return sp->SendJsonCommand(json);
}

bool CommandBridge::SendJumpIAP(DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    return sp->SendJsonCommand("{\"cmd\":50}");
}

bool CommandBridge::SendRawYsc(const char *json, DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp || !json) return false;
    DebugLog("TX YSC: %s", json);
    DebugLogger::Log("TX YSC: %s", json);
    return sp->SendJsonCommand(json);
}

bool CommandBridge::SendRawMakcu(const char *text, DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp || !text) return false;
    DebugLog("TX MAKCU: %s", text);
    DebugLogger::Log("TX MAKCU: %s", text);
    int len = (int)strlen(text);
    return sp->SendRaw((const uint8_t *)text, len);
}

/* ---- Gamepad↔mouse mapping config (firmware cmd 100..104) ----
 * Firmware parses + persists + applies; driver just forwards. */
bool CommandBridge::SendGamepadConfig(const char *json, DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp || !json) return false;
    char buf[4096];
    int n = snprintf(buf, sizeof(buf), "{\"cmd\":100,\"data\":%s}", json);
    if (n <= 0 || n >= (int)sizeof(buf)) return false;
    DebugLog("TX GMAP set: %s", buf);
    DebugLogger::Log("TX GMAP set: %s", buf);
    return sp->SendJsonCommand(buf);
}
bool CommandBridge::SendGetGamepadConfig(DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    DebugLog("TX GMAP get");
    return sp->SendJsonCommand("{\"cmd\":101}");
}
bool CommandBridge::SendGamepadEnable(bool on, DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    char json[64];
    snprintf(json, sizeof(json), "{\"cmd\":102,\"on\":%d}", on ? 1 : 0);
    DebugLog("TX GMAP enable: %s", json);
    DebugLogger::Log("TX GMAP enable: %s", json);
    return sp->SendJsonCommand(json);
}
bool CommandBridge::SendGamepadReset(DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    DebugLog("TX GMAP reset");
    return sp->SendJsonCommand("{\"cmd\":104}");
}

/* ---- Mouse interpolation (firmware cmd 51/52/53) ---- */
bool CommandBridge::SendInterpSet(int enable, int profile, int windowMs, int maxWindowMs, DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    char json[96];
    snprintf(json, sizeof(json), "{\"cmd\":51,\"e\":%d,\"p\":%d,\"w\":%d,\"mx\":%d}",
             enable ? 1 : 0, profile, windowMs, maxWindowMs);
    DebugLog("TX interp set: %s", json);
    DebugLogger::Log("TX interp set: %s", json);
    return sp->SendJsonCommand(json);
}
bool CommandBridge::SendInterpGet(DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    DebugLog("TX interp get");
    return sp->SendJsonCommand("{\"cmd\":52}");
}
bool CommandBridge::SendInterpReset(DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    DebugLog("TX interp reset");
    return sp->SendJsonCommand("{\"cmd\":53}");
}

/* ---- Debug capture commands ----
 * Command codes must stay in sync with firmware (USART1Dma.h).
 *   700 enter / 701 exit / 702 status
 *   710 dev_descr / 711 cfg_descr / 712 report_descr(offset,max)
 *   713 report(idx) / 714 clear / 715 device_info
 */
bool CommandBridge::DebugEnter(DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    return sp->SendJsonCommand("{\"cmd\":700}");
}
bool CommandBridge::DebugExit(DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    return sp->SendJsonCommand("{\"cmd\":701}");
}
bool CommandBridge::DebugStatus(DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    return sp->SendJsonCommand("{\"cmd\":702}");
}
bool CommandBridge::DebugGetDevDescr(DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    return sp->SendJsonCommand("{\"cmd\":710}");
}
bool CommandBridge::DebugGetCfgDescr(DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    return sp->SendJsonCommand("{\"cmd\":711}");
}
bool CommandBridge::DebugGetReportDescr(uint16_t offset, uint16_t maxBytes, DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    char json[80];
    snprintf(json, sizeof(json), "{\"cmd\":712,\"offset\":%u,\"max\":%u}",
             (unsigned)offset, (unsigned)maxBytes);
    return sp->SendJsonCommand(json);
}
bool CommandBridge::DebugGetReport(uint8_t idx, DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    char json[64];
    snprintf(json, sizeof(json), "{\"cmd\":713,\"idx\":%u}", (unsigned)idx);
    return sp->SendJsonCommand(json);
}
bool CommandBridge::DebugClearReports(DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    return sp->SendJsonCommand("{\"cmd\":714}");
}
bool CommandBridge::DebugGetDeviceInfo(DeviceRole __role) {
    SerialPort *sp = Resolve(__role); if (!sp) return false;
    return sp->SendJsonCommand("{\"cmd\":715}");
}
