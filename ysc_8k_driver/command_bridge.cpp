#include "command_bridge.h"
#include "serial_port.h"
#include "main.h"
#include "debug_logger.h"
#include <cstdio>

SerialPort *CommandBridge::s_serial = nullptr;

void CommandBridge::Init(SerialPort *serial) {
    s_serial = serial;
}

bool CommandBridge::SendMouseMove(int32_t x, int32_t y, int steps) {
    if (!s_serial) return false;
    char json[128];
    snprintf(json, sizeof(json), "{\"cmd\":30,\"x\":%d,\"y\":%d,\"c\":%d}",
             (int)x, (int)y, steps);
    DebugLog("TX serial: %s", json);
    DebugLogger::Log("TX serial: %s", json);
    return s_serial->SendJsonCommand(json);
}

bool CommandBridge::SendMoveTow(int32_t x, int32_t y, int steps) {
    if (!s_serial) return false;
    char json[128];
    snprintf(json, sizeof(json), "{\"cmd\":31,\"x\":%d,\"y\":%d,\"c\":%d}",
             (int)x, (int)y, steps);
    DebugLog("TX serial: %s", json);
    DebugLogger::Log("TX serial: %s", json);
    return s_serial->SendJsonCommand(json);
}

bool CommandBridge::SendMouseButton(uint8_t button_mask, bool pressed) {
    if (!s_serial) return false;
    char json[64];
    snprintf(json, sizeof(json), "{\"cmd\":33,\"b\":%u,\"s\":%u}",
             (unsigned)button_mask, pressed ? 1 : 0);
    DebugLog("TX serial: %s", json);
    DebugLogger::Log("TX serial: %s", json);
    return s_serial->SendJsonCommand(json);
}

bool CommandBridge::SendMouseWheel(int32_t wheel) {
    // Firmware has no dedicated wheel command; acknowledge silently
    (void)wheel;
    return true;
}

bool CommandBridge::SendKeyboard(uint8_t ctrl, const uint8_t keys[10]) {
    // Firmware has no keyboard serial command; acknowledge silently
    (void)ctrl; (void)keys;
    return true;
}

bool CommandBridge::SendKeyboardKey(uint8_t kc, bool down) {
    /* Firmware cmd 45: inject one keyboard press/release on the impersonated
     * device's real keyboard endpoint. kc = HID keycode (0x04-0xE7; modifiers
     * 0xE0-0xE7). Device replies 404 no_keyboard if it has no keyboard iface. */
    if (!s_serial) return false;
    char json[64];
    snprintf(json, sizeof(json), "{\"cmd\":45,\"kc\":%u,\"down\":%u}",
             (unsigned)kc, down ? 1 : 0);
    DebugLog("TX serial: %s", json);
    DebugLogger::Log("TX serial: %s", json);
    return s_serial->SendJsonCommand(json);
}

bool CommandBridge::SendKeyboardReleaseAll() {
    /* Firmware cmd 46: release every injected key. */
    if (!s_serial) return false;
    DebugLog("TX serial: {\"cmd\":46}");
    DebugLogger::Log("TX serial: {\"cmd\":46}");
    return s_serial->SendJsonCommand("{\"cmd\":46}");
}

bool CommandBridge::SendUploadStatus(bool enable) {
    if (!s_serial) return false;
    char json[64];
    snprintf(json, sizeof(json), "{\"cmd\":34,\"status\":%d}", enable ? 1 : 0);
    DebugLog("TX serial: %s", json);
    DebugLogger::Log("TX serial: %s", json);
    return s_serial->SendJsonCommand(json);
}

bool CommandBridge::SendJumpIAP() {
    if (!s_serial) return false;
    return s_serial->SendJsonCommand("{\"cmd\":50}");
}

bool CommandBridge::SendRawYsc(const char *json) {
    if (!s_serial || !json) return false;
    DebugLog("TX YSC: %s", json);
    DebugLogger::Log("TX YSC: %s", json);
    return s_serial->SendJsonCommand(json);
}

