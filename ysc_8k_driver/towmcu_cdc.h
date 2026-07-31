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

#endif // TOWMCU_CDC_H
