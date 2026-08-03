#include "main.h"
#include "iap_upgrader.h"
#include "pipe_server.h"
#include "serial_port.h"
#include "debug_logger.h"
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>
#include <thread>

/* ── IAP Protocol Constants ── */
static const uint8_t  CMD_PROM   = 0x80;
static const uint8_t  CMD_ERASE  = 0x81;
static const uint8_t  CMD_VERIFY = 0x82;
static const uint8_t  CMD_END    = 0x83;
static const uint8_t  CMD_GETVER = 0x84;

static const uint8_t  START_M[] = "<START>";
static const uint8_t  END_M[]   = "<END>";
static const int      CHUNK_SZ  = 60;

static const DWORD    IAP_BAUDS[] = { 115200, 230400, 460800, 921600,
                                       1000000, 1500000, 2000000, 3000000, 4000000 };
static const int      IAP_BAUD_COUNT = 9;

// 容错参数（参考 towmcu_iap_upgrader.cpp 的 v2 模型）
//   * 每帧最多 FRAME_RETRIES 次尝试；第一次超时后给设备 RETRY_BREATHE_MS 喘息
//   * ERASE / PROM 单帧慢超时：Flash 擦写会让设备短暂无响应，3s 上限已覆盖最坏场景
//   * VERIFY 单帧快超时：不写 Flash，500ms 足够；chunk 间节流 VERIFY_CHUNK_DELAY_MS
//     避免 USB 端点过载
static const int      FRAME_RETRIES         = 3;
static const int      FRAME_TIMEOUT_FAST_MS = 500;
static const int      FRAME_TIMEOUT_SLOW_MS = 3000;
static const int      RETRY_BREATHE_MS      = 100;
static const int      VERIFY_CHUNK_DELAY_MS = 5;

volatile bool IAPUpgrader::s_running = false;
volatile bool IAPUpgrader::s_cancel  = false;

/* ── Helpers ── */

static void SendLog(PipeServer *pipe, const char *msg, const char *cls = "") {
    if (!pipe) return;
    // 1024 容纳超时现场 hex dump（最多 64 字节 hex ≈ 192 字节 + 中文前缀）
    char body[1024];
    snprintf(body, sizeof(body), "\"message\":\"%s\",\"cls\":\"%s\"", msg, cls);
    pipe->SendEvent("iap_log", body);
}

static void SendProgress(PipeServer *pipe, int cur, int total, const char *status) {
    if (!pipe) return;
    char body[256];
    snprintf(body, sizeof(body), "\"current\":%d,\"total\":%d,\"status\":\"%s\"", cur, total, status);
    pipe->SendEvent("iap_progress", body);
}

static void SendDone(PipeServer *pipe, bool ok, const char *err = "", const char *code = "") {
    if (!pipe) return;
    // body 留大到 384，容纳超时现场 hex dump（最长 err）+ 可选 code 字段
    char body[384];
    if (code && code[0]) {
        snprintf(body, sizeof(body), "\"success\":%s,\"error\":\"%s\",\"code\":\"%s\"",
                 ok ? "true" : "false", err, code);
    } else {
        snprintf(body, sizeof(body), "\"success\":%s,\"error\":\"%s\"",
                 ok ? "true" : "false", err);
    }
    pipe->SendEvent("iap_done", body);
}

/* ── IAP Frame Builder ── */

static int BuildIAPFrame(uint8_t cmd, const uint8_t *data, int dataLen, uint8_t *out) {
    uint8_t payload[128];
    int ppos = 0;

    payload[ppos++] = cmd;
    payload[ppos++] = (uint8_t)dataLen;

    // rev bytes for ERASE and VERIFY
    if (cmd == CMD_ERASE || cmd == CMD_VERIFY) {
        payload[ppos++] = 0; payload[ppos++] = 0;
        payload[ppos++] = 0; payload[ppos++] = 0;
    }

    if (dataLen > 0 && data) {
        memcpy(payload + ppos, data, dataLen);
        ppos += dataLen;
    }

    // Checksum (sum of all payload bytes, little-endian uint16)
    uint16_t cksum = 0;
    for (int i = 0; i < ppos; i++) cksum += payload[i];
    payload[ppos++] = (uint8_t)(cksum & 0xFF);
    payload[ppos++] = (uint8_t)((cksum >> 8) & 0xFF);

    // Full frame: <START> + 2-byte total_len + payload + <END>
    int totalLen = 7 + 2 + ppos + 5;
    int opos = 0;
    memcpy(out + opos, START_M, 7);                    opos += 7;
    out[opos++] = (uint8_t)((totalLen >> 8) & 0xFF);
    out[opos++] = (uint8_t)(totalLen & 0xFF);
    memcpy(out + opos, payload, ppos);                  opos += ppos;
    memcpy(out + opos, END_M, 5);                       opos += 5;
    return opos;
}

