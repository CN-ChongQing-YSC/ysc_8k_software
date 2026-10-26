import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';

type Handler = (data: any) => void;

interface PickedFile {
  path: string;
  info: string;
}

const handlers = new Map<string, Set<Handler>>();
const downloadedFirmwarePaths = new Map<string, string>();
let tauriUnlisten: (() => void) | null = null;
let listenerStarting = false;

function emitLocal(event: string, data: any): void {
  const set = handlers.get(event);
  if (!set) return;
  for (const fn of Array.from(set)) {
    try {
      fn(data);
    } catch (e) {
      console.error('[driver-api-shim] handler error', event, e);
    }
  }
}

async function ensureTauriListener(): Promise<void> {
  if (tauriUnlisten || listenerStarting) return;
  listenerStarting = true;
  try {
    tauriUnlisten = await listen<string>('ysc:driver-event', (event) => {
      try {
        const data = JSON.parse(event.payload);
        if (data && typeof data.type === 'string') {
          emitLocal(data.type, data);
        }
      } catch (e) {
        console.error('[driver-api-shim] bad driver event', e, event.payload);
      }
    });
  } catch (e) {
    console.error('[driver-api-shim] listen failed', e);
  } finally {
    listenerStarting = false;
  }
}

function on(event: string, callback: Handler): () => void {
  let set = handlers.get(event);
  if (!set) {
    set = new Set();
    handlers.set(event, set);
  }
  set.add(callback);
  void ensureTauriListener();
  return () => {
    const current = handlers.get(event);
    if (!current) return;
    current.delete(callback);
    if (current.size === 0) handlers.delete(event);
  };
}

function off(event: string, callback?: Handler): void {
  if (!callback) {
    handlers.delete(event);
    return;
  }
  const set = handlers.get(event);
  if (!set) return;
  set.delete(callback);
  if (set.size === 0) handlers.delete(event);
}

function fire(type: string, params?: Record<string, unknown>): void {
  const payload = { type, ...(params || {}) };
  // 等事件监听就绪再下发，避免 UI 刚挂载时 enum_ports 的异步响应丢在监听建立之前。
  void ensureTauriListener()
    .then(() => invoke('ysc_driver_send', { payload }))
    .catch((e) => {
      console.error('[driver-api-shim] send failed', type, e);
    });
}

function send(type: string, params?: Record<string, unknown>): void {
  // Electron 主进程里被拦截的窗口/退出命令，在 Tauri 里同样不下发驱动。
  if (type === 'window_minimize') {
    void invoke('ysc_window_minimize').catch(() => undefined);
    return;
  }
  if (type === 'window_maximize') {
    void invoke('ysc_window_toggle_maximize').catch(() => undefined);
    return;
  }
  if (type === 'app_quit') {
    void invoke('ysc_quit').catch(() => undefined);
    return;
  }

  // V1/V2/V3 的在线下载都把文件缓存在本地；这里把缓存转成宿主
  // iap_start_mem / towmcu_start_mem 需要的 base64，再走既有 IAP 状态机。
  if (type === 'iap_start_mem' || type === 'towmcu_start_mem') {
    const p = params || {};
    const deviceType = String(p.deviceType || '');
    const path = downloadedFirmwarePaths.get(deviceType);
    const logEvent = type === 'iap_start_mem' ? 'iap_log' : 'iap2_log';
    if (!path) {
      emitLocal(logEvent, {
        message: '未找到在线固件缓存，请先点击“检查更新/下载”后再升级',
        cls: 'err',
      });
      return;
    }
    void invoke<string>('ysc_firmware_read_base64', { path })
      .then((data) => {
        if (type === 'iap_start_mem') {
          fire('iap_start_mem', { data, baud: p.baud });
        } else {
          fire('towmcu_start_mem', { port: p.port, data });
        }
      })
      .catch((e) => {
        emitLocal(logEvent, {
          message: '读取在线固件缓存失败: ' + String(e),
          cls: 'err',
        });
      });
    return;
  }

  fire(type, params);
}

async function pickFile(title: string): Promise<PickedFile | null> {
  try {
    const selected = await open({
      title,
      multiple: false,
      directory: false,
      filters: [
        { name: '加密固件 (.bin)', extensions: ['bin'] },
        { name: '所有文件', extensions: ['*'] },
      ],
    });
    if (!selected || Array.isArray(selected)) return null;
    return await invoke<PickedFile>('ysc_file_info', { path: selected });
  } catch (e) {
    console.error('[driver-api-shim] open file failed', e);
    return null;
  }
}

async function openIAPFile(): Promise<PickedFile | null> {
  return pickFile('选择加密固件文件');
}

async function openTowmcuFile(side: 'left' | 'right'): Promise<PickedFile | null> {
  return pickFile(side === 'left' ? '选择左固件 (Left)' : '选择右固件 (Right)');
}

async function openV3File(): Promise<PickedFile | null> {
  return pickFile('选择 YSC V3 加密固件 (.bin)');
}

async function setAutoFlash(on: boolean, path?: string): Promise<boolean> {
  try {
    if (on) {
      return await invoke<boolean>('ysc_auto_flash_start', { path: path ?? null });
    }
    return await invoke<boolean>('ysc_auto_flash_stop');
  } catch (e) {
    console.error('[driver-api-shim] setAutoFlash failed', e);
    return false;
  }
}

async function getAutoFlashStatus(): Promise<boolean> {
  try {
    return await invoke<boolean>('ysc_auto_flash_status');
  } catch {
    return false;
  }
}

async function getAppVersion(): Promise<string> {
  try {
    return await invoke<string>('ysc_get_version');
  } catch {
    return '2.0.0';
  }
}

