import * as net from 'net';
import { EventEmitter } from 'events';

const PIPE_PATH = '\\\\.\\pipe\\ysc_8k_driver';

/**
 * 设备角色：双设备场景下一台透传鼠标、一台透传键盘。
 * 驱动按角色路由命令（鼠标命令发鼠标机、键盘命令发键盘机），
 * 前端只需按角色读取状态。单设备时驱动会退回"第一个已连接设备"。
 */
export type DeviceRoleStr = 'mouse' | 'kbd' | 'none';

/** 一个设备槽位（驱动侧 DeviceSlot 的镜像） */
export interface DeviceSlotInfo {
  dev: number;            // 槽位下标 0/1
  role: DeviceRoleStr;
  auto: boolean;          // 角色是自动识别(true)还是人工指定(false)
  connected: boolean;
  port: string;
  baud: number;
  vid: number;            // 上游设备 VID（-1 = 未知）
  pid: number;
  product: string;        // 上游设备产品名
  ifaceMask: number;      // bit0=见到键盘接口, bit1=见到鼠标接口
  probeDone: number;      // 1=识别已有结论（成功或超时放弃）
}

/** 鼠标物理状态快照 */
export interface MonitorState {
  valid: number;
  buttons: number;
  x: number;
  y: number;
}

/** 键盘物理状态快照（只含物理按键，不含上位机注入的键） */
export interface KbdState {
  valid: number;
  mod: number;            // 修饰键位图：bit0..7 = HID 0xE0..0xE7
  count: number;          // keys[] 有效项数
  overflow: number;       // 超出 6 个未列出的键数
  keys: number[];         // 非修饰 HID 键码，升序
}

/** 物理护栏校准的单轴统计（驱动 human_guard 事件） */
export interface GuardAxisStat {
  count: number;
  min: number;
  p5: number;
  median: number;
  last: number;
  suggest: number;
  reliable: boolean;
  recent: number[];
}

export interface HumanGuardEvent {
  phase: 'idle' | 'running' | 'done' | 'error';
  session?: number;
  axis?: 'x' | 'y';
  duration?: number;
  elapsed?: number;
  frames?: number;
  timebase?: 'device' | 'host' | 'mixed' | 'none';
  reason?: string;
  dirX?: number;
  dirY?: number;
  x?: GuardAxisStat;
  y?: GuardAxisStat;
}

export interface DriverEvents {
  ready: void;
  ports_list: { ports: string[] };
  ports_changed: { ports: string[] };
  serial_connected: { port: string; baud: number };
  serial_error: { message: string };
  serial_disconnected: void;
  baudrate_switched: { baud: number };
  baudrate_failed: { message: string };
  kmnet_started: { port: number; ip: string };
  kmnet_error: { message: string };
  kmnet_stopped: void;
  upload_status: { enable: boolean };
  monitor_data: {
    buttons: number; x: number; y: number; wheel: number;
    mouseValid?: number;          // 鼠标侧是否收到过数据
    kbd?: KbdState;               // 键盘侧物理状态（新增）
  };
  // 物理护栏校准（驱动侧测量）
  human_guard: HumanGuardEvent;
  log: { level: string; message: string };
  local_ip: { ip: string };
  version: { version: string };
  state: {
    serialConnected: boolean;
    serialPort: string;
    serialBaud: number;
    netRunning: boolean;
    netPort: number;
    mouse?: MonitorState;         // 新增
    kbd?: KbdState;               // 新增
  };
  disconnected: void;
  connected: void;
  error: Error;
  // ── 双设备（一鼠标机 + 一键盘机）──
  device_list: { devices: DeviceSlotInfo[] };
  device_attached: { dev: number; port: string; baud: number };
  device_detached: { dev: number };
  device_lost: { dev: number; role: DeviceRoleStr; message: string };
  kbd_state: KbdState;
  kbd_result: { ok: boolean };
  debug_cmd_result: {
    cmd: 'enter' | 'exit' | 'status' | 'dev_descr' | 'cfg_descr' |
         'rep_descr' | 'report' | 'clear' | 'device_info';
    ok: boolean;
    error?: string;
  };
  debug_response: {
    code: number;
    message: string | null;
    data: string | null;
  };
  // ── 8K V2 firmware update ──
  towmcu_ports: {
    ports: { port: string; serial: string; side: string; desc: string }[];
  };
  towmcu_version: { port: string; version: string; mode: 'APP' | 'IAP' };
  iap_entered: { port: string };  // port === '' 表示进入 IAP 失败
  iap2_log: { message: string; cls?: string };
  iap2_progress: { current: number; total: number; status: string };
  iap2_done: { success: boolean; error?: string; code?: string };
  // ── CH343 / WCH 驱动健康度（VID 1A86 设备是否缺驱动）──
  ch343_driver_status: {
    present: boolean;   // 是否存在任何 VID 1A86 设备
    problem: boolean;   // 是否存在异常（problem 码 != 0）设备
    problems: { pid: string; name: string; code: number }[];
  };
}

