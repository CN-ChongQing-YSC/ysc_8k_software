#include "human_guard.h"
#include "main.h"
#include "pipe_server.h"

#include <cstdio>
#include <cstdlib>
#include <cstring>

// 物理护栏测量参数（与固件 PhysicalGuard 的方向迟滞保持一致）
#define HG_MIN_MS            5u     // 低于 5ms 视为传感器/单帧噪声
#define HG_MAX_MS            500u   // 高于 500ms 不算“快速反向”
#define HG_HYST              3      // 反向累计达到 3 counts 才认为方向翻转
#define HG_GAP_RESET_MS      50u    // 停顿超过 50ms 重新起算方向
#define HG_RECENT_MAX        32u
#define HG_DEFAULT_DURATION  5000
#define HG_MAX_DURATION      15000

struct AxisStat {
    uint32_t hist[HG_MAX_MS + 1];   // 索引 = 间隔 ms
    uint32_t count;                 // 记录到的反向次数
    uint32_t minMs;
    uint32_t lastMs;
    int8_t   dir;                   // -1/0/+1
    uint32_t revT;                  // 最近一次反转的时基
    uint32_t lastT;                 // 上一帧时基
    int32_t  oppAcc;                // 反向累计
    uint32_t recent[HG_RECENT_MAX];
    uint32_t recentCount;           // 累计写入次数（用于环形定位）
};

// 测量状态（串口读线程 / 工作线程 / 管道线程共享）
static CRITICAL_SECTION s_cs;
static bool             s_csInit = false;
static volatile bool    s_active = false;
static volatile LONG    s_stop = 0;
static HANDLE           s_hWake = NULL;
static HANDLE           s_hThread = NULL;

static AxisStat         s_x = {};
static AxisStat         s_y = {};
static char             s_axis = 'x';
static int              s_duration = HG_DEFAULT_DURATION;
static ULONGLONG        s_startMs = 0;
static ULONGLONG        s_lastProgressMs = 0;
static uint32_t         s_session = 0;
static uint32_t         s_frames = 0;
static bool             s_sawT = false;
static bool             s_sawNoT = false;
static bool             s_stopRequested = false;

static char             s_lastResult[2048] = {};
static bool             s_haveResult = false;

// ---------------------------------------------------------------------------
// 极简 JSON 扫描（与 monitor_push.cpp 同风格，避免引入第三方库）
// ---------------------------------------------------------------------------
static bool ExtractJsonInt(const char *json, const char *key, int &out) {
    char search[64];
    snprintf(search, sizeof(search), "\"%s\":", key);
    const char *p = strstr(json, search);
    if (!p) return false;
    p += strlen(search);
    while (*p == ' ') p++;
    if (*p == '-' || (*p >= '0' && *p <= '9')) {
        out = atoi(p);
        return true;
    }
    return false;
}

static bool ExtractJsonU32(const char *json, const char *key, uint32_t &out) {
    char search[64];
    snprintf(search, sizeof(search), "\"%s\":", key);
    const char *p = strstr(json, search);
    if (!p) return false;
    p += strlen(search);
    while (*p == ' ') p++;
    if (*p < '0' || *p > '9') return false;
    out = (uint32_t)strtoul(p, nullptr, 10);
    return true;
}

static bool ExtractDataString(const char *json, char *out, int outSize) {
    const char *key = "\"data\":\"";
    const char *p = strstr(json, key);
    if (!p) return false;
    p += strlen(key);

    int i = 0;
    while (*p && *p != '"' && i < outSize - 1) {
        if (*p == '\\' && *(p + 1)) p++;
        out[i++] = *p++;
    }
    out[i] = '\0';
    return i > 0;
}

// ---------------------------------------------------------------------------
// 单轴状态机（调用方必须持有 s_cs）
// ---------------------------------------------------------------------------
static void FeedAxisLocked(AxisStat &st, int delta, uint32_t t) {
    if (st.lastT != 0) {
        uint32_t gap = (uint32_t)(t - st.lastT);
        if (gap > HG_GAP_RESET_MS) {
            st.dir = 0;
            st.oppAcc = 0;
        }
    }
    st.lastT = t;

    if (delta == 0) return;

    int8_t s = (delta > 0) ? 1 : -1;
    if (st.dir == 0) {
        st.dir = s;
        st.revT = t;
        st.oppAcc = 0;
        return;
    }
    if (s == st.dir) {
        st.oppAcc = 0;
        return;
    }

    st.oppAcc += delta;
    if (st.oppAcc >= HG_HYST || st.oppAcc <= -HG_HYST) {
        uint32_t dt = (uint32_t)(t - st.revT);
        if (dt >= HG_MIN_MS && dt <= HG_MAX_MS) {
            st.hist[dt]++;
            st.count++;
            st.lastMs = dt;
            if (st.minMs == 0 || dt < st.minMs) st.minMs = dt;
            st.recent[st.recentCount % HG_RECENT_MAX] = dt;
            st.recentCount++;
        }
        st.dir = s;
        st.revT = t;
        st.oppAcc = 0;
    }
}

