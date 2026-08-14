import { createApp } from 'vue';
import { createPinia } from 'pinia';
import App from './App.vue';
import { bootstrap } from './bootstrap';
import '@ysc/core/ui/tokens.css';
import './styles.css';

const app = createApp(App);
app.use(createPinia());
bootstrap(); // 构造 WebSerialTransport + YscDevice 并注入 store
app.mount('#app');
