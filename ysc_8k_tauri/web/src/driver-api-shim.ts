import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';

type Handler = (data: any) => void;

interface PickedFile {
  path: string;
  info: string;
}

const handlers = new Map<string, Set<Handler>>();
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

  // 在线固件下载缓存尚未接入；先给出明确日志，避免静默失败。
  if (type === 'iap_start_mem' || type === 'towmcu_start_mem') {
    emitLocal(type === 'iap_start_mem' ? 'iap_log' : 'iap2_log', {
      message: '在线固件缓存尚未接入 Tauri 版，请使用“手动选择文件”升级',
      cls: 'err',
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

// 在线固件服务：先占位，后续移植 firmware-service.ts。
async function checkFirmwareLatest(_deviceType: string): Promise<any> {
  return null;
}
async function listFirmwareVersions(_deviceType: string): Promise<any[]> {
  return [];
}
async function downloadFirmware(info: any): Promise<void> {
  emitLocal('firmware:download_done', {
    deviceType: info?.deviceType,
    success: false,
    error: '在线固件下载尚未接入 Tauri 版，请使用手动文件',
  });
}
function recordFirmwareInstalled(_deviceType: string, _version: string): void {
  /* no-op until firmware service lands */
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
