#ifndef COMMAND_BRIDGE_H
#define COMMAND_BRIDGE_H

#include <cstdint>

class SerialPort;

class CommandBridge {
public:
    static void Init(SerialPort *serial);

    static bool SendMouseMove(int32_t x, int32_t y, int steps = 1);
    static bool SendMoveTow(int32_t x, int32_t y, int steps = 1);
    static bool SendMouseButton(uint8_t button_mask, bool pressed);
    static bool SendMouseWheel(int32_t wheel);
    static bool SendKeyboard(uint8_t ctrl, const uint8_t keys[10]);
    /* Keyboard injection over the impersonated device's real keyboard endpoint
     * (firmware cmd 45/46). kc is a HID keycode 0x04-0xE7 (modifiers 0xE0-0xE7).
     * No-op at the device end if it has no keyboard interface (replies 404). */
    static bool SendKeyboardKey(uint8_t kc, bool down);   // {"cmd":45,"kc":..,"down":..}
    static bool SendKeyboardReleaseAll();                  // {"cmd":46}
    /* Type a mixed-case ASCII string char-by-char (firmware cmd 47). Firmware
     * owns case: it reads the PC's CapsLock state (HID LED report) and auto-
     * presses Shift per char. s must be non-empty and <= 128 bytes. Returns
     * false if not connected or s is empty/over-length. */
    static bool SendKeyboardTypeString(const char *s);    // {"cmd":47,"s":"<escaped>"}
    static bool SendUploadStatus(bool enable);
    static bool SendJumpIAP();

    static bool SendRawYsc(const char *json);
    static bool SendRawMakcu(const char *text);

    /* Gamepad↔mouse mapping config (firmware cmd 100..104).
     * Firmware owns the mapping logic + flash persistence; the driver is just
     * an editor/bridge between Electron UI and the dongle. */
    static bool SendGamepadConfig(const char *json);   // {"cmd":100,"data":{...}}
    static bool SendGetGamepadConfig();                // {"cmd":101}
    static bool SendGamepadEnable(bool on);            // {"cmd":102,"on":...}
    static bool SendGamepadReset();                    // {"cmd":104}

    /* Debug capture commands (forwarded to firmware over serial).
     * Responses are delivered asynchronously through the PipeServer event
     * "debug_response" once the firmware replies. */
    static bool DebugEnter();
    static bool DebugExit();
    static bool DebugStatus();
    static bool DebugGetDevDescr();
    static bool DebugGetCfgDescr();
    static bool DebugGetReportDescr(uint16_t offset, uint16_t maxBytes);
    static bool DebugGetReport(uint8_t idx);
    static bool DebugClearReports();
    static bool DebugGetDeviceInfo();

private:
    static SerialPort *s_serial;
};

#endif // COMMAND_BRIDGE_H
