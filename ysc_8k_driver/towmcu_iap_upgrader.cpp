// towmcu_iap_upgrader.cpp — CDC firmware upgrade for the towmcu dual-MCU board.
//
// Mirrors tools/towmcu_iap_tool.py. Critical differences from the v1
// iap_upgrader.cpp (which targets a single UART MCU @ 1.5 Mbaud):
//   * CDC baud is irrelevant — open at 115200, never switch.
//   * Polling reader (ClearCommError + ReadFile on a synchronous handle), NOT
//     blocking overlapped reads — Windows CDC occasionally overruns timeouts.
//   * Frame parser tolerates BOTH <START><len><payload><END> (IAP) and
//     <START><payload><END> (APP) — the APP responds without a length prefix.
//   * After {"cmd":50} the device re-enumerates as TOWMCUIAP on a possibly
//     different COM number — must rescan.
//   * 3 retries / 500ms per IAP frame; VERIFY sleeps 5ms between chunks;
//     CMD_END gets NO response.
//   * g_serial is torn down ONLY if it currently holds the same port.

#include "main.h"
#include "towmcu_iap_upgrader.h"
#include "towmcu_cdc.h"
#include "pipe_server.h"
#include "serial_port.h"
#include "debug_logger.h"

#include <windows.h>
#include <thread>
#include <vector>
#include <string>
#include <algorithm>
#include <cstdio>
#include <cstring>

/* ── Protocol Constants (match towmcu_iap_tool.py:39-54) ── */

static const uint8_t  CMD_PROM   = 0x80;
static const uint8_t  CMD_ERASE  = 0x81;
static const uint8_t  CMD_VERIFY = 0x82;
static const uint8_t  CMD_END    = 0x83;
static const uint8_t  CMD_GETVER = 0x84;  // sent as JSON {"cmd":132}

static const char     START_M[]  = "<START>";
static const char     END_M[]    = "<END>";
static const int      CHUNK_SZ   = 60;

static const DWORD    CDC_BAUD   = 115200;     // irrelevant for CDC, just valid
static const int      FRAME_RETRIES         = 3;     // per IAP frame
// towMcuIAP 固件端 EP3 IN 应答由主循环 cdc_tx_pump 推动；当主循环被 Flash 擦写、
// WS2812B DMA 或 4KB 页编程阻塞时，EP3 busy 状态实测可保持到 ~1.5s（用户日志
// 显示每次超时都正好踩 1500ms 上限）。把 PROM/ERASE 单帧超时拉到 3000ms 后，
// PC 端能在固件恢复后立即收到 ACK，避免无谓重发；VERIFY 阶段固件不写 Flash，
// 维持快速超时。
static const int      FRAME_TIMEOUT_FAST_MS = 500;    // VERIFY
static const int      FRAME_TIMEOUT_SLOW_MS = 3000;   // ERASE / PROM
static const int      RETRY_BREATHE_MS      = 100;    // 第一次超时后给固件 EP3 的喘息时间
static const int      VERSION_TIMEOUT_MS = 1500;
static const int      IAP_RESCAN_TIMEOUT_MS = 10000;
static const int      IAP_RESCAN_POLL_MS = 400;
static const int      VERIFY_CHUNK_DELAY_MS = 5;

volatile bool TowmcuIAPUpgrader::s_running = false;
volatile bool TowmcuIAPUpgrader::s_cancel  = false;

/* ── Pipe event helpers (iap2_* — isolated from v1 iap_*) ── */

static void SendLog(PipeServer *pipe, const char *msg, const char *cls = "") {
    if (!pipe) return;
    // body 留大到 1024 字节，容纳超时现场 hex dump（最多 64 字节 hex ≈ 192 字节 + 中文前缀）
    char body[1024];
    snprintf(body, sizeof(body), "\"message\":\"%s\",\"cls\":\"%s\"", msg, cls);
    pipe->SendEvent("iap2_log", body);
}

static void SendProgress(PipeServer *pipe, int cur, int total, const char *status) {
    if (!pipe) return;
    char body[256];
    snprintf(body, sizeof(body), "\"current\":%d,\"total\":%d,\"status\":\"%s\"",
             cur, total, status);
    pipe->SendEvent("iap2_progress", body);
}

static void SendDone(PipeServer *pipe, bool ok, const char *err = "") {
    if (!pipe) return;
    char body[256];
    snprintf(body, sizeof(body), "\"success\":%s,\"error\":\"%s\"",
             ok ? "true" : "false", err);
    pipe->SendEvent("iap2_done", body);
}

static std::string PortNarrow(const wchar_t *w) {
    char buf[32];
    snprintf(buf, sizeof(buf), "%ls", w);
    return buf;
}

/* ── Synchronous CDC serial open + write ── */
//
// A SYNCHRONOUS handle (no FILE_FLAG_OVERLAPPED) with COMMTIMEOUTS set to
// (MAXDWORD, 0, 0) gives instant-return reads — ReadFile yields whatever bytes
// are buffered, or 0. This is the documented Windows idiom for polling a COM
// port and avoids the overlapped-read timeout overrun seen on CDC devices.

