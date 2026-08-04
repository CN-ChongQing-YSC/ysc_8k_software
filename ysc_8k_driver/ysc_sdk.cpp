// ysc_sdk.cpp — YSC 8K 驱动 SDK 实现
// ============================================================================
// 自包含的 DLL 实现。串口连接 / 帧编解码 / 波特率探测与切换 逻辑严格对照
// ysc_8k_driver/serial_port.cpp（同一帧格式、同一 DCB、同一探测/切换流程），
// 仅从“异步读写线程 + 回调”模型改写为“同步发送并等待返回”模型，方便易语言 /
// Python 等语言直接调用。串口枚举复用 towmcu_cdc.cpp 的 EnumTowmcuPorts()。
// ============================================================================

#define WIN32_LEAN_AND_MEAN
#include <winsock2.h>
#include <windows.h>
#include <setupapi.h>
#include <cfgmgr32.h>

#include <cstdio>
#include <cstring>
#include <cwchar>
#include <string>
#include <vector>
#include <algorithm>

#include "ysc_sdk.h"
#include "towmcu_cdc.h"

#pragma comment(lib, "setupapi.lib")
#pragma comment(lib, "cfgmgr32.lib")

// ----------------------------------------------------------------------------
// 常量（与 serial_port.h / main.h 保持一致）
// ----------------------------------------------------------------------------
static const uint32_t SUPPORTED_BAUDS[] = {
    115200, 230400, 460800, 921600, 1000000, 1500000, 2000000, 3000000, 4000000
};
static const int SUPPORTED_BAUD_COUNT = (int)(sizeof(SUPPORTED_BAUDS) / sizeof(SUPPORTED_BAUDS[0]));

static constexpr const char *START_MARKER = "<START>";
static constexpr const char *END_MARKER   = "<END>";
static constexpr int START_MARKER_LEN     = 7;
static constexpr int END_MARKER_LEN       = 5;
static constexpr int HEADER_LEN           = START_MARKER_LEN + 2; // <START> + 2B 长度
static constexpr int MAX_PAYLOAD          = 4096;

#define YSC_SDK_VERSION "1.0.0"

// 线程局部的最近一次错误描述
static thread_local char g_lastErr[512] = {0};

static void SetErr(const char *fmt, ...) {
    va_list args;
    va_start(args, fmt);
    vsnprintf(g_lastErr, sizeof(g_lastErr), fmt, args);
    va_end(args);
}

// 安全字符串拷贝（始终 '\0' 结尾）
static void SafeCopy(char *dst, int dstSize, const char *src) {
    if (!dst || dstSize <= 0) return;
    if (!src) { dst[0] = '\0'; return; }
    int i = 0;
    for (; i < dstSize - 1 && src[i]; i++) dst[i] = src[i];
    dst[i] = '\0';
}

// ----------------------------------------------------------------------------
// 帧编解码（对照 serial_port.cpp::SerialPort::SendJsonCommand / BuildVersionFrame）
// ----------------------------------------------------------------------------
static int BuildFrame(uint8_t *buf, int bufCap, const char *json) {
    int jsonLen = (int)strlen(json);
    uint16_t packetLen = (uint16_t)(HEADER_LEN + jsonLen + END_MARKER_LEN); // = 14 + jsonLen
    int total = START_MARKER_LEN + 2 + jsonLen + END_MARKER_LEN;
    if (total > bufCap) return -1;
    int pos = 0;
    memcpy(buf + pos, START_MARKER, START_MARKER_LEN); pos += START_MARKER_LEN;
    buf[pos++] = (uint8_t)(packetLen >> 8);          // 大端
    buf[pos++] = (uint8_t)(packetLen & 0xFF);
    memcpy(buf + pos, json, jsonLen);                 pos += jsonLen;
    memcpy(buf + pos, END_MARKER, END_MARKER_LEN);    pos += END_MARKER_LEN;
    return pos;
}