type EventHandler<K extends keyof DriverEvents> = DriverEvents[K] extends void
  ? () => void
  : (data: DriverEvents[K]) => void;

export class DriverPipe extends EventEmitter {
  private client: net.Socket | null = null;
  private lineBuffer = '';
  private _connected = false;
  /* 自动重连：驱动是被 Electron 拉起来的子进程，但它也可能被外部重启
   * （手动调驱动、崩溃重启、升级替换）。一旦管道断开，旧 Socket 上的所有
   * send() 都会静默失败（writable 为 false），界面就会永远停在最后一帧状态、
   * 既不报错也不更新 —— 排查时极易被误判成"固件没数据"。
   * 这里在断开后自动重试，直到重新连上或调用方显式 disconnect()。 */
  private _reconnectTimer: NodeJS.Timeout | null = null;
  private _reconnectDelay = 500;
  private _closedByUser = false;
  private _autoReconnectEnabled = false;

  get isConnected(): boolean {
    return this._connected;
  }

  /** 启动自动重连（主进程启动时调用一次；重复调用无副作用）。 */
  enableAutoReconnect(): void {
    if (this._autoReconnectEnabled) return;
    this._autoReconnectEnabled = true;
    this._closedByUser = false;
    this.on('disconnected', () => this.scheduleReconnect());
    this.on('error', () => this.scheduleReconnect());
  }

  private scheduleReconnect(): void {
    if (this._closedByUser || this._reconnectTimer) return;
    const delay = this._reconnectDelay;
    // 退避到最多 3s，避免驱动长期不在时狂刷连接
    this._reconnectDelay = Math.min(this._reconnectDelay * 2, 3000);
    this._reconnectTimer = setTimeout(() => {
      this._reconnectTimer = null;
      if (this._closedByUser || this._connected) return;
      this.connect(2000)
        .then(() => {
          this._reconnectDelay = 500;
          /* 重连成功后主动要一次全量状态：断线期间设备可能已被重新枚举，
           * 前端需要一份新快照来替换掉冻结的旧数据。 */
          this.enumPorts();
          this.send('device_list');
        })
        .catch(() => this.scheduleReconnect());
    }, delay);
  }