static HANDLE OpenCdcSerial(const wchar_t *port) {
    wchar_t fullPath[64];
    swprintf_s(fullPath, L"\\\\.\\%s", port);
    HANDLE h = CreateFileW(fullPath, GENERIC_READ | GENERIC_WRITE, 0, NULL,
                           OPEN_EXISTING, 0 /* no FILE_FLAG_OVERLAPPED */, NULL);
    if (h == INVALID_HANDLE_VALUE) return NULL;

    DCB dcb = {};
    dcb.DCBlength = sizeof(dcb);
    GetCommState(h, &dcb);
    dcb.BaudRate = CDC_BAUD;
    dcb.ByteSize = 8;
    dcb.StopBits = ONESTOPBIT;
    dcb.Parity   = NOPARITY;
    dcb.fDtrControl = DTR_CONTROL_ENABLE;
    dcb.fRtsControl = RTS_CONTROL_ENABLE;
    SetCommState(h, &dcb);

    COMMTIMEOUTS tmo = {};
    tmo.ReadIntervalTimeout         = MAXDWORD;  // instant-return combo
    tmo.ReadTotalTimeoutMultiplier  = 0;
    tmo.ReadTotalTimeoutConstant    = 0;
    tmo.WriteTotalTimeoutMultiplier = 0;
    tmo.WriteTotalTimeoutConstant   = 2000;
    SetCommTimeouts(h, &tmo);

    PurgeComm(h, PURGE_TXCLEAR | PURGE_RXCLEAR);
    return h;
}

static bool CdcWrite(HANDLE h, const uint8_t *data, int len) {
    DWORD written = 0;
    if (!WriteFile(h, data, len, &written, NULL)) return false;
    return (int)written == len;
}

/* ── Polling Reader ── */
//
// Background thread that loops ClearCommError + ReadFile (instant-return) and
// accumulates bytes into a buffer. WaitForFrame() pulls one complete
// <START>...<END> frame, tolerating both length-prefixed (IAP) and bare
// (APP) framing. Mirrors towmcu_iap_tool.py SerialReader (lines 83-127).

class PollingReader {
public:
    PollingReader() : m_hPort(NULL), m_running(false) {
        InitializeCriticalSection(&m_cs);
        m_hDataEvent = CreateEvent(NULL, TRUE, FALSE, NULL);
    }
    ~PollingReader() {
        Stop();
        DeleteCriticalSection(&m_cs);
        if (m_hDataEvent) CloseHandle(m_hDataEvent);
    }

    void Start(HANDLE hPort) {
        m_hPort = hPort;
        m_running = true;
        ResetEvent(m_hDataEvent);
        m_thread = std::thread(&PollingReader::Loop, this);
    }

    void Stop() {
        m_running = false;
        if (m_thread.joinable()) m_thread.join();
    }

    // Wait for one complete framed payload, or empty on timeout.
    std::vector<uint8_t> WaitForFrame(int timeoutMs) {
        DWORD start = GetTickCount();
        while (GetTickCount() - start < (DWORD)timeoutMs) {
            std::vector<uint8_t> frame;
            if (TryExtractFrame(frame)) return frame;
            // Wait for new data signal (or 10ms, then re-check timeout).
            WaitForSingleObject(m_hDataEvent, 10);
            ResetEvent(m_hDataEvent);
        }
        return {};
    }

    // Discard any buffered bytes (call before re-sending a frame on retry).
    void Drain() {
        EnterCriticalSection(&m_cs);
        m_buf.clear();
        LeaveCriticalSection(&m_cs);
        ResetEvent(m_hDataEvent);
    }

    // 诊断辅助：拍摄当前 buffer 拷贝（不消耗），用于超时时把残留字节打日志
    std::vector<uint8_t> Snapshot() {
        EnterCriticalSection(&m_cs);
        std::vector<uint8_t> snap = m_buf;
        LeaveCriticalSection(&m_cs);
        return snap;
    }
    // 距最后一次 ReadFile 收到字节过了多少 ms；返回 0xFFFFFFFF 表示从未收到
    DWORD MsSinceLastRx() {
        EnterCriticalSection(&m_cs);
        DWORD t = m_lastRxTick;
        LeaveCriticalSection(&m_cs);
        if (t == 0) return 0xFFFFFFFF;
        return GetTickCount() - t;
    }
    // 累计收到的字节数（用于判断设备在超时期间是否在持续发数据）
    size_t TotalRxBytes() {
        EnterCriticalSection(&m_cs);
        size_t n = m_totalRxBytes;
        LeaveCriticalSection(&m_cs);
        return n;
    }

private:
    void Loop() {
        uint8_t tmp[1024];
        while (m_running) {
            DWORD got = 0;
            if (ReadFile(m_hPort, tmp, sizeof(tmp), &got, NULL) && got > 0) {
                EnterCriticalSection(&m_cs);
                m_buf.insert(m_buf.end(), tmp, tmp + got);
                m_totalRxBytes += got;
                m_lastRxTick = GetTickCount();
                LeaveCriticalSection(&m_cs);
                SetEvent(m_hDataEvent);
            } else {
                Sleep(1);  // no data pending — yield (py:109)
            }
        }
    }

