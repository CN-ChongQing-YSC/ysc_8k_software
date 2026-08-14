<template>
  <section class="card device-card">
    <header class="dc-head">
      <h2>我的设备</h2>
      <span class="state-pill" :class="dev.connected ? 'on' : 'off'">
        {{ dev.connected ? '已连接' : '未连接' }}
      </span>
    </header>

    <!-- 未连接：授权 + 选择 + 连接 -->
    <template v-if="!dev.connected">
      <div v-if="isWeb" class="row">
        <button class="btn primary" :disabled="dev.connecting" @click="onAddDevice">＋ 选择设备</button>
        <button class="btn" @click="dev.refreshPorts()">刷新</button>
      </div>

      <div v-if="dev.ports.length" class="row">
        <select v-model="selectedPort" class="select">
          <option v-for="p in dev.ports" :key="p.id" :value="p.id">
            {{ p.label }}<template v-if="p.side"> · {{ p.side }}</template>
          </option>
        </select>
        <select v-model.number="selectedBaud" class="select baud">
          <option :value="115200">115200</option>
          <option :value="921600">921600</option>
          <option :value="1000000">1M</option>
          <option :value="2000000">2M</option>
          <option :value="4000000">4M</option>
        </select>
        <button class="btn primary" :disabled="!selectedPort || dev.connecting" @click="onConnect">
          {{ dev.connecting ? '连接中…' : '连接' }}
        </button>
      </div>
      <p v-else-if="isWeb" class="hint">
        尚未授权 YSC 设备。点「选择设备」，在系统弹窗中选 YSC 串口（VID 1A86 / PID FE0C）。
      </p>
      <p v-else class="hint">未发现设备，请插入 YSC 设备后点「刷新」。</p>
    </template>

    <!-- 已连接：设备信息 -->
    <template v-else>
      <div class="info-grid">
        <div><span class="k">端口</span><span class="v mono">{{ dev.port }}</span></div>
        <div><span class="k">波特率</span><span class="v mono">{{ dev.baud }}</span></div>
        <div><span class="k">固件版本</span>
          <span class="v mono">{{ dev.version ? dev.version.raw : (dev.versionRaw || '—') }}</span>
        </div>
        <div v-if="dev.version?.side"><span class="k">侧别</span>
          <span class="v" :class="`side-${dev.version.side.toLowerCase()}`">{{ dev.version.side }}</span>
        </div>
      </div>
      <div class="row">
        <button class="btn" @click="dev.fetchVersion()">读取版本</button>
        <button class="btn danger" @click="dev.disconnect()">断开</button>
      </div>
    </template>

    <p v-if="dev.lastError" class="err">{{ dev.lastError }}</p>
  </section>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';
import { useDeviceStore } from '../store/device-store';
import { getPlatform } from '../platform';

const dev = useDeviceStore();
const isWeb = getPlatform() === 'web';

const selectedPort = ref('');
const selectedBaud = ref(115200);

watch(
  () => dev.ports,
  (list) => {
    if (list.length && !selectedPort.value) selectedPort.value = list[0].id;
  },
  { immediate: true },
);

async function onAddDevice(): Promise<void> {
  const info = await dev.requestPort();
  if (info && !selectedPort.value) selectedPort.value = info.id;
}

async function onConnect(): Promise<void> {
  if (!selectedPort.value) return;
  try {
    await dev.connect(selectedPort.value, selectedBaud.value);
  } catch {
    /* 错误已在 store.lastError */
  }
}
</script>

<style scoped>
.device-card {
  max-width: 560px;
}
.dc-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 4px;
}
.dc-head h2 {
  font-size: var(--text-md);
  font-weight: var(--weight-bold);
  line-height: var(--leading-tight);
}
.state-pill {
  font-size: 11px;
  padding: 3px 9px;
  border-radius: 999px;
}
.state-pill.on {
  background: var(--pill-on-bg);
  color: var(--pill-on-fg);
}
.state-pill.off {
  background: var(--pill-off-bg);
  color: var(--pill-off-fg);
}
.row {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  align-items: center;
}
.btn {
  background: var(--btn-bg);
  color: var(--btn-text);
  border: 1px solid var(--border);
  border-radius: var(--btn-radius);
  padding: 7px 14px;
  font-size: 13px;
  font-weight: 600;
  transition: var(--transition-fast);
}
.btn:hover:not(:disabled) {
  background: var(--btn-bg-hover);
  border-color: var(--border-strong);
  color: var(--text-primary);
}
.btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
.btn.primary {
  background: var(--btn-primary-bg);
  color: var(--btn-primary-text);
  border-color: var(--btn-primary-bg);
}
.btn.primary:hover:not(:disabled) {
  background: #6ee7a0;
  border-color: #6ee7a0;
  color: var(--btn-primary-text);
}
.btn.danger {
  background: transparent;
  color: var(--accent-red);
  border-color: rgba(248, 113, 113, 0.4);
}
.btn.danger:hover:not(:disabled) {
  background: rgba(248, 113, 113, 0.12);
  border-color: var(--accent-red);
}
.btn:focus-visible {
  outline: none;
  box-shadow: var(--ring-focus);
}
.select {
  background-color: var(--bg-input);
  color: var(--text-primary);
  border: 1px solid var(--border);
  border-radius: var(--btn-radius);
  padding: 7px 30px 7px 10px;
  font-size: var(--text-sm);
  font-family: var(--font-ui);
  font-weight: var(--weight-semibold);
  min-width: 130px;
  transition: var(--transition-fast);
  appearance: none;
  -webkit-appearance: none;
  -moz-appearance: none;
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='14' height='14' viewBox='0 0 24 24' fill='none' stroke='%23a6adb5' stroke-width='2.5' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpolyline points='6 9 12 15 18 9'/%3E%3C/svg%3E");
  background-repeat: no-repeat;
  background-position: right 8px center;
  cursor: pointer;
}
.select:hover {
  border-color: var(--border-strong);
}
.select:focus {
  outline: none;
  border-color: var(--border-focus);
  box-shadow: var(--ring-focus);
}
.select.baud {
  min-width: 90px;
}
.hint {
  font-size: 12px;
  color: var(--text-muted);
  line-height: 1.6;
}
.err {
  font-size: 12px;
  color: var(--accent-red);
}
.info-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
  background: var(--bg-input);
  border-radius: var(--panel-radius);
  padding: 12px;
}
.info-grid > div {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.k {
  font-size: 11px;
  color: var(--text-muted);
}
.v {
  font-size: 14px;
  font-weight: 500;
}
.v.mono {
  font-family: var(--font-mono);
}
.side-left {
  color: var(--devicecard-side-left);
}
.side-right {
  color: var(--devicecard-side-right);
}
.side-iap {
  color: var(--devicecard-side-iap);
}
</style>
