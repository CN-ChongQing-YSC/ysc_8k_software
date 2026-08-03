#ifndef TOWMCU_CDC_H
#define TOWMCU_CDC_H

#include <string>
#include <vector>
#include <cstdint>

// VID/PID of the towmcu dual-MCU board (native USB-CDC).
#define TOWMCU_VID 0x1A86
#define TOWMCU_PID 0xFE0C

// A CDC COM port whose USB VID/PID match the towmcu board.
struct TowmcuPort {
    std::wstring portName;    // e.g. L"COM7"
    std::string  usbSerial;   // "TOWMCULEFT" / "TOWMCURIGHT" / "TOWMCUIAP" / ""
    std::string  description; // friendly name or device desc (display only)
    std::string  side;        // "Left" / "Right" / "IAP" / "" (derived from usbSerial)
};

// Enumerate present CDC COM ports matching VID 0x1A86 / PID 0xFE0C.
// Uses SetupAPI (GUID_DEVINTERFACE_COMPORT) so the USB iSerialNumber is
// available — the registry-only EnumComPorts() in main.cpp cannot do this.
// Sorted by COM number ascending.
std::vector<TowmcuPort> EnumTowmcuPorts();

// Map a USB iSerialNumber string to a side label. "" if unrecognized.
const char *TowmcuSideFromSerial(const std::string &serial);

// 一个 present 但驱动异常（problem 码 != 0，如黄叹号）的 WCH/CH34x 设备。
struct WchDeviceProblem {
    std::string  pidHex;        // 从硬件 ID 解析的 PID（如 "FE0C" / "55D3"），便于 UI 区分
    std::string  friendlyName;  // 友好名称或设备描述（仅展示用）
    unsigned long problemCode;  // CM_PROB_*（28 = 驱动未安装）
};

// WCH/CH34x 设备的驱动健康度快照。
struct Ch343DriverStatus {
    bool anyPresent;   // 是否存在任何 VID 1A86 的 present 设备（无论好坏）
    bool anyProblem;   // 是否存在 present 但 problem 码 != 0 的 VID 1A86 设备
    std::vector<WchDeviceProblem> problems;  // 异常设备列表（anyProblem=true 时非空）
};

// 检测 VID 1A86（WCH/CH34x，含本 8K 的 PID FE0C 与其他 CH343 产品）的设备驱动状态。
// 覆盖 inbox usbser 失效（FE0C）与 CH343 驱动缺失两类场景。枚举所有 present 设备节点
// （不只 COM 接口，因为「无驱动」的设备不会暴露 COM 接口），按硬件 ID 过滤 VID_1A86，
// 用 CM_Get_DevNode_Status 取 problem 码。供 UI 判断是否弹「缺驱动」提示。
Ch343DriverStatus DetectCh343DriverStatus();

#endif // TOWMCU_CDC_H