    // Try to pull one frame out of m_buf. Returns true if a full frame was
    // found and removed. Dual framing tolerance (py:130-163).
    bool TryExtractFrame(std::vector<uint8_t> &out) {
        EnterCriticalSection(&m_cs);

        if (m_buf.size() < 13) { LeaveCriticalSection(&m_cs); return false; }

        // Find <START>.
        size_t s = 0;
        bool foundStart = false;
        for (; s + 7 <= m_buf.size(); s++) {
            if (memcmp(m_buf.data() + s, START_M, 7) == 0) { foundStart = true; break; }
        }
        if (!foundStart) {
            if (m_buf.size() > 6) m_buf.erase(m_buf.begin(), m_buf.end() - 6);
            LeaveCriticalSection(&m_cs);
            return false;
        }
        if (s > 0) m_buf.erase(m_buf.begin(), m_buf.begin() + s);  // drop garbage

        // Find <END> after <START>.
        const size_t contentStart = 7;
        size_t e = contentStart;
        bool foundEnd = false;
        for (; e + 5 <= m_buf.size(); e++) {
            if (memcmp(m_buf.data() + e, END_M, 5) == 0) { foundEnd = true; break; }
        }
        if (!foundEnd) {
            if (m_buf.size() > 4096) m_buf.clear();  // unbounded growth guard
            LeaveCriticalSection(&m_cs);
            return false;
        }

        const size_t contentLen = e - contentStart;
        std::vector<uint8_t> payload;
        if (contentLen >= 2) {
            // Length-prefix discriminator (py:157): declared total_len
            // == contentLen + 12 only when the first 2 bytes are the BE u16
            // length field. JSON bodies start with '{' (0x7B) which never
            // satisfies this for realistic sizes.
            uint16_t declared = ((uint16_t)m_buf[contentStart] << 8) |
                                m_buf[contentStart + 1];
            if (declared == contentLen + 12)
                payload.assign(m_buf.begin() + contentStart + 2,
                               m_buf.begin() + e);
            else
                payload.assign(m_buf.begin() + contentStart,
                               m_buf.begin() + e);
        } else {
            payload.assign(m_buf.begin() + contentStart, m_buf.begin() + e);
        }

        // Consume <START> + content + <END>.
        m_buf.erase(m_buf.begin(), m_buf.begin() + e + 5);
        LeaveCriticalSection(&m_cs);

        out = std::move(payload);
        return true;
    }

    HANDLE        m_hPort;
    std::thread   m_thread;
    volatile bool m_running;
    CRITICAL_SECTION m_cs;
    std::vector<uint8_t> m_buf;
    HANDLE        m_hDataEvent;
    // 诊断计数（在 Loop 里更新，在 Snapshot/MsSinceLastRx/TotalRxBytes 里读）
    DWORD         m_lastRxTick = 0;   // 最后一次 ReadFile 收到字节的 GetTickCount
    size_t        m_totalRxBytes = 0; // 累计收到的字节数
};

/* ── Frame builders ── */

// Binary IAP frame: <START><BE total_len><cmd><len>[rev 4B for ERASE/VERIFY]
// <data><LE cksum><END>. Identical layout to v1 BuildIAPFrame (iap_upgrader.cpp:56).
static int BuildIAPFrame(uint8_t cmd, const uint8_t *data, int dataLen, uint8_t *out) {
    uint8_t payload[128];
    int ppos = 0;
    payload[ppos++] = cmd;
    payload[ppos++] = (uint8_t)dataLen;
    if (cmd == CMD_ERASE || cmd == CMD_VERIFY) {
        payload[ppos++] = 0; payload[ppos++] = 0;
        payload[ppos++] = 0; payload[ppos++] = 0;
    }
    if (dataLen > 0 && data) {
        memcpy(payload + ppos, data, dataLen);
        ppos += dataLen;
    }
    uint16_t cksum = 0;
    for (int i = 0; i < ppos; i++) cksum += payload[i];
    payload[ppos++] = (uint8_t)(cksum & 0xFF);
    payload[ppos++] = (uint8_t)((cksum >> 8) & 0xFF);

    int totalLen = 7 + 2 + ppos + 5;
    int opos = 0;
    memcpy(out + opos, START_M, 7);                     opos += 7;
    out[opos++] = (uint8_t)((totalLen >> 8) & 0xFF);
    out[opos++] = (uint8_t)(totalLen & 0xFF);
    memcpy(out + opos, payload, ppos);                  opos += ppos;
    memcpy(out + opos, END_M, 5);                       opos += 5;
    return opos;
}

// JSON control frame: <START><BE total_len><json body><END> (no cmd/cksum).
static int BuildJsonFrame(const char *json, uint8_t *out) {
    int jlen = (int)strlen(json);
    int totalLen = 7 + 2 + jlen + 5;
    int opos = 0;
    memcpy(out + opos, START_M, 7);                     opos += 7;
    out[opos++] = (uint8_t)((totalLen >> 8) & 0xFF);
    out[opos++] = (uint8_t)(totalLen & 0xFF);
    memcpy(out + opos, json, jlen);                     opos += jlen;
    memcpy(out + opos, END_M, 5);                       opos += 5;
    return opos;
}

/* ── Tiny JSON string extractor (handles "k":"v" and "k": "v") ── */

static std::string JsonGetString(const std::string &json, const std::string &key) {
    for (const char *fmt : { "\"%s\":\"", "\"%s\": \"" }) {
        char needle[64];
        snprintf(needle, sizeof(needle), fmt, key.c_str());
        size_t k = json.find(needle);
        if (k != std::string::npos) {
            k += strlen(needle);
            size_t end = json.find('"', k);
            if (end != std::string::npos)
                return json.substr(k, end - k);
        }
    }
    return "";
}

/* ── CDC one-shot helpers ── */

