<template>
  <AppShell :banner="banner">
    <div class="view-grid">
      <!-- 我的设备 -->
      <template v-if="ui.activeView === 'device-card'">
        <DeviceCard />
        <section class="card help-card">
          <h2 class="card-title">快速入门</h2>
          <ol class="steps">
            <li>用 <b>Chrome / Edge</b> 打开本页(localhost 或 HTTPS)。</li>
            <li>点「选择设备」,在系统弹窗选 YSC 串口(<span class="mono">VID 1A86 / PID FE0C</span>)。</li>
            <li>选波特率,点「连接」—— 版本/监控/键盘等随即可用。</li>
            <li>左侧导航切换各功能面板。</li>
          </ol>
          <p class="hint">
            <span class="material-symbols-outlined ph-inline">info</span>
            Firefox / Safari 不支持 Web Serial,请用 Chromium 内核浏览器。授权过的端口下次自动记住。
          </p>
        </section>
      </template>

      <!-- 实时监控 -->
      <template v-else-if="ui.activeView === 'monitor'">
        <section v-if="dev.connected" class="card">
          <h2 class="card-title">实时监控</h2>
          <Toggle
            :model-value="dev.monitorOn"
            label="实时上报"
            @update:model-value="onToggleMonitor"
          />
          <div class="monitor-grid">
            <div><span class="k">按键</span><span class="v mono">{{ formatHex(dev.monitor.buttons, 4) }}</span></div>
            <div><span class="k">X</span><span class="v mono">{{ dev.monitor.x }}</span></div>
            <div><span class="k">Y</span><span class="v mono">{{ dev.monitor.y }}</span></div>
            <div><span class="k">帧数</span><span class="v mono">{{ dev.monitorCount }}</span></div>
          </div>
        </section>
        <div v-else class="placeholder"><div class="ph-icon">📡</div><div>请先在「我的设备」连接设备</div></div>
      </template>

      <!-- 键盘测试 -->
      <template v-else-if="ui.activeView === 'keyboard-tester'">
        <section v-if="dev.connected" class="card">
          <h2 class="card-title">键盘测试</h2>
          <p class="hint">按住下方按键向设备注入 HID 按键（cmd 45），松开自动释放。</p>
          <div class="kbd">
            <button
              v-for="k in testKeys"
              :key="k.kc"
              class="key"
              @pointerdown.prevent="pressKey(k.kc, true)"
              @pointerup="pressKey(k.kc, false)"
              @pointerleave="pressKey(k.kc, false)"
              @pointercancel="pressKey(k.kc, false)"
            >{{ k.label }}</button>
          </div>
          <button class="btn" @click="releaseAll">释放全部</button>
        </section>
        <div v-else class="placeholder"><div class="ph-icon">⌨️</div><div>请先在「我的设备」连接设备</div></div>
      </template>

      <!-- 其余视图：占位 -->
      <template v-else>
        <div class="placeholder">
          <div class="ph-icon">🚧</div>
          <div>{{ placeholderLabel }}</div>
        </div>
      </template>
    </div>
  </AppShell>
</template>

<script setup lang="ts">
import { computed, onMounted, watch } from 'vue';
import { AppShell, DeviceCard, Toggle, ALL_VIEW_KEYS } from '@ysc/core/ui';
import { useDeviceStore, useUiStore } from '@ysc/core/store';
import { isWebSerialSupported } from '@ysc/core/platform';

const dev = useDeviceStore();
const ui = useUiStore();
const webSerialOk = isWebSerialSupported();

const banner = webSerialOk
  ? ''
  : '当前浏览器不支持 Web Serial API。请用 Chrome / Edge 桌面端，并通过 HTTPS 或 localhost 打开。';

const testKeys = [
  { label: 'W', kc: 0x1a },
  { label: 'A', kc: 0x04 },
  { label: 'S', kc: 0x16 },
  { label: 'D', kc: 0x07 },
  { label: 'Space', kc: 0x2c },
  { label: 'Enter', kc: 0x28 },
];

