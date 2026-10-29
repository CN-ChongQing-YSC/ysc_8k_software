import { createApp } from 'vue';
import GamepadTester from '@ysc/ui/components/GamepadTester.vue';
import '@ysc/core/ui/tokens.css';
import '@ysc/ui/styles/global.css';
import { initI18n } from '@ysc/ui/i18n/index.js';
import { installTauriDriverApi } from './driver-api-shim';

installTauriDriverApi();
initI18n();
// initI18n() 会把 document.title 设成主应用标题；独立窗口改回自己的标题
//（Tauri 会把窗口标题同步成 document.title）。
document.title = '手柄测试器';

// 独立窗口：standalone 模式铺满窗口，标题栏区域可拖动，X 关闭本窗口。
createApp(GamepadTester, { standalone: true }).mount('#app');
