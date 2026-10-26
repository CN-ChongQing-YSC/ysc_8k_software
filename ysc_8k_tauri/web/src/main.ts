import { createApp } from 'vue';
import { createPinia } from 'pinia';
import { invoke } from '@tauri-apps/api/core';
import App from '@ysc/ui/App.vue';
import '@ysc/core/ui/tokens.css';
import '@ysc/ui/styles/global.css';
import { initI18n } from '@ysc/ui/i18n/index.js';
import { installTauriDriverApi } from './driver-api-shim';
import { installDesktopShell } from './desktop-shell';
import {
  DriverPipeTransport,
  YscDevice,
  useDeviceStore,
  useUiStore,
} from '@ysc/core';

installTauriDriverApi();
installDesktopShell();
initI18n();

const app = createApp(App);
app.use(createPinia());
app.mount('#app');

// 共享核心 store 继续用 DriverPipeTransport 包同一个 driverApi 兼容层；
// 事件名/载荷与旧管道一致，NavRail/StatusBar 等共享 UI 无需改。
const api = (window as any).driverApi;
if (api) {
  const store = useDeviceStore();
  store.attach(new YscDevice(new DriverPipeTransport(api)));
  void store.refreshPorts();
}

// 启动自动烧录：Rust 侧通过 --auto-flash-v3=<path> 或 exe 同目录 v3_firmware.bin
// 提供路径；这里先切到 V3 页，等面板挂载/事件监听就绪后再让 Rust 开始。
async function startAutoFlashIfConfigured(): Promise<void> {
  try {
    const autoPath = await invoke<string | null>('ysc_auto_flash_info');
    if (!autoPath) return;
    useUiStore().setView('firmware-v3');
    window.setTimeout(() => {
      // 显式传 path:null —— 走启动时解析到的默认镜像（exe 同目录 v3_firmware.bin）；
      // 不传参在部分 Tauri 版本上会被当成"缺必填参数"。
      void invoke('ysc_auto_flash_start', { path: autoPath }).catch((e) => {
        console.error('[ysc] auto flash start failed', e);
      });
    }, 1200);
  } catch (e) {
    console.error('[ysc] auto flash info failed', e);
  }
}

void startAutoFlashIfConfigured();