/* ── Raw Serial I/O for IAP ── */

static HANDLE OpenSerialRaw(const wchar_t *port, DWORD baud) {
    wchar_t fullPath[64];
    swprintf_s(fullPath, L"\\\\.\\%s", port);
    HANDLE h = CreateFileW(fullPath, GENERIC_READ | GENERIC_WRITE, 0, NULL,
                           OPEN_EXISTING, FILE_FLAG_OVERLAPPED, NULL);
    if (h == INVALID_HANDLE_VALUE) return NULL;

    DCB dcb = {};
    dcb.DCBlength = sizeof(dcb);
    GetCommState(h, &dcb);
    dcb.BaudRate = baud;
    dcb.ByteSize = 8;
    dcb.StopBits = ONESTOPBIT;
    dcb.Parity   = NOPARITY;
    dcb.fDtrControl = DTR_CONTROL_ENABLE;
    dcb.fRtsControl = RTS_CONTROL_ENABLE;
    SetCommState(h, &dcb);

    COMMTIMEOUTS tmo = {};
    tmo.ReadIntervalTimeout = MAXDWORD;
    tmo.ReadTotalTimeoutMultiplier = 0;
    tmo.ReadTotalTimeoutConstant = 100;
    tmo.WriteTotalTimeoutMultiplier = 0;
    tmo.WriteTotalTimeoutConstant = 1000;
    SetCommTimeouts(h, &tmo);

    PurgeComm(h, PURGE_TXCLEAR | PURGE_RXCLEAR);
    return h;
}

static bool RawWrite(HANDLE h, const uint8_t *data, int len) {
    OVERLAPPED ov = {};
    ov.hEvent = CreateEvent(NULL, TRUE, FALSE, NULL);
    DWORD written = 0;
    WriteFile(h, data, len, &written, &ov);
    WaitForSingleObject(ov.hEvent, 2000);
    GetOverlappedResult(h, &ov, &written, FALSE);
    CloseHandle(ov.hEvent);
    return (int)written == len;
}

static int RawRead(HANDLE h, uint8_t *buf, int bufSize, int timeoutMs) {
    OVERLAPPED ov = {};
    ov.hEvent = CreateEvent(NULL, TRUE, FALSE, NULL);
    DWORD read = 0;
    ReadFile(h, buf, bufSize, &read, &ov);
    WaitForSingleObject(ov.hEvent, timeoutMs);
    GetOverlappedResult(h, &ov, &read, FALSE);
    CloseHandle(ov.hEvent);
    return (int)read;
}

/* ── Hex dump helper（照搬 towmcu_iap_upgrader.cpp:476-486） ── */

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

/* ── 帧发-收重试核心（参考 towmcu_iap_upgrader.cpp:418-472 的 SendRecvRetry） ── */
//
// 替换原 ParseIAPResponse。在 timeoutMs 内反复 RawRead 累积到本地 buf 并扫描
// <START>...<END> 完整帧；找到立即返回 ok=true。失败时把读到的字节保留到
// lastResidual 供调用方 hex dump。共重试 FRAME_RETRIES 次，第一次失败给设备
// RETRY_BREATHE_MS 喘息。
//
// 与 v2 的差异：v1 没有 PollingReader 后台读线程，所以拿不到"距上次 RX ms"
// 和"累计 RX 字节"两个诊断字段；保留 lastResidual + 时间字段已足够定位问题。

struct IAPFrameResult {
    bool    ok;
    uint8_t status;
    int     attempts;
    DWORD   lastMs;
    DWORD   worstMs;
    std::vector<uint8_t> lastResidual;
};

