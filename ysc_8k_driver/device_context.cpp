#include "device_context.h"
#include "serial_port.h"
#include "main.h"
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

// 从 "data":"<hex>" 里解析 CONFIG 描述符，返回 HID 接口协议位图。
// 帧格式来自固件 dbg_send_descr：{"code":711,"message":"descr","data":"<hex>"}
static bool ParseCfgIfaceMask(const char *json, int *maskOut) {
    const char *p = strstr(json, "\"data\":\"");
    if (!p) return false;
    p += 8;   // strlen("\"data\":\"")

    uint8_t buf[512];
    int n = 0;
    auto hexv = [](char c) -> int {
        if (c >= '0' && c <= '9') return c - '0';
        if (c >= 'a' && c <= 'f') return c - 'a' + 10;
        if (c >= 'A' && c <= 'F') return c - 'A' + 10;
        return -1;
    };
    while (*p && *p != '"' && n < (int)sizeof(buf)) {
        char hi = *p++;
        if (!*p || *p == '"') break;
        char lo = *p++;
        int h = hexv(hi), l = hexv(lo);
        if (h < 0 || l < 0) break;
        buf[n++] = (uint8_t)((h << 4) | l);
    }
    if (n < 9 || buf[1] != 0x02) return false;   // 必须是 CONFIG 描述符

    int mask = 0;
    int total = buf[2] | (buf[3] << 8);
    if (total > n) total = n;
    for (int i = 0; i + 8 < total; ) {
        int blen = buf[i];
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

static bool ParseVidPidProduct(const char *json, int *vid, int *pid,
                               char *product, int prodSize) {
    const char *p = strstr(json, "\"vid\":");
    if (!p) return false;
    *vid = atoi(p + 6);
    p = strstr(json, "\"pid\":");
    if (!p) return false;
    *pid = atoi(p + 6);
    p = strstr(json, "\"product\":\"");
    if (p && product && prodSize > 0) {
        p += 11;
        int i = 0;
        while (*p && *p != '"' && i < prodSize - 1) product[i++] = *p++;
        product[i] = '\0';
    }
    return true;
}

// 返回 true = 该帧被探测流程消费，不再向上层转发
static bool ProbeConsume(int slot, const char *json) {
    if (slot < 0 || slot >= DEV_MAX_SLOTS) return false;
    DeviceSlot &d = s_slots[slot];
    if (d.probeState == 2) return false;             // 已判定：全部转发

    bool is715 = strstr(json, "\"code\":715") != nullptr;
    bool is711 = strstr(json, "\"code\":711") != nullptr;
    if (!is715 && !is711) return false;

    if (is715) {
        int vid = -1, pid = -1;
        char prod[48] = {};
        if (ParseVidPidProduct(json, &vid, &pid, prod, sizeof(prod))) {
            d.vid = vid;
            d.pid = pid;
            strncpy_s(d.product, prod, _TRUNCATE);
        }
        return true;
    }

    int mask = 0;
    if (ParseCfgIfaceMask(json, &mask)) {
        d.ifaceMask = mask;
        if (mask & 0x1)      d.role = DEV_ROLE_KBD;    // 上游是键盘
        else if (mask & 0x2) d.role = DEV_ROLE_MOUSE;  // 上游是鼠标
        else                 d.role = DEV_ROLE_NONE;   // 既无键盘也无鼠标接口
        d.autoDetected = true;
        d.probeState = 2;
        s_probeStage[slot] = 2;
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
    d.probeState = 0;
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
    // 单设备兼容：没有任何设备声明该角色时退回第一个已连接设备，
    // 这样"只有一台设备、命令不带设备维度"的旧用法行为完全不变。
    for (int i = 0; i < DEV_MAX_SLOTS; i++)
        if (s_slots[i].connected) return i;
    return -1;
}

SerialPort *DeviceContext::SerialForRole(int role) {
    int slot = SlotByRole(role);
    return (slot >= 0) ? s_slots[slot].serial : nullptr;
}

SerialPort *DeviceContext::SerialForSlot(int slot) {
    DeviceSlot *d = Get(slot);
    return d ? d->serial : nullptr;
}

bool DeviceContext::SetRole(int slot, int role) {
    DeviceSlot *d = Get(slot);
    if (!d) return false;
    if (role != DEV_ROLE_NONE && role != DEV_ROLE_MOUSE && role != DEV_ROLE_KBD)
        return false;
    d->role = role;
    d->autoDetected = false;      // 人工指定优先，自动探测不再改动
    d->probeState = 2;
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
        if (d.probeState == 2) continue;         // 已判定
        if (d.autoDetected) continue;
        if (now > d.probeDeadline) continue;     // 超时：放弃自动识别

        if (s_probeStage[i] == 0) {
            d.serial->SendJsonCommand("{\"cmd\":715}");
            s_probeStage[i] = 1;
            continue;
        }
        if (s_probeStage[i] == 1) {
            d.serial->SendJsonCommand("{\"cmd\":711}");
            s_probeStage[i] = 2;
            continue;
        }
    }
}
