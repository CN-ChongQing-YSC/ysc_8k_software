#define WIN32_LEAN_AND_MEAN
#include <winsock2.h>
#include <windows.h>
#include <shellapi.h>
#include <ws2tcpip.h>
#include <dbt.h>

#include "main.h"
#include "serial_port.h"
#include "kmboxnet_server.h"
#include "command_bridge.h"
#include "monitor_push.h"
#include "debug_logger.h"
#include "pipe_server.h"
#include "iap_upgrader.h"
#include "base64.h"
#include "towmcu_cdc.h"
#include "towmcu_iap_upgrader.h"

#include <vector>
#include <string>
#include <algorithm>
#include <cstdio>
#include <cstdarg>
#include <timeapi.h>
#pragma comment(lib, "winmm.lib")

// {86E0D1E0-8089-11D0-9CE4-08003E301F73} — GUID_DEVINTERFACE_COMPORT.
// Defined inline to avoid the <initguid.h> ordering trap (同 towmcu_cdc.cpp)。
static const GUID GUID_DEVINTERFACE_COMPORT_LOCAL = {
    0x86e0d1e0, 0x8089, 0x11d0,
    { 0x9c, 0xe4, 0x08, 0x00, 0x3e, 0x30, 0x1f, 0x73 }
};
// WM_DEVICECHANGE 防抖定时器：一次插拔连发多条，合并成一次 EnumComPorts 推送
#define TIMER_PORT_CHANGE         1001
#define PORT_CHANGE_DEBOUNCE_MS   400

// ========== Pipe Server ==========
PipeServer g_pipeServer;

// ========== Debug Window ==========
static LRESULT CALLBACK DebugWndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    switch (msg) {
    case WM_SIZE:
        if (g_app.hwndDebugEdit)
            MoveWindow(g_app.hwndDebugEdit, 0, 0, LOWORD(lParam), HIWORD(lParam), TRUE);
        return 0;
    case WM_CLOSE:
        g_app.debugWindowVisible = false;
        DebugLogger::SetDebugWindowOpen(false);
        ShowWindow(hwnd, SW_HIDE);
        return 0;
    }
    return DefWindowProcW(hwnd, msg, wParam, lParam);
}

static void CreateDebugWindow() {
    if (g_app.hwndDebug) {
        g_app.debugWindowVisible = true;
        ShowWindow(g_app.hwndDebug, SW_SHOW);
        SetForegroundWindow(g_app.hwndDebug);
        DebugLogger::SetDebugWindowOpen(true);
        return;
    }

    g_app.hwndDebug = CreateWindowExW(WS_EX_APPWINDOW,
        L"YscDebugClass", L"YSC 8K Driver - 调试窗口",
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_SIZEBOX | WS_MINIMIZEBOX,
        CW_USEDEFAULT, CW_USEDEFAULT, 700, 500,
        NULL, NULL, g_app.hInstance, NULL);

    g_app.hwndDebugEdit = CreateWindowExW(0, L"EDIT", L"",
        WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_HSCROLL |
        ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL | ES_AUTOHSCROLL,
        0, 0, 700, 500,
        g_app.hwndDebug, NULL, g_app.hInstance, NULL);

    SendMessage(g_app.hwndDebugEdit, EM_SETLIMITTEXT, 0x7FFFFFFE, 0);

    HFONT hFont = CreateFontW(-13, 0, 0, 0, FW_NORMAL, FALSE, FALSE, FALSE,
        DEFAULT_CHARSET, 0, 0, CLEARTYPE_QUALITY, FIXED_PITCH, L"Consolas");
    SendMessage(g_app.hwndDebugEdit, WM_SETFONT, (WPARAM)hFont, FALSE);

    g_app.debugWindowVisible = true;
    ShowWindow(g_app.hwndDebug, SW_SHOW);
    DebugLogger::SetDebugWindowOpen(true);
}

void DebugLog(const char *fmt, ...) {
    if (!g_app.debugWindowVisible) return;

    char buf[1024];
    SYSTEMTIME st;
    GetLocalTime(&st);
    int prefix = snprintf(buf, sizeof(buf), "[%02d:%02d:%02d.%03d] ",
            st.wHour, st.wMinute, st.wSecond, st.wMilliseconds);

    va_list args;
    va_start(args, fmt);
    int n = vsnprintf(buf + prefix, sizeof(buf) - prefix - 2, fmt, args);
    va_end(args);

    if (n >= 0 && prefix + n + 2 < (int)sizeof(buf)) {
        buf[prefix + n] = '\r';
        buf[prefix + n + 1] = '\n';
        buf[prefix + n + 2] = '\0';
    }

    size_t len = strlen(buf) + 1;
    char *msg = (char*)malloc(len);
    if (msg) {
        memcpy(msg, buf, len);
        PostMessage(g_app.hwndMain, WM_DEBUG_LOG, 0, (LPARAM)msg);
    }
}

// ========== Global State ==========
AppState g_app = {};
SerialPort g_serial;
static KmboxnetServer g_kmServer;

// ========== Forward Declarations ==========
static std::vector<std::wstring> EnumComPorts();
static void UpdateTrayTip();
static void ShowTrayMenu(HWND hwnd);
static std::string GetLocalIP();

// ========== Get Local IP ==========
static std::string GetLocalIP() {
    char hostname[256] = {};
    if (gethostname(hostname, sizeof(hostname)) != 0) return "127.0.0.1";
    addrinfo hints = {}, *res;
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_DGRAM;
    if (getaddrinfo(hostname, NULL, &hints, &res) != 0) return "127.0.0.1";
    char ip[INET_ADDRSTRLEN] = {};
    for (auto p = res; p; p = p->ai_next) {
        sockaddr_in *addr = (sockaddr_in*)p->ai_addr;
        if (addr->sin_addr.s_addr != htonl(INADDR_LOOPBACK)) {
            inet_ntop(AF_INET, &addr->sin_addr, ip, sizeof(ip));
            break;
        }
    }
    if (ip[0] == '\0') {
        // Fallback: use loopback
        auto p = res;
        if (p) inet_ntop(AF_INET, &((sockaddr_in*)p->ai_addr)->sin_addr, ip, sizeof(ip));
    }
    freeaddrinfo(res);
    return ip[0] ? ip : "127.0.0.1";
}

// ========== Pipe Command Callback ==========
static void OnPipeCommand(const char *json, void *userData);