static IAPFrameResult SendRecvRetry(HANDLE h, const uint8_t *frame, int frameLen,
                                    int timeoutMs) {
    IAPFrameResult r = { false, 0, 0, 0, 0, {} };
    for (int attempt = 0; attempt < FRAME_RETRIES; attempt++) {
        r.attempts = attempt + 1;
        PurgeComm(h, PURGE_RXCLEAR);  // drain（v1 无应用层缓冲，PurgeComm 足够）

        DWORD t0 = GetTickCount();
        bool writeOk = RawWrite(h, frame, frameLen);
        DWORD elapsed = GetTickCount() - t0;

        std::vector<uint8_t> buf;
        bool found = false;

        if (writeOk) {
            DWORD deadline = t0 + (DWORD)timeoutMs;
            while (GetTickCount() < deadline) {
                uint8_t tmp[256];
                int remaining = (int)sizeof(tmp) - (int)buf.size();
                if (remaining <= 0) break;  // 缓冲上限保护
                int n = RawRead(h, tmp, remaining, 50);
                if (n > 0) buf.insert(buf.end(), tmp, tmp + n);

                // 扫描 <START>...<END> 完整帧（逻辑同原 ParseIAPResponse）
                int total = (int)buf.size();
                for (int i = 0; i <= total - 14; i++) {
                    if (memcmp(buf.data() + i, "<START>", 7) != 0) continue;
                    if (i + 9 > total) break;
                    int frameLenTotal = ((int)buf[i + 7] << 8) | buf[i + 8];
                    int endPos = i + frameLenTotal;
                    if (endPos > total) break;
                    if (memcmp(buf.data() + endPos - 5, "<END>", 5) != 0) continue;
                    int payloadLen = frameLenTotal - 14;
                    if (payloadLen >= 2) {
                        r.status = buf[i + 10];  // 第二字节是 status
                        found = true;
                        break;
                    }
                }
                if (found) break;
            }
            elapsed = GetTickCount() - t0;
        }

        r.lastMs = elapsed;
        if (elapsed > r.worstMs) r.worstMs = elapsed;

        if (found) {
            r.ok = true;
            return r;
        }

        r.lastResidual = buf;  // 失败现场供 hex dump
        // 第一次超时后给设备短暂喘息，让固件把积压的 ACK 推完
        if (attempt == 0) Sleep(RETRY_BREATHE_MS);
    }
    return r;
}

/* ── Detect IAP Baudrate ── */

// Helper: convert wide port name to narrow for logging
static std::string PortToStr(const wchar_t *w) {
    char buf[32];
    snprintf(buf, sizeof(buf), "%ls", w);
    return buf;
}

static DWORD DetectIAPBaud(PipeServer *pipe, const wchar_t *port, HANDLE *hOut, DWORD hintBaud = 0) {
    // Build version query frame
    uint8_t verFrame[64];
    int verLen = 0;
    {
        const char *json = "{\"cmd\":132}";
        int jlen = (int)strlen(json);
        uint16_t pktLen = (uint16_t)(14 + jlen);
        memcpy(verFrame, "<START>", 7); verLen = 7;
        verFrame[verLen++] = (uint8_t)(pktLen >> 8);
        verFrame[verLen++] = (uint8_t)(pktLen & 0xFF);
        memcpy(verFrame + verLen, json, jlen); verLen += jlen;
        memcpy(verFrame + verLen, "<END>", 5); verLen += 5;
    }

    // Build probe order: hint baud first, then the rest
    DWORD probeOrder[IAP_BAUD_COUNT];
    int probeCount = 0;
    if (hintBaud > 0) probeOrder[probeCount++] = hintBaud;
    for (int i = 0; i < IAP_BAUD_COUNT; i++) {
        if (IAP_BAUDS[i] != hintBaud) probeOrder[probeCount++] = IAP_BAUDS[i];
    }

    for (int round = 0; round < 3; round++) {
        for (int i = 0; i < probeCount; i++) {
            HANDLE h = OpenSerialRaw(port, probeOrder[i]);
            if (!h) continue;

            PurgeComm(h, PURGE_TXCLEAR | PURGE_RXCLEAR);
            Sleep(20);
            RawWrite(h, verFrame, verLen);

            uint8_t buf[128];
            int n = RawRead(h, buf, sizeof(buf), 500);
            CloseHandle(h);

            if (n > 0) {
                // Check for valid response: starts with <START> and contains "code":200
                bool hasCode200 = false;
                for (int k = 9; k < n - 10; k++) {
                    if (memcmp(buf + k, "\"code\":200", 10) == 0 ||
                        memcmp(buf + k, "\"code\": 200", 11) == 0) {
                        hasCode200 = true;
                        break;
                    }
                }
                if (n >= 9 && memcmp(buf, "<START>", 7) == 0 && hasCode200) {
                    HANDLE h2 = OpenSerialRaw(port, probeOrder[i]);
                    if (h2) {
                        *hOut = h2;
                        return probeOrder[i];
                    }
                }
            }
        }
        if (round < 2) Sleep(300);
    }
    return 0;
}