// Open port, send {"cmd":132}, parse JSON response, return version string
// (APP: "ysc-towmcu-L v1.0"; IAP: "YSC-IAP"). "" on failure.
static std::string QueryVersionOnPort(const wchar_t *port) {
    HANDLE h = OpenCdcSerial(port);
    if (!h) return "";

    PollingReader reader;
    reader.Start(h);

    uint8_t frame[64];
    int flen = BuildJsonFrame("{\"cmd\":132}", frame);
    CdcWrite(h, frame, flen);

    std::vector<uint8_t> payload = reader.WaitForFrame(VERSION_TIMEOUT_MS);
    reader.Stop();
    CloseHandle(h);

    if (payload.empty()) return "";
    std::string json(payload.begin(), payload.end());

    if (json.find("\"code\":200") == std::string::npos &&
        json.find("\"code\": 200") == std::string::npos) return "";

    std::string ver = JsonGetString(json, "data");
    if (ver.empty()) ver = JsonGetString(json, "message");
    return ver;
}

// Open port, send {"cmd":50}, close. Device resets into IAP on success.
static bool SendJumpIAP(const wchar_t *port) {
    HANDLE h = OpenCdcSerial(port);
    if (!h) return false;
    uint8_t frame[64];
    int flen = BuildJsonFrame("{\"cmd\":50}", frame);
    bool ok = CdcWrite(h, frame, flen);
    CloseHandle(h);
    return ok;
}

// Poll EnumTowmcuPorts for a device whose side label is "IAP" (USB serial
// == TOWMCUIAP). Returns the port name, or L"" on timeout.
static std::wstring FindIAPPort(int timeoutMs) {
    DWORD start = GetTickCount();
    while (GetTickCount() - start < (DWORD)timeoutMs) {
        auto ports = EnumTowmcuPorts();
        for (auto &p : ports)
            if (p.side == "IAP") return p.portName;
        Sleep(IAP_RESCAN_POLL_MS);
    }
    return L"";
}

/* ── IAP frame send + retry ── */

struct FrameResult {
    bool    ok;
    uint8_t status;
    int     attempts;       // 实际尝试次数（≥1，==1 表示一次成功无重试）
    int     lastAttemptMs;  // 最后一次尝试的往返耗时（写完到收到完整 ACK）
    int     worstAttemptMs; // 多次重试中最坏一次的往返耗时
    // 诊断现场（最后一次超时时的 buffer 与 RX 节奏）。
    // 用于在调用端打 hex dump，区分以下三种卡住场景：
    //   (1) 设备完全静默：lastTimeoutBuffer 空，msSinceRx ≈ 超时上限
    //   (2) 收到半个帧：  lastTimeoutBuffer 非 14 字节，缺 <END>
    //   (3) 解析失败：    lastTimeoutBuffer 含完整 <START>...<END> 但 payload 异常
    std::vector<uint8_t> lastTimeoutBuffer;
    DWORD   msSinceRxAtTimeout = 0;
    size_t  totalRxAtTimeout = 0;
};

static FrameResult SendRecvRetry(HANDLE h, PollingReader &reader,
                                 const uint8_t *frame, int frameLen,
                                 int timeoutMs = FRAME_TIMEOUT_FAST_MS) {
    FrameResult res = { false, 0, 0, 0, 0, {}, 0, 0 };
    for (int attempt = 0; attempt < FRAME_RETRIES; attempt++) {
        res.attempts = attempt + 1;
        reader.Drain();
        DWORD writeStart = GetTickCount();
        if (!CdcWrite(h, frame, frameLen)) {
            // CdcWrite 失败也算一次 attempt（写失败说明 USB 句柄可能抖动）
            int el = (int)(GetTickCount() - writeStart);
            if (el > res.worstAttemptMs) res.worstAttemptMs = el;
            res.lastAttemptMs = el;
            // 写失败也保留现场
            res.lastTimeoutBuffer = reader.Snapshot();
            res.msSinceRxAtTimeout = reader.MsSinceLastRx();
            res.totalRxAtTimeout = reader.TotalRxBytes();
            continue;
        }
        std::vector<uint8_t> payload = reader.WaitForFrame(timeoutMs);
        int el = (int)(GetTickCount() - writeStart);
        res.lastAttemptMs = el;
        if (el > res.worstAttemptMs) res.worstAttemptMs = el;
        if (payload.size() >= 2) {
            res.ok = true;
            res.status = payload[1];  // [0x00][status]
            return res;
        }
        // timeout — 保留超时现场供调用方打印 hex dump（py:246 "响应超时，重发本帧"）
        res.lastTimeoutBuffer = reader.Snapshot();
        res.msSinceRxAtTimeout = reader.MsSinceLastRx();
        res.totalRxAtTimeout = reader.TotalRxBytes();
        // 第一次超时后给固件 EP3 一点喘息时间，让 cdc_tx_pump 推完积压的 ACK，
        // USB 主机也重新轮询 IN 端点；第二次重发命中率显著提升。
        // （基于实测：用户日志显示设备在 1500ms 那一刻已 ready，重发几 ms 内即拿到 ACK。）
        if (attempt == 0) Sleep(RETRY_BREATHE_MS);
    }
    return res;
}

