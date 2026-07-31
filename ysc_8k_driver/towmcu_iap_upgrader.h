#ifndef TOWMCU_IAP_UPGRADER_H
#define TOWMCU_IAP_UPGRADER_H

#include <string>
#include <cstdint>
#include <vector>

class PipeServer;

// Firmware upgrade for the towmcu dual-MCU board over USB-CDC.
//
// Mirrors tools/towmcu_iap_tool.py: query version -> (if APP) send cmd:50 ->
// find re-enumerated TOWMCUIAP port -> ERASE/PROGRAM/VERIFY/END at 115200
// (CDC baud is irrelevant). Emits iap2_log / iap2_progress / iap2_done so the
// v2 panel is fully isolated from the v1 IAPUpgrader's iap_* events.
class TowmcuIAPUpgrader {
public:
    // Kick off one side's upgrade on a detached worker thread.
    // port: user-selected CDC COMx (APP or IAP mode).
    // fwPath: absolute path to the encrypted .bin (convert/{left,right}/ysc_*.bin)
    //         — used by the manual file-picker path.
    static void Start(const wchar_t *port, const char *fwPath, PipeServer *pipe);
    // In-memory variant: firmware bytes arrive over the pipe as Base64
    // (online-download path — no intermediate file on disk).
    static void Start(const wchar_t *port, const uint8_t *data, size_t len, PipeServer *pipe);
    static void Cancel();
    static bool IsRunning();
    static bool IsCancelled() { return s_cancel; }  // for RunIapSequence

    // One-shot CDC helpers (each spawns its own short-lived thread so the
    // pipe handler returns immediately). No-op if an upgrade is running.
    // QueryVersion emits "towmcu_version" + an iap2_log line.
    static void QueryVersion(const wchar_t *port, PipeServer *pipe);
    // EnterIAP sends {"cmd":50} and polls for TOWMCUIAP re-enumeration.
    static void EnterIAP(const wchar_t *port, PipeServer *pipe);

private:
    static void Worker(std::wstring port, std::vector<uint8_t> firmware, PipeServer *pipe);
    static volatile bool s_running;
    static volatile bool s_cancel;
};

#endif // TOWMCU_IAP_UPGRADER_H