/* ── Switch Baudrate ── */

static bool SwitchBaud(HANDLE *h, const wchar_t *port, DWORD curBaud, DWORD newBaud) {
    if (curBaud == newBaud) return true;

    // Send baudrate switch command
    char json[64];
    snprintf(json, sizeof(json), "{\"cmd\":133,\"baud\":%lu}", (unsigned long)newBaud);
    uint8_t frame[80];
    int jlen = (int)strlen(json);
    uint16_t pktLen = (uint16_t)(14 + jlen);
    int pos = 0;
    memcpy(frame + pos, "<START>", 7); pos += 7;
    frame[pos++] = (uint8_t)(pktLen >> 8);
    frame[pos++] = (uint8_t)(pktLen & 0xFF);
    memcpy(frame + pos, json, jlen); pos += jlen;
    memcpy(frame + pos, "<END>", 5); pos += 5;

    RawWrite(*h, frame, pos);

    // Wait for response
    uint8_t buf[64];
    RawRead(*h, buf, sizeof(buf), 500);

    Sleep(100);
    CloseHandle(*h);

    // Reopen at new baud
    *h = OpenSerialRaw(port, newBaud);
    if (!*h) return false;

    // Verify with version query
    uint8_t verFrame[64];
    int vlen = 0;
    {
        const char *vjson = "{\"cmd\":132}";
        int vjlen = (int)strlen(vjson);
        uint16_t vpktLen = (uint16_t)(14 + vjlen);
        memcpy(verFrame, "<START>", 7); vlen = 7;
        verFrame[vlen++] = (uint8_t)(vpktLen >> 8);
        verFrame[vlen++] = (uint8_t)(vpktLen & 0xFF);
        memcpy(verFrame + vlen, vjson, vjlen); vlen += vjlen;
        memcpy(verFrame + vlen, "<END>", 5); vlen += 5;
    }

    RawWrite(*h, verFrame, vlen);
    int n = RawRead(*h, buf, sizeof(buf), 1000);
    return n > 0;
}

/* ── Main Worker ── */