bool CommandBridge::SendRawMakcu(const char *text) {
    if (!s_serial || !text) return false;
    DebugLog("TX MAKCU: %s", text);
    DebugLogger::Log("TX MAKCU: %s", text);
    int len = (int)strlen(text);
    return s_serial->SendRaw((const uint8_t *)text, len);
}

/* ---- Gamepad↔mouse mapping config (firmware cmd 100..104) ----
 * Firmware parses + persists + applies; driver just forwards. */
bool CommandBridge::SendGamepadConfig(const char *json) {
    if (!s_serial || !json) return false;
    char buf[4096];
    int n = snprintf(buf, sizeof(buf), "{\"cmd\":100,\"data\":%s}", json);
    if (n <= 0 || n >= (int)sizeof(buf)) return false;
    DebugLog("TX GMAP set: %s", buf);
    DebugLogger::Log("TX GMAP set: %s", buf);
    return s_serial->SendJsonCommand(buf);
}
bool CommandBridge::SendGetGamepadConfig() {
    if (!s_serial) return false;
    DebugLog("TX GMAP get");
    return s_serial->SendJsonCommand("{\"cmd\":101}");
}
bool CommandBridge::SendGamepadEnable(bool on) {
    if (!s_serial) return false;
    char json[64];
    snprintf(json, sizeof(json), "{\"cmd\":102,\"on\":%d}", on ? 1 : 0);
    DebugLog("TX GMAP enable: %s", json);
    DebugLogger::Log("TX GMAP enable: %s", json);
    return s_serial->SendJsonCommand(json);
}
bool CommandBridge::SendGamepadReset() {
    if (!s_serial) return false;
    DebugLog("TX GMAP reset");
    return s_serial->SendJsonCommand("{\"cmd\":104}");
}

/* ---- Debug capture commands ----
 * Command codes must stay in sync with firmware (USART1Dma.h).
 *   700 enter / 701 exit / 702 status
 *   710 dev_descr / 711 cfg_descr / 712 report_descr(offset,max)
 *   713 report(idx) / 714 clear / 715 device_info
 */
bool CommandBridge::DebugEnter() {
    if (!s_serial) return false;
    return s_serial->SendJsonCommand("{\"cmd\":700}");
}
bool CommandBridge::DebugExit() {
    if (!s_serial) return false;
    return s_serial->SendJsonCommand("{\"cmd\":701}");
}
bool CommandBridge::DebugStatus() {
    if (!s_serial) return false;
    return s_serial->SendJsonCommand("{\"cmd\":702}");
}
bool CommandBridge::DebugGetDevDescr() {
    if (!s_serial) return false;
    return s_serial->SendJsonCommand("{\"cmd\":710}");
}
bool CommandBridge::DebugGetCfgDescr() {
    if (!s_serial) return false;
    return s_serial->SendJsonCommand("{\"cmd\":711}");
}
bool CommandBridge::DebugGetReportDescr(uint16_t offset, uint16_t maxBytes) {
    if (!s_serial) return false;
    char json[80];
    snprintf(json, sizeof(json), "{\"cmd\":712,\"offset\":%u,\"max\":%u}",
             (unsigned)offset, (unsigned)maxBytes);
    return s_serial->SendJsonCommand(json);
}
bool CommandBridge::DebugGetReport(uint8_t idx) {
    if (!s_serial) return false;
    char json[64];
    snprintf(json, sizeof(json), "{\"cmd\":713,\"idx\":%u}", (unsigned)idx);
    return s_serial->SendJsonCommand(json);
}
bool CommandBridge::DebugClearReports() {
    if (!s_serial) return false;
    return s_serial->SendJsonCommand("{\"cmd\":714}");
}
bool CommandBridge::DebugGetDeviceInfo() {
    if (!s_serial) return false;
    return s_serial->SendJsonCommand("{\"cmd\":715}");
}