// 校验缓冲区里是否含一条带 "code":200 的完整响应帧（对照 HasValidResponse）
static bool HasValidResponse(const uint8_t *rx, int rxLen) {
    for (int i = 0; i <= rxLen - 14; i++) {
        if (memcmp(rx + i, START_MARKER, START_MARKER_LEN) != 0) continue;
        if (i + 9 > rxLen) continue;
        uint16_t totalLen = ((uint16_t)rx[i + 7] << 8) | rx[i + 8];
        int endPos = i + totalLen;
        if (endPos > rxLen || endPos < 5) continue;
        if (memcmp(rx + endPos - 5, END_MARKER, END_MARKER_LEN) != 0) continue;
        int payloadStart = i + 9;
        int payloadLen = totalLen - 14;
        if (payloadLen <= 0 || payloadStart + payloadLen > rxLen) continue;
        char tmp[256];
        if (payloadLen >= (int)sizeof(tmp)) continue;
        memcpy(tmp, rx + payloadStart, payloadLen);
        tmp[payloadLen] = '\0';
        if (strstr(tmp, "\"code\":200") || strstr(tmp, "\"code\": 200"))
            return true;
    }
    return false;
}

// ----------------------------------------------------------------------------
// 连接句柄
// ----------------------------------------------------------------------------
struct YscDevice {
    HANDLE  hPort;
    uint32_t baud;
    wchar_t portName[16];

    // 同步发送/接收互斥，避免多线程破坏帧状态机
    CRITICAL_SECTION cs;

    YscDevice() : hPort(NULL), baud(0) {
        portName[0] = L'\0';
        InitializeCriticalSection(&cs);
    }
    ~YscDevice() {
        DeleteCriticalSection(&cs);
    }
};

// 打开一个同步（非 overlapped）串口，设置 DCB/超时（对照 DetectBaudrate 的打开方式）
static HANDLE OpenComSync(const wchar_t *portName, uint32_t baud,
                          uint32_t readTotalTimeoutMs) {
    wchar_t fullPath[64];
    swprintf_s(fullPath, L"\\\\.\\%s", portName);

    HANDLE h = CreateFileW(fullPath, GENERIC_READ | GENERIC_WRITE, 0, NULL,
                           OPEN_EXISTING, 0, NULL); // 注意：非 overlapped
    if (h == INVALID_HANDLE_VALUE) return NULL;

    DCB dcb = {};
    dcb.DCBlength = sizeof(dcb);
    if (!GetCommState(h, &dcb)) { CloseHandle(h); return NULL; }
    dcb.BaudRate = baud;
    dcb.ByteSize = 8;
    dcb.Parity   = NOPARITY;
    dcb.StopBits = ONESTOPBIT;
    dcb.fDtrControl = DTR_CONTROL_ENABLE;
    dcb.fRtsControl = RTS_CONTROL_ENABLE;
    if (!SetCommState(h, &dcb)) { CloseHandle(h); return NULL; }

    COMMTIMEOUTS to = {};
    to.ReadIntervalTimeout        = 1;
    to.ReadTotalTimeoutConstant   = readTotalTimeoutMs;
    to.ReadTotalTimeoutMultiplier = 0;
    to.WriteTotalTimeoutConstant  = 200;
    to.WriteTotalTimeoutMultiplier = 0;
    SetCommTimeouts(h, &to);

    PurgeComm(h, PURGE_RXCLEAR | PURGE_TXCLEAR);
    return h;
}