static std::string ExtractJsonString(const char *json, const char *key) {
    char search[64];
    snprintf(search, sizeof(search), "\"%s\":", key);
    const char *p = strstr(json, search);
    if (!p) return "";
    p += strlen(search);
    while (*p == ' ') p++;
    if (*p != '"') return "";
    p++;
    std::string result;
    while (*p && *p != '"') {
        if (*p == '\\' && *(p + 1)) p++;
        result += *p++;
    }
    return result;
}

static int ExtractJsonInt(const char *json, const char *key) {
    char search[64];
    snprintf(search, sizeof(search), "\"%s\":", key);
    const char *p = strstr(json, search);
    if (!p) return 0;
    p += strlen(search);
    while (*p == ' ') p++;
    return atoi(p);
}

static bool ExtractJsonBool(const char *json, const char *key) {
    char search[64];
    snprintf(search, sizeof(search), "\"%s\":", key);
    const char *p = strstr(json, search);
    if (!p) return false;
    p += strlen(search);
    while (*p == ' ') p++;
    return (*p == 't' || *p == '1');
}

// Forward firmware debug-capture responses (code 700..715) to Electron.
// Firmware response shape: {"code":NNN,"message":"..","data":".."}
// Re-emitted as:           {"type":"debug_response","code":NNN,"message":"..","data":".."}
static void ForwardDebugResponse(const uint8_t *data, int len) {
    if (len < 8 || len > 8000) return;
    char json[8000];
    if (len >= (int)sizeof(json)) return;
    memcpy(json, data, len);
    json[len] = '\0';

    int code = ExtractJsonInt(json, "code");
    // Forward debug (700..715) AND gamepad-mapper config responses (100..104,
    // includes async pad plug/unplug event 103) to Electron.
    bool is_debug = (code >= 700 && code <= 715);
    bool is_gmap  = (code >= 100 && code <= 104);
    // Also forward control responses (code==200 with non-null message) so the
    // UI can read back macro/jitter config. Monitor-push responses carry
    // message:null and are excluded.
    std::string msg = ExtractJsonString(json, "message");
    bool is_ysc_ctrl = (code == 200 && !msg.empty());
    if (!is_debug && !is_gmap && !is_ysc_ctrl) return;

    if (json[0] != '{' || json[len - 1] != '}') return;
    json[len - 1] = '\0';
    const char *inner = json + 1;

    char body[8200];
    int n = snprintf(body, sizeof(body), "%s", inner);
    if (n <= 0 || n >= (int)sizeof(body)) return;

    g_pipeServer.SendEvent("debug_response", body);
}

// Combined serial response handler: existing monitor push + debug forwarding.
static void OnSerialResponseCombo(const uint8_t *data, int len, void *userData) {
    MonitorPush::OnSerialResponse(data, len, userData);
    ForwardDebugResponse(data, len);
}