void IAPUpgrader::Worker(std::vector<uint8_t> firmware, PipeServer *pipe, uint32_t targetBaud) {
    s_running = true;
    s_cancel = false;

    auto fail = [&](const char *msg) {
        SendLog(pipe, msg, "err");
        SendDone(pipe, false, msg);
        s_running = false;
    };
    // 带 code 的失败（如 FIRMWARE_MISMATCH），让前端按 code 弹专属恢复提示
    auto failCode = [&](const char *msg, const char *code) {
        SendLog(pipe, msg, "err");
        SendDone(pipe, false, msg, code);
        s_running = false;
    };

    long fsize = (long)firmware.size();

    char tmp[256];       // 短日志（重试摘要、阶段完成）
    char detail[768];    // 长日志（超时现场 hex dump）
    snprintf(tmp, sizeof(tmp), "固件大小: %ld 字节 (%.1f KB)", fsize, fsize / 1024.0);
    SendLog(pipe, tmp);

    int totalChunks = (fsize + CHUNK_SZ - 1) / CHUNK_SZ;

    const wchar_t *portName = g_app.serialPortName;
    DWORD knownBaud = g_app.serialBaudRate;
    bool wasConnected = g_app.serialConnected;

    if (portName[0] == 0) {
        fail("未指定串口端口"); return;
    }

    // Stop SerialPort read/write threads to get exclusive raw access
    if (wasConnected) {
        g_serial.StopReadThread();
        if (g_serial.IsConnected()) {
            // Get the underlying handle from SerialPort — close it ourselves
            g_serial.Disconnect();
        }
        g_app.serialConnected = false;
    }

    // Open serial at known baud rate directly
    HANDLE hSerial = NULL;
    DWORD baud = 0;

    if (knownBaud > 0) {
        snprintf(tmp, sizeof(tmp), "打开 %s @ %lu ...", PortToStr(portName).c_str(), (unsigned long)knownBaud);
        SendLog(pipe, tmp);
        hSerial = OpenSerialRaw(portName, knownBaud);
        if (hSerial) baud = knownBaud;
    }

    if (!hSerial) {
        SendLog(pipe, "探测波特率...");
        baud = DetectIAPBaud(pipe, portName, &hSerial, knownBaud);
    }

    if (baud == 0 || !hSerial) {
        fail("无法连接设备"); return;
    }

    snprintf(tmp, sizeof(tmp), "已连接 @ %lu", (unsigned long)baud);
    SendLog(pipe, tmp);

    // Switch to target baud (targetBaud == 0 means keep current — user override)
    if (targetBaud == 0) {
        snprintf(tmp, sizeof(tmp), "保持当前波特率 %lu (不切换)", (unsigned long)baud);
        SendLog(pipe, tmp);
    } else if (baud != targetBaud) {
        snprintf(tmp, sizeof(tmp), "切换波特率 %lu -> %lu ...", (unsigned long)baud, (unsigned long)targetBaud);
        SendLog(pipe, tmp);
        if (!SwitchBaud(&hSerial, portName, baud, targetBaud)) {
            if (hSerial) CloseHandle(hSerial);
            fail("波特率切换失败"); return;
        }
        baud = targetBaud;
        SendLog(pipe, "波特率切换成功");
    }

    // Step 1: ERASE
    SendLog(pipe, "发送擦除命令...");
    SendProgress(pipe, 0, totalChunks, "擦除");

    uint8_t frame[128];
    int flen = BuildIAPFrame(CMD_ERASE, nullptr, 0, frame);
    {
        DWORD eraseStart = GetTickCount();
        IAPFrameResult r = SendRecvRetry(hSerial, frame, flen, FRAME_TIMEOUT_SLOW_MS);
        DWORD eraseMs = GetTickCount() - eraseStart;
        if (!r.ok) {
            snprintf(detail, sizeof(detail),
                     "擦除超时: 尝试 %d/%d 次 (单次上限 %dms, 最坏 %lums), "
                     "残留 %d 字节:",
                     r.attempts, FRAME_RETRIES, FRAME_TIMEOUT_SLOW_MS,
                     (unsigned long)r.worstMs, (int)r.lastResidual.size());
            HexDump(detail + strlen(detail), (int)sizeof(detail) - (int)strlen(detail) - 1,
                    r.lastResidual.data(), (int)r.lastResidual.size());
            SendLog(pipe, detail, "err");
            CloseHandle(hSerial); fail("擦除: 响应超时"); return;
        }
        if (r.status != 0) {
            CloseHandle(hSerial);
            snprintf(tmp, sizeof(tmp), "擦除失败 (错误码: 0x%02X)", r.status);
            fail(tmp); return;
        }
        snprintf(tmp, sizeof(tmp), "擦除成功 (%lums, 尝试 %d 次, 最坏单次 %lums)",
                 eraseMs, r.attempts, (unsigned long)r.worstMs);
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
        DWORD lastRetryMs = 0;
        for (int i = 0; i < totalChunks; i++) {
            if (s_cancel) { CloseHandle(hSerial); fail("用户取消"); return; }
            int chunkLen = (i == totalChunks - 1) ? (fsize - i * CHUNK_SZ) : CHUNK_SZ;
            flen = BuildIAPFrame(CMD_PROM, firmware.data() + i * CHUNK_SZ, chunkLen, frame);
            DWORD blockStart = GetTickCount();
            IAPFrameResult r = SendRecvRetry(hSerial, frame, flen, FRAME_TIMEOUT_SLOW_MS);
            DWORD blockMs = GetTickCount() - blockStart;
            if (r.attempts > 1) {
                retryBlocks++;
                totalRetries += (r.attempts - 1);
                lastRetryBlock = i + 1;
                lastRetryAttempts = r.attempts;
                lastRetryMs = r.worstMs;
                snprintf(tmp, sizeof(tmp),
                         "编程块 %d/%d 触发重试: 第 %d/%d 次成功, 最坏 %lums, 块耗时 %lums",
                         i + 1, totalChunks, r.attempts, FRAME_RETRIES,
                         (unsigned long)r.worstMs, blockMs);
                SendLog(pipe, tmp, "warn");
                snprintf(detail, sizeof(detail),
                         "  ↳ 残留 %d 字节:",
                         (int)r.lastResidual.size());
                HexDump(detail + strlen(detail), (int)sizeof(detail) - (int)strlen(detail) - 1,
                        r.lastResidual.data(), (int)r.lastResidual.size());
                SendLog(pipe, detail, "warn");
            }
            if (!r.ok) {
                snprintf(detail, sizeof(detail),
                         "编程块 %d/%d 全部超时: %d 次尝试, 最坏 %lums, 残留 %d 字节:",
                         i + 1, totalChunks, r.attempts, (unsigned long)r.worstMs,
                         (int)r.lastResidual.size());
                HexDump(detail + strlen(detail), (int)sizeof(detail) - (int)strlen(detail) - 1,
                        r.lastResidual.data(), (int)r.lastResidual.size());
                CloseHandle(hSerial); fail(detail); return;
            }
            if (r.status != 0) {
                snprintf(tmp, sizeof(tmp), "编程块 %d/%d 失败 (0x%02X)", i + 1, totalChunks, r.status);
                CloseHandle(hSerial); fail(tmp); return;
            }
            if (r.lastMs > maxChunkMs) maxChunkMs = r.lastMs;
            sumChunkMs += r.lastMs;
            if (i % 10 == 0 || i == totalChunks - 1) {
                snprintf(tmp, sizeof(tmp), "编程 %d/%d", i + 1, totalChunks);
                SendProgress(pipe, i + 1, totalChunks, tmp);
            }
        }
        DWORD progMs = GetTickCount() - t1;
        snprintf(tmp, sizeof(tmp),
                 "编程完成: 总耗时 %.2fs (%.1f KB/s), 单块最坏 %lums / 平均 %lums, "
                 "重试块 %d/%d, 重试次数 %d (最后: 块 %d 第 %d 次 %lums)",
                 progMs / 1000.0, fsize / (progMs / 1000.0) / 1024.0,
                 maxChunkMs, sumChunkMs / (totalChunks > 0 ? totalChunks : 1),
                 retryBlocks, totalChunks, totalRetries,
                 lastRetryBlock, lastRetryAttempts, (unsigned long)lastRetryMs);
        SendLog(pipe, tmp);
    }

    // Step 3: VERIFY (5ms pacing between chunks)
    SendLog(pipe, "开始校验...");
    SendProgress(pipe, 0, totalChunks, "校验");
    {
        DWORD t2 = GetTickCount();
        DWORD maxChunkMs = 0, sumChunkMs = 0;
        int retryBlocks = 0, totalRetries = 0;
        int lastRetryBlock = -1;
        int lastRetryAttempts = 0;
        DWORD lastRetryMs = 0;
        for (int i = 0; i < totalChunks; i++) {
            if (s_cancel) { CloseHandle(hSerial); fail("用户取消"); return; }
            int chunkLen = (i == totalChunks - 1) ? (fsize - i * CHUNK_SZ) : CHUNK_SZ;
            flen = BuildIAPFrame(CMD_VERIFY, firmware.data() + i * CHUNK_SZ, chunkLen, frame);
            DWORD blockStart = GetTickCount();
            IAPFrameResult r = SendRecvRetry(hSerial, frame, flen, FRAME_TIMEOUT_FAST_MS);
            DWORD blockMs = GetTickCount() - blockStart;
            if (r.attempts > 1) {
                retryBlocks++;
                totalRetries += (r.attempts - 1);
                lastRetryBlock = i + 1;
                lastRetryAttempts = r.attempts;
                lastRetryMs = r.worstMs;
                snprintf(tmp, sizeof(tmp),
                         "校验块 %d/%d 触发重试: 第 %d/%d 次成功, 最坏 %lums, 块耗时 %lums",
                         i + 1, totalChunks, r.attempts, FRAME_RETRIES,
                         (unsigned long)r.worstMs, blockMs);
                SendLog(pipe, tmp, "warn");
                snprintf(detail, sizeof(detail),
                         "  ↳ 残留 %d 字节:",
                         (int)r.lastResidual.size());
                HexDump(detail + strlen(detail), (int)sizeof(detail) - (int)strlen(detail) - 1,
                        r.lastResidual.data(), (int)r.lastResidual.size());
                SendLog(pipe, detail, "warn");
            }
            if (!r.ok) {
                snprintf(detail, sizeof(detail),
                         "校验块 %d/%d 全部超时: %d 次尝试, 最坏 %lums, 残留 %d 字节:",
                         i + 1, totalChunks, r.attempts, (unsigned long)r.worstMs,
                         (int)r.lastResidual.size());
                HexDump(detail + strlen(detail), (int)sizeof(detail) - (int)strlen(detail) - 1,
                        r.lastResidual.data(), (int)r.lastResidual.size());
                CloseHandle(hSerial); fail(detail); return;
            }
            if (r.status != 0) {
                snprintf(tmp, sizeof(tmp), "校验块 %d/%d 失败 — 固件不匹配", i + 1, totalChunks);
                CloseHandle(hSerial); failCode(tmp, "FIRMWARE_MISMATCH"); return;
            }
            if (r.lastMs > maxChunkMs) maxChunkMs = r.lastMs;
            sumChunkMs += r.lastMs;
            if (i % 10 == 0 || i == totalChunks - 1) {
                snprintf(tmp, sizeof(tmp), "校验 %d/%d", i + 1, totalChunks);
                SendProgress(pipe, i + 1, totalChunks, tmp);
            }
            Sleep(VERIFY_CHUNK_DELAY_MS);
        }
        DWORD verMs = GetTickCount() - t2;
        snprintf(tmp, sizeof(tmp),
                 "校验通过: 总耗时 %.2fs, 单块最坏 %lums / 平均 %lums, "
                 "重试块 %d/%d, 重试次数 %d (最后: 块 %d 第 %d 次 %lums)",
                 verMs / 1000.0,
                 maxChunkMs, sumChunkMs / (totalChunks > 0 ? totalChunks : 1),
                 retryBlocks, totalChunks, totalRetries,
                 lastRetryBlock, lastRetryAttempts, (unsigned long)lastRetryMs);
        SendLog(pipe, tmp);
    }

    // Step 4: END
    SendLog(pipe, "发送结束命令...");
    flen = BuildIAPFrame(CMD_END, nullptr, 0, frame);
    RawWrite(hSerial, frame, flen);

    SendProgress(pipe, totalChunks, totalChunks, "完成");
    SendLog(pipe, "固件升级完成! 设备将重启进入 APP");
    SendDone(pipe, true);

    CloseHandle(hSerial);

    // Wait for device to reboot, then reconnect
    SendLog(pipe, "等待设备重启...");
    Sleep(2000);

    SendLog(pipe, "重新连接设备...");
    HANDLE hReconn = NULL;
    DWORD reconnBaud = DetectIAPBaud(pipe, portName, &hReconn, baud);
    if (reconnBaud > 0 && hReconn) {
        CloseHandle(hReconn);
        if (g_serial.Connect(portName, reconnBaud)) {
            g_app.serialConnected = true;
            g_app.serialBaudRate = reconnBaud;
            g_serial.StartReadThread(g_app.hStopEvent);
            char body[128];
            snprintf(body, sizeof(body), "\"port\":\"%s\",\"baud\":%lu", PortToStr(portName).c_str(), (unsigned long)reconnBaud);
            g_pipeServer.SendEvent("serial_connected", body);
            snprintf(tmp, sizeof(tmp), "已重连 @ %lu", (unsigned long)reconnBaud);
            SendLog(pipe, tmp, "info");
        }
    } else {
        SendLog(pipe, "自动重连失败，请手动连接", "warn");
    }

    s_running = false;
}