// ----------------------------------------------------------------------------
// 同步收一条完整帧：边读边跑帧状态机（对照 ReadLoop 的 RX 状态机）
// 成功返回 payload 长度并填 payload（不含 <END>），失败/超时返回 -1
// ----------------------------------------------------------------------------
static int RecvFrame(HANDLE hPort, int timeoutMs,
                     uint8_t *payload, int payloadCap) {
    enum RxState { RX_IDLE, RX_HEADER, RX_PAYLOAD, RX_END } state = RX_IDLE;
    int startMatch = 0;
    uint8_t hdr[2];
    int hdrLen = 0;
    int payloadLen = 0;
    int payloadGot = 0;
    uint8_t endBuf[END_MARKER_LEN];
    int endGot = 0;

    DWORD t0 = GetTickCount();
    DWORD deadline = t0 + (DWORD)(timeoutMs < 0 ? 1000 : timeoutMs);

    uint8_t chunk[512];
    for (;;) {
        if (GetTickCount() >= deadline) return -1;

        DWORD got = 0;
        if (!ReadFile(hPort, chunk, sizeof(chunk), &got, NULL)) return -1;
        if (got == 0) { Sleep(1); continue; } // 让出 CPU，继续等

        for (DWORD i = 0; i < got; i++) {
            uint8_t b = chunk[i];
            switch (state) {
            case RX_IDLE:
                if (b == (uint8_t)START_MARKER[startMatch]) {
                    if (++startMatch == START_MARKER_LEN) {
                        startMatch = 0;
                        state = RX_HEADER;
                        hdrLen = 0;
                    }
                } else {
                    if (startMatch > 0)
                        startMatch = (b == '<') ? 1 : 0;
                }
                break;

            case RX_HEADER:
                hdr[hdrLen++] = b;
                if (hdrLen == 2) {
                    uint16_t totalLen = ((uint16_t)hdr[0] << 8) | hdr[1];
                    payloadLen = totalLen - HEADER_LEN - END_MARKER_LEN;
                    if (payloadLen < 0 || payloadLen > MAX_PAYLOAD) {
                        state = RX_IDLE; // 坏帧，重新同步
                    } else {
                        payloadGot = 0;
                        endGot = 0;
                        state = (payloadLen == 0) ? RX_END : RX_PAYLOAD;
                    }
                }
                break;

            case RX_PAYLOAD:
                if (payloadGot < payloadCap)
                    payload[payloadGot] = b;
                if (++payloadGot == payloadLen)
                    state = RX_END;
                break;

            case RX_END:
                endBuf[endGot++] = b;
                if (endGot == END_MARKER_LEN) {
                    if (memcmp(endBuf, END_MARKER, END_MARKER_LEN) == 0)
                        return payloadLen; // 完整一帧
                    state = RX_IDLE;       // 末尾不匹配，重新同步
                }
                break;
            }
        }
    }
}

// 同步发送一帧 + 收一帧。返回 payload 长度或负数（-2 超时，-1 其它错误）
static int SendAndWait(YscDevice *dev, const char *json, int timeoutMs,
                       char *outBuf, int bufSize) {
    if (!dev || !dev->hPort) { SetErr("not connected"); return -1; }
    if (!json) { SetErr("null json"); return -1; }

    uint8_t frame[4608];
    int frameLen = BuildFrame(frame, sizeof(frame), json);
    if (frameLen < 0) { SetErr("json too long"); return -1; }

    PurgeComm(dev->hPort, PURGE_RXCLEAR | PURGE_TXCLEAR);

    DWORD written = 0;
    if (!WriteFile(dev->hPort, frame, frameLen, &written, NULL) ||
        (int)written != frameLen) {
        SetErr("WriteFile failed, err=%lu", GetLastError());
        return -1;
    }

    uint8_t payload[MAX_PAYLOAD];
    int n = RecvFrame(dev->hPort, timeoutMs, payload, sizeof(payload));
    if (n < 0) { SetErr("timeout / no response within %d ms", timeoutMs); return -2; }

    if (outBuf && bufSize > 0) {
        int copy = (n < bufSize - 1) ? n : (bufSize - 1);
        memcpy(outBuf, payload, copy);
        outBuf[copy] = '\0';
    }
    return n; // 返回真实负载长度（可能 > bufSize-1 表示截断）
}

// ----------------------------------------------------------------------------
// 0. 信息
// ----------------------------------------------------------------------------
YSC_API const char* YSC_CALL Ysc_SdkVersion(void) { return YSC_SDK_VERSION; }

YSC_API const char* YSC_CALL Ysc_LastError(void) { return g_lastErr; }

