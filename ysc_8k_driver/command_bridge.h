#ifndef COMMAND_BRIDGE_H
#define COMMAND_BRIDGE_H

#include <cstdint>

class SerialPort;

// 命令的目标设备角色。双设备场景下（一台透传鼠标、一台透传键盘）驱动把
// 鼠标类命令发往鼠标机、键盘类命令发往键盘机，调用方不必知道设备的存在。
// 单设备时 SlotByRole 会退回"第一个已连接设备"，行为与旧版一致。
enum DeviceRole;

class CommandBridge {
public:
    static void Init(SerialPort *serial);

    // 按角色解析目标串口（供需要自行发送的实现复用）
    static SerialPort *Resolve(DeviceRole role);

    // ---- 鼠标类（目标：透传鼠标的那台设备）----
    static bool SendMouseMove(int32_t x, int32_t y, int steps, DeviceRole role);
    static bool SendMoveTow(int32_t x, int32_t y, int steps, DeviceRole role);
    static bool SendMouseButton(uint8_t button_mask, bool pressed, DeviceRole role);
    /* 整份按键位图覆盖（固件 cmd 61）：kmboxnet 的 mouse_left/right/middle/
     * side1/side2/mouse_all 都是"全量状态"语义，用它一次下发最省事也最稳
     * （不必在驱动里维护影子状态，也就不会出现粘键）。 */
    static bool SendMouseButtonMask(uint8_t button_mask, DeviceRole role);
    /* 一次性滚轮（固件 cmd 42，下一帧报告带一次后清零）。kmboxnet 的
     * kmNet_mouse_wheel()/mouse_all() 是"一次滚 N 格"，不能用 cmd 35 的 hold
     * （hold 会按回报率重复发，8K 下一毫秒就是 8 格）。 */
    static bool SendMouseWheel(int32_t wheel, DeviceRole role);
    /* 按键弹起（固件 cmd 58）：物理按住的键仍参与上报合成，但通过内部屏蔽位
     * 让主机看到这些键已松开。release=true → {"cmd":58,"b":mask,"s":1}；
     * release=false → s=0 恢复。固件通常不回包，按即发即弃处理。 */
    static bool SendButtonRelease(uint8_t button_mask, bool release, DeviceRole role);
    /* 强制松开（固件 cmd 67）：b 位图，t=自动恢复毫秒（0/缺省=保持到恢复）。
     * b=0 立即全部恢复；不带 b 的查询走 SendForceReleaseQuery。
     * 固件应答 message=force_release / force_restore，data={b,remain}。 */
    static bool SendForceRelease(uint8_t button_mask, uint32_t duration_ms, DeviceRole role);
    static bool SendForceReleaseQuery(DeviceRole role);                 // {"cmd":67}
    static bool SendForceRestore(DeviceRole role);                      // {"cmd":67,"b":0}
    /* 物理屏蔽位图（固件 cmd 60）：bit0..4 按键，bit5 X，bit6 Y，bit7 滚轮；
     * mask=0 解除全部。kmboxnet 的 mask_mouse_* / unmask_all 落到这里。 */
    static bool SendMouseMask(uint8_t mask, DeviceRole role);
    /* 盒子软重启（固件 cmd 57）。kmboxnet 的 kmNet_reboot()。 */
    static bool SendReboot(DeviceRole role);
    /* kmboxnet 的 CMD_KEYBOARD_ALL 是"整份键盘状态"语义，内部做状态差分后
     * 转成固件 cmd 45 的单键按下/释放。 */
    static bool SendKeyboard(uint8_t ctrl, const uint8_t keys[10]);

