/**
 * YscDevice —— 传输无关的设备命令层。
 *
 * 订阅 transport 的 'frame' 事件，把原始 payload 解释为语义事件（monitor/version），
 * 连接级事件（connected/disconnected/error/ports/usb*）原样透传。
 * 对外提供类型化的命令方法（queryVersion / setUploadStatus / keyboardKey ...），
 * 以及 transport 的便捷委托（connect/disconnect/requestPort）。
 *
 * 监控帧字段名按 {b,x,y[,wheel]} 约定（兼容嵌套在 data 下）。
 * 版本响应字段按 v/version/ver/msg/纯文本 宽松匹配。
 * —— 两处 schema 待真机确认后可收敛（见 plan 验证项）。
 */
import { Emitter, decodeUtf8, tryParseJson } from '../utils';
import type { DeviceEvent, DeviceTransport, PortFilter, PortInfo } from '../transport/types';
import {
  CMD_QUERY_VERSION,
  CMD_UPLOAD_STATUS,
  CMD_KEYBOARD_KEY,
  CMD_KEYBOARD_RELEASE_ALL,
  CMD_KEYBOARD_TYPE_STRING,
} from '../protocol/commands';

export class YscDevice {
  private bus = new Emitter<DeviceEvent>();
  private transport: DeviceTransport;

  /** 版本查询的一次性回调（简单请求/响应关联）。 */
  private versionResolver: ((v: string) => void) | null = null;
  private versionTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(transport: DeviceTransport) {
    this.transport = transport;
    // 透传连接级事件；帧自行解释
    this.transport.on((e) => {
      if (e.type === 'frame') return;
      this.bus.emit(e);
    });
    this.transport.onType('frame', (e) => this.handleFrame(e.payload));
  }

  private handleFrame(payload: Uint8Array): void {
    const text = decodeUtf8(payload);
    const json = tryParseJson<any>(text);

    if (json) {
      // 监控帧：{b,x,y} 平铺或嵌套于 data（以 x/y 数值为判定依据）
      const m =
        typeof json.x === 'number' && typeof json.y === 'number'
          ? json
          : json.data && typeof json.data === 'object' && typeof json.data.x === 'number'
            ? json.data
            : null;
      if (m) {
        this.bus.emit({
          type: 'monitor',
          buttons: Number(m.b ?? 0),
          x: Number(m.x ?? 0),
          y: Number(m.y ?? 0),
          wheel: Number(m.wheel ?? 0),
        });
        return;
      }
      // 版本响应
      const v =
        json.v ?? json.version ?? json.ver ?? (typeof json.msg === 'string' ? json.msg : undefined);
      if (this.versionResolver && v != null) {
        this.resolveVersion(String(v));
        return;
      }
    } else if (this.versionResolver && payload.length > 0) {
      // 非 JSON 的纯文本版本串
      this.resolveVersion(text);
      return;
    }

    // 其余帧透传（debug/gamepad 回读等，待后续阶段细化）
    this.bus.emit({ type: 'frame', payload });
  }

  private resolveVersion(v: string): void {
    if (this.versionTimer) clearTimeout(this.versionTimer);
    const fn = this.versionResolver;
    this.versionResolver = null;
    this.versionTimer = null;
    if (fn) fn(v);
  }

  // ── 事件订阅 ──
  on(handler: (e: DeviceEvent) => void): () => void {
    return this.bus.onAny(handler);
  }
  onType<K extends DeviceEvent['type']>(
    type: K,
    handler: (e: Extract<DeviceEvent, { type: K }>) => void,
  ): () => void {
    return this.bus.on(type, handler);
  }

  // ── transport 委托 ──
  get connected(): boolean {
    return this.transport.connected;
  }
  enumeratePorts(): Promise<PortInfo[]> {
    return this.transport.enumeratePorts();
  }
  requestPort(opts?: PortFilter): Promise<PortInfo> {
    return this.transport.requestPort(opts);
  }
  connect(portId: string, baud?: number): Promise<{ port: string; baud: number }> {
    return this.transport.connect(portId, baud);
  }
  disconnect(): Promise<void> {
    return this.transport.disconnect();
  }
  sendJson(json: string): Promise<boolean> {
    return this.transport.sendJson(json);
  }

  // ── 命令 ──
  /** 查询固件版本（cmd 132）。下一帧回复即视为版本串。 */
  async queryVersion(timeoutMs = 1500): Promise<string> {
    return new Promise<string>((resolve, reject) => {
      this.versionResolver = (v) => resolve(v);
      this.versionTimer = setTimeout(() => {
        this.versionResolver = null;
        this.versionTimer = null;
        reject(new Error('查询版本超时'));
      }, timeoutMs);
      this.transport.sendJson(JSON.stringify({ cmd: CMD_QUERY_VERSION })).catch((e) => {
        if (this.versionTimer) clearTimeout(this.versionTimer);
        this.versionResolver = null;
        this.versionTimer = null;
        reject(e);
      });
    });
  }

  /** 开关鼠标状态上报（cmd 34），开启后设备会持续推送监控帧。 */
  setUploadStatus(enable: boolean): Promise<boolean> {
    return this.transport.sendJson(
      JSON.stringify({ cmd: CMD_UPLOAD_STATUS, status: enable ? 1 : 0 }),
    );
  }

  /** 键盘按键（cmd 45）。kc 为 HID keycode。 */
  keyboardKey(kc: number, down: boolean): Promise<boolean> {
    return this.transport.sendJson(
      JSON.stringify({ cmd: CMD_KEYBOARD_KEY, kc, down: down ? 1 : 0 }),
    );
  }

  /** 释放全部键盘按键（cmd 46）。 */
  keyboardReleaseAll(): Promise<boolean> {
    return this.transport.sendJson(JSON.stringify({ cmd: CMD_KEYBOARD_RELEASE_ALL }));
  }

  /** 键盘输入字符串（cmd 47，≤128 字符）。 */
  keyboardTypeString(s: string): Promise<boolean> {
    return this.transport.sendJson(JSON.stringify({ cmd: CMD_KEYBOARD_TYPE_STRING, s }));
  }
}