static void ComputeStats(const AxisStat &st, uint32_t &p5, uint32_t &median, bool &reliable) {
    p5 = 0;
    median = 0;
    reliable = (st.count >= 5);
    if (st.count == 0) return;

    uint32_t need5 = (st.count * 5u + 99u) / 100u;
    if (need5 < 1) need5 = 1;
    uint32_t need50 = (st.count + 1u) / 2u;

    uint32_t cum = 0;
    bool got5 = false;
    for (uint32_t d = HG_MIN_MS; d <= HG_MAX_MS; d++) {
        if (st.hist[d] == 0) continue;
        cum += st.hist[d];
        if (!got5 && cum >= need5) {
            p5 = d;
            got5 = true;
        }
        if (cum >= need50) {
            median = d;
            break;
        }
    }
}

static int BuildAxisJson(char *out, int outSize, const AxisStat &st) {
    if (!out || outSize <= 0) return 0;
    out[0] = '\0';

    uint32_t p5 = 0, median = 0;
    bool reliable = false;
    ComputeStats(st, p5, median, reliable);
    uint32_t suggest = reliable ? st.minMs : 0;

    int n = snprintf(out, outSize,
                     "\"count\":%lu,\"min\":%lu,\"p5\":%lu,\"median\":%lu,"
                     "\"last\":%lu,\"suggest\":%lu,\"reliable\":%s,\"recent\":[",
                     (unsigned long)st.count,
                     (unsigned long)st.minMs,
                     (unsigned long)p5,
                     (unsigned long)median,
                     (unsigned long)st.lastMs,
                     (unsigned long)suggest,
                     reliable ? "true" : "false");
    if (n < 0 || n >= outSize) return 0;

    uint32_t total = st.recentCount;
    uint32_t cnt = (total < HG_RECENT_MAX) ? total : HG_RECENT_MAX;
    uint32_t start = (total > HG_RECENT_MAX) ? (total % HG_RECENT_MAX) : 0;
    for (uint32_t i = 0; i < cnt; i++) {
        uint32_t v = st.recent[(start + i) % HG_RECENT_MAX];
        int m = snprintf(out + n, outSize - n, "%s%lu", i ? "," : "", (unsigned long)v);
        if (m < 0 || n + m >= outSize) return 0;
        n += m;
    }

    // 只收尾 recent 数组；外层轴对象的 { } 由调用方 "x":{%s} 提供。
    // 这里若再写 }，会多出一个右括号，把外层对象提前闭合，整条事件 JSON 就坏了。
    int m = snprintf(out + n, outSize - n, "]");
    if (m < 0 || n + m >= outSize) return 0;
    return n + m;
}

// ---------------------------------------------------------------------------
// 事件组装/发送
// ---------------------------------------------------------------------------
static void SendSnapshotLockedCopies(void) {
    // 先在锁内复制，避免在串口读线程上做 JSON 组装。
    AxisStat x, y;
    char axis;
    int duration;
    uint32_t frames;
    uint32_t session;
    ULONGLONG startMs;
    bool sawT, sawNoT;
    const char *timebase;

    EnterCriticalSection(&s_cs);
    x = s_x;
    y = s_y;
    axis = s_axis;
    duration = s_duration;
    frames = s_frames;
    session = s_session;
    startMs = s_startMs;
    sawT = s_sawT;
    sawNoT = s_sawNoT;
    if (sawT && sawNoT) timebase = "mixed";
    else if (sawT)      timebase = "device";
    else if (sawNoT)    timebase = "host";
    else                timebase = "none";
    LeaveCriticalSection(&s_cs);

    ULONGLONG elapsed64 = GetTickCount64() - startMs;
    if (elapsed64 > 0xFFFFFFFFull) elapsed64 = 0xFFFFFFFFull;

    char xj[768] = {}, yj[768] = {};
    if (!BuildAxisJson(xj, sizeof(xj), x)) xj[0] = '\0';
    if (!BuildAxisJson(yj, sizeof(yj), y)) yj[0] = '\0';

    char body[2048];
    int n = snprintf(body, sizeof(body),
                     "\"phase\":\"running\",\"session\":%lu,\"axis\":\"%c\","
                     "\"duration\":%d,\"elapsed\":%lu,\"frames\":%lu,"
                     "\"timebase\":\"%s\",\"reason\":\"running\","
                     "\"dirX\":%d,\"dirY\":%d,"
                     "\"x\":{%s},\"y\":{%s}",
                     (unsigned long)session, axis, duration,
                     (unsigned long)elapsed64, (unsigned long)frames,
                     timebase, (int)x.dir, (int)y.dir, xj, yj);
    if (n <= 0 || n >= (int)sizeof(body)) return;
    g_pipeServer.SendEvent("human_guard", body);
}