    // ---- 键盘类（目标：透传键盘的那台设备）----
    /* Keyboard injection over the impersonated device's real keyboard endpoint
     * (firmware cmd 45/46). kc is a HID keycode 0x04-0xE7 (modifiers 0xE0-0xE7).
     * No-op at the device end if it has no keyboard interface (replies 404). */
    static bool SendKeyboardKey(uint8_t kc, bool down, DeviceRole role);   // {"cmd":45,"kc":..,"down":..}
    static bool SendKeyboardReleaseAll(DeviceRole role);                   // {"cmd":46}
    /* Type a mixed-case ASCII string char-by-char (firmware cmd 47). Firmware
     * owns case: it reads the PC's CapsLock state (HID LED report) and auto-
     * presses Shift per char. s must be non-empty and <= 128 bytes. Returns
     * false if not connected or s is empty/over-length. */
    static bool SendKeyboardTypeString(const char *s, DeviceRole role);    // {"cmd":47,"s":"<escaped>"}

    static bool SendUploadStatus(bool enable, DeviceRole role);
    static bool SendJumpIAP(DeviceRole role);

    static bool SendRawYsc(const char *json, DeviceRole role);
    static bool SendRawMakcu(const char *text, DeviceRole role);

    /* Gamepad↔mouse mapping config (firmware cmd 100..104).
     * Firmware owns the mapping logic + flash persistence; the driver is just
     * an editor/bridge between Electron UI and the dongle. */
    static bool SendGamepadConfig(const char *json, DeviceRole role);   // {"cmd":100,"data":{...}}
    static bool SendGetGamepadConfig(DeviceRole role);                 // {"cmd":101}
    static bool SendGamepadEnable(bool on, DeviceRole role);           // {"cmd":102,"on":...}
    static bool SendGamepadReset(DeviceRole role);                     // {"cmd":104}

    /* 设备数据线路选择（固件 cmd 62/63）：把手柄/鼠标的数据处理显式分开。
     *   mode: 0=鼠标线路(默认) 1=手柄线路。固件落盘持久化，立即生效；
     *   两种模式下透传都照常工作，只是不匹配线路的数据不参与处理。 */
    static bool SendDevModeGet(DeviceRole role);                       // {"cmd":62}
    static bool SendDevModeSet(int mode, DeviceRole role);             // {"cmd":63,"m":0|1}

    /* Mouse interpolation (firmware cmd 51/52/53). Spreads low-rate software
     * move commands (e.g. 200Hz) into smooth micro-increments at the device's
     * real polling rate (up to 8kHz). Firmware persists the config; driver is
     * a bridge. windowMs=0 adaptive (EMA of cmd-30 cadence), >0 fixed window. */
    static bool SendInterpSet(int enable, int profile, int windowMs, int maxWindowMs, DeviceRole role);  // {"cmd":51,...}
    static bool SendInterpGet(DeviceRole role);                                                          // {"cmd":52}
    static bool SendInterpReset(DeviceRole role);                                                        // {"cmd":53}

    /* Debug capture commands (forwarded to firmware over serial).
     * Responses are delivered asynchronously through the PipeServer event
     * "debug_response" once the firmware replies. */
    static bool DebugEnter(DeviceRole role);
    static bool DebugExit(DeviceRole role);
    static bool DebugStatus(DeviceRole role);
    static bool DebugGetDevDescr(DeviceRole role);
    static bool DebugGetCfgDescr(DeviceRole role);
    static bool DebugGetReportDescr(uint16_t offset, uint16_t maxBytes, DeviceRole role);
    static bool DebugGetReport(uint8_t idx, DeviceRole role);
    static bool DebugClearReports(DeviceRole role);
    static bool DebugGetDeviceInfo(DeviceRole role);
    /* 按键链路诊断（固件 cmd 716/717/718）：
     *   716 设置过滤模式（0=全部 1=仅变化 2=仅按键轨迹），会清空固件缓冲；
     *   717 取计数器 + 鼠标布局快照；
     *   718 分页取记录（含方向 up/down、端点、按键字节、分类/原因）。 */
    static bool DebugSetMode(uint8_t mode, DeviceRole role);
    static bool DebugGetDiag(DeviceRole role);
    static bool DebugGetEvents(uint16_t from, uint16_t n, DeviceRole role);

private:
    static SerialPort *s_serial;
};

#endif // COMMAND_BRIDGE_H
