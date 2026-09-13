#include "device_context.h"
#include "serial_port.h"
#include "main.h"
#include "debug_logger.h"
#include <cstring>
#include <cstdio>
#include <cstdlib>

static DeviceSlot        s_slots[DEV_MAX_SLOTS];
static DeviceContext::DataCallback s_dataCb = nullptr;
static void              *s_dataUser = nullptr;
static DeviceContext::LostCallback s_lostCb = nullptr;
static void              *s_lostUser = nullptr;

// 探测节流：探测是串口往返，不能每轮主循环都发
static DWORD s_lastProbeTick = 0;

// 探测状态机阶段：每槽独立（0=待发715 1=待发711 2=完成）
static int s_probeStage[DEV_MAX_SLOTS] = {};

// --------------------------------------------------------------------------
// 角色自动识别 —— 解析上游设备身份
//
// 固件 cmd 715 回 {vid,pid,product}；cmd 711 回上游的 CONFIG 描述符（hex），
// 后者含每个 HID 接口的 bInterfaceProtocol：0x01=键盘、0x02=鼠标 ——
// 这是判定角色的权威依据。只看 vid/pid 不够：同一只键鼠复合接收器两种角色
// 都可能出现，无法区分"上游插的是鼠标还是键盘"。
// --------------------------------------------------------------------------

// 从 711 回包里取 CONFIG 描述符的 hex 串，返回 HID 接口协议位图。
//
// 注意固件实际回的是**嵌套 JSON**（cJSON 会把内层对象的引号转义）：
//   {"code":711,"message":"ok",
//    "data":"{\"total\":84,\"offset\":0,\"len\":84,\"hex\":\"0902540003...\"}"}
// 所以 hex 在 data 内层对象的 "hex" 字段里，**不是** data 本身 ——
// 早先按 "data":"<hex>" 解析永远拿不到内容，角色识别必然失败。
// 另外该回包可能被截断、也可能不带 offset（整条一次给），故做容错取值。
static bool ParseCfgIfaceMask(const char *json, int *maskOut) {
    const char *p = strstr(json, "hex");
    if (!p) return false;
    p = strchr(p, ':');
    if (!p) return false;
    p++;
    while (*p == ' ' || *p == '\\' || *p == '"') p++;
    if (!*p) return false;

    uint8_t buf[512];
    int n = 0;
    auto hexv = [](char c) -> int {
        if (c >= '0' && c <= '9') return c - '0';
        if (c >= 'a' && c <= 'f') return c - 'a' + 10;
        if (c >= 'A' && c <= 'F') return c - 'A' + 10;
        return -1;
    };
    while (*p && n < (int)sizeof(buf)) {
        if (*p == '\\' || *p == '"') break;      // 内层字符串结束
        int hi = hexv(p[0]);
        if (hi < 0) break;
        int lo = hexv(p[1]);
        if (lo < 0) break;
        buf[n++] = (uint8_t)((hi << 4) | lo);
        p += 2;
    }
    if (n < 9 || buf[1] != 0x02) return false;   // 必须是 CONFIG 描述符

    /* 解析：CONFIG 头后按 bLength 步进，取每个 HID 接口的 bInterfaceProtocol。
     * total 字段可能因为回包截断而偏大，所以以实际解析到的字节数为界。 */
    int mask = 0;
    int total = buf[2] | (buf[3] << 8);
    if (total > n) total = n;
    for (int i = 0; i + 8 < total; ) {
        int blen  = buf[i];
        int btype = buf[i + 1];
        if (blen < 2) break;
        if (btype == 0x04 && blen >= 9) {            // INTERFACE
            uint8_t cls   = buf[i + 5];
            uint8_t proto = buf[i + 7];
            if (cls == 0x03) {                        // HID
                if (proto == 0x01)      mask |= 0x1;  // keyboard
                else if (proto == 0x02) mask |= 0x2;  // mouse
            }
        }
        i += blen;
    }
    *maskOut = mask;
    return true;
}

/* 从 JSON 里取某个 key 后的整数，兼容两种引号形态。
 *
 * 为什么不能直接 strstr("\"vid\":")：固件回的 715 里 data 是**嵌套 JSON**，
 * 内层引号在传输时是带反斜杠的 —— 线上实际是 `{\"vid\":13652,...}`。
 * 而 C 源码里的 "\"vid\":" 编译出来是 `"vid":`（无反斜杠），永远匹配不上，
 * 于是 vid/pid 恒为 0。这里只搜 `key\` 这个不带引号的前缀，
 * 从而对 `"key":` / `\"key\":` 两种形态都成立。 */