// 把字节序列格式化为 hex 字符串，最多 dump maxBytes 字节，超出加 "...(共N字节)"
// 返回写入到 out 的字符数（不含结尾 \0）
static int HexDump(char *out, int outSz, const uint8_t *data, int n, int maxBytes = 64) {
    int pos = 0;
    int dumpN = n > maxBytes ? maxBytes : n;
    for (int i = 0; i < dumpN && pos < outSz - 4; i++) {
        pos += snprintf(out + pos, outSz - pos, " %02X", data[i]);
    }
    if (n > maxBytes) {
        pos += snprintf(out + pos, outSz - pos, " ...(共%d字节)", n);
    }
    return pos;
}

/* ── Core: open IAP port + ERASE/PROGRAM/VERIFY/END ── */
//
// Returns true on success (caller emits iap2_done true). On failure, emits
// iap2_log + iap2_done(false) itself and returns false.

static bool RunIapSequence(const wchar_t *iapPort,
                          const std::vector<uint8_t> &firmware,
                          long fsize, PipeServer *pipe) {
    char tmp[256];       // 短日志（重试摘要、阶段完成）
    char detail[768];    // 长日志（超时现场 hex dump）
    int totalChunks = (fsize + CHUNK_SZ - 1) / CHUNK_SZ;

    auto fail = [&](const char *msg) {
        SendLog(pipe, msg, "err");
        SendDone(pipe, false, msg);
        return false;
    };

    HANDLE h = OpenCdcSerial(iapPort);
    if (!h) return fail("无法打开 IAP 端口");

    PollingReader reader;
    reader.Start(h);

    // Step 1: ERASE
    SendLog(pipe, "发送擦除命令 ...");
    SendProgress(pipe, 0, totalChunks, "擦除");
    {
        uint8_t frame[128];
        int flen = BuildIAPFrame(CMD_ERASE, nullptr, 0, frame);
        DWORD eraseStart = GetTickCount();
        FrameResult r = SendRecvRetry(h, reader, frame, flen, FRAME_TIMEOUT_SLOW_MS);
        DWORD eraseMs = GetTickCount() - eraseStart;
        if (!r.ok) {
            snprintf(detail, sizeof(detail),
                     "擦除超时: 尝试 %d/%d 次 (单次上限 %dms, 最坏 %dms), "
                     "RX 累计 %zuB, 距上次RX %lums, buffer 残留 %d 字节:",
                     r.attempts, FRAME_RETRIES, FRAME_TIMEOUT_SLOW_MS, r.worstAttemptMs,
                     r.totalRxAtTimeout, (unsigned long)r.msSinceRxAtTimeout,
                     (int)r.lastTimeoutBuffer.size());
            HexDump(detail + strlen(detail), (int)sizeof(detail) - (int)strlen(detail) - 1,
                    r.lastTimeoutBuffer.data(), (int)r.lastTimeoutBuffer.size());
            SendLog(pipe, detail, "err");
            reader.Stop(); CloseHandle(h); return fail("擦除: 响应超时");
        }
        if (r.status != 0) {
            snprintf(tmp, sizeof(tmp), "擦除失败 (错误码: 0x%02X)", r.status);
            reader.Stop(); CloseHandle(h); return fail(tmp);
        }
        snprintf(tmp, sizeof(tmp), "擦除成功 (%lums, 尝试 %d 次, 最坏单次 %dms, RX累计 %zuB)",
                 eraseMs, r.attempts, r.lastAttemptMs, reader.TotalRxBytes());
        SendLog(pipe, tmp);
    }

    // Step 2: PROGRAM
    snprintf(tmp, sizeof(tmp), "开始编程 (%d 块, 单块 %d ms 超时) ...",
             totalChunks, FRAME_TIMEOUT_SLOW_MS);
    SendLog(pipe, tmp);
    {
        DWORD t1 = GetTickCount();
        DWORD maxChunkMs = 0, sumChunkMs = 0;
        int retryBlocks = 0, totalRetries = 0;
        int lastRetryBlock = -1;
        int lastRetryAttempts = 0;
        int lastRetryMs = 0;
        size_t rxBefore = reader.TotalRxBytes();
        for (int i = 0; i < totalChunks; i++) {
            if (TowmcuIAPUpgrader::IsCancelled()) { reader.Stop(); CloseHandle(h); return fail("用户取消"); }
            int chunkLen = (i == totalChunks - 1) ? (fsize - i * CHUNK_SZ) : CHUNK_SZ;
            uint8_t frame[128];
            int flen = BuildIAPFrame(CMD_PROM, firmware.data() + i * CHUNK_SZ, chunkLen, frame);
            DWORD blockStart = GetTickCount();
            FrameResult r = SendRecvRetry(h, reader, frame, flen, FRAME_TIMEOUT_SLOW_MS);
            DWORD blockMs = GetTickCount() - blockStart;
            if (r.attempts > 1) {
                retryBlocks++;
                totalRetries += (r.attempts - 1);
                lastRetryBlock = i + 1;
                lastRetryAttempts = r.attempts;
                lastRetryMs = r.worstAttemptMs;
                // 重试摘要（短日志）
                snprintf(tmp, sizeof(tmp),
                         "编程块 %d/%d 触发重试: 第 %d/%d 次成功, 最坏 %dms, 块耗时 %lums",
                         i + 1, totalChunks, r.attempts, FRAME_RETRIES,
                         r.worstAttemptMs, blockMs);
                SendLog(pipe, tmp, "warn");
                // 超时现场（长日志，含 hex dump）—— 区分三种卡住场景
                snprintf(detail, sizeof(detail),
                         "  ↳ 超时现场: RX累计 %zuB (本块 +B%zu), 距上次RX %lums, buffer残留 %d字节:",
                         r.totalRxAtTimeout, r.totalRxAtTimeout - rxBefore,
                         (unsigned long)r.msSinceRxAtTimeout,
                         (int)r.lastTimeoutBuffer.size());
                HexDump(detail + strlen(detail), (int)sizeof(detail) - (int)strlen(detail) - 1,
                        r.lastTimeoutBuffer.data(), (int)r.lastTimeoutBuffer.size());
                SendLog(pipe, detail, "warn");
            }
            if (!r.ok) {
                snprintf(detail, sizeof(detail),
                         "编程块 %d/%d 全部超时: %d 次尝试, 最坏 %dms, "
                         "RX累计 %zuB, 距上次RX %lums, buffer残留 %d字节:",
                         i + 1, totalChunks, r.attempts, r.worstAttemptMs,
                         r.totalRxAtTimeout, (unsigned long)r.msSinceRxAtTimeout,
                         (int)r.lastTimeoutBuffer.size());
                HexDump(detail + strlen(detail), (int)sizeof(detail) - (int)strlen(detail) - 1,
                        r.lastTimeoutBuffer.data(), (int)r.lastTimeoutBuffer.size());
                reader.Stop(); CloseHandle(h); return fail(detail);
            }
            if (r.status != 0) {
                snprintf(tmp, sizeof(tmp), "编程块 %d/%d 失败 (0x%02X)", i + 1, totalChunks, r.status);
                reader.Stop(); CloseHandle(h); return fail(tmp);
            }
            rxBefore = reader.TotalRxBytes();
            if (r.lastAttemptMs > (long)maxChunkMs) maxChunkMs = r.lastAttemptMs;
            sumChunkMs += r.lastAttemptMs;
            if (i % 10 == 0 || i == totalChunks - 1) {
                snprintf(tmp, sizeof(tmp), "编程 %d/%d", i + 1, totalChunks);
                SendProgress(pipe, i + 1, totalChunks, tmp);
            }
        }
        DWORD progMs = GetTickCount() - t1;
        snprintf(tmp, sizeof(tmp),
                 "编程完成: 总耗时 %.2fs (%.1f KB/s), 单块最坏 %lums / 平均 %lums, "
                 "重试块 %d/%d, 重试次数 %d (最后: 块 %d 第 %d 次 %dms)",
                 progMs / 1000.0, fsize / (progMs / 1000.0) / 1024.0,
                 maxChunkMs, sumChunkMs / (totalChunks > 0 ? totalChunks : 1),
                 retryBlocks, totalChunks, totalRetries,
                 lastRetryBlock, lastRetryAttempts, lastRetryMs);
        SendLog(pipe, tmp);
    }

    // Step 3: VERIFY (5ms pacing between chunks)
    SendLog(pipe, "开始校验 ...");
    SendProgress(pipe, 0, totalChunks, "校验");
    {
        DWORD t2 = GetTickCount();
        DWORD maxChunkMs = 0, sumChunkMs = 0;
        int retryBlocks = 0, totalRetries = 0;
        int lastRetryBlock = -1;
        int lastRetryAttempts = 0;
        int lastRetryMs = 0;
        size_t rxBefore = reader.TotalRxBytes();
        for (int i = 0; i < totalChunks; i++) {
            if (TowmcuIAPUpgrader::IsCancelled()) { reader.Stop(); CloseHandle(h); return fail("用户取消"); }
            int chunkLen = (i == totalChunks - 1) ? (fsize - i * CHUNK_SZ) : CHUNK_SZ;
            uint8_t frame[128];
            int flen = BuildIAPFrame(CMD_VERIFY, firmware.data() + i * CHUNK_SZ, chunkLen, frame);
            DWORD blockStart = GetTickCount();
            FrameResult r = SendRecvRetry(h, reader, frame, flen, FRAME_TIMEOUT_FAST_MS);
            DWORD blockMs = GetTickCount() - blockStart;
            if (r.attempts > 1) {
                retryBlocks++;
                totalRetries += (r.attempts - 1);
                lastRetryBlock = i + 1;
                lastRetryAttempts = r.attempts;
                lastRetryMs = r.worstAttemptMs;
                snprintf(tmp, sizeof(tmp),
                         "校验块 %d/%d 触发重试: 第 %d/%d 次成功, 最坏 %dms, 块耗时 %lums",
                         i + 1, totalChunks, r.attempts, FRAME_RETRIES,
                         r.worstAttemptMs, blockMs);
                SendLog(pipe, tmp, "warn");
                snprintf(detail, sizeof(detail),
                         "  ↳ 超时现场: RX累计 %zuB (本块 +B%zu), 距上次RX %lums, buffer残留 %d字节:",
                         r.totalRxAtTimeout, r.totalRxAtTimeout - rxBefore,
                         (unsigned long)r.msSinceRxAtTimeout,
                         (int)r.lastTimeoutBuffer.size());
                HexDump(detail + strlen(detail), (int)sizeof(detail) - (int)strlen(detail) - 1,
                        r.lastTimeoutBuffer.data(), (int)r.lastTimeoutBuffer.size());
                SendLog(pipe, detail, "warn");
            }
            if (!r.ok) {
                snprintf(detail, sizeof(detail),
                         "校验块 %d/%d 全部超时: %d 次尝试, 最坏 %dms, "
                         "RX累计 %zuB, 距上次RX %lums, buffer残留 %d字节:",
                         i + 1, totalChunks, r.attempts, r.worstAttemptMs,
                         r.totalRxAtTimeout, (unsigned long)r.msSinceRxAtTimeout,
                         (int)r.lastTimeoutBuffer.size());
                HexDump(detail + strlen(detail), (int)sizeof(detail) - (int)strlen(detail) - 1,
                        r.lastTimeoutBuffer.data(), (int)r.lastTimeoutBuffer.size());
                reader.Stop(); CloseHandle(h); return fail(detail);
            }
            if (r.status != 0) {
                snprintf(tmp, sizeof(tmp), "校验块 %d/%d 失败 — 固件不匹配", i + 1, totalChunks);
                reader.Stop(); CloseHandle(h); return fail(tmp);
            }
            rxBefore = reader.TotalRxBytes();
            if (r.lastAttemptMs > (long)maxChunkMs) maxChunkMs = r.lastAttemptMs;
            sumChunkMs += r.lastAttemptMs;
            if (i % 10 == 0 || i == totalChunks - 1) {
                snprintf(tmp, sizeof(tmp), "校验 %d/%d", i + 1, totalChunks);
                SendProgress(pipe, i + 1, totalChunks, tmp);
            }
            Sleep(VERIFY_CHUNK_DELAY_MS);  // py:304
        }
        DWORD verMs = GetTickCount() - t2;
        snprintf(tmp, sizeof(tmp),
                 "校验通过: 总耗时 %.2fs, 单块最坏 %lums / 平均 %lums, "
                 "重试块 %d/%d, 重试次数 %d (最后: 块 %d 第 %d 次 %dms)",
                 verMs / 1000.0,
                 maxChunkMs, sumChunkMs / (totalChunks > 0 ? totalChunks : 1),
                 retryBlocks, totalChunks, totalRetries,
                 lastRetryBlock, lastRetryAttempts, lastRetryMs);
        SendLog(pipe, tmp);
    }

    // Step 4: END (no response expected — py:315-317)
    SendLog(pipe, "发送结束命令 ...");
    {
        uint8_t frame[128];
        int flen = BuildIAPFrame(CMD_END, nullptr, 0, frame);
        CdcWrite(h, frame, flen);
    }

    reader.Stop();
    CloseHandle(h);

    SendProgress(pipe, totalChunks, totalChunks, "完成");
    SendLog(pipe, "固件升级完成! 设备将跳转到 APP ...");
    SendDone(pipe, true);
    return true;
}