onMounted(() => {
  ui.loadView();
  if (!ALL_VIEW_KEYS.includes(ui.activeView)) ui.setView('device-card');
});

watch(
  () => dev.connected,
  (on) => {
    if (on) void dev.fetchVersion();
  },
);

const placeholderLabel = computed(() => {
  switch (ui.activeView) {
    case 'kmnet':
    case 'ch343':
      return '该功能为桌面版专属（浏览器沙箱限制）';
    case 'macro':
    case 'jitter':
    case 'mouse-curve':
    case 'gamepad':
    case 'command-tester':
    case 'docs':
    case 'firmware-v1':
    case 'firmware-v2':
    case 'debug':
      return '该面板正在迁移到共享核心（Phase 4/5/6）';
    default:
      return '敬请期待';
  }
});

function formatHex(n: number, width = 4): string {
  return n.toString(16).toUpperCase().padStart(width, '0');
}
async function onToggleMonitor(on: boolean): Promise<void> {
  await dev.toggleMonitor(on);
}
function pressKey(kc: number, down: boolean): void {
  void dev.device?.keyboardKey(kc, down);
}
function releaseAll(): void {
  void dev.device?.keyboardReleaseAll();
}
</script>

<style scoped>
.view-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
  gap: 16px;
  align-content: start;
}
.card-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-secondary);
}
.hint {
  font-size: 12px;
  color: var(--text-muted);
  line-height: 1.5;
}
.monitor-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 8px;
}
.monitor-grid > div {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  background: var(--bg-input);
  border: 1px solid var(--border-soft);
  border-radius: var(--btn-radius);
  padding: 10px 12px;
}
.monitor-grid .k {
  font-size: var(--text-xs);
  font-weight: var(--weight-bold);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}
.monitor-grid .v {
  font-size: var(--text-md);
}
.kbd {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
.key {
  min-width: 56px;
  height: 52px;
  background: var(--bg-tertiary);
  color: var(--text-primary);
  border: 1px solid var(--border);
  border-bottom: 3px solid var(--bg-deepest);
  border-radius: var(--panel-radius);
  font-family: var(--font-ui);
  font-size: var(--text-sm);
  font-weight: var(--weight-bold);
  letter-spacing: 0.5px;
  user-select: none;
  touch-action: none;
  transition: var(--transition-fast);
}
.key:hover {
  background: var(--bg-hover);
  border-color: var(--border-strong);
}
.key:active {
  background: var(--accent-selected);
  color: var(--bg-deepest);
  border-color: var(--accent-selected);
  border-bottom-width: 1px;
  transform: translateY(2px);
}
.key:focus-visible {
  outline: none;
  box-shadow: var(--ring-focus);
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
.k {
  color: var(--text-muted);
}
.v {
  color: var(--text-primary);
  font-weight: 500;
}
.v.mono {
  font-family: var(--font-mono);
}
.help-card {
  max-width: 560px;
}
.steps {
  list-style: none;
  counter-reset: step;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.steps li {
  counter-increment: step;
  position: relative;
  padding-left: 30px;
  font-size: var(--text-sm);
  color: var(--text-secondary);
  line-height: 1.5;
}
.steps li::before {
  content: counter(step);
  position: absolute;
  left: 0;
  top: -1px;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--bg-selected-soft);
  color: var(--accent-selected);
  font-size: 11px;
  font-weight: var(--weight-bold);
  display: flex;
  align-items: center;
  justify-content: center;
}
.steps b {
  color: var(--text-primary);
  font-weight: var(--weight-bold);
}
.mono {
  font-family: var(--font-mono);
  font-size: 12px;
  background: var(--bg-input);
  padding: 1px 5px;
  border-radius: var(--radius-sm);
}
.ph-inline {
  font-size: 16px;
  vertical-align: -3px;
  margin-right: 4px;
  color: var(--accent-amber);
}
</style>
