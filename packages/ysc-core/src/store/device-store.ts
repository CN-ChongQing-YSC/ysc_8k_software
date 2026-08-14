/**
 * useDeviceStore —— 设备状态（连接/端口/版本/监控/错误）。
 * 由注入的 YscDevice 事件驱动，替换 ysc_8k_electron App.vue 的 prop 透传。
 */
import { defineStore } from 'pinia';
import { ref, markRaw } from 'vue';
import type { YscDevice } from '../device/ysc-device';
import type { PortInfo } from '../transport/types';
import { parseVersion, type VersionInfo } from '../device/version-probe';

export const useDeviceStore = defineStore('ysc-device', () => {
  const device = ref<YscDevice | null>(null);
  const connected = ref(false);
  const connecting = ref(false);
  const port = ref('');
  const baud = ref(0);
  const ports = ref<PortInfo[]>([]);
  const versionRaw = ref('');
  const version = ref<VersionInfo | null>(null);
  const monitor = ref({ buttons: 0, x: 0, y: 0, wheel: 0 });
  const monitorOn = ref(false);
  const lastError = ref('');
  /** 监控帧计数（调试/健康指示）。 */
  const monitorCount = ref(0);

  /** 注入平台相关的 YscDevice（web/Electron 各自构造 transport）。 */
  function attach(dev: YscDevice): void {
    device.value = markRaw(dev);
    dev.on((e) => {
      switch (e.type) {
        case 'connected':
          connected.value = true;
          connecting.value = false;
          port.value = e.port;
          baud.value = e.baud;
          lastError.value = '';
          break;
        case 'disconnected':
          connected.value = false;
          connecting.value = false;
          monitorOn.value = false;
          break;
        case 'error':
          lastError.value = e.message;
          connecting.value = false;
          break;
        case 'ports':
          ports.value = e.ports;
          break;
        case 'monitor':
          monitor.value = { buttons: e.buttons, x: e.x, y: e.y, wheel: e.wheel };
          monitorCount.value++;
          break;
        default:
          break;
      }
    });
  }

  async function refreshPorts(): Promise<void> {
    if (!device.value) return;
    try {
      ports.value = await device.value.enumeratePorts();
    } catch (e: any) {
      lastError.value = e?.message || String(e);
    }
  }

  async function requestPort(): Promise<PortInfo | null> {
    if (!device.value) return null;
    try {
      const info = await device.value.requestPort();
      await refreshPorts();
      return info;
    } catch (e: any) {
      // 用户取消选择器等
      return null;
    }
  }

  async function connect(portId: string, baudVal?: number): Promise<void> {
    if (!device.value) return;
    connecting.value = true;
    lastError.value = '';
    try {
      await device.value.connect(portId, baudVal);
    } catch (e: any) {
      connecting.value = false;
      lastError.value = e?.message || String(e);
      throw e;
    }
  }

  async function disconnect(): Promise<void> {
    if (device.value) await device.value.disconnect();
  }

  async function fetchVersion(timeoutMs?: number): Promise<void> {
    if (!device.value || !connected.value) return;
    try {
      versionRaw.value = await device.value.queryVersion(timeoutMs);
      version.value = parseVersion(versionRaw.value);
    } catch (e: any) {
      lastError.value = e?.message || String(e);
    }
  }

  async function toggleMonitor(on: boolean): Promise<void> {
    if (!device.value || !connected.value) return;
    monitorOn.value = on;
    await device.value.setUploadStatus(on);
    if (!on) monitor.value = { buttons: 0, x: 0, y: 0, wheel: 0 };
  }

  return {
    device,
    connected,
    connecting,
    port,
    baud,
    ports,
    versionRaw,
    version,
    monitor,
    monitorOn,
    monitorCount,
    lastError,
    attach,
    refreshPorts,
    requestPort,
    connect,
    disconnect,
    fetchVersion,
    toggleMonitor,
  };
});