/* ── Worker ── */

void TowmcuIAPUpgrader::Worker(std::wstring port, std::vector<uint8_t> firmware,
                                PipeServer *pipe) {
    s_running = true;
    s_cancel = false;

    auto fail = [&](const char *msg) {
        SendLog(pipe, msg, "err");
        SendDone(pipe, false, msg);
    };

    long fsize = (long)firmware.size();

    {
        char tmp[160];
        snprintf(tmp, sizeof(tmp), "固件大小: %ld 字节 (%.1f KB)", fsize, fsize / 1024.0);
        SendLog(pipe, tmp);
    }

    // Conditionally tear down g_serial (ONLY if it holds the same port).
    bool toreDownSerial = false;
    if (g_app.serialConnected &&
        wcscmp(g_app.serialPortName, port.c_str()) == 0) {
        g_serial.StopReadThread();
        g_serial.Disconnect();
        g_app.serialConnected = false;
        toreDownSerial = true;
    }

    // Preamble: confirm device mode, boot into IAP if needed, find IAP port.
    {
        SendLog(pipe, "确认设备模式 ...");
        std::string version = QueryVersionOnPort(port.c_str());
        std::wstring iapPort = port;

        if (version.empty()) {
            fail("无法与设备通信 — 请检查端口/连线");
        } else if (version.find("-IAP") != std::string::npos) {
            char msg[256];
            snprintf(msg, sizeof(msg), "当前: [IAP] %s", version.c_str());
            SendLog(pipe, msg, "info");
            RunIapSequence(iapPort.c_str(), firmware, fsize, pipe);
        } else {
            // APP mode — enter IAP, then find the re-enumerated port.
            char msg[256];
            snprintf(msg, sizeof(msg), "当前: [APP] %s", version.c_str());
            SendLog(pipe, msg, "info");
            SendLog(pipe, "设备在 APP 模式 — 发送进入下载模式 (cmd 50) ...");
            SendJumpIAP(port.c_str());
            Sleep(1500);

            SendLog(pipe, "寻找 IAP 端口 (TOWMCUIAP) ...");
            iapPort = FindIAPPort(IAP_RESCAN_TIMEOUT_MS);
            if (iapPort.empty()) {
                fail("未找到 IAP 端口 — 设备未进入下载模式");
            } else {
                snprintf(msg, sizeof(msg), "IAP 端口: %s (YSC-IAP)",
                         PortNarrow(iapPort.c_str()).c_str());
                SendLog(pipe, msg, "info");
                RunIapSequence(iapPort.c_str(), firmware, fsize, pipe);
            }
        }
    }

    // Conditionally reconnect g_serial (device has re-enumerated with the
    // same side serial after a successful upgrade). Reached by fallthrough
    // from the preamble block above.
    if (toreDownSerial) {
        SendLog(pipe, "等待设备重启 ...");
        Sleep(2000);
        if (g_serial.Connect(port.c_str(), CDC_BAUD)) {
            g_app.serialConnected = true;
            g_app.serialBaudRate = CDC_BAUD;
            g_serial.StartReadThread(g_app.hStopEvent);
            SendLog(pipe, "已重新连接串口", "info");
        } else {
            SendLog(pipe, "串口自动重连失败，请手动连接", "warn");
        }
    }

    s_running = false;
}