static bool JsonIntAfterKey(const char *json, const char *key, int *out, const char **after) {
    char pat[24];
    int n = snprintf(pat, sizeof(pat), "%s\\", key);
    if (n <= 0 || n >= (int)sizeof(pat)) return false;
    const char *p = strstr(json, pat);
    if (!p) return false;
    /* 关键：key 后面线上是 `\":`（引号+冒号），必须 locate 到冒号之后才是数字。
     * 早先直接 p += strlen(pat) 会停在 ':' 上，把数值解析成 0 / 直接失败。 */
    p = strchr(p, ':');
    if (!p) return false;
    p++;
    while (*p == ' ' || *p == '\\' || *p == '"') p++;
    if (*p != '-' && (*p < '0' || *p > '9')) return false;
    *out = atoi(p);
    if (after) *after = p;
    return true;
}

static bool ParseVidPidProduct(const char *json, int *vid, int *pid,
                               char *product, int prodSize) {
    if (!JsonIntAfterKey(json, "vid", vid, nullptr)) return false;
    if (!JsonIntAfterKey(json, "pid", pid, nullptr)) return false;

    if (product && prodSize > 0) {
        product[0] = '\0';
        const char *p = strstr(json, "product\\");
        if (p) {
            p = strchr(p, ':');              // 跳到冒号后
            if (p) {
                p++;
                while (*p == ' ') p++;
                while (*p == '\\' || *p == '"') p++;   // 跳过 \" 或 "
                int i = 0;
                while (*p && *p != '"' && *p != '\\' && i < prodSize - 1)
                    product[i++] = *p++;
                product[i] = '\0';
            }
        }
    }
    return true;
}

// 返回 true = 该帧被探测流程消费，不再向上层转发
static bool ProbeConsume(int slot, const char *json) {
    if (slot < 0 || slot >= DEV_MAX_SLOTS) return false;
    DeviceSlot &d = s_slots[slot];
    /* 判据用 probeDone 而不是 probeState：probeState==2 在"已发出 711 等待
     * 回包"时就会置上，用它会让 715/711 的回包被当成普通数据直接转发掉，
     * 探测永远拿不到结论。 */
    if (d.probeDone) return false;                   // 已有结论：全部转发

    bool is715 = strstr(json, "\"code\":715") != nullptr;
    bool is711 = strstr(json, "\"code\":711") != nullptr;
    if (is715) DebugLogger::Log("PROBE got715 raw=%.180s", json);
    if (is711) DebugLogger::Log("PROBE got711 raw=%.120s", json);
    if (!is715 && !is711) return false;

    if (is715) {
        int vid = -1, pid = -1;
        char prod[48] = {};
        if (ParseVidPidProduct(json, &vid, &pid, prod, sizeof(prod))) {
            d.vid = vid;
            d.pid = pid;
            strncpy_s(d.product, prod, _TRUNCATE);
            d.probeGotInfo = 1;
        }
    } else {
        int mask = 0;
        if (ParseCfgIfaceMask(json, &mask)) {
            d.ifaceMask = mask;
            d.probeGotCfg = 1;
        }
    }

    /* 两块都到齐才下结论。
     * 只看 711 就定角色是不行的：设备可能仍处在上一次 debug_enter 留下的
     * 调试模式里，711 会先于本次探测到达，结果 role 判出来了但 vid/pid 是 0。
     * 等 715 到齐（或 ProbeTick 超时兜底）再收尾。 */
    if (d.probeGotInfo && d.probeGotCfg) {
        bool hasKbd = (d.ifaceMask & 0x1) != 0;
        bool hasMouse = (d.ifaceMask & 0x2) != 0;
        if (hasKbd && hasMouse) {
            /* 两个接口都有 —— 同款透传盒子都是这样，描述符分不出角色。
             * 保留 none 并标记 ambiguous，等设备实际数据来定
             *（鼠标机在报 mouse 数据、键盘机在报 kbd 数据）。 */
            d.role = DEV_ROLE_NONE;
            d.probeAmbiguous = 1;
        } else if (hasKbd) {
            d.role = DEV_ROLE_KBD;
        } else if (hasMouse) {
            d.role = DEV_ROLE_MOUSE;
        } else {
            d.role = DEV_ROLE_NONE;
        }
        d.autoDetected = 1;
        d.probeState = 2;
        d.probeDone = 1;
        s_probeStage[slot] = 2;
        DebugLogger::Log("PROBE done slot=%d mask=%d vid=%04X pid=%04X role=%d ambig=%d",
                         slot, d.ifaceMask, (unsigned)d.vid, (unsigned)d.pid,
                         d.role, d.probeAmbiguous);
    }
    return true;
}

