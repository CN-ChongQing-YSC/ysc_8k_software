/**
 * DriverPipeTransport —— Electron 版传输适配器。
 *
 * 包裹 renderer 的 window.driverApi（经 preload contextBridge 注入），把命名管道
 * 与 C++ ysc_8k_driver.exe 的事件翻译为统一 DeviceEvent。构造时注入 api，核心库
 * 不直接引用 window。
 *
 * 与 WebSerialTransport 的关键差异：
 *  - requestPort：no-op（驱动自己开 COM 口；UI 用枚举下拉选择，不走选择器手势）。
 *  - 端口枚举：enum_ports（COM 名）+ towmcu_list_ports（带 side 分类）。
 *  - sendJson：路由到 send_ysc {cmd: json}；IAP/MAKCU 走驱动专用命令，不走 sendRaw。
 *  - 固件烧录：驱动在 C++ 内跑 ERASE→PROGRAM→VERIFY→END 循环，本适配器只转发进度。
 *
 * 已知 gap（Phase 2-finish，需硬件验证）：设备固件版本（cmd 132 回读）目前在驱动侧
 * 主要用于波特率探测，未必转发为 renderer 事件；故 queryVersion 在 Electron 上可能
 * 超时，直到驱动补一个 device_response 转发。连接/监控/键盘/手柄/调试路径已完整。
 */
import { Emitter } from '../utils';
import type { DeviceEvent, DeviceTransport, PortFilter, PortInfo, TowmcuSide } from './types';

/** 注入的 driverApi 表面（仅用到的子集）。 */
export interface DriverApiLike {
  send(type: string, params?: Record<string, unknown>): void;
  isConnected(): Promise<boolean>;
  on(event: string, callback: (data: any) => void): () => void;
  off(event: string, callback?: any): void;
}

function mapSide(s: string): TowmcuSide {
  const u = (s || '').toUpperCase();
  if (u.includes('LEFT')) return 'Left';
  if (u.includes('RIGHT')) return 'Right';
  if (u.includes('IAP')) return 'IAP';
  return '';
}

export class DriverPipeTransport implements DeviceTransport {
  readonly kind = 'driver-pipe' as const;

  private api: DriverApiLike;
  private bus = new Emitter<DeviceEvent>();
  private _connected = false;
  private unsubscribers: Array<() => void> = [];
  private _curPort = '';
  private _curBaud = 0;

  constructor(api: DriverApiLike) {
    this.api = api;
    this.wireEvents();
  }

  get connected(): boolean {
    return this._connected;
  }

  private subscribe(event: string, cb: (data: any) => void): void {
    const off = this.api.on(event, cb);
    this.unsubscribers.push(off);
  }

  private wireEvents(): void {
    this.subscribe('serial_connected', (d) => {
      this._connected = true;
      this._curPort = d.port;
      this._curBaud = d.baud;
      this.bus.emit({ type: 'connected', port: d.port, baud: d.baud });
    });
    this.subscribe('serial_disconnected', () => {
      this._connected = false;
      this.bus.emit({ type: 'disconnected' });
    });
    this.subscribe('serial_error', (d) => {
      this.bus.emit({ type: 'error', message: d.message || '串口错误' });
    });
    this.subscribe('baudrate_switched', (d) => {
      this._curBaud = d.baud;
      this.bus.emit({ type: 'baudrate', baud: d.baud, ok: true });
    });
    this.subscribe('baudrate_failed', (d) => {
      this.bus.emit({ type: 'baudrate', baud: 0, ok: false });
      this.bus.emit({ type: 'error', message: d.message || '切换波特率失败' });
    });
    this.subscribe('monitor_data', (d) => {
      this.bus.emit({
        type: 'monitor',
        buttons: d.buttons,
        x: d.x,
        y: d.y,
        wheel: d.wheel ?? 0,
      });
    });
    this.subscribe('version', (d) => {
      // 注意：这是"驱动版本"（C++ 硬编码），非设备固件版本
      this.bus.emit({ type: 'version', version: d.version });
    });
    this.subscribe('ports_list', (d) => {
      const list: PortInfo[] = (d.ports || []).map((p: string) => ({
        id: p,
        label: p,
        side: '',
      }));
      this.bus.emit({ type: 'ports', ports: list });
    });
    this.subscribe('towmcu_ports', (d) => {
      const list: PortInfo[] = (d.ports || []).map((p: any) => ({
        id: p.port,
        label: p.port,
        side: mapSide(p.side || p.serial || ''),
        description: p.desc,
      }));
      this.bus.emit({ type: 'ports', ports: list });
    });
  }

  async enumeratePorts(): Promise<PortInfo[]> {
    // 触发驱动枚举；事件 arrive 异步，这里返回当前已知快照
    this.api.send('enum_ports');
    this.api.send('towmcu_list_ports');
    return [];
  }

  async requestPort(_opts?: PortFilter): Promise<PortInfo> {
    // Electron：驱动自开 COM 口，UI 用枚举下拉；本方法在桌面流程不应被调用
    throw new Error('桌面版使用端口下拉选择，无需 requestPort');
  }

  async connect(portId: string, baud?: number): Promise<{ port: string; baud: number }> {
    this.api.send('serial_connect', { port: portId, baud: baud ?? 0 });
    // 实际结果由 serial_connected / serial_error 事件回推
    return { port: portId, baud: baud ?? 0 };
  }

  async disconnect(): Promise<void> {
    this.api.send('serial_disconnect');
  }

  async sendJson(json: string): Promise<boolean> {
    this.api.send('send_ysc', { cmd: json });
    return true;
  }

  async sendRaw(_bytes: Uint8Array): Promise<boolean> {
    // 桌面版裸串口由驱动托管（IAP 走 iap_start_mem / towmcu_start_mem，MAKCU 走 send_makcu）
    throw new Error('桌面版不支持 sendRaw；IAP/MAKCU 走驱动专用命令');
  }

  async jumpToIap(): Promise<boolean> {
    this.api.send('jump_iap');
    return true;
  }

  supportsUsbHotplug(): boolean {
    return false; // 驱动经 ports_changed 事件推端口变化，无 USB 级热插拔
  }

  // ── DeviceTransport 接口实现 ──
  on(handler: (e: DeviceEvent) => void): () => void {
    return this.bus.onAny(handler);
  }

  onType<K extends DeviceEvent['type']>(
    type: K,
    handler: (e: Extract<DeviceEvent, { type: K }>) => void,
  ): () => void {
    return this.bus.on(type, handler);
  }

  dispose(): void {
    for (const off of this.unsubscribers) {
      try {
        off();
      } catch {
        /* ignore */
      }
    }
    this.unsubscribers = [];
    this.bus.clear();
  }
}
