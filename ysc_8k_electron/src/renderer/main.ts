import { createApp } from 'vue';
import { createPinia } from 'pinia';
import App from './App.vue';
import '@ysc/core/ui/tokens.css'; // Wooting 配色系统（替代旧 styles/variables.css）
import './styles/global.css';
import { initI18n } from './i18n/index.js';
import { bootstrapCore } from './bootstrap-core';

initI18n();
const app = createApp(App);
app.use(createPinia());
app.mount('#app');
// 挂载后接入共享核心（store 在 pinia 安装后可用）
bootstrapCore();