// --------------------------------------------------------------------------
// 串口数据入口：先给探测消费，再转上层
// --------------------------------------------------------------------------
static void OnSerialDataForSlot(const uint8_t *data, int len, void *userData) {
    intptr_t slot = (intptr_t)userData;
    if (len > 0 && len < 1024) {
        char json[1024];
        memcpy(json, data, len);
        json[len] = '\0';
        if (ProbeConsume((int)slot, json)) return;
    }
    if (s_dataCb) s_dataCb((int)slot, data, len, s_dataUser);
}

// --------------------------------------------------------------------------
// 槽位生命周期
// --------------------------------------------------------------------------
DeviceSlot *DeviceContext::Get(int slot) {
    if (slot < 0 || slot >= DEV_MAX_SLOTS) return nullptr;
    return &s_slots[slot];
}

int DeviceContext::Count() {
    int n = 0;
    for (int i = 0; i < DEV_MAX_SLOTS; i++) if (s_slots[i].InUse()) n++;
    return n;
}

int DeviceContext::Attach(int slot, const wchar_t *port, DWORD baud) {
    if (slot < 0 || slot >= DEV_MAX_SLOTS || !port) return -1;

    DeviceSlot &d = s_slots[slot];
    if (d.serial) Detach(slot);          // 复用时先断开旧的

    d.serial = new SerialPort();
    if (!d.serial->Connect(port, baud)) {
        delete d.serial;
        d.serial = nullptr;
        return -1;
    }

    d.serial->SetResponseCallback(OnSerialDataForSlot, (void *)(intptr_t)slot);
    d.serial->StartReadThread(g_app.hStopEvent);

    wcsncpy_s(d.port, port, _TRUNCATE);
    d.baud = baud;
    d.connected = true;
    d.role = DEV_ROLE_NONE;
    d.autoDetected = false;
    d.roleManual = false;
    d.probeAmbiguous = 0;
    d.probeState = 0;
    d.probeDone = 0;
    d.probeGotInfo = 0;
    d.probeGotCfg = 0;
    d.probeDeadline = GetTickCount() + 5000;
    d.ifaceMask = 0;
    d.vid = d.pid = -1;
    d.product[0] = '\0';
    s_probeStage[slot] = 0;
    return slot;
}

void DeviceContext::Detach(int slot) {
    DeviceSlot *d = Get(slot);
    if (!d) return;
    bool wasInUse = d->InUse();
    if (d->serial) {
        d->serial->StopReadThread();
        d->serial->Disconnect();
        delete d->serial;
        d->serial = nullptr;
    }
    d->connected = false;
    d->port[0] = '\0';
    d->baud = 0;
    d->probeState = 0;
    d->ifaceMask = 0;
    s_probeStage[slot] = 0;
    if (wasInUse && s_lostCb) s_lostCb(slot, s_lostUser);
}

void DeviceContext::DetachAll() {
    for (int i = 0; i < DEV_MAX_SLOTS; i++) Detach(i);
}

// --------------------------------------------------------------------------
// 按角色取设备
// --------------------------------------------------------------------------
int DeviceContext::SlotByRole(int role) {
    if (role != DEV_ROLE_NONE) {
        for (int i = 0; i < DEV_MAX_SLOTS; i++)
            if (s_slots[i].connected && s_slots[i].role == role) return i;
    }
    /* 单设备兼容 / 未指定角色：解析到"第一个已连接设备"。
     *   role == DEV_ROLE_NONE —— 调用方本来就没有设备偏好（如 UI 的 raw 命令），
     *   沿用旧行为；
     *   两台都连着、且**明确要的角色**又没被指定 —— 返回 -1 拒绝，绝不退回 slot 0：
     *   那会把键盘命令静默发到鼠标盒（固件回 404 no_keyboard，调用方却看到成功）。
     *   集火场景下角色由用户在设备面板里指定（device_set_role → SetRole）。 */
    int only = -1, n = 0;
    for (int i = 0; i < DEV_MAX_SLOTS; i++) {
        if (s_slots[i].connected) { n++; only = i; }
    }
    if (role == DEV_ROLE_NONE) return only;
    return (n == 1) ? only : -1;
}

