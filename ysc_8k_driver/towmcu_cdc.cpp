// towmcu_cdc.cpp — SetupAPI-based enumeration of towmcu CDC COM ports.
//
// Mirrors what pyserial's serial.tools.list_ports.comports() does under the
// hood: walks GUID_DEVINTERFACE_COMPORT, reads each device's hardware ID tree
// to match VID 0x1A86 / PID 0xFE0C and extract the USB iSerialNumber
// (TOWMCULEFT / TOWMCURIGHT / TOWMCUIAP). The registry-only EnumComPorts()
// in main.cpp cannot see VID/PID/serial, hence this separate enumerator.

#include "towmcu_cdc.h"

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <setupapi.h>
#include <cfgmgr32.h>
#include <algorithm>

#pragma comment(lib, "setupapi.lib")
#pragma comment(lib, "cfgmgr32.lib")

// {86E0D1E0-8089-11D0-9CE4-08003E301F73} — GUID_DEVINTERFACE_COMPORT.
// Defined inline to avoid the <initguid.h> ordering trap.
static const GUID GUID_DEVINTERFACE_COMPORT_LOCAL = {
    0x86e0d1e0, 0x8089, 0x11d0,
    { 0x9c, 0xe4, 0x08, 0x00, 0x3e, 0x30, 0x1f, 0x73 }
};

const char *TowmcuSideFromSerial(const std::string &serial) {
    if (serial == "TOWMCULEFT")  return "Left";
    if (serial == "TOWMCURIGHT") return "Right";
    if (serial == "TOWMCUIAP")   return "IAP";
    return "";
}

static int ComNumber(const std::wstring &name) {
    if (name.size() > 3 && name.compare(0, 3, L"COM") == 0)
        return _wtoi(name.c_str() + 3);
    return 0;
}

// Case-insensitive "does the hardware ID contain VID_1A86 and PID_FE0C".
static bool HwIdMatchesVidPid(const wchar_t *hwId) {
    if (!hwId) return false;
    // Hardware IDs use uppercase hex in practice, but be defensive.
    return wcsstr(hwId, L"VID_1A86") != nullptr &&
           wcsstr(hwId, L"PID_FE0C") != nullptr;
}

// The last path node of a hardware/instance ID is the USB iSerialNumber
// for direct CDC devices (e.g. "USB\VID_1A86&PID_FE0C\TOWMCULEFT").
// For Windows-generated instance IDs it contains '&' (e.g. "6&abcd1234&0"),
// in which case we return "" and let the caller walk up the tree.
static std::string ExtractSerialFromHwId(const wchar_t *hwId) {
    if (!hwId) return "";
    const wchar_t *bs = wcsrchr(hwId, L'\\');
    if (!bs) return "";
    const wchar_t *node = bs + 1;
    if (*node == L'\0' || wcschr(node, L'&')) return ""; // Windows instance ID
    std::string s;
    for (const wchar_t *p = node; *p && *p < 128; p++)
        s.push_back((char)*p);
    return s;
}

// Walk the device tree (this node + up to 5 ancestors) looking for a node
// whose hardware ID matches VID/PID. On match, fill *serialOut with the
// iSerialNumber from the FIRST matching node that carries one (best-effort).
// Returns true if a VID/PID match was found anywhere in the tree.
static bool MatchTowmcuInTree(DEVINST devInst, std::string *serialOut) {
    std::string bestSerial;
    bool matched = false;
    DEVINST cur = devInst;
    for (int depth = 0; depth < 6 && cur != 0; depth++) {
        wchar_t idBuf[512] = {};
        if (CM_Get_Device_IDW(cur, idBuf, _countof(idBuf), 0) == CR_SUCCESS &&
            HwIdMatchesVidPid(idBuf)) {
            matched = true;
            std::string s = ExtractSerialFromHwId(idBuf);
            if (!s.empty()) { bestSerial = s; break; }
            // Matched but no serial on this node — keep walking up.
        }
        DEVINST parent = 0;
        if (CM_Get_Parent(&parent, cur, 0) != CR_SUCCESS || parent == 0) break;
        cur = parent;
    }
    if (serialOut) *serialOut = bestSerial;
    return matched;
}

// Read the "PortName" value from the device's Device Parameters registry key.
// This is the canonical "COMxx" name for the port — more reliable than
// parsing "(COMxx)" out of the friendly name.
static std::wstring ReadPortName(HDEVINFO hDev, PSP_DEVINFO_DATA did) {
    HKEY hKey = SetupDiOpenDevRegKey(hDev, did, DICS_FLAG_GLOBAL, 0,
                                     DIREG_DEV, KEY_READ);
    if (hKey == INVALID_HANDLE_VALUE) return L"";
    wchar_t portName[64] = {};
    DWORD type = 0, sz = sizeof(portName);
    LONG rc = RegQueryValueExW(hKey, L"PortName", NULL, &type,
                               (LPBYTE)portName, &sz);
    RegCloseKey(hKey);
    if (rc != ERROR_SUCCESS || type != REG_SZ) return L"";
    // Strip a trailing "(COMxx)"-style friendly suffix if present — PortName
    // is normally bare "COM7", but be safe.
    return portName;
}

static std::string Narrow(const wchar_t *w) {
    if (!w) return "";
    char buf[256] = {};
    WideCharToMultiByte(CP_UTF8, 0, w, -1, buf, (int)sizeof(buf), NULL, NULL);
    return buf;
}

std::vector<TowmcuPort> EnumTowmcuPorts() {
    std::vector<TowmcuPort> out;

    HDEVINFO hDev = SetupDiGetClassDevsW(
        &GUID_DEVINTERFACE_COMPORT_LOCAL, NULL, NULL,
        DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
    if (hDev == INVALID_HANDLE_VALUE) return out;

    SP_DEVINFO_DATA did = { sizeof(did) };
    for (DWORD i = 0; SetupDiEnumDeviceInfo(hDev, i, &did); i++) {
        std::string serial;
        if (!MatchTowmcuInTree(did.DevInst, &serial)) continue;

        std::wstring portName = ReadPortName(hDev, &did);
        if (portName.empty()) continue;

        TowmcuPort p;
        p.portName  = portName;
        p.usbSerial = serial;
        p.side      = TowmcuSideFromSerial(p.usbSerial);

        // Friendly name for display (e.g. "USB Serial Device (COM7)"),
        // falling back to device desc, then to the USB serial.
        wchar_t friendly[256] = {};
        if (SetupDiGetDeviceRegistryPropertyW(hDev, &did, SPDRP_FRIENDLYNAME,
                NULL, (PBYTE)friendly, sizeof(friendly), NULL) && friendly[0]) {
            p.description = Narrow(friendly);
        } else {
            wchar_t devdesc[256] = {};
            if (SetupDiGetDeviceRegistryPropertyW(hDev, &did, SPDRP_DEVICEDESC,
                    NULL, (PBYTE)devdesc, sizeof(devdesc), NULL) && devdesc[0]) {
                p.description = Narrow(devdesc);
            } else {
                p.description = serial;
            }
        }

        out.push_back(std::move(p));
    }
    SetupDiDestroyDeviceInfoList(hDev);

    std::sort(out.begin(), out.end(),
        [](const TowmcuPort &a, const TowmcuPort &b) {
            return ComNumber(a.portName) < ComNumber(b.portName);
        });
    return out;
}