static void OnPipeCommand(const char *json, void *userData) {
    std::string type = ExtractJsonString(json, "type");
    if (type.empty()) return;

    if (type == "enum_ports") {
        auto ports = EnumComPorts();
        std::string body = "\"ports\":[";
        for (size_t i = 0; i < ports.size(); i++) {
            if (i > 0) body += ",";
            std::string port(ports[i].begin(), ports[i].end());
            body += "\"" + port + "\"";
        }
        body += "]";
        g_pipeServer.SendEvent("ports_list", body.c_str());
    }
    else if (type == "serial_connect") {
        std::string port = ExtractJsonString(json, "port");
        int baudReq = ExtractJsonInt(json, "baud");
        if (port.empty()) {
            g_pipeServer.SendEvent("serial_error", "\"message\":\"No port specified\"");
            return;
        }
        std::wstring wport(port.begin(), port.end());
        uint32_t baud = 0;
        if (baudReq > 0) {
            // Validate baudrate
            bool valid = false;
            for (int i = 0; i < SUPPORTED_BAUD_COUNT; i++) {
                if (SUPPORTED_BAUDS[i] == (DWORD)baudReq) { valid = true; baud = (uint32_t)baudReq; break; }
            }
            if (!valid) {
                g_pipeServer.SendEvent("serial_error", "\"message\":\"Invalid baudrate\"");
                return;
            }
        }
        if (baud == 0) baud = SerialPort::DetectBaudrate(wport.c_str());
        if (baud == 0) {
            g_pipeServer.SendEvent("serial_error", "\"message\":\"无法检测到设备\"");
            return;
        }
        if (g_serial.Connect(wport.c_str(), baud)) {
            DebugLog("Serial CONNECTED %s @ %u", port.c_str(), baud);
            DebugLogger::Log("SERIAL CONNECTED %s @ %u", port.c_str(), baud);
            wcscpy_s(g_app.serialPortName, wport.c_str());
            g_app.serialBaudRate = baud;
            g_app.serialConnected = true;
            g_serial.StartReadThread(g_app.hStopEvent);
            UpdateTrayTip();
            char body[128];
            snprintf(body, sizeof(body), "\"port\":\"%s\",\"baud\":%u", port.c_str(), baud);
            g_pipeServer.SendEvent("serial_connected", body);
        } else {
            g_pipeServer.SendEvent("serial_error", "\"message\":\"无法打开串口\"");
        }
    }
    else if (type == "serial_disconnect") {
        g_serial.Disconnect();
        g_app.serialConnected = false;
        DebugLogger::Log("SERIAL DISCONNECTED");
        UpdateTrayTip();
        g_pipeServer.SendEvent("serial_disconnected", nullptr);
    }
    else if (type == "switch_baudrate") {
        int baudReq = ExtractJsonInt(json, "baud");
        if (baudReq > 0 && g_app.serialConnected) {
            uint32_t newBaud = (uint32_t)baudReq;
            if (newBaud != g_app.serialBaudRate) {
                if (g_serial.SwitchBaudrate(newBaud)) {
                    g_app.serialBaudRate = newBaud;
                    g_serial.StartReadThread(g_app.hStopEvent);
                    UpdateTrayTip();
                    char body[64];
                    snprintf(body, sizeof(body), "\"baud\":%u", newBaud);
                    g_pipeServer.SendEvent("baudrate_switched", body);
                } else {
                    g_app.serialConnected = false;
                    UpdateTrayTip();
                    g_pipeServer.SendEvent("baudrate_failed", "\"message\":\"切换失败，设备可能已断开\"");
                }
            }
        }
    }
    else if (type == "kmnet_start") {
        int port = ExtractJsonInt(json, "port");
        if (port <= 0) port = KMBOXNET_DEFAULT_PORT;
        if (g_kmServer.Start((uint16_t)port)) {
            g_app.netServerRunning = true;
            g_app.netPort = (uint16_t)port;
            DebugLog("Net service START on port %u", (uint16_t)port);
            DebugLogger::Log("NET SERVICE START on port %u", (uint16_t)port);
            UpdateTrayTip();
            std::string ip = GetLocalIP();
            char body[128];
            snprintf(body, sizeof(body), "\"port\":%u,\"ip\":\"%s\",\"mac\":\"%08X\"", (uint16_t)port, ip.c_str(), DEFAULT_MAC);
            g_pipeServer.SendEvent("kmnet_started", body);
        } else {
            g_pipeServer.SendEvent("kmnet_error", "\"message\":\"Net 服务启动失败\"");
        }
    }
    else if (type == "kmnet_stop") {
        DebugLog("Net service STOP");
        DebugLogger::Log("NET SERVICE STOP");
        g_kmServer.Stop();
        g_app.netServerRunning = false;
        UpdateTrayTip();
        g_pipeServer.SendEvent("kmnet_stopped", nullptr);
    }
    else if (type == "upload_enable") {
        bool enable = ExtractJsonBool(json, "enable");
        CommandBridge::SendUploadStatus(enable);
        char body[32];
        snprintf(body, sizeof(body), "\"enable\":%s", enable ? "true" : "false");
        g_pipeServer.SendEvent("upload_status", body);
    }
    else if (type == "jump_iap") {
        CommandBridge::SendJumpIAP();
    }
    else if (type == "get_local_ip") {
        std::string ip = GetLocalIP();
        char body[64];
        snprintf(body, sizeof(body), "\"ip\":\"%s\"", ip.c_str());
        g_pipeServer.SendEvent("local_ip", body);
    }
    else if (type == "get_version") {
        g_pipeServer.SendEvent("version", "\"version\":\"1.14.0\"");
    }
    else if (type == "get_monitor") {
        auto m = MonitorPush::GetLatest();
        char body[128];
        snprintf(body, sizeof(body), "\"buttons\":%d,\"x\":%d,\"y\":%d,\"wheel\":%d",
                 m.buttons, m.x, m.y, m.wheel);
        g_pipeServer.SendEvent("monitor_data", body);
    }
    else if (type == "get_state") {
        char body[512];
        std::string serialPort;
        if (g_app.serialConnected)
            serialPort = std::string(g_app.serialPortName, g_app.serialPortName + wcslen(g_app.serialPortName));
        snprintf(body, sizeof(body),
            "\"serialConnected\":%s,\"serialPort\":\"%s\",\"serialBaud\":%lu,"
            "\"netRunning\":%s,\"netPort\":%u",
            g_app.serialConnected ? "true" : "false",
            serialPort.c_str(),
            g_app.serialBaudRate,
            g_app.netServerRunning ? "true" : "false",
            g_app.netPort);
        g_pipeServer.SendEvent("state", body);
    }
    else if (type == "send_ysc") {
        std::string cmd = ExtractJsonString(json, "cmd");
        if (cmd.empty()) {
            g_pipeServer.SendEvent("send_result", "\"ok\":false,\"error\":\"empty cmd\"");
        } else if (!g_app.serialConnected) {
            g_pipeServer.SendEvent("send_result", "\"ok\":false,\"error\":\"未连接设备\"");
        } else {
            bool ok = CommandBridge::SendRawYsc(cmd.c_str());
            char body[256];
            snprintf(body, sizeof(body), "\"ok\":%s,\"dir\":\"tx\",\"proto\":\"ysc\",\"cmd\":\"%s\"",
                     ok ? "true" : "false", cmd.c_str());
            g_pipeServer.SendEvent("send_result", body);
        }
    }
    else if (type == "set_gamepad_config") {
        // data = stringified config JSON object; driver wraps into {"cmd":100,"data":<cfg>}
        std::string cfg = ExtractJsonString(json, "data");
        if (!g_app.serialConnected)
            g_pipeServer.SendEvent("send_result", "\"ok\":false,\"error\":\"未连接设备\"");
        else if (cfg.empty())
            g_pipeServer.SendEvent("send_result", "\"ok\":false,\"error\":\"empty data\"");
        else {
            bool ok = CommandBridge::SendGamepadConfig(cfg.c_str());
            g_pipeServer.SendEvent("send_result", ok ? "\"ok\":true,\"cmd\":\"gmap_set\"" : "\"ok\":false");
        }
    }
    else if (type == "get_gamepad_config") {
        if (!g_app.serialConnected)
            g_pipeServer.SendEvent("send_result", "\"ok\":false,\"error\":\"未连接设备\"");
        else {
            // response arrives asynchronously as code 101 via ForwardDebugResponse
            bool ok = CommandBridge::SendGetGamepadConfig();
            g_pipeServer.SendEvent("send_result", ok ? "\"ok\":true,\"cmd\":\"gmap_get\"" : "\"ok\":false");
        }
    }
    else if (type == "set_gamepad_enable") {
        int on = ExtractJsonInt(json, "on");
        if (!g_app.serialConnected)
            g_pipeServer.SendEvent("send_result", "\"ok\":false,\"error\":\"未连接设备\"");
        else {
            bool ok = CommandBridge::SendGamepadEnable(on != 0);
            g_pipeServer.SendEvent("send_result", ok ? "\"ok\":true,\"cmd\":\"gmap_enable\"" : "\"ok\":false");
        }
    }
    else if (type == "reset_gamepad_config") {
        if (!g_app.serialConnected)
            g_pipeServer.SendEvent("send_result", "\"ok\":false,\"error\":\"未连接设备\"");
        else {
            bool ok = CommandBridge::SendGamepadReset();
            g_pipeServer.SendEvent("send_result", ok ? "\"ok\":true,\"cmd\":\"gmap_reset\"" : "\"ok\":false");
        }
    }
    else if (type == "set_interp_config") {
        // {"type":"set_interp_config","enable":..,"profile":..,"window":..,"maxw":..} → firmware cmd 51
        int enable  = ExtractJsonInt(json, "enable");
        int profile = ExtractJsonInt(json, "profile");
        int window  = ExtractJsonInt(json, "window");
        int maxw    = ExtractJsonInt(json, "maxw");
        if (!g_app.serialConnected)
            g_pipeServer.SendEvent("send_result", "\"ok\":false,\"error\":\"未连接设备\"");
        else {
            bool ok = CommandBridge::SendInterpSet(enable, profile, window, maxw);
            g_pipeServer.SendEvent("send_result", ok ? "\"ok\":true,\"cmd\":\"interp_set\"" : "\"ok\":false");
        }
    }
    else if (type == "get_interp_config") {
        if (!g_app.serialConnected)
            g_pipeServer.SendEvent("send_result", "\"ok\":false,\"error\":\"未连接设备\"");
        else {
            // response arrives asynchronously as debug_response(message="interp") via ForwardDebugResponse
            bool ok = CommandBridge::SendInterpGet();
            g_pipeServer.SendEvent("send_result", ok ? "\"ok\":true,\"cmd\":\"interp_get\"" : "\"ok\":false");
        }
    }
    else if (type == "reset_interp_config") {
        if (!g_app.serialConnected)
            g_pipeServer.SendEvent("send_result", "\"ok\":false,\"error\":\"未连接设备\"");
        else {
            bool ok = CommandBridge::SendInterpReset();
            g_pipeServer.SendEvent("send_result", ok ? "\"ok\":true,\"cmd\":\"interp_reset\"" : "\"ok\":false");
        }
    }
    else if (type == "send_makcu") {
        std::string cmd = ExtractJsonString(json, "cmd");
        if (cmd.empty()) {
            g_pipeServer.SendEvent("send_result", "\"ok\":false,\"error\":\"empty cmd\"");
        } else if (!g_app.serialConnected) {
            g_pipeServer.SendEvent("send_result", "\"ok\":false,\"error\":\"未连接设备\"");
        } else {
            bool ok = CommandBridge::SendRawMakcu(cmd.c_str());
            char body[256];
            snprintf(body, sizeof(body), "\"ok\":%s,\"dir\":\"tx\",\"proto\":\"makcu\",\"cmd\":\"%s\"",
                     ok ? "true" : "false", cmd.c_str());
            g_pipeServer.SendEvent("send_result", body);
        }
    }
    else if (type == "iap_enter") {
        if (!g_app.serialConnected) {
            g_pipeServer.SendEvent("iap_log", "\"message\":\"设备未连接\",\"cls\":\"err\"");
            return;
        }
        CommandBridge::SendJumpIAP();
        Sleep(200);
        g_pipeServer.SendEvent("iap_log", "\"message\":\"已进入下载模式\",\"cls\":\"info\"");
    }
    else if (type == "iap_start") {
        if (IAPUpgrader::IsRunning()) {
            g_pipeServer.SendEvent("iap_log", "\"message\":\"升级正在进行中\",\"cls\":\"warn\"");
            return;
        }
        std::string path = ExtractJsonString(json, "path");
        if (path.empty()) {
            g_pipeServer.SendEvent("iap_log", "\"message\":\"未选择固件文件\",\"cls\":\"err\"");
            return;
        }
        // baud: 0 = 保持当前探测到的波特率不切换; >0 = 切换到该波特率
        uint32_t targetBaud = (uint32_t)ExtractJsonInt(json, "baud");
        IAPUpgrader::Start(path.c_str(), &g_pipeServer, targetBaud);
    }
    else if (type == "iap_start_mem") {
        // 在线下载直传：data 字段为 Base64 编码的固件字节。
        if (IAPUpgrader::IsRunning()) {
            g_pipeServer.SendEvent("iap_log", "\"message\":\"升级正在进行中\",\"cls\":\"warn\"");
            return;
        }
        std::string b64 = ExtractJsonString(json, "data");
        if (b64.empty()) {
            g_pipeServer.SendEvent("iap_log", "\"message\":\"未收到固件数据\",\"cls\":\"err\"");
            return;
        }
        std::vector<uint8_t> fw;
        if (!base64::decode(b64, fw) || fw.empty()) {
            g_pipeServer.SendEvent("iap_log", "\"message\":\"固件 Base64 解码失败\",\"cls\":\"err\"");
            return;
        }
        // baud: 0 = 保持当前探测到的波特率不切换; >0 = 切换到该波特率
        uint32_t targetBaud = (uint32_t)ExtractJsonInt(json, "baud");
        IAPUpgrader::Start(fw.data(), fw.size(), &g_pipeServer, targetBaud);
    }
    else if (type == "iap_cancel") {
        IAPUpgrader::Cancel();
    }
    else if (type == "towmcu_list_ports") {
        // 升级进行中时跳过 SetupAPI 全枚举 —— 防止 Windows USB 栈在设备忙时
        // 返回脏状态/COM 号短暂消失，导致前端误判 selectedPort 消失并清空。
        // 前端在升级态本就 stopPortPolling，这里是双保险。
        if (TowmcuIAPUpgrader::IsRunning()) {
            g_pipeServer.SendEvent("towmcu_ports", "\"ports\":[]");
            return;
        }
        // CDC ports filtered by VID 0x1A86 / PID 0xFE0C, with USB iSerialNumber.
        auto ports = EnumTowmcuPorts();
        std::string body = "\"ports\":[";
        for (size_t i = 0; i < ports.size(); i++) {
            if (i > 0) body += ",";
            std::string pn(ports[i].portName.begin(), ports[i].portName.end());
            char entry[384];
            snprintf(entry, sizeof(entry),
                "{\"port\":\"%s\",\"serial\":\"%s\",\"side\":\"%s\",\"desc\":\"%s\"}",
                pn.c_str(),
                ports[i].usbSerial.c_str(),
                ports[i].side.c_str(),
                ports[i].description.c_str());
            body += entry;
        }
        body += "]";
        g_pipeServer.SendEvent("towmcu_ports", body.c_str());
    }
    else if (type == "check_ch343_driver") {
        // 检测 WCH/CH34x (VID 1A86) 设备驱动健康度，供前端弹「缺驱动」提示。
        Ch343DriverStatus st = DetectCh343DriverStatus();
        std::string body = "\"present\":";
        body += (st.anyPresent ? "true" : "false");
        body += ",\"problem\":";
        body += (st.anyProblem ? "true" : "false");
        body += ",\"problems\":[";
        for (size_t i = 0; i < st.problems.size(); i++) {
            if (i > 0) body += ",";
            char entry[384];
            snprintf(entry, sizeof(entry),
                "{\"pid\":\"%s\",\"name\":\"%s\",\"code\":%lu}",
                st.problems[i].pidHex.c_str(),
                st.problems[i].friendlyName.c_str(),
                st.problems[i].problemCode);
            body += entry;
        }
        body += "]";
        g_pipeServer.SendEvent("ch343_driver_status", body.c_str());
    }
    else if (type == "towmcu_query_version") {
        std::string port = ExtractJsonString(json, "port");
        if (port.empty()) {
            g_pipeServer.SendEvent("iap2_log",
                "\"message\":\"No port specified\",\"cls\":\"err\"");
            return;
        }
        std::wstring wport(port.begin(), port.end());
        TowmcuIAPUpgrader::QueryVersion(wport.c_str(), &g_pipeServer);
    }
    else if (type == "towmcu_enter_iap") {
        std::string port = ExtractJsonString(json, "port");
        if (port.empty()) {
            g_pipeServer.SendEvent("iap2_log",
                "\"message\":\"No port specified\",\"cls\":\"err\"");
            return;
        }
        std::wstring wport(port.begin(), port.end());
        TowmcuIAPUpgrader::EnterIAP(wport.c_str(), &g_pipeServer);
    }
    else if (type == "towmcu_start") {
        if (TowmcuIAPUpgrader::IsRunning()) {
            g_pipeServer.SendEvent("iap2_log",
                "\"message\":\"升级正在进行中\",\"cls\":\"warn\"");
            return;
        }
        std::string port = ExtractJsonString(json, "port");
        std::string path = ExtractJsonString(json, "path");
        if (port.empty() || path.empty()) {
            g_pipeServer.SendEvent("iap2_log",
                "\"message\":\"未指定端口或固件\",\"cls\":\"err\"");
            return;
        }
        std::wstring wport(port.begin(), port.end());
        TowmcuIAPUpgrader::Start(wport.c_str(), path.c_str(), &g_pipeServer);
    }
    else if (type == "towmcu_start_mem") {
        // 在线下载直传：data 字段为 Base64 编码的固件字节。
        if (TowmcuIAPUpgrader::IsRunning()) {
            g_pipeServer.SendEvent("iap2_log", "\"message\":\"升级正在进行中\",\"cls\":\"warn\"");
            return;
        }
        std::string port = ExtractJsonString(json, "port");
        std::string b64  = ExtractJsonString(json, "data");
        if (port.empty() || b64.empty()) {
            g_pipeServer.SendEvent("iap2_log", "\"message\":\"未指定端口或固件数据\",\"cls\":\"err\"");
            return;
        }
        std::vector<uint8_t> fw;
        if (!base64::decode(b64, fw) || fw.empty()) {
            g_pipeServer.SendEvent("iap2_log", "\"message\":\"固件 Base64 解码失败\",\"cls\":\"err\"");
            return;
        }
        std::wstring wport(port.begin(), port.end());
        TowmcuIAPUpgrader::Start(wport.c_str(), fw.data(), fw.size(), &g_pipeServer);
    }
    else if (type == "towmcu_cancel") {
        TowmcuIAPUpgrader::Cancel();
    }
    else if (type == "debug_enter") {
        bool ok = g_app.serialConnected && CommandBridge::DebugEnter();
        g_pipeServer.SendEvent("debug_cmd_result",
            ok ? "\"cmd\":\"enter\",\"ok\":true"
               : "\"cmd\":\"enter\",\"ok\":false,\"error\":\"未连接设备\"");
    }
    else if (type == "debug_exit") {
        bool ok = g_app.serialConnected && CommandBridge::DebugExit();
        g_pipeServer.SendEvent("debug_cmd_result",
            ok ? "\"cmd\":\"exit\",\"ok\":true"
               : "\"cmd\":\"exit\",\"ok\":false,\"error\":\"未连接设备\"");
    }
    else if (type == "debug_status") {
        bool ok = g_app.serialConnected && CommandBridge::DebugStatus();
        if (!ok)
            g_pipeServer.SendEvent("debug_cmd_result",
                "\"cmd\":\"status\",\"ok\":false,\"error\":\"未连接设备\"");
    }
    else if (type == "debug_get_dev_descr") {
        bool ok = g_app.serialConnected && CommandBridge::DebugGetDevDescr();
        if (!ok)
            g_pipeServer.SendEvent("debug_cmd_result",
                "\"cmd\":\"dev_descr\",\"ok\":false,\"error\":\"未连接设备\"");
    }
    else if (type == "debug_get_cfg_descr") {
        bool ok = g_app.serialConnected && CommandBridge::DebugGetCfgDescr();
        if (!ok)
            g_pipeServer.SendEvent("debug_cmd_result",
                "\"cmd\":\"cfg_descr\",\"ok\":false,\"error\":\"未连接设备\"");
    }
    else if (type == "debug_get_rep_descr") {
        int offset = ExtractJsonInt(json, "offset");
        int maxBytes = ExtractJsonInt(json, "max");
        if (maxBytes <= 0) maxBytes = 200;
        bool ok = g_app.serialConnected &&
                  CommandBridge::DebugGetReportDescr((uint16_t)offset, (uint16_t)maxBytes);
        if (!ok)
            g_pipeServer.SendEvent("debug_cmd_result",
                "\"cmd\":\"rep_descr\",\"ok\":false,\"error\":\"未连接设备\"");
    }
    else if (type == "debug_get_report") {
        int idx = ExtractJsonInt(json, "idx");
        bool ok = g_app.serialConnected && CommandBridge::DebugGetReport((uint8_t)idx);
        if (!ok)
            g_pipeServer.SendEvent("debug_cmd_result",
                "\"cmd\":\"report\",\"ok\":false,\"error\":\"未连接设备\"");
    }
    else if (type == "debug_clear_reports") {
        bool ok = g_app.serialConnected && CommandBridge::DebugClearReports();
        g_pipeServer.SendEvent("debug_cmd_result",
            ok ? "\"cmd\":\"clear\",\"ok\":true"
               : "\"cmd\":\"clear\",\"ok\":false,\"error\":\"未连接设备\"");
    }
    else if (type == "debug_get_device_info") {
        bool ok = g_app.serialConnected && CommandBridge::DebugGetDeviceInfo();
        if (!ok)
            g_pipeServer.SendEvent("debug_cmd_result",
                "\"cmd\":\"device_info\",\"ok\":false,\"error\":\"未连接设备\"");
    }
}

