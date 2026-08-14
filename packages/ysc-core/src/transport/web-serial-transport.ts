/**
 * WebSerialTransport —— 基于 navigator.serial (Web Serial API) 的传输适配器。
 *
 * 职责（保持"哑"：只管连接 + 帧编解码，不做业务语义解释）：
 *  - 端口授权/枚举/连接/断开
 *  - 写：sendJson（加帧头帧尾）/ sendRaw（裸字节，IAP/MAKCU）
 *  - 读：常驻 readLoop 喂入 FrameParser，每个还原帧 emit 'frame'（原始 payload）
 *  - 连接级事件：connected / disconnected / error / ports / usbConnect / usbDisconnect
 *
 * 帧内容（monitor/version/debug 回读）的语义解释交给 YscDevice。
 *
 * 浏览器限制：requestPort 须由用户手势调用；仅 Chromium 系 + 安全源可用。
 */
import { FrameParser } from '../protocol/rx-parser';
import { buildJsonFrame } from '../protocol/frame';
import { YSC_USB_VENDOR, YSC_USB_PRODUCT } from '../protocol/constants';
import { Emitter, decodeUtf8 } from '../utils';
import type { DeviceEvent, DeviceTransport, PortFilter, PortInfo } from './types';

type SerialPortLike = any; // Web Serial SerialPort（运行时存在即可，类型在 lib 不全）

export class WebSerialTransport implements DeviceTransport {
  readonly kind = 'web-serial' as const;

  private serial: SerialPortLike;
  /** 授权端口池：id → SerialPort。 */
  private ports = new Map<string, SerialPortLike>();
  private nextId = 1;

  private activePort: SerialPortLike | null = null;
  private activeId: string | null = null;
  private reader: ReadableStreamDefaultReader<Uint8Array> | null = null;
  private writer: WritableStreamDefaultWriter | null = null;
  private parser: FrameParser;
  private readAbort = false;
  private _connected = false;

  private bus = new Emitter<DeviceEvent>();

  constructor() {
    if (typeof navigator === 'undefined' || !('serial' in navigator)) {
      throw new Error('Web Serial API 不可用（需 Chromium 系浏览器 + 安全源）');
    }
    this.serial = (navigator as any).serial;
    this.parser = new FrameParser((payload) => {
      // 把每个还原帧作为原始 payload 投递；YscDevice 负责语义解释
      this.bus.emit({ type: 'frame', payload });
    });
    // 已授权端口的热插拔
    this.serial.addEventListener('connect', this.onUsbConnect);
    this.serial.addEventListener('disconnect', this.onUsbDisconnect);
  }

  get connected(): boolean {
    return this._connected;
  }

  private idFor(port: SerialPortLike): string {
    for (const [id, p] of this.ports) if (p === port) return id;
    return '';
  }

  private portToInfo(port: SerialPortLike): PortInfo {
    const info = port.getInfo?.() || {};
    const id = this.idFor(port) || `ws${this.nextId++}`;
    if (!this.ports.has(id)) this.ports.set(id, port);
    const vid = info.usbVendorId;
    const pid = info.usbProductId;
    const label =
      vid != null ? `USB ${vid.toString(16).padStart(4, '0')}:${pid?.toString(16).padStart(4, '0') ?? ''}` : 'USB 串口';
    return { id, label, usbVendorId: vid, usbProductId: pid, side: '', description: label };
  }

  private onUsbConnect = (ev: Event) => {
    const port = (ev as any).target as SerialPortLike;
    if (port) this.bus.emit({ type: 'usbConnect', port: this.portToInfo(port) });
  };

  private onUsbDisconnect = (ev: Event) => {
    const port = (ev as any).target as SerialPortLike;
    if (port) {
      const info = this.portToInfo(port);
      this.bus.emit({ type: 'usbDisconnect', port: info });
      if (port === this.activePort) this.handleLoss('设备已断开');
    }
  };

