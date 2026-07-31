import * as net from 'net';
import { EventEmitter } from 'events';

const PIPE_PATH = '\\\\.\\pipe\\ysc_8k_driver';

export interface DriverEvents {
  ready: void;
  ports_list: { ports: string[] };
  serial_connected: { port: string; baud: number };
  serial_error: { message: string };
  serial_disconnected: void;
  baudrate_switched: { baud: number };
  baudrate_failed: { message: string };
  kmnet_started: { port: number; ip: string };
  kmnet_error: { message: string };
  kmnet_stopped: void;
  upload_status: { enable: boolean };
  monitor_data: { buttons: number; x: number; y: number; wheel: number };
  log: { level: string; message: string };
  local_ip: { ip: string };
  version: { version: string };
  state: {
    serialConnected: boolean;
    serialPort: string;
    serialBaud: number;
    netRunning: boolean;
    netPort: number;
  };
  disconnected: void;
  connected: void;
  error: Error;
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
  iap2_done: { success: boolean; error?: string };
}

type EventHandler<K extends keyof DriverEvents> = DriverEvents[K] extends void
  ? () => void
  : (data: DriverEvents[K]) => void;

export class DriverPipe extends EventEmitter {
  private client: net.Socket | null = null;
  private lineBuffer = '';
  private _connected = false;

  get isConnected(): boolean {
    return this._connected;
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
}