/* ── Public API ── */

// 旧入口：从本地文件读取（手动选文件）。读入内存后走与 towmcu_start_mem
// 相同的 Worker，确保两条路径的烧录行为完全一致。
void TowmcuIAPUpgrader::Start(const wchar_t *port, const char *fwPath,
                                PipeServer *pipe) {
    if (s_running) return;
    FILE *fp = nullptr;
    fopen_s(&fp, fwPath, "rb");
    if (!fp) {
        char msg[256];
        snprintf(msg, sizeof(msg), "无法打开固件文件: %s", fwPath);
        SendLog(pipe, msg, "err");
        SendDone(pipe, false, msg);
        return;
    }
    fseek(fp, 0, SEEK_END);
    long fsize = ftell(fp);
    fseek(fp, 0, SEEK_SET);
    std::vector<uint8_t> firmware(fsize);
    if ((long)fread(firmware.data(), 1, fsize, fp) != fsize) {
        fclose(fp);
        SendLog(pipe, "读取固件文件失败", "err");
        SendDone(pipe, false, "读取固件文件失败");
        return;
    }
    fclose(fp);
    std::thread(Worker, std::wstring(port), std::move(firmware), pipe).detach();
}

// 新入口：直接接受内存中的固件字节（管道 Base64 解码后）。
void TowmcuIAPUpgrader::Start(const wchar_t *port, const uint8_t *data, size_t len,
                                PipeServer *pipe) {
    if (s_running) return;
    if (!data || len == 0) {
        SendLog(pipe, "固件数据为空", "err");
        SendDone(pipe, false, "固件数据为空");
        return;
    }
    std::vector<uint8_t> firmware(data, data + len);
    std::thread(Worker, std::wstring(port), std::move(firmware), pipe).detach();
}