// ========== Entry Point ==========
int WINAPI wWinMain(HINSTANCE hInstance, HINSTANCE, LPWSTR, int) {
    g_app.hInstance = hInstance;

    // Single instance
    g_app.hMutex = CreateMutexW(NULL, FALSE, L"Ysc8kDriver_SingleInstance");
    if (GetLastError() == ERROR_ALREADY_EXISTS) {
        MessageBoxW(NULL, L"程序已在运行", L"YSC 8K Driver",
                    MB_OK | MB_ICONWARNING);
        return 1;
    }

    // Winsock
    WSADATA wsa;
    if (WSAStartup(MAKEWORD(2, 2), &wsa) != 0) return 1;

    // 将系统定时器精度从默认15.6ms提升到1ms
    // 这直接影响串口 ReadFile 超时精度和线程调度响应速度
    timeBeginPeriod(1);

    // Init debug logger
    DebugLogger::Init();

    CommandBridge::Init(&g_serial);
    MonitorPush::Init(&g_kmServer);
    g_serial.SetResponseCallback(OnSerialResponseCombo, nullptr);

    // Start named pipe server for Electron UI
    g_pipeServer.Start();
    g_pipeServer.SetCommandCallback(OnPipeCommand, nullptr);
    g_app.pipeServer = &g_pipeServer;

    // Window class
    WNDCLASSEXW wc = { sizeof(wc) };
    wc.lpfnWndProc   = WndProc;
    wc.hInstance      = hInstance;
    wc.hIcon          = LoadIcon(NULL, IDI_APPLICATION);
    wc.hCursor        = LoadCursor(NULL, IDC_ARROW);
    wc.hbrBackground  = (HBRUSH)(COLOR_WINDOW + 1);
    wc.lpszClassName  = L"Ysc8kDriverClass";
    RegisterClassExW(&wc);

    // Debug window class
    WNDCLASSEXW dwc = { sizeof(dwc) };
    dwc.lpfnWndProc   = DebugWndProc;
    dwc.hInstance      = hInstance;
    dwc.hCursor        = LoadCursor(NULL, IDC_ARROW);
    dwc.hbrBackground  = (HBRUSH)(COLOR_WINDOW + 1);
    dwc.lpszClassName  = L"YscDebugClass";
    RegisterClassExW(&dwc);

    // Hidden window
    g_app.hwndMain = CreateWindowExW(0, wc.lpszClassName, L"YSC 8K Driver",
        WS_OVERLAPPED, CW_USEDEFAULT, CW_USEDEFAULT, 400, 300,
        NULL, NULL, hInstance, NULL);
    if (!g_app.hwndMain) {
        WSACleanup();
        return 1;
    }

    // 读线程异常退出（串口物理断开/IO 错误）时，通过此窗口 PostMessage 通知主线程清理。
    g_serial.SetNotifyWindow(g_app.hwndMain);

    // 注册 COM 口设备接口变更通知 —— 让隐藏窗口收到 WM_DEVICECHANGE（插拔即时推送）。
    // 仅限 GUID_DEVINTERFACE_COMPORT，避免 U 盘/HID 等无关设备刷屏。
    // 失败不致命：窗口聚焦刷新 enum_ports 仍是兜底。
    {
        DEV_BROADCAST_DEVICEINTERFACE_W filter = {};
        filter.dbcc_size = sizeof(filter);
        filter.dbcc_devicetype = DBT_DEVTYP_DEVICEINTERFACE;
        filter.dbcc_classguid = GUID_DEVINTERFACE_COMPORT_LOCAL;
        g_app.hDevNotify = RegisterDeviceNotificationW(
            g_app.hwndMain, &filter, DEVICE_NOTIFY_WINDOW_HANDLE);
    }

    g_app.hStopEvent = CreateEvent(NULL, TRUE, FALSE, NULL);
    InitializeCriticalSection(&g_app.csSerialWrite);

    // Tray icon
    g_app.nid.cbSize           = sizeof(g_app.nid);
    g_app.nid.hWnd             = g_app.hwndMain;
    g_app.nid.uID              = 1;
    g_app.nid.uFlags           = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    g_app.nid.uCallbackMessage = WM_TRAYICON;
    g_app.nid.hIcon            = LoadIcon(NULL, IDI_APPLICATION);
    wcscpy_s(g_app.nid.szTip, L"YSC 8K Driver - 未连接");
    Shell_NotifyIconW(NIM_ADD, &g_app.nid);

    // Message loop
    MSG msg;
    while (GetMessage(&msg, NULL, 0, 0)) {
        TranslateMessage(&msg);
        DispatchMessage(&msg);
    }

    // Cleanup
    g_pipeServer.Stop();
    DebugLogger::Shutdown();
    g_kmServer.Stop();
    g_serial.Disconnect();
    if (g_app.hwndDebug) DestroyWindow(g_app.hwndDebug);
    Shell_NotifyIconW(NIM_DELETE, &g_app.nid);
    if (g_app.hStopEvent) CloseHandle(g_app.hStopEvent);
    DeleteCriticalSection(&g_app.csSerialWrite);
    timeEndPeriod(1);
    WSACleanup();
    if (g_app.hMutex) CloseHandle(g_app.hMutex);

    return (int)msg.wParam;
}