YSC_API const uint32_t* YSC_CALL Ysc_SupportedBaudrates(int *count) {
    if (count) *count = SUPPORTED_BAUD_COUNT;
    return SUPPORTED_BAUDS;
}

// ----------------------------------------------------------------------------
// JSON 字符串转义
// ----------------------------------------------------------------------------
static void JsonEscapeAppend(std::string &out, const char *s) {
    if (!s) return;
    for (const char *p = s; *p; p++) {
        switch (*p) {
        case '"':  out += "\\\""; break;
        case '\\': out += "\\\\"; break;
        case '\n': out += "\\n";  break;
        case '\r': out += "\\r";  break;
        case '\t': out += "\\t";  break;
        default:
            if ((unsigned char)*p < 0x20) {
                char b[8];
                snprintf(b, sizeof(b), "\\u%04x", (unsigned)*p);
                out += b;
            } else {
                out += *p;
            }
        }
    }
}

static std::string NarrowW(const wchar_t *w) {
    if (!w) return "";
    char buf[512];
    WideCharToMultiByte(CP_UTF8, 0, w, -1, buf, (int)sizeof(buf), NULL, NULL);
    return std::string(buf);
}

// ----------------------------------------------------------------------------
// 1. 串口枚举
// ----------------------------------------------------------------------------

// —— GUID_DEVINTERFACE_COMPORT ——
static const GUID GUID_DEVINTERFACE_COMPORT_LOCAL = {
    0x86e0d1e0, 0x8089, 0x11d0,
    { 0x9c, 0xe4, 0x08, 0x00, 0x3e, 0x30, 0x1f, 0x73 }
};

