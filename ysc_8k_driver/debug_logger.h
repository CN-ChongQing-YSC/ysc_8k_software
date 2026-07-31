#ifndef DEBUG_LOGGER_H
#define DEBUG_LOGGER_H

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdio.h>
#include <stdint.h>

class DebugLogger {
public:
    static void Init();
    static void Shutdown();

    static void Log(const char *fmt, ...);
    static void LogHex(const char *label, const uint8_t *data, int len);
    static void SetDebugWindowOpen(bool open);

private:
    static void WriteTimestamp(FILE *fp);
    static void MaybeFlush();

    static CRITICAL_SECTION s_cs;
    static FILE *s_fp;
    static bool s_debugWindowOpen;
    static char s_path[MAX_PATH];
    static int s_lineCount;
    static DWORD s_lastFlushTick;
};

#endif // DEBUG_LOGGER_H