static void SendFinal(const char *reason, bool isError) {
    AxisStat x, y;
    char axis;
    int duration;
    uint32_t frames;
    uint32_t session;
    ULONGLONG startMs;
    const char *timebase;

    EnterCriticalSection(&s_cs);
    x = s_x;
    y = s_y;
    axis = s_axis;
    duration = s_duration;
    frames = s_frames;
    session = s_session;
    startMs = s_startMs;
    if (s_sawT && s_sawNoT) timebase = "mixed";
    else if (s_sawT)      timebase = "device";
    else if (s_sawNoT)    timebase = "host";
    else                  timebase = "none";
    LeaveCriticalSection(&s_cs);

    ULONGLONG elapsed64 = GetTickCount64() - startMs;
    if (elapsed64 > 0xFFFFFFFFull) elapsed64 = 0xFFFFFFFFull;

    char xj[768] = {}, yj[768] = {};
    if (!BuildAxisJson(xj, sizeof(xj), x)) xj[0] = '\0';
    if (!BuildAxisJson(yj, sizeof(yj), y)) yj[0] = '\0';

    const char *phase = isError ? "error" : "done";
    char body[2048];
    int n = snprintf(body, sizeof(body),
                     "\"phase\":\"%s\",\"session\":%lu,\"axis\":\"%c\","
                     "\"duration\":%d,\"elapsed\":%lu,\"frames\":%lu,"
                     "\"timebase\":\"%s\",\"reason\":\"%s\","
                     "\"dirX\":%d,\"dirY\":%d,"
                     "\"x\":{%s},\"y\":{%s}",
                     phase, (unsigned long)session, axis, duration,
                     (unsigned long)elapsed64, (unsigned long)frames,
                     timebase, reason, (int)x.dir, (int)y.dir, xj, yj);
    if (n <= 0 || n >= (int)sizeof(body)) return;

    EnterCriticalSection(&s_cs);
    if (n < (int)sizeof(s_lastResult)) {
        memcpy(s_lastResult, body, (size_t)n + 1);
        s_haveResult = true;
    }
    LeaveCriticalSection(&s_cs);

    g_pipeServer.SendEvent("human_guard", body);
}

static void Finish(const char *reason) {
    if (!s_csInit) return;

    bool wasActive = false;
    EnterCriticalSection(&s_cs);
    if (s_active) {
        s_active = false;
        wasActive = true;
    }
    LeaveCriticalSection(&s_cs);

    if (!wasActive) return;

    bool isError = (strcmp(reason, "device_lost") == 0 ||
                    strcmp(reason, "shutdown") == 0 ||
                    strcmp(reason, "restarted") == 0);
    SendFinal(reason, isError);
}

// ---------------------------------------------------------------------------
// 工作线程
// ---------------------------------------------------------------------------
static DWORD WINAPI WorkerProc(LPVOID) {
    while (!s_stop) {
        WaitForSingleObject(s_hWake, 50);
        if (s_stop) break;

        bool doFinish = false;
        bool doProgress = false;
        const char *reason = "timeout";

        EnterCriticalSection(&s_cs);
        if (s_active) {
            ULONGLONG now = GetTickCount64();
            if (s_stopRequested) {
                doFinish = true;
                reason = "stopped";
            } else if (now - s_startMs >= (ULONGLONG)s_duration) {
                doFinish = true;
                reason = "timeout";
            } else if (now - s_lastProgressMs >= 100) {
                s_lastProgressMs = now;
                doProgress = true;
            }
        }
        LeaveCriticalSection(&s_cs);

        if (doFinish) {
            Finish(reason);
            continue;
        }
        if (doProgress) {
            SendSnapshotLockedCopies();
        }
    }
    return 0;
}

// ---------------------------------------------------------------------------
// 公开接口
// ---------------------------------------------------------------------------
void HumanGuard::Init() {
    if (s_csInit) return;

    InitializeCriticalSection(&s_cs);
    s_csInit = true;
    s_active = false;
    s_stop = 0;
    s_stopRequested = false;
    s_haveResult = false;
    s_lastResult[0] = '\0';

    s_hWake = CreateEvent(NULL, FALSE, FALSE, NULL);
    if (s_hWake) {
        s_hThread = CreateThread(NULL, 0, WorkerProc, NULL, 0, NULL);
    }
}