SerialPort *DeviceContext::SerialForRole(int role) {
    int slot = SlotByRole(role);
    return (slot >= 0) ? s_slots[slot].serial : nullptr;
}

SerialPort *DeviceContext::SerialForSlot(int slot) {
    DeviceSlot *d = Get(slot);
    return d ? d->serial : nullptr;
}

bool DeviceContext::SwitchBaud(int slot, DWORD baud) {
    DeviceSlot *d = Get(slot);
    if (!d || !d->serial || !d->connected || baud == 0) return false;
    if (d->baud == baud) return true;          /* 已是目标速率，无需切换 */

    const DWORD oldBaud = d->baud;

    if (!d->serial->SwitchBaudrate(baud)) {
        /* 切到新速率后验证失败。此时固件也可能没切成（例如它不支持该速率，
         * 直接丢弃了 cmd 133），两边就此失联 —— 必须把两端都拉回原速率，
         * 否则设备表面上"还在"，实际只能靠断电恢复。 */
        if (d->serial->SwitchBaudrate(oldBaud)) {
            d->serial->SetResponseCallback(OnSerialDataForSlot, (void *)(intptr_t)slot);
            d->serial->StartReadThread(g_app.hStopEvent);
            d->baud = oldBaud;
            d->connected = true;
            d->probeState = 0;
            d->probeDone = 0;
            d->probeDeadline = GetTickCount() + 5000;
            s_probeStage[slot] = 0;
            DebugLogger::Log("DEVICE slot=%d baud %u unsupported, reverted to %u",
                             slot, (unsigned)baud, (unsigned)oldBaud);
        } else {
            d->connected = false;
        }
        return false;
    }

    /* 切换过程停掉了读线程，必须重挂回调并重启，否则设备显示"已连接"但收不到数据 */
    d->serial->SetResponseCallback(OnSerialDataForSlot, (void *)(intptr_t)slot);
    d->serial->StartReadThread(g_app.hStopEvent);

    d->baud = baud;
    d->connected = true;
    /* 新速率下重新探一轮角色（切换后固件侧 USART 也变了，旧探测结果不可信） */
    d->probeState = 0;
    d->probeDone = 0;
    d->probeGotInfo = 0;
    d->probeGotCfg = 0;
    d->probeDeadline = GetTickCount() + 5000;
    s_probeStage[slot] = 0;
    return true;
}

bool DeviceContext::SetRole(int slot, int role) {
    DeviceSlot *d = Get(slot);
    if (!d) return false;
    if (role != DEV_ROLE_NONE && role != DEV_ROLE_MOUSE && role != DEV_ROLE_KBD)
        return false;

    if (role == DEV_ROLE_NONE) {
        /* "交回自动识别"：重新发起一轮探测。
         * 旧实现这里直接置 probeState=2，导致用户选「自动」后探测被永久关闭，
         * 只能拔插设备才能恢复。 */
        d->role = DEV_ROLE_NONE;
        d->autoDetected = false;
        d->roleManual = false;      // 交回自动识别
        d->probeAmbiguous = 0;
        d->probeDone = 0;
        d->probeState = 0;
        d->probeGotInfo = 0;
        d->probeGotCfg = 0;
        d->probeDeadline = GetTickCount() + 5000;
        s_probeStage[slot] = 0;
        return true;
    }

    d->role = role;
    d->autoDetected = false;
    d->roleManual = true;         // 人工指定优先：数据流判据与探测都不再覆盖
    d->probeAmbiguous = 0;
    d->probeState = 2;
    d->probeDone = 1;
    return true;
}

void DeviceContext::SetDataCallback(DataCallback cb, void *userData) {
    s_dataCb = cb;
    s_dataUser = userData;
}