// 枚举全部 COM 口（不区分厂商）。返回 (portName, friendly) 列表
struct AnyComPort { std::wstring name; std::string desc; };
static std::vector<AnyComPort> EnumAllComPorts() {
    std::vector<AnyComPort> out;
    HDEVINFO hDev = SetupDiGetClassDevsW(&GUID_DEVINTERFACE_COMPORT_LOCAL, NULL, NULL,
                                         DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
    if (hDev == INVALID_HANDLE_VALUE) return out;

    SP_DEVINFO_DATA did = { sizeof(did) };
    for (DWORD i = 0; SetupDiEnumDeviceInfo(hDev, i, &did); i++) {
        HKEY hKey = SetupDiOpenDevRegKey(hDev, &did, DICS_FLAG_GLOBAL, 0,
                                         DIREG_DEV, KEY_READ);
        if (hKey == INVALID_HANDLE_VALUE) continue;
        wchar_t portName[64] = {};
        DWORD type = 0, sz = sizeof(portName);
        LONG rc = RegQueryValueExW(hKey, L"PortName", NULL, &type, (LPBYTE)portName, &sz);
        RegCloseKey(hKey);
        if (rc != ERROR_SUCCESS || portName[0] == L'\0') continue;

        AnyComPort p;
        p.name = portName;
        wchar_t friendly[256] = {};
        if (SetupDiGetDeviceRegistryPropertyW(hDev, &did, SPDRP_FRIENDLYNAME,
                NULL, (PBYTE)friendly, sizeof(friendly), NULL) && friendly[0]) {
            p.desc = NarrowW(friendly);
        } else {
            wchar_t devdesc[256] = {};
            if (SetupDiGetDeviceRegistryPropertyW(hDev, &did, SPDRP_DEVICEDESC,
                    NULL, (PBYTE)devdesc, sizeof(devdesc), NULL) && devdesc[0])
                p.desc = NarrowW(devdesc);
        }
        out.push_back(std::move(p));
    }
    SetupDiDestroyDeviceInfoList(hDev);

    std::sort(out.begin(), out.end(), [](const AnyComPort &a, const AnyComPort &b) {
        // 按 COM 数字升序
        auto num = [](const std::wstring &n) -> int {
            if (n.size() > 3 && n.compare(0, 3, L"COM") == 0) return _wtoi(n.c_str() + 3);
            return 0;
        };
        return num(a.name) < num(b.name);
    });
    return out;
}

static int WriteJsonResult(char *outBuf, int bufSize, const std::string &s) {
    int need = (int)s.size();
    if (outBuf && bufSize > 0) {
        int copy = (need < bufSize - 1) ? need : (bufSize - 1);
        memcpy(outBuf, s.c_str(), copy);
        outBuf[copy] = '\0';
    }
    return need;
}

YSC_API int YSC_CALL Ysc_ListPorts(char *outBuf, int bufSize) {
    std::vector<TowmcuPort> ports = EnumTowmcuPorts();
    std::string s = "[";
    for (size_t i = 0; i < ports.size(); i++) {
        std::string port = NarrowW(ports[i].portName.c_str());
        s += "{\"port\":\""; JsonEscapeAppend(s, port.c_str());
        s += "\",\"serial\":\""; JsonEscapeAppend(s, ports[i].usbSerial.c_str());
        s += "\",\"side\":\""; JsonEscapeAppend(s, ports[i].side.c_str());
        s += "\",\"desc\":\""; JsonEscapeAppend(s, ports[i].description.c_str());
        s += "\"}";
        if (i + 1 < ports.size()) s += ",";
    }
    s += "]";
    return WriteJsonResult(outBuf, bufSize, s);
}

YSC_API int YSC_CALL Ysc_ListAllComPorts(char *outBuf, int bufSize) {
    std::vector<AnyComPort> ports = EnumAllComPorts();
    std::string s = "[";
    for (size_t i = 0; i < ports.size(); i++) {
        std::string port = NarrowW(ports[i].name.c_str());
        s += "{\"port\":\""; JsonEscapeAppend(s, port.c_str());
        s += "\",\"desc\":\""; JsonEscapeAppend(s, ports[i].desc.c_str());
        s += "\"}";
        if (i + 1 < ports.size()) s += ",";
    }
    s += "]";
    return WriteJsonResult(outBuf, bufSize, s);
}

// ----------------------------------------------------------------------------
// 2. 连接生命周期
// ----------------------------------------------------------------------------
YSC_API uint32_t YSC_CALL Ysc_DetectBaudrate(const char *portName) {
    if (!portName) { SetErr("null portName"); return 0; }

    wchar_t wport[16];
    MultiByteToWideChar(CP_UTF8, 0, portName, -1, wport, 16);

    uint8_t frame[64];
    int frameLen = BuildFrame(frame, sizeof(frame), "{\"cmd\":132}");
    if (frameLen < 0) { SetErr("build frame failed"); return 0; }

    // 收敛式探测顺序：4M -> 115200 -> 3M -> 230400 ...（对照 DetectBaudrate）
    int probeOrder[16];
    int left = 0, right = SUPPORTED_BAUD_COUNT - 1, pos = 0;
    bool highNext = true;
    while (left <= right) {
        probeOrder[pos++] = highNext ? right-- : left++;
        highNext = !highNext;
    }

    for (int oi = 0; oi < SUPPORTED_BAUD_COUNT; oi++) {
        uint32_t baud = SUPPORTED_BAUDS[probeOrder[oi]];
        HANDLE h = OpenComSync(wport, baud, 50);
        if (!h) continue;

        PurgeComm(h, PURGE_RXCLEAR | PURGE_TXCLEAR);
        Sleep(10);

        DWORD written = 0;
        WriteFile(h, frame, frameLen, &written, NULL);

        uint8_t rxBuf[256];
        int rxLen = 0;
        DWORD t0 = GetTickCount();
        while (GetTickCount() - t0 < 200 && rxLen < (int)sizeof(rxBuf)) {
            DWORD got = 0;
            if (!ReadFile(h, rxBuf + rxLen, sizeof(rxBuf) - rxLen, &got, NULL)) break;
            if (got > 0) {
                rxLen += (int)got;
                if (HasValidResponse(rxBuf, rxLen)) {
                    CloseHandle(h);
                    return baud;
                }
            }
        }
        CloseHandle(h);
    }
    SetErr("no baudrate responded to {\"cmd\":132}");
    return 0;
}

YSC_API YscDevice* YSC_CALL Ysc_Connect(const char *portName, uint32_t baudRate,
                                        char *errBuf, int errBufSize) {
    if (errBuf && errBufSize > 0) errBuf[0] = '\0';
    if (!portName) {
        SetErr("null portName");
        SafeCopy(errBuf, errBufSize, "null portName");
        return NULL;
    }

    YscDevice *dev = new (std::nothrow) YscDevice();
    if (!dev) { SetErr("out of memory"); return NULL; }

    MultiByteToWideChar(CP_UTF8, 0, portName, -1, dev->portName, 16);

    uint32_t baud = baudRate;
    if (baud == 0) {
        baud = Ysc_DetectBaudrate(portName);
        if (baud == 0) {
            SafeCopy(errBuf, errBufSize, g_lastErr);
            delete dev;
            return NULL;
        }
    }

    dev->hPort = OpenComSync(dev->portName, baud, 20);
    if (!dev->hPort) {
        DWORD e = GetLastError();
        SetErr("open %s @ %u failed, err=%lu", portName, baud, e);
        if (errBuf && errBufSize > 0)
            snprintf(errBuf, errBufSize, "open %s @ %u failed (err=%lu)", portName, baud, e);
        delete dev;
        return NULL;
    }
    dev->baud = baud;
    return dev;
}

YSC_API void YSC_CALL Ysc_Disconnect(YscDevice *dev) {
    if (!dev) return;
    if (dev->hPort) { CloseHandle(dev->hPort); dev->hPort = NULL; }
    dev->baud = 0;
    delete dev;
}

YSC_API int YSC_CALL Ysc_IsConnected(YscDevice *dev) {
    return dev && dev->hPort ? 1 : 0;
}

YSC_API uint32_t YSC_CALL Ysc_GetBaudrate(YscDevice *dev) {
    return dev ? dev->baud : 0;
}

YSC_API int YSC_CALL Ysc_GetPortName(YscDevice *dev, char *outBuf, int bufSize) {
    if (!dev) { SetErr("null dev"); return -1; }
    std::string s = NarrowW(dev->portName);
    return WriteJsonResult(outBuf, bufSize, s);
}

// ----------------------------------------------------------------------------
// 3. 波特率切换（对照 SerialPort::SwitchBaudrate）
// ----------------------------------------------------------------------------
YSC_API int YSC_CALL Ysc_SwitchBaudrate(YscDevice *dev, uint32_t newBaud) {
    if (!dev || !dev->hPort) { SetErr("not connected"); return 0; }

    bool valid = false;
    for (int i = 0; i < SUPPORTED_BAUD_COUNT; i++)
        if (SUPPORTED_BAUDS[i] == newBaud) { valid = true; break; }
    if (!valid) { SetErr("unsupported baud %u", newBaud); return 0; }

    EnterCriticalSection(&dev->cs);

    // 1) 发送切换命令
    char json[64];
    snprintf(json, sizeof(json), "{\"cmd\":133,\"baud\":%u}", newBaud);
    uint8_t frame[64];
    int frameLen = BuildFrame(frame, sizeof(frame), json);
    PurgeComm(dev->hPort, PURGE_RXCLEAR | PURGE_TXCLEAR);
    DWORD written = 0;
    WriteFile(dev->hPort, frame, frameLen, &written, NULL);

    Sleep(200); // 等待设备处理

    // 2) 关闭旧端口
    CloseHandle(dev->hPort);
    dev->hPort = NULL;
    Sleep(100);

    // 3) 以新波特率重开
    dev->hPort = OpenComSync(dev->portName, newBaud, 200);
    if (!dev->hPort) {
        SetErr("reopen %ls @ %u failed", dev->portName, newBaud);
        LeaveCriticalSection(&dev->cs);
        return 0;
    }

    // 4) 用版本查询校验
    int vlen = BuildFrame(frame, sizeof(frame), "{\"cmd\":132}");
    PurgeComm(dev->hPort, PURGE_RXCLEAR | PURGE_TXCLEAR);
    WriteFile(dev->hPort, frame, vlen, &written, NULL);

    uint8_t rxBuf[256];
    int rxLen = 0;
    DWORD t0 = GetTickCount();
    bool ok = false;
    while (GetTickCount() - t0 < 500 && rxLen < (int)sizeof(rxBuf)) {
        DWORD got = 0;
        if (!ReadFile(dev->hPort, rxBuf + rxLen, sizeof(rxBuf) - rxLen, &got, NULL)) break;
        if (got > 0) {
            rxLen += (int)got;
            if (HasValidResponse(rxBuf, rxLen)) { ok = true; break; }
        }
    }

    LeaveCriticalSection(&dev->cs);

    if (!ok) {
        // 校验失败：保持当前（新波特率）连接，但报告失败
        SetErr("baudrate switch verify failed");
        return 0;
    }
    dev->baud = newBaud;
    return 1;
}

// ----------------------------------------------------------------------------
// 4. 核心发送
// ----------------------------------------------------------------------------
YSC_API int YSC_CALL Ysc_SendCommand(YscDevice *dev, const char *json,
                                     int timeoutMs, char *outBuf, int bufSize) {
    if (!dev) { SetErr("null dev"); return -1; }
    if (timeoutMs <= 0) timeoutMs = 1000;
    EnterCriticalSection(&dev->cs);
    int r = SendAndWait(dev, json, timeoutMs, outBuf, bufSize);
    LeaveCriticalSection(&dev->cs);
    return r;
}

YSC_API int YSC_CALL Ysc_SendCommandNoWait(YscDevice *dev, const char *json) {
    if (!dev || !dev->hPort) { SetErr("not connected"); return 0; }
    if (!json) { SetErr("null json"); return 0; }
    uint8_t frame[4608];
    int frameLen = BuildFrame(frame, sizeof(frame), json);
    if (frameLen < 0) { SetErr("json too long"); return 0; }

    EnterCriticalSection(&dev->cs);
    PurgeComm(dev->hPort, PURGE_TXCLEAR);
    DWORD written = 0;
    BOOL ok = WriteFile(dev->hPort, frame, frameLen, &written, NULL);
    LeaveCriticalSection(&dev->cs);

    if (!ok || (int)written != frameLen) { SetErr("WriteFile failed"); return 0; }
    return 1;
}

YSC_API int YSC_CALL Ysc_SendRaw(YscDevice *dev, const uint8_t *data, int len) {
    if (!dev || !dev->hPort) { SetErr("not connected"); return 0; }
    if (!data || len <= 0) { SetErr("bad data"); return 0; }
    EnterCriticalSection(&dev->cs);
    DWORD written = 0;
    BOOL ok = WriteFile(dev->hPort, data, len, &written, NULL);
    LeaveCriticalSection(&dev->cs);
    if (!ok || (int)written != len) { SetErr("WriteFile failed"); return 0; }
    return 1;
}

YSC_API int YSC_CALL Ysc_QueryVersion(YscDevice *dev, int timeoutMs,
                                      char *outBuf, int bufSize) {
    return Ysc_SendCommand(dev, "{\"cmd\":132}", timeoutMs, outBuf, bufSize);
}

// ----------------------------------------------------------------------------
// 5. 常用 YSC 命令封装（命令码对照 command_bridge.cpp）
//    移动/按键类无可靠返回 -> NoWait；查询类等待返回。
// ----------------------------------------------------------------------------
YSC_API int YSC_CALL Ysc_MouseMove(YscDevice *dev, int x, int y, int steps) {
    char j[128];
    snprintf(j, sizeof(j), "{\"cmd\":30,\"x\":%d,\"y\":%d,\"c\":%d}", x, y, steps);
    return Ysc_SendCommandNoWait(dev, j);
}
YSC_API int YSC_CALL Ysc_MouseMoveTow(YscDevice *dev, int x, int y, int steps) {
    char j[128];
    snprintf(j, sizeof(j), "{\"cmd\":31,\"x\":%d,\"y\":%d,\"c\":%d}", x, y, steps);
    return Ysc_SendCommandNoWait(dev, j);
}
YSC_API int YSC_CALL Ysc_MouseButton(YscDevice *dev, uint8_t buttonMask, int pressed) {
    char j[64];
    snprintf(j, sizeof(j), "{\"cmd\":33,\"b\":%u,\"s\":%u}", (unsigned)buttonMask, pressed ? 1 : 0);
    return Ysc_SendCommandNoWait(dev, j);
}
YSC_API int YSC_CALL Ysc_KeyboardKey(YscDevice *dev, uint8_t keycode, int down) {
    char j[64];
    snprintf(j, sizeof(j), "{\"cmd\":45,\"kc\":%u,\"down\":%u}", (unsigned)keycode, down ? 1 : 0);
    return Ysc_SendCommandNoWait(dev, j);
}
YSC_API int YSC_CALL Ysc_KeyboardReleaseAll(YscDevice *dev) {
    return Ysc_SendCommandNoWait(dev, "{\"cmd\":46}");
}
YSC_API int YSC_CALL Ysc_KeyboardTypeString(YscDevice *dev, const char *s) {
    // Firmware cmd 47: type a mixed-case ASCII string char-by-char. The firmware
    // owns case (CapsLock-aware via the HID LED Output report) and auto-presses
    // Shift per character. s must be non-empty and <= 128 bytes (KBD_TYPE_MAX).
    if (!s || !*s) { SetErr("empty string"); return 0; }
    if (strlen(s) > 128) { SetErr("string too long (max 128)"); return 0; }
    std::string json = "{\"cmd\":47,\"s\":\"";
    JsonEscapeAppend(json, s);
    json += "\"}";
    return Ysc_SendCommandNoWait(dev, json.c_str());
}
YSC_API int YSC_CALL Ysc_UploadStatus(YscDevice *dev, int enable) {
    char j[64];
    snprintf(j, sizeof(j), "{\"cmd\":34,\"status\":%d}", enable ? 1 : 0);
    return Ysc_SendCommandNoWait(dev, j);
}
YSC_API int YSC_CALL Ysc_JumpIAP(YscDevice *dev) {
    return Ysc_SendCommandNoWait(dev, "{\"cmd\":50}");
}
YSC_API int YSC_CALL Ysc_GamepadGetConfig(YscDevice *dev, int timeoutMs,
                                          char *outBuf, int bufSize) {
    return Ysc_SendCommand(dev, "{\"cmd\":101}", timeoutMs, outBuf, bufSize);
}
YSC_API int YSC_CALL Ysc_GamepadSetConfig(YscDevice *dev, const char *configJson) {
    if (!configJson) { SetErr("null configJson"); return 0; }
    char buf[4096];
    int n = snprintf(buf, sizeof(buf), "{\"cmd\":100,\"data\":%s}", configJson);
    if (n <= 0 || n >= (int)sizeof(buf)) { SetErr("config too long"); return 0; }
    return Ysc_SendCommandNoWait(dev, buf);
}
YSC_API int YSC_CALL Ysc_GamepadEnable(YscDevice *dev, int on) {
    char j[64];
    snprintf(j, sizeof(j), "{\"cmd\":102,\"on\":%d}", on ? 1 : 0);
    return Ysc_SendCommandNoWait(dev, j);
}
YSC_API int YSC_CALL Ysc_GamepadReset(YscDevice *dev) {
    return Ysc_SendCommandNoWait(dev, "{\"cmd\":104}");
}

// DLL 入口（可选）：进程/线程附加时无需特殊处理
BOOL APIENTRY DllMain(HMODULE, DWORD reason, LPVOID) {
    switch (reason) {
    case DLL_PROCESS_ATTACH:
    case DLL_THREAD_ATTACH:
    case DLL_THREAD_DETACH:
    case DLL_PROCESS_DETACH:
        break;
    }
    return TRUE;
}