  async enumeratePorts(): Promise<PortInfo[]> {
    const list: SerialPortLike[] = await this.serial.getPorts();
    return list.map((p) => this.portToInfo(p));
  }

  async requestPort(opts?: PortFilter): Promise<PortInfo> {
    const filter = {
      filters: [
        {
          usbVendorId: opts?.usbVendor ?? YSC_USB_VENDOR,
          usbProductId: opts?.usbProduct ?? YSC_USB_PRODUCT,
        },
      ],
    };
    const port: SerialPortLike = await this.serial.requestPort(filter);
    return this.portToInfo(port);
  }

  async connect(portId: string, baud?: number): Promise<{ port: string; baud: number }> {
    const port = this.ports.get(portId);
    if (!port) throw new Error(`未知端口 id: ${portId}`);
    if (this._connected) await this.disconnect();

    const rate = baud ?? 115200;
    await port.open({ baudRate: rate });

    this.activePort = port;
    this.activeId = portId;
    this.parser.reset();
    this.readAbort = false;

    this.writer = port.writable.getWriter();
    this.startReadLoop(port);

    this._connected = true;
    const ev: DeviceEvent = { type: 'connected', port: portId, baud: rate };
    this.bus.emit(ev);
    return { port: portId, baud: rate };
  }

  private startReadLoop(port: SerialPortLike): void {
    (async () => {
      const reader: ReadableStreamDefaultReader<Uint8Array> = port.readable.getReader();
      this.reader = reader;
      try {
        while (!this.readAbort) {
          const { done, value } = await reader.read();
          if (done) break;
          if (value) this.parser.feed(value);
        }
      } catch (e: any) {
        if (!this.readAbort) this.handleLoss(e?.message || '读取异常');
      } finally {
        try {
          reader.releaseLock();
        } catch {
          /* ignore */
        }
        this.reader = null;
      }
    })();
  }

  async disconnect(): Promise<void> {
    this.readAbort = true;
    try {
      this.reader?.cancel?.();
    } catch {
      /* ignore */
    }
    try {
      if (this.writer) {
        await this.writer.close().catch(() => {});
        this.writer.releaseLock();
      }
    } catch {
      /* ignore */
    }
    this.writer = null;
    try {
      if (this.activePort) {
        await this.activePort.close().catch(() => {});
      }
    } catch {
      /* ignore */
    }
    this.activePort = null;
    this.activeId = null;
    if (this._connected) {
      this._connected = false;
      this.bus.emit({ type: 'disconnected' });
    }
  }

  private handleLoss(message: string): void {
    this.readAbort = true;
    const was = this._connected;
    this._connected = false;
    this.activePort = null;
    this.activeId = null;
    this.writer = null;
    if (was) {
      this.bus.emit({ type: 'error', message });
      this.bus.emit({ type: 'disconnected' });
    }
  }

  async sendJson(json: string): Promise<boolean> {
    return this.sendRaw(buildJsonFrame(json));
  }

  async sendRaw(bytes: Uint8Array): Promise<boolean> {
    if (!this.writer) return false;
    try {
      await this.writer.write(bytes);
      return true;
    } catch (e: any) {
      this.handleLoss(e?.message || '写入失败');
      return false;
    }
  }

  async jumpToIap(): Promise<boolean> {
    return this.sendJson('{"cmd":50}');
  }

  supportsUsbHotplug(): boolean {
    return true;
  }

  on(handler: (e: DeviceEvent) => void): () => void {
    return this.bus.onAny(handler);
  }

  onType<K extends DeviceEvent['type']>(
    type: K,
    handler: (e: Extract<DeviceEvent, { type: K }>) => void,
  ): () => void {
    return this.bus.on(type, handler);
  }

  /** 当前活动端口标签（供 UI 显示）。 */
  get activeLabel(): string {
    return this.activeId ?? '';
  }

  /** 仅用于工具：把 payload 当文本看（调试用）。 */
  static debugDecode(payload: Uint8Array): string {
    return decodeUtf8(payload);
  }
}