void DeviceContext::SetLostCallback(LostCallback cb, void *userData) {
    s_lostCb = cb;
    s_lostUser = userData;
}

int DeviceContext::SlotByReaderGen(LONG gen) {
    for (int i = 0; i < DEV_MAX_SLOTS; i++)
        if (s_slots[i].serial && s_slots[i].serial->ReaderGen() == gen) return i;
    return -1;
}

/* 数据流兜底判据：设备实际上报的是鼠标(start)还是键盘数据。
 *
 * 为什么需要它：基于描述符的角色识别要固件支持 cmd 715/711（调试抓包），
 * 旧固件不认这两条命令 → 拿不到描述符 → 识别永远没结论。而"设备在报鼠标
 * 数据还是键盘数据"这件事所有固件版本都成立，所以用它兜底。
 *
 * 只在自动识别还没结论、且解析出明确类型时才落角色（probeDone=1），
 * 避免过早锁死；用户手动指定过的槽位不动。 */
void DeviceContext::NoteDataKind(int slot, int kind) {
    if (slot < 0 || slot >= DEV_MAX_SLOTS) return;
    DeviceSlot &d = s_slots[slot];
    if (!d.connected) return;
    if (kind == 0)      d.sawMouseData = 1;
    else if (kind == 1) d.sawKbdData = 1;
    else return;

    /* 用户手动指定过就不动。注意不能拿 (autoDetected==false) 反推：
     * 自动识别完成后 autoDetected 也是 true，而"自动判定为模糊态"
     * (probeAmbiguous) 恰恰需要被数据流判据覆盖。 */
    if (d.roleManual) return;

    d.role = (kind == 0) ? DEV_ROLE_MOUSE : DEV_ROLE_KBD;
    d.probeAmbiguous = 0;      // 数据流给出了确定答案，不再是模糊态
    d.autoDetected = 1;
    d.probeDone = 1;
    d.probeState = 2;
    DebugLogger::Log("PROBE role by data slot=%d kind=%d role=%d", slot, kind, d.role);
}

// --------------------------------------------------------------------------
// 探测驱动：主循环周期调用。发 715 拿身份、发 711 拿接口协议判角色。
// 到 probeDeadline 仍判不出来就停手，留给 UI 手动指定角色。
// --------------------------------------------------------------------------
void DeviceContext::ProbeTick() {
    DWORD now = GetTickCount();
    if (now - s_lastProbeTick < 100) return;     // 100ms 节流
    s_lastProbeTick = now;

    for (int i = 0; i < DEV_MAX_SLOTS; i++) {
        DeviceSlot &d = s_slots[i];
        if (!d.serial || !d.connected) continue;
        if (d.probeDone) continue;               // 已有结论
        if (d.autoDetected) continue;

        /* 超时：必须落一个"结论"，否则 probeDone 永远为 0，
         * 前端会永远停在"识别中…"。此时 role 保持 none，
         * 由用户在下拉里手动指定。 */
        if (now > d.probeDeadline) {
            /* 超时也尽量用已到手的信息给个结论：只拿到描述符就按描述符判，
             * 只拿到身份就保持 none。 */
            if (d.probeGotCfg) {
                if (d.ifaceMask & 0x1)      d.role = DEV_ROLE_KBD;
                else if (d.ifaceMask & 0x2) d.role = DEV_ROLE_MOUSE;
                d.autoDetected = true;
            }
            d.probeDone = 1;
            d.probeState = 2;
            DebugLogger::Log("PROBE timeout slot=%d gotInfo=%d gotCfg=%d mask=%d role=%d",
                             i, d.probeGotInfo, d.probeGotCfg, d.ifaceMask, d.role);
            continue;
        }

        if (s_probeStage[i] == 0) {
            d.serial->SendJsonCommand("{\"cmd\":715}");
            s_probeStage[i] = 1;
            continue;
        }
        if (s_probeStage[i] == 1) {
            /* 715 已发出。这里**不能**置 probeDone/probeState —— 那会让
             * 715 自身的回包被 ProbeConsume 当成普通数据处理掉。 */
            d.serial->SendJsonCommand("{\"cmd\":711}");
            s_probeStage[i] = 2;
            continue;
        }
        /* stage==2：715/711 都已发出，等回包（超时分支在上面兜底） */
    }
}