// ========== Window Procedure ==========
LRESULT CALLBACK WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    switch (msg) {
    case WM_TRAYICON:
        if (LOWORD(lParam) == WM_RBUTTONUP)
            ShowTrayMenu(hwnd);
        break;

    case WM_DEBUG_LOG: {
        char *msg = (char*)lParam;
        if (g_app.hwndDebugEdit && msg) {
            int textLen = GetWindowTextLengthA(g_app.hwndDebugEdit);
            if (textLen > 65536) {
                SendMessageA(g_app.hwndDebugEdit, EM_SETSEL, 0, textLen / 2);
                SendMessageA(g_app.hwndDebugEdit, EM_REPLACESEL, FALSE, (LPARAM)"");
                textLen = GetWindowTextLengthA(g_app.hwndDebugEdit);
            }
            SendMessageA(g_app.hwndDebugEdit, EM_SETSEL, textLen, textLen);
            SendMessageA(g_app.hwndDebugEdit, EM_REPLACESEL, FALSE, (LPARAM)msg);
        }
        free(msg);
        break;
    }

    case WM_COMMAND:
        HandleMenuCommand(LOWORD(wParam));
        break;

    case WM_DESTROY:
        if (g_app.hDevNotify) {
            UnregisterDeviceNotification(g_app.hDevNotify);
            g_app.hDevNotify = NULL;
        }
        SetEvent(g_app.hStopEvent);
        PostQuitMessage(0);
        break;

    case WM_SERIAL_LOST: {
        // 读线程异常退出（串口物理断开/IO 错误）上报。guard：仅当仍是当前 reader
        // （wParam 代际 == 当前 m_readerGen，即期间没重连换新 reader）且确实还"已连接"
        // 时才清理 —— 防止重连后的过期消息误断新连接，也挡住重复上报。
        // 全程主线程，与其它命令处理串行，无并发；Disconnect 此刻读线程已退出，立即返回。
        if (g_app.serialConnected && (LONG)wParam == g_serial.ReaderGen()) {
            DebugLogger::Log("SERIAL unexpected disconnect (physical/IO), cleaning up");
            g_serial.Disconnect();
            g_app.serialConnected = false;
            UpdateTrayTip();
            g_pipeServer.SendEvent("serial_disconnected", nullptr);
        }
        break;
    }

    case WM_DEVICECHANGE: {
        // COM 口插拔：一次插拔连发多条 WM_DEVICECHANGE（设备接口 + 端口设备节点等），
        // 用定时器防抖合并成一次 EnumComPorts 推送。
        if (wParam == DBT_DEVICEARRIVAL || wParam == DBT_DEVICEREMOVECOMPLETE) {
            KillTimer(hwnd, TIMER_PORT_CHANGE);
            SetTimer(hwnd, TIMER_PORT_CHANGE, PORT_CHANGE_DEBOUNCE_MS, NULL);
        }
        return TRUE;  // WM_DEVICECHANGE 规范要求返回 TRUE
    }

    case WM_TIMER: {
        if (wParam == TIMER_PORT_CHANGE) {
            KillTimer(hwnd, TIMER_PORT_CHANGE);
            auto ports = EnumComPorts();
            std::string body = "\"ports\":[";
            for (size_t i = 0; i < ports.size(); i++) {
                if (i > 0) body += ",";
                std::string p(ports[i].begin(), ports[i].end());
                body += "\"" + p + "\"";
            }
            body += "]";
            // 同时推两份：ports_changed(语义事件) + ports_list(零改动复用 App.vue 现有监听器)
            g_pipeServer.SendEvent("ports_changed", body.c_str());
            g_pipeServer.SendEvent("ports_list", body.c_str());
        }
        return 0;
    }

    default:
        return DefWindowProc(hwnd, msg, wParam, lParam);
    }
    return 0;
}