/* ── Public API ── */

// 旧入口：从本地文件读取（手动选文件）。读入内存后走与 iap_start_mem 相同
// 的 Worker，确保两条路径的烧录行为完全一致。
void IAPUpgrader::Start(const char *firmwarePath, PipeServer *pipe, uint32_t targetBaud) {
    if (s_running) return;
    FILE *fp = nullptr;
    fopen_s(&fp, firmwarePath, "rb");
    if (!fp) {
        char msg[256];
        snprintf(msg, sizeof(msg), "无法打开固件文件: %s", firmwarePath);
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
    std::thread(Worker, std::move(firmware), pipe, targetBaud).detach();
}

// 新入口：直接接受内存中的固件字节（管道 Base64 解码后）。
void IAPUpgrader::Start(const uint8_t *data, size_t len, PipeServer *pipe, uint32_t targetBaud) {
    if (s_running) return;
    if (!data || len == 0) {
        SendLog(pipe, "固件数据为空", "err");
        SendDone(pipe, false, "固件数据为空");
        return;
    }
    std::vector<uint8_t> firmware(data, data + len);
    std::thread(Worker, std::move(firmware), pipe, targetBaud).detach();
}

void IAPUpgrader::Cancel() {
    s_cancel = true;
}

bool IAPUpgrader::IsRunning() {
    return s_running;
}
