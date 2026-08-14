/**
 * 传输层契约。两套适配器（WebSerialTransport / DriverPipeTransport）实现同一接口，
 * 上层 YscDevice 与 store 据此保持传输无关。
 *
 * 设计要点：
 *  - connect/requestPort 分离：浏览器需用户手势触发 requestPort；桌面端由驱动枚举后直接 connect。
 *  - sendJson 统一走 <START><BE u16><json><END> 帧；sendRaw 发已构造好的 IAP 二进制帧。
 *  - 入站字节流由适配器内部用 FrameParser 还原为帧，再按内容分发为语义 DeviceEvent。
 */

/** v2 towmcu 侧别（Left/Right/IAP），网页版经 cmd132 版本探测得出。 */
export type TowmcuSide = '' | 'Left' | 'Right' | 'IAP';

/** 一个可连接的端口描述。 */
export interface PortInfo {
  /** 端口稳定标识（网页版为内部自增 key；Electron 为 "COMn"）。 */
  id: string;
  /** 展示名（"COM7" / "USB 串口设备 (COM7)"）。 */
  label: string;
  /** USB 厂商 ID（网页版可得；Electron 通常省略）。 */
  usbVendorId?: number;
  /** USB 产品 ID。 */
  usbProductId?: number;
  /** v2 侧别分类。 */
  side?: TowmcuSide;
  /** 额外描述。 */
  description?: string;
}

/** 统一事件流。适配器把各自的原始事件翻译成这些语义事件。 */
export type DeviceEvent =
  | { type: 'connected'; port: string; baud: number }
  | { type: 'disconnected' }
  | { type: 'error'; message: string }
  | { type: 'baudrate'; baud: number; ok: boolean }
  | {
      type: 'monitor';
      buttons: number;
      x: number;
      y: number;
      wheel: number;
    }
  | { type: 'version'; version: string }
  | { type: 'state'; serialConnected: boolean; serialPort: string; serialBaud: number }
  | { type: 'ports'; ports: PortInfo[] }
  | { type: 'iapLog'; message: string; cls?: string }
  | { type: 'iapProgress'; current: number; total: number; status: string }
  | { type: 'iapDone'; success: boolean; error?: string; code?: string }
  | { type: 'usbConnect'; port: PortInfo }
  | { type: 'usbDisconnect'; port: PortInfo }
  /** 原始帧：未被识别为上述语义的 JSON/IAP payload，交给 YscDevice 分发（debug/gamepad 回读等）。 */
  | { type: 'frame'; payload: Uint8Array };

/** requestPort 过滤条件（网页版用于筛选 YSC CDC 设备）。 */
export interface PortFilter {
  usbVendor?: number;
  usbProduct?: number;
}

/** 传输适配器接口。 */
export interface DeviceTransport {
  readonly kind: 'web-serial' | 'driver-pipe';
  /** 当前是否已连接。 */
  readonly connected: boolean;

  /** 列出已授权/已枚举的端口（不弹选择器）。 */
  enumeratePorts(): Promise<PortInfo[]>;
  /** 请求一个端口。浏览器：弹系统选择器（需用户手势）；Electron：no-op 直接回传。 */
  requestPort(opts?: PortFilter): Promise<PortInfo>;
  /** 打开端口连接。baud 省略时用 115200（v2 CDC）或自动探测（v1）。 */
  connect(portId: string, baud?: number): Promise<{ port: string; baud: number }>;
  /** 关闭连接。 */
  disconnect(): Promise<void>;

  /** 发送 JSON 控制帧（自动加 <START><len><END> 外层）。 */
  sendJson(json: string): Promise<boolean>;
  /** 发送已构造的原始字节（IAP 二进制帧或 MAKCU 文本）。 */
  sendRaw(bytes: Uint8Array): Promise<boolean>;

  /** 订阅统一事件流，返回取消订阅函数。 */
  on(handler: (e: DeviceEvent) => void): () => void;
  /** 仅订阅某类事件（便捷）。 */
  onType<K extends DeviceEvent['type']>(
    type: K,
    handler: (e: Extract<DeviceEvent, { type: K }>) => void,
  ): () => void;

  /** 跳转 IAP（cmd 50）。重枚举由上层烧录状态机处理。 */
  jumpToIap(): Promise<boolean>;
  /** 是否支持 USB 热插拔事件（网页版 true）。 */
  supportsUsbHotplug(): boolean;
}

/** 适配器构造时传入的事件发射器；避免适配器反向依赖 store。 */
export type DeviceEventEmitter = (e: DeviceEvent) => void;