async function isConnected(): Promise<boolean> {
  try {
    return await invoke<boolean>('ysc_driver_is_connected');
  } catch {
    return false;
  }
}

async function setLang(lang: 'zh' | 'en'): Promise<void> {
  try {
    await invoke('ysc_set_lang', { lang });
  } catch {
    /* no-op */
  }
}

// 在线固件服务：同一台 firmware_server，按 deviceType 区分 V1/V2/V3。
async function checkFirmwareLatest(deviceType: string): Promise<any> {
  try {
    return await invoke<any>('ysc_firmware_latest', { deviceType });
  } catch (e) {
    console.error('[driver-api-shim] firmware latest failed', e);
    return null;
  }
}

async function listFirmwareVersions(deviceType: string): Promise<any[]> {
  try {
    return await invoke<any[]>('ysc_firmware_list', { deviceType });
  } catch (e) {
    console.error('[driver-api-shim] firmware list failed', e);
    return [];
  }
}

async function downloadFirmware(info: any): Promise<void> {
  if (!info || !info.deviceType || !info.filename) {
    emitLocal('firmware:download_done', {
      deviceType: info?.deviceType,
      success: false,
      error: '固件信息不完整，无法下载',
    });
    return;
  }
  try {
    const result = await invoke<any>('ysc_firmware_download', { info });
    if (!result || !result.path) {
      throw new Error('下载未返回缓存路径');
    }
    downloadedFirmwarePaths.set(info.deviceType, result.path);
    emitLocal('firmware:download_done', {
      deviceType: info.deviceType,
      success: true,
      info: result.info,
      path: result.path,
      version: result.version,
      filename: result.filename,
      size: result.size,
      sha256: result.sha256,
    });
  } catch (e) {
    console.error('[driver-api-shim] firmware download failed', e);
    emitLocal('firmware:download_done', {
      deviceType: info.deviceType,
      success: false,
      error: String(e),
      version: info.version,
      filename: info.filename,
    });
  }
}

function recordFirmwareInstalled(deviceType: string, version: string): void {
  void invoke('ysc_firmware_record_installed', { deviceType, version }).catch((e) => {
    console.error('[driver-api-shim] record installed failed', e);
  });
}

async function exportPdf(_html: string, _filename?: string): Promise<{
  success?: boolean;
  canceled?: boolean;
  path?: string;
  error?: string;
}> {
  return {
    success: false,
    error: 'PDF 导出在 Tauri 首版暂未接入；请使用网页版或等待后续版本',
  };
}

export interface DriverApi {
  send(type: string, params?: Record<string, unknown>): void;
  isConnected(): Promise<boolean>;
  on(event: string, callback: Handler): () => void;
  off(event: string, callback?: Handler): void;
  getAppVersion(): Promise<string>;
  setLang(lang: 'zh' | 'en'): Promise<void>;
  openIAPFile(): Promise<PickedFile | null>;
  openTowmcuFile(side: 'left' | 'right'): Promise<PickedFile | null>;
  openV3File(): Promise<PickedFile | null>;
  /** 自动烧录开关（V3 页）：on=true 开始等设备插入，=false 停止。 */
  setAutoFlash(on: boolean, path?: string): Promise<boolean>;
  /** 自动烧录线程是否还在等设备。 */
  getAutoFlashStatus(): Promise<boolean>;
  checkFirmwareLatest(deviceType: string): Promise<any>;
  listFirmwareVersions(deviceType: string): Promise<any[]>;
  downloadFirmware(info: any): Promise<void>;
  recordFirmwareInstalled(deviceType: string, version: string): void;
  exportPdf(html: string, filename?: string): Promise<any>;
  checkCh343Driver(): void;
  installCh343Driver(): Promise<{ ok: boolean; error?: string }>;
  debugEnter(): void;
  debugExit(): void;
  debugStatus(): void;
  debugGetDevDescr(): void;
  debugGetCfgDescr(): void;
  debugGetRepDescr(offset?: number, max?: number): void;
  debugGetReport(idx: number): void;
  debugClearReports(): void;
  debugGetDeviceInfo(): void;
}

export function installTauriDriverApi(): void {
  const api: DriverApi = {
    send,
    isConnected,
    on,
    off,
    getAppVersion,
    setLang,
    openIAPFile,
    openTowmcuFile,
    openV3File,
    setAutoFlash,
    getAutoFlashStatus,
    checkFirmwareLatest,
    listFirmwareVersions,
    downloadFirmware,
    recordFirmwareInstalled,
    exportPdf,
    checkCh343Driver: () => send('check_ch343_driver'),
    installCh343Driver: async () => ({
      ok: false,
      error: 'CH343 提权安装在 Tauri 首版暂未接入',
    }),
    debugEnter: () => send('debug_enter'),
    debugExit: () => send('debug_exit'),
    debugStatus: () => send('debug_status'),
    debugGetDevDescr: () => send('debug_get_dev_descr'),
    debugGetCfgDescr: () => send('debug_get_cfg_descr'),
    debugGetRepDescr: (offset = 0, max = 200) => send('debug_get_rep_descr', { offset, max }),
    debugGetReport: (idx: number) => send('debug_get_report', { idx }),
    debugClearReports: () => send('debug_clear_reports'),
    debugGetDeviceInfo: () => send('debug_get_device_info'),
  };

  (window as any).driverApi = api;
  (window as any).yscPlatform = { desktop: true, host: 'tauri' };
  (window as any).yscWindow = {
    isMaximized: () => invoke<boolean>('ysc_window_is_maximized'),
  };
}