void HumanGuard::Shutdown() {
    if (!s_csInit) return;

    Finish("shutdown");

    InterlockedExchange(&s_stop, 1);
    if (s_hWake) SetEvent(s_hWake);
    if (s_hThread) {
        WaitForSingleObject(s_hThread, 2000);
        CloseHandle(s_hThread);
        s_hThread = NULL;
    }
    if (s_hWake) {
        CloseHandle(s_hWake);
        s_hWake = NULL;
    }

    DeleteCriticalSection(&s_cs);
    s_csInit = false;
}

void HumanGuard::OnSerialFrame(const uint8_t *data, int len) {
    if (!s_active) return;
    if (!data || len <= 0 || len >= 1024) return;

    char json[1024];
    memcpy(json, data, (size_t)len);
    json[len] = '\0';

    /* 只处理鼠标监控帧：message 为 null（当前固件）、不带 message（旧固件）
     * 或 "mouse"（更早的实时通道）。控制回包/键盘帧一律跳过。 */
    const char *m = strstr(json, "\"message\":");
    if (m) {
        const char *p = m + 10;   // strlen("\"message\":")
        while (*p == ' ') p++;
        bool isNull  = (strncmp(p, "null", 4) == 0);
        bool isMouse = (strncmp(p, "\"mouse\"", 7) == 0);
        if (!isNull && !isMouse) return;
    }

    char inner[512];
    if (!ExtractDataString(json, inner, sizeof(inner))) return;

    int x = 0, y = 0;
    if (!ExtractJsonInt(inner, "x", x)) return;
    if (!ExtractJsonInt(inner, "y", y)) return;

    uint32_t t = 0;
    bool hasT = ExtractJsonU32(inner, "t", t);

    EnterCriticalSection(&s_cs);
    if (!s_active) {
        LeaveCriticalSection(&s_cs);
        return;
    }

    if (hasT) {
        s_sawT = true;
    } else {
        s_sawNoT = true;
        t = (uint32_t)GetTickCount64();
    }

    s_frames++;
    FeedAxisLocked(s_x, x, t);
    FeedAxisLocked(s_y, y, t);
    LeaveCriticalSection(&s_cs);
}

bool HumanGuard::Begin(const char *axis, int durationMs) {
    if (!s_csInit) return false;

    char ax = 'x';
    if (axis && (axis[0] == 'y' || axis[0] == 'Y')) ax = 'y';

    if (durationMs < 1000) durationMs = 1000;
    if (durationMs > HG_MAX_DURATION) durationMs = HG_MAX_DURATION;

    EnterCriticalSection(&s_cs);
    s_active = false;               // 若上一轮还在跑，直接替换（UI 不会并发开始）
    memset(&s_x, 0, sizeof(s_x));
    memset(&s_y, 0, sizeof(s_y));
    s_axis = ax;
    s_duration = durationMs;
    s_startMs = GetTickCount64();
    s_lastProgressMs = s_startMs;
    s_session = (uint32_t)s_startMs;
    s_frames = 0;
    s_sawT = false;
    s_sawNoT = false;
    s_stopRequested = false;
    s_haveResult = false;
    s_lastResult[0] = '\0';
    s_active = true;
    LeaveCriticalSection(&s_cs);

    if (s_hWake) SetEvent(s_hWake);
    SendSnapshotLockedCopies();     // 立即回一个 running，UI 不用等 100ms
    return true;
}

void HumanGuard::Stop() {
    if (!s_csInit) return;

    EnterCriticalSection(&s_cs);
    if (s_active) s_stopRequested = true;
    LeaveCriticalSection(&s_cs);

    if (s_hWake) SetEvent(s_hWake);
}

void HumanGuard::SendStatus() {
    if (!s_csInit) return;

    bool active = false;
    bool haveResult = false;
    char last[2048] = {};

    EnterCriticalSection(&s_cs);
    active = s_active;
    haveResult = s_haveResult;
    if (haveResult) {
        memcpy(last, s_lastResult, sizeof(last));
        last[sizeof(last) - 1] = '\0';
    }
    LeaveCriticalSection(&s_cs);

    if (active) {
        SendSnapshotLockedCopies();
    } else if (haveResult) {
        g_pipeServer.SendEvent("human_guard", last);
    } else {
        g_pipeServer.SendEvent("human_guard", "\"phase\":\"idle\"");
    }
}

void HumanGuard::Abort(const char *reason) {
    if (!s_csInit) return;
    Finish(reason ? reason : "aborted");
}

void HumanGuard::Reset() {
    if (!s_csInit) return;

    EnterCriticalSection(&s_cs);
    s_active = false;
    s_stopRequested = false;
    memset(&s_x, 0, sizeof(s_x));
    memset(&s_y, 0, sizeof(s_y));
    s_frames = 0;
    s_sawT = false;
    s_sawNoT = false;
    s_haveResult = false;
    s_lastResult[0] = '\0';
    LeaveCriticalSection(&s_cs);
}