// ========== Menu Command Handler ==========
void HandleMenuCommand(UINT cmdId) {
    // COM port selection
    if (cmdId >= IDM_COM_PORT_BASE && cmdId <= IDM_COM_PORT_MAX) {
        int index = cmdId - IDM_COM_PORT_BASE;
        auto ports = EnumComPorts();
        if (index < (int)ports.size()) {
            // Auto-detect baudrate
            uint32_t baud = SerialPort::DetectBaudrate(ports[index].c_str());
            if (baud == 0) {
                DebugLog("DetectBaudrate FAILED on %ls", ports[index].c_str());
                MessageBoxW(g_app.hwndMain,
                    L"无法检测到设备\n请确认设备已连接且上电",
                    L"错误", MB_OK | MB_ICONERROR);
                return;
            }
            if (g_serial.Connect(ports[index].c_str(), baud)) {
                DebugLog("Serial CONNECTED %ls @ %u", ports[index].c_str(), baud);
                DebugLogger::Log("SERIAL CONNECTED %ls @ %u", ports[index].c_str(), baud);
                wcscpy_s(g_app.serialPortName, ports[index].c_str());
                g_app.serialBaudRate = baud;
                g_app.serialConnected = true;
                g_serial.StartReadThread(g_app.hStopEvent);
                UpdateTrayTip();
                // Notify Electron
                std::string portName(ports[index].begin(), ports[index].end());
                char body[128];
                snprintf(body, sizeof(body), "\"port\":\"%s\",\"baud\":%u", portName.c_str(), baud);
                g_pipeServer.SendEvent("serial_connected", body);
            } else {
                MessageBoxW(g_app.hwndMain, L"无法打开串口", L"错误",
                            MB_OK | MB_ICONERROR);
            }
        }
        return;
    }

    // Baudrate switching
    if (cmdId >= IDM_BAUDRATE_BASE && cmdId <= IDM_BAUDRATE_MAX) {
        int index = cmdId - IDM_BAUDRATE_BASE;
        if (index < SUPPORTED_BAUD_COUNT && g_app.serialConnected) {
            uint32_t newBaud = SUPPORTED_BAUDS[index];
            if (newBaud != g_app.serialBaudRate) {
                if (g_serial.SwitchBaudrate(newBaud)) {
                    g_app.serialBaudRate = newBaud;
                    g_serial.StartReadThread(g_app.hStopEvent);
                    UpdateTrayTip();
                    char body[64];
                    snprintf(body, sizeof(body), "\"baud\":%u", newBaud);
                    g_pipeServer.SendEvent("baudrate_switched", body);
                } else {
                    // Switch failed, device may be disconnected
                    g_app.serialConnected = false;
                    UpdateTrayTip();
                    g_pipeServer.SendEvent("baudrate_failed", "\"message\":\"切换失败，设备可能已断开\"");
                    MessageBoxW(g_app.hwndMain,
                        L"波特率切换失败\n设备可能已断开",
                        L"错误", MB_OK | MB_ICONERROR);
                }
            }
        }
        return;
    }

    switch (cmdId) {
    case IDM_DISCONNECT_SERIAL:
        g_serial.Disconnect();
        g_app.serialConnected = false;
        DebugLogger::Log("SERIAL DISCONNECTED");
        UpdateTrayTip();
        g_pipeServer.SendEvent("serial_disconnected", nullptr);
        break;

    case IDM_NET_SERVICE:
        if (g_app.netServerRunning) {
            DebugLog("Net service STOP");
            DebugLogger::Log("NET SERVICE STOP");
            g_kmServer.Stop();
            g_app.netServerRunning = false;
            g_pipeServer.SendEvent("kmnet_stopped", nullptr);
        } else {
            if (g_kmServer.Start(g_app.netPort)) {
                DebugLog("Net service START on port %u", g_app.netPort);
                DebugLogger::Log("NET SERVICE START on port %u", g_app.netPort);
                g_app.netServerRunning = true;
                std::string ip = GetLocalIP();
                char body[128];
                snprintf(body, sizeof(body), "\"port\":%u,\"ip\":\"%s\",\"mac\":\"%08X\"", g_app.netPort, ip.c_str(), DEFAULT_MAC);
                g_pipeServer.SendEvent("kmnet_started", body);
            } else {
                MessageBoxW(g_app.hwndMain, L"Net 服务启动失败", L"错误",
                            MB_OK | MB_ICONERROR);
            }
        }
        UpdateTrayTip();
        break;


    case IDM_DEBUG_LOG:
        if (g_app.debugWindowVisible) {
            g_app.debugWindowVisible = false;
            DebugLogger::SetDebugWindowOpen(false);
            if (g_app.hwndDebug) ShowWindow(g_app.hwndDebug, SW_HIDE);
        } else {
            CreateDebugWindow();
            DebugLog("=== Debug window opened ===");
        }
        break;

    case IDM_EXIT:
        DestroyWindow(g_app.hwndMain);
        break;
    }
}