  connect(timeout = 5000): Promise<void> {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.client?.destroy();
        this.client = null;
        reject(new Error('Connection timeout'));
      }, timeout);

      this.client = net.createConnection(PIPE_PATH, () => {
        clearTimeout(timer);
        this._connected = true;
        this.emit('connected');
        resolve();
      });

      this.client.on('data', (chunk: Buffer) => {
        this.lineBuffer += chunk.toString('utf8');
        const lines = this.lineBuffer.split('\n');
        this.lineBuffer = lines.pop()!;
        for (const line of lines) {
          const trimmed = line.trim();
          if (trimmed) this.handleMessage(trimmed);
        }
      });

      this.client.on('close', () => {
        clearTimeout(timer);
        this._connected = false;
        this.emit('disconnected');
      });

      this.client.on('error', (err: Error) => {
        clearTimeout(timer);
        if (!this._connected) {
          reject(err);
        } else {
          this._connected = false;
          this.emit('error', err);
        }
      });
    });
  }

  private handleMessage(line: string) {
    try {
      const msg = JSON.parse(line);
      if (msg.type) {
        this.emit(msg.type, msg);
        // 通配事件：让主进程能"默认转发所有驱动事件"。
        // 之前只转发白名单里的事件，结果驱动新增一类事件（如 kmnet_stat）时
        // 忘了同步白名单就会**静默丢弃**，界面永远空白且毫无线索。
        this.emit('*', msg);
      }
    } catch {
      // Ignore non-JSON lines
    }
  }

  send(type: string, params: Record<string, unknown> = {}): void {
    if (!this.client?.writable) return;
    const msg = JSON.stringify({ type, ...params }) + '\n';
    this.client.write(msg);
  }

  disconnect(): void {
    this._closedByUser = true;
    if (this._reconnectTimer) {
      clearTimeout(this._reconnectTimer);
      this._reconnectTimer = null;
    }
    this.client?.destroy();
    this.client = null;
    this._connected = false;
  }

  // Convenience methods
  enumPorts() { this.send('enum_ports'); }
  connectSerial(port: string, baud = 0) { this.send('serial_connect', { port, baud }); }
  disconnectSerial() { this.send('serial_disconnect'); }
  switchBaudrate(baud: number) { this.send('switch_baudrate', { baud }); }
  startKmnet(port: number) { this.send('kmnet_start', { port }); }
  stopKmnet() { this.send('kmnet_stop'); }
  enableUpload(enable: boolean) { this.send('upload_enable', { enable }); }
  jumpIAP() { this.send('jump_iap'); }
  getLocalIp() { this.send('get_local_ip'); }
  getVersion() { this.send('get_version'); }
  getState() { this.send('get_state'); }

  // ── 双设备（一鼠标机 + 一键盘机）──
  /** 列设备槽位（结果经 device_list 事件回推） */
  deviceList() { this.send('device_list'); }
  /** 把设备挂到槽位。role 省略=驱动自动识别（探测上游是鼠标还是键盘） */
  deviceAttach(dev: number, port: string, baud = 0, role?: DeviceRoleStr) {
    const params: Record<string, unknown> = { dev, port, baud };
    if (role && role !== 'none') params.role = role;
    this.send('device_attach', params);
  }
  deviceDetach(dev: number) { this.send('device_detach', { dev }); }
  /** 人工指定角色（优先于自动识别） */
  deviceSetRole(dev: number, role: DeviceRoleStr) {
    this.send('device_set_role', { dev, role });
  }
  /** 键盘物理状态查询（结果经 kbd_state 事件回推） */
  getKbdState() { this.send('get_kbd_state'); }
  /** 键盘单键注入：kc 为 HID 键码 0x04-0xE7（修饰键 0xE0-0xE7） */
  kbdKey(kc: number, down: boolean) {
    this.send('kbd_key', { kc, down: down ? 1 : 0 });
  }
  /** 逐字符打字符串（≤128 字节），固件自动处理大小写与 CapsLock */
  kbdTypeString(s: string) { this.send('kbd_type_string', { s }); }
  kbdReleaseAll() { this.send('kbd_release_all'); }

  // Debug capture (sends command; responses come back via 'debug_response' event)
  debugEnter()                    { this.send('debug_enter'); }
  debugExit()                     { this.send('debug_exit'); }
  debugStatus()                   { this.send('debug_status'); }
  debugGetDevDescr()              { this.send('debug_get_dev_descr'); }
  debugGetCfgDescr()              { this.send('debug_get_cfg_descr'); }
  debugGetRepDescr(offset = 0, max = 200) {
    this.send('debug_get_rep_descr', { offset, max });
  }
  debugGetReport(idx: number)     { this.send('debug_get_report', { idx }); }
  debugClearReports()             { this.send('debug_clear_reports'); }
  debugGetDeviceInfo()            { this.send('debug_get_device_info'); }

  // ── 8K V2 firmware update ──
  towmcuListPorts()                { this.send('towmcu_list_ports'); }
  towmcuQueryVersion(port: string) { this.send('towmcu_query_version', { port }); }
  towmcuEnterIAP(port: string)     { this.send('towmcu_enter_iap', { port }); }
  towmcuStart(port: string, path: string) {
    this.send('towmcu_start', { port, path });
  }
  // 在线下载直传：把已下载到内存的固件 Buffer 以 Base64 编码塞进 JSON，
  // 通过现有命名管道发给 C++ 端的 iap_start_mem / towmcu_start_mem 处理器。
  iapStartMem(buf: Buffer, sha256: string, baud?: number) {
    const params: Record<string, unknown> = {
      data: buf.toString('base64'),
      size: buf.length,
      sha256,
    };
    if (baud !== undefined) params.baud = baud;
    this.send('iap_start_mem', params);
  }
  towmcuStartMem(port: string, buf: Buffer, sha256: string) {
    this.send('towmcu_start_mem', { port, data: buf.toString('base64'), size: buf.length, sha256 });
  }
  towmcuCancel()                   { this.send('towmcu_cancel'); }

  // ── CH343 / WCH 驱动健康度检测（结果经 ch343_driver_status 事件回推）──
  checkCh343Driver()               { this.send('check_ch343_driver'); }
}
