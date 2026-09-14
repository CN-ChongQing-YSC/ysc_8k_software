#include "debug_logger.h"
#include "main.h"
#include "pipe_server.h"
#include <cstring>
#include <share.h>

CRITICAL_SECTION DebugLogger::s_cs = {};
FILE *DebugLogger::s_fp = nullptr;
bool DebugLogger::s_debugWindowOpen = false;
char DebugLogger::s_path[MAX_PATH] = {};
int DebugLogger::s_lineCount = 0;
DWORD DebugLogger::s_lastFlushTick = 0;

void DebugLogger::Init() {
    InitializeCriticalSection(&s_cs);

    GetModuleFileNameA(NULL, s_path, MAX_PATH);
    char *slash = strrchr(s_path, '\\');
    if (slash) strcpy_s(slash + 1, MAX_PATH - (int)(slash + 1 - s_path), "ysc_driver_debug.log");
    else strcpy_s(s_path, "ysc_driver_debug.log");

    s_fp = _fsopen(s_path, "w", _SH_DENYNO);  // 共享读取：驱动运行中日志可被外部实时查看
    if (s_fp) {
        SYSTEMTIME st;
        GetLocalTime(&st);
        fprintf(s_fp, "=== YSC 8K Driver Debug Log Started %04d-%02d-%02d %02d:%02d:%02d ===\r\n",
                st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond);
        fprintf(s_fp, "=== Tags: [DBG_OPEN] = debug window visible, [DBG_CLOSED] = debug window hidden ===\r\n\r\n");
        fflush(s_fp);
    }
    s_lineCount = 2;
    s_lastFlushTick = GetTickCount();
}

void DebugLogger::Shutdown() {
    EnterCriticalSection(&s_cs);
    if (s_fp) {
        fprintf(s_fp, "\r\n=== Log Ended ===\r\n");
        fclose(s_fp);
        s_fp = nullptr;
    }
    LeaveCriticalSection(&s_cs);
    DeleteCriticalSection(&s_cs);
}

void DebugLogger::SetDebugWindowOpen(bool open) {
    EnterCriticalSection(&s_cs);
    bool wasOpen = s_debugWindowOpen;
    s_debugWindowOpen = open;
    if (s_fp && wasOpen != open) {
        WriteTimestamp(s_fp);
        fprintf(s_fp, " [%s] Debug window state changed to: %s\r\n",
                open ? "DBG_OPEN" : "DBG_CLOSED",
                open ? "VISIBLE" : "HIDDEN");
        fflush(s_fp);
        s_lineCount++;
    }
    LeaveCriticalSection(&s_cs);
}

void DebugLogger::Log(const char *fmt, ...) {
    EnterCriticalSection(&s_cs);
    if (!s_fp) { LeaveCriticalSection(&s_cs); return; }

    if (s_lineCount >= 50000) {
        fclose(s_fp);
        s_fp = _fsopen(s_path, "w", _SH_DENYNO);  // 共享读取：驱动运行中日志可被外部实时查看
        if (s_fp) {
            SYSTEMTIME st;
            GetLocalTime(&st);
            fprintf(s_fp, "=== Log Rotated at %02d:%02d:%02d ===\r\n", st.wHour, st.wMinute, st.wSecond);
        }
        s_lineCount = 1;
    }

    WriteTimestamp(s_fp);
    fprintf(s_fp, " [%s] ", s_debugWindowOpen ? "DBG_OPEN" : "DBG_CLOSED");

    va_list args;
    va_start(args, fmt);
    vfprintf(s_fp, fmt, args);
    va_end(args);

    fprintf(s_fp, "\r\n");
    s_lineCount++;
    MaybeFlush();

    /* 注意：这里**不再**把日志逐行推给 Electron 管道。原因有两条：
     *   1) 前端没有任何消费者 —— main.js 的 forwardEvents 里没有 'log'，
     *      推过去只会被解析后丢掉；
     *   2) 更严重的是性能/稳定性：逐行推送是从串口读线程发起的（每收一帧就
     *      几条日志），客户端一慢就会阻塞串口读线程；客户端异常退出时管道里
     *      的积压还会让断连清理卡住。日志留在文件里（实时可读）就够了。 */
    LeaveCriticalSection(&s_cs);
}

void DebugLogger::LogHex(const char *label, const uint8_t *data, int len) {
    EnterCriticalSection(&s_cs);
    if (!s_fp) { LeaveCriticalSection(&s_cs); return; }

    WriteTimestamp(s_fp);
    fprintf(s_fp, " [%s] %s (%d bytes):", s_debugWindowOpen ? "DBG_OPEN" : "DBG_CLOSED", label, len);

    int showLen = (len > 64) ? 64 : len;
    for (int i = 0; i < showLen; i++)
        fprintf(s_fp, " %02X", data[i]);
    if (len > 64)
        fprintf(s_fp, " ...");

    fprintf(s_fp, "\r\n");
    s_lineCount++;
    MaybeFlush();

    LeaveCriticalSection(&s_cs);
}

void DebugLogger::WriteTimestamp(FILE *fp) {
    SYSTEMTIME st;
    GetLocalTime(&st);
    fprintf(fp, "[%02d:%02d:%02d.%03d]", st.wHour, st.wMinute, st.wSecond, st.wMilliseconds);
}

void DebugLogger::MaybeFlush() {
    DWORD now = GetTickCount();
    if (now - s_lastFlushTick >= 500) {
        fflush(s_fp);
        s_lastFlushTick = now;
    }
}