// ========== Tray Menu (dynamic) ==========
static void ShowTrayMenu(HWND hwnd) {
    POINT pt;
    GetCursorPos(&pt);

    HMENU hMenu = CreatePopupMenu();

    if (g_app.serialConnected) {
        wchar_t text[128];
        swprintf_s(text, L"断开串口 (%s @ %lu)",
                   g_app.serialPortName, g_app.serialBaudRate);
        AppendMenuW(hMenu, MF_STRING, IDM_DISCONNECT_SERIAL, text);

        // Baudrate submenu
        HMENU hBaud = CreatePopupMenu();
        for (int i = 0; i < SUPPORTED_BAUD_COUNT; i++) {
            UINT flags = MF_STRING;
            if (SUPPORTED_BAUDS[i] == g_app.serialBaudRate) flags |= MF_CHECKED;
            wchar_t label[32];
            swprintf_s(label, L"%lu", SUPPORTED_BAUDS[i]);
            AppendMenuW(hBaud, flags, IDM_BAUDRATE_BASE + i, label);
        }
        AppendMenuW(hMenu, MF_POPUP, (UINT_PTR)hBaud, L"切换波特率");
    } else {
        auto ports = EnumComPorts();
        if (ports.empty()) {
            AppendMenuW(hMenu, MF_STRING | MF_GRAYED, 0, L"无可用串口");
        } else {
            HMENU hCom = CreatePopupMenu();
            for (int i = 0; i < (int)ports.size(); i++)
                AppendMenuW(hCom, MF_STRING, IDM_COM_PORT_BASE + i,
                            ports[i].c_str());
            AppendMenuW(hMenu, MF_POPUP, (UINT_PTR)hCom, L"连接串口");
        }
    }

    AppendMenuW(hMenu, MF_SEPARATOR, 0, NULL);

    // Net service with IP:port display
    UINT netFlags = MF_STRING | (g_app.netServerRunning ? MF_CHECKED : 0);
    wchar_t netLabel[128];
    if (g_app.netServerRunning) {
        std::string ip = GetLocalIP();
        std::wstring wip(ip.begin(), ip.end());
        swprintf_s(netLabel, L"Net 服务 (%s:%u)", wip.c_str(), g_app.netPort);
    } else {
        wcscpy_s(netLabel, L"Net 服务");
    }
    AppendMenuW(hMenu, netFlags, IDM_NET_SERVICE, netLabel);

    AppendMenuW(hMenu, MF_SEPARATOR, 0, NULL);
    UINT logFlags = MF_STRING | (g_app.debugWindowVisible ? MF_CHECKED : 0);
    AppendMenuW(hMenu, logFlags, IDM_DEBUG_LOG, L"调试窗口");
    AppendMenuW(hMenu, MF_SEPARATOR, 0, NULL);
    AppendMenuW(hMenu, MF_STRING, IDM_EXIT,     L"退出");

    SetForegroundWindow(hwnd);
    TrackPopupMenu(hMenu, TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
                   pt.x, pt.y, 0, hwnd, NULL);
    PostMessage(hwnd, WM_NULL, 0, 0);

    DestroyMenu(hMenu);
}