void TowmcuIAPUpgrader::Cancel() {
    s_cancel = true;
}

bool TowmcuIAPUpgrader::IsRunning() {
    return s_running;
}

void TowmcuIAPUpgrader::QueryVersion(const wchar_t *port, PipeServer *pipe) {
    if (s_running) {
        SendLog(pipe, "升级正在进行中，无法查询版本", "warn");
        return;
    }
    std::wstring portCopy(port);
    std::thread([portCopy, pipe]() {
        std::string ver = QueryVersionOnPort(portCopy.c_str());
        std::string mode = (ver.find("-IAP") != std::string::npos) ? "IAP" : "APP";

        char body[384];
        snprintf(body, sizeof(body),
                 "\"port\":\"%s\",\"version\":\"%s\",\"mode\":\"%s\"",
                 PortNarrow(portCopy.c_str()).c_str(), ver.c_str(), mode.c_str());
        pipe->SendEvent("towmcu_version", body);

        if (ver.empty()) {
            SendLog(pipe, "查询版本失败 — 设备无响应", "err");
        } else {
            char msg[256];
            snprintf(msg, sizeof(msg), "[%s] %s", mode.c_str(), ver.c_str());
            SendLog(pipe, msg, "info");
        }
    }).detach();
}

void TowmcuIAPUpgrader::EnterIAP(const wchar_t *port, PipeServer *pipe) {
    if (s_running) {
        SendLog(pipe, "升级正在进行中", "warn");
        return;
    }
    std::wstring portCopy(port);
    std::thread([portCopy, pipe]() {
        SendLog(pipe, "发送进入下载模式 (cmd 50) ...");
        if (!SendJumpIAP(portCopy.c_str())) {
            SendLog(pipe, "进入下载模式失败 — 无法打开端口", "err");
            // 通知前端结束等待（port 为空表示失败）
            pipe->SendEvent("iap_entered", "\"port\":\"\"");
            return;
        }
        Sleep(1500);
        SendLog(pipe, "寻找 IAP 端口 (TOWMCUIAP) ...");
        std::wstring iap = FindIAPPort(IAP_RESCAN_TIMEOUT_MS);
        if (iap.empty()) {
            SendLog(pipe, "未找到 IAP 端口 — 请检查设备", "err");
            pipe->SendEvent("iap_entered", "\"port\":\"\"");
            return;
        }
        char msg[160];
        snprintf(msg, sizeof(msg), "已进入 IAP: %s (YSC-IAP)",
                 PortNarrow(iap.c_str()).c_str());
        SendLog(pipe, msg, "info");
        // 通知前端 IAP 端口已就绪 —— 前端基于此自动选中 IAP 端口，
        // 不再依赖 1 秒一次的端口轮询 + 端口号字符串比对（在 USB 转串口 CDC 复用
        // 同一 COM 号时永远找不到"新"端口，导致 30s 超时）。
        char body[128];
        snprintf(body, sizeof(body), "\"port\":\"%s\"",
                 PortNarrow(iap.c_str()).c_str());
        pipe->SendEvent("iap_entered", body);
    }).detach();
}
