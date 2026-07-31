import { createApp } from 'vue';
import App from './App.vue';
import './styles/variables.css';
import './styles/global.css';
import { initI18n } from './i18n/index.js';

initI18n();
createApp(App).mount('#app');