// ========== COM Port Enumeration ==========
static std::vector<std::wstring> EnumComPorts() {
    std::vector<std::wstring> ports;
    HKEY hKey;
    if (RegOpenKeyExW(HKEY_LOCAL_MACHINE,
            L"HARDWARE\\DEVICEMAP\\SERIALCOMM",
            0, KEY_READ, &hKey) != ERROR_SUCCESS)
        return ports;

    DWORD index = 0;
    for (;;) {
        wchar_t name[256];
        BYTE data[256];
        DWORD nameSize = _countof(name);
        DWORD dataSize = sizeof(data);
        DWORD type;
        LONG rc = RegEnumValueW(hKey, index, name, &nameSize,
                                NULL, &type, data, &dataSize);
        if (rc != ERROR_SUCCESS) break;
        if (type == REG_SZ && dataSize >= sizeof(wchar_t)) {
            wchar_t* s = reinterpret_cast<wchar_t*>(data);
            size_t len = wcsnlen(s, dataSize / sizeof(wchar_t));
            ports.emplace_back(s, len);
        }
        index++;
    }

    RegCloseKey(hKey);

    std::sort(ports.begin(), ports.end(),
        [](const std::wstring& a, const std::wstring& b) {
            if (a.size() > 3 && b.size() > 3 &&
                a.compare(0, 3, L"COM") == 0 &&
                b.compare(0, 3, L"COM") == 0) {
                return _wtoi(a.c_str() + 3) < _wtoi(b.c_str() + 3);
            }
            return a < b;
        });

    return ports;
}

// ========== Tray Tooltip ==========
static void UpdateTrayTip() {
    wchar_t tip[128] = L"YSC 8K Driver";

    if (g_app.serialConnected) {
        wchar_t buf[64];
        swprintf_s(buf, L" - %s @ %lu", g_app.serialPortName,
                   g_app.serialBaudRate);
        wcscat_s(tip, buf);
    } else {
        wcscat_s(tip, L" - 未连接");
    }

    if (g_app.netServerRunning) {
        wchar_t buf[32];
        swprintf_s(buf, L" | Net:%u", g_app.netPort);
        wcscat_s(tip, buf);
    }

    wcscpy_s(g_app.nid.szTip, tip);
    Shell_NotifyIconW(NIM_MODIFY, &g_app.nid);
}
