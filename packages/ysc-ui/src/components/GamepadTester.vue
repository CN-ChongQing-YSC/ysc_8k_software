<template>
  <div class="gp-overlay" :class="{ standalone }" @click.self="requestClose">
    <div class="gp-dialog" :class="{ standalone }" role="dialog" :aria-modal="!standalone">
      <header
        class="gp-head"
        :class="{ drag: standalone }"
        :data-tauri-drag-region="standalone ? '' : null"
      >
        <div class="gp-title" :data-tauri-drag-region="standalone ? '' : null">
          <span class="gp-live" :class="{ on: !!pad }"></span>
          {{ t('gamepad.testerTitle') }}
        </div>
        <span class="gp-head-hint" :data-tauri-drag-region="standalone ? '' : null">{{ t('gamepad.testerHint') }}</span>
        <button class="gp-close" :title="t('gamepad.close')" @click="requestClose">✕</button>
      </header>

      <div class="gp-slots">
        <button
          v-for="(p, i) in pads"
          :key="i"
          class="gp-slot"
          :class="{ active: i === selected, on: !!p }"
          :title="p ? p.id : t('gamepad.noneDetected')"
          @click="selected = i"
        >
          <span class="gp-slot-n">{{ i + 1 }}:</span>
          <span class="gp-slot-name">{{ p ? shortName(p.id) : t('gamepad.noneDetected') }}</span>
          <span class="gp-slot-state">{{ p ? t('gamepad.connected') : '—' }}</span>
        </button>
      </div>

      <div v-if="pad" class="gp-main">
        <section class="gp-left">
          <h2 class="gp-name">{{ padName }}</h2>
          <div class="gp-subname">{{ mappingLabel }}</div>

          <div class="gp-info">
            <div><span>{{ t('gamepad.index') }}</span><b>{{ pad.index }}</b></div>
            <div><span>{{ t('gamepad.connected') }}</span><b>{{ pad.connected ? t('gamepad.yes') : t('gamepad.no') }}</b></div>
            <div><span>{{ t('gamepad.mapping') }}</span><b>{{ mappingRaw }}</b></div>
            <div><span>{{ t('gamepad.timestamp') }}</span><b>{{ pad.timestamp ? pad.timestamp.toFixed(2) : '—' }}</b></div>
            <div><span>{{ t('gamepad.vibration') }}</span><b>{{ hasVibration ? t('gamepad.yes') : t('gamepad.no') }}</b></div>
          </div>

          <div class="gp-section-title">{{ t('gamepad.buttons') }}</div>
          <div class="gp-buttons">
            <div
              v-for="(b, i) in buttonList"
              :key="i"
              class="gp-btn"
              :class="{ pressed: b.pressed, analog: b.value > 0 && !b.pressed }"
            >
              <span class="gp-btn-name">B{{ i }}</span>
              <span class="gp-btn-val">{{ b.value.toFixed(2) }}</span>
            </div>
          </div>

          <div class="gp-axes">
            <div class="gp-stick-box">
              <div class="gp-section-title">{{ t('gamepad.leftStick') }}</div>
              <div class="gp-axis-vals">
                <div><span>AXIS 0</span><b>{{ axes[0].toFixed(5) }}</b></div>
                <div><span>AXIS 1</span><b>{{ axes[1].toFixed(5) }}</b></div>
              </div>
              <svg class="gp-stick-svg" viewBox="-110 -110 220 220" aria-hidden="true">
                <circle cx="0" cy="0" r="100" class="ring" />
                <line x1="-100" y1="0" x2="100" y2="0" class="cross" />
                <line x1="0" y1="-100" x2="0" y2="100" class="cross" />
                <polyline v-if="traceL.length > 1" :points="tracePoints(traceL)" class="trace" />
                <circle :cx="axes[0] * 100" :cy="axes[1] * 100" r="7" class="dot" :class="{ on: stickActive(0, 1) }" />
              </svg>
            </div>
            <div class="gp-stick-box">
              <div class="gp-section-title">{{ t('gamepad.rightStick') }}</div>
              <div class="gp-axis-vals">
                <div><span>AXIS 2</span><b>{{ axes[2].toFixed(5) }}</b></div>
                <div><span>AXIS 3</span><b>{{ axes[3].toFixed(5) }}</b></div>
              </div>
              <svg class="gp-stick-svg" viewBox="-110 -110 220 220" aria-hidden="true">
                <circle cx="0" cy="0" r="100" class="ring" />
                <line x1="-100" y1="0" x2="100" y2="0" class="cross" />
                <line x1="0" y1="-100" x2="0" y2="100" class="cross" />
                <polyline v-if="traceR.length > 1" :points="tracePoints(traceR)" class="trace" />
                <circle :cx="axes[2] * 100" :cy="axes[3] * 100" r="7" class="dot" :class="{ on: stickActive(2, 3) }" />
              </svg>
            </div>
          </div>

          <div class="gp-actions">
            <button class="gp-action" :class="{ active: circularity }" @click="toggleCircularity">
              ◎ {{ t('gamepad.testCircularity') }}
            </button>
            <button v-if="circularity" class="gp-action" @click="clearTraces">{{ t('gamepad.clearTrace') }}</button>
            <button class="gp-action" :disabled="!hasVibration" @click="vibrate(1000)">📳 {{ t('gamepad.vibration1s') }}</button>
            <button class="gp-action" :class="{ active: vibrating }" :disabled="!hasVibration" @click="toggleInfiniteVibration">
              {{ vibrating ? t('gamepad.stopVibration') : t('gamepad.vibrationInfinite') }}
            </button>
          </div>
          <div v-if="!hasVibration" class="gp-note">{{ t('gamepad.noVibration') }}</div>
        </section>

        <section class="gp-right">
          <svg class="gp-pad" viewBox="0 0 420 300" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <!-- body -->
            <path class="body" d="M120 70 h180 c40 0 70 24 80 64 l15 70 c8 40 -10 70 -40 70 c-20 0 -35 -15 -55 -45 l-30 -45 c-10 -15 -25 -20 -40 -20 s-30 5 -40 20 l-30 45 c-20 30 -35 45 -55 45 c-30 0 -48 -30 -40 -70 l15 -70 c10 -40 40 -64 80 -64 z" />
            <!-- triggers / shoulders -->
            <rect class="trigger" x="104" y="20" width="62" height="15" rx="7" :style="{ opacity: 0.22 + 0.78 * trigger(6) }" />
            <rect class="trigger" x="254" y="20" width="62" height="15" rx="7" :style="{ opacity: 0.22 + 0.78 * trigger(7) }" />
            <text x="135" y="32" class="pad-label">LT</text>
            <text x="285" y="32" class="pad-label">RT</text>
            <rect class="shoulder" x="112" y="43" width="66" height="16" rx="8" :class="{ on: pressed(4) }" />
            <rect class="shoulder" x="242" y="43" width="66" height="16" rx="8" :class="{ on: pressed(5) }" />
            <text x="145" y="56" class="pad-label">LB</text>
            <text x="275" y="56" class="pad-label">RB</text>

            <!-- left stick -->
            <circle class="stick-base" cx="140" cy="140" r="34" :class="{ on: pressed(10) }" />
            <circle class="stick-top" :cx="140 + axes[0] * 20" :cy="140 + axes[1] * 20" r="16" />

            <!-- right stick -->
            <circle class="stick-base" cx="280" cy="200" r="34" :class="{ on: pressed(11) }" />
            <circle class="stick-top" :cx="280 + axes[2] * 20" :cy="200 + axes[3] * 20" r="16" />

            <!-- d-pad -->
            <g class="dpad">
              <rect class="dpad-arm" x="104" y="172" width="18" height="56" rx="5" :class="{ on: pressed(12) || pressed(13) }" />
              <rect class="dpad-arm" x="85" y="191" width="56" height="18" rx="5" :class="{ on: pressed(14) || pressed(15) }" />
              <path class="dpad-dir" d="M113 172 l-8 9 h16 z" :class="{ on: pressed(12) }" />
              <path class="dpad-dir" d="M113 228 l-8 -9 h16 z" :class="{ on: pressed(13) }" />
              <path class="dpad-dir" d="M85 200 l9 -8 v16 z" :class="{ on: pressed(14) }" />
              <path class="dpad-dir" d="M141 200 l-9 -8 v16 z" :class="{ on: pressed(15) }" />
            </g>

            <!-- ABXY -->
            <g class="abxy">
              <circle class="face" cx="310" cy="100" r="15" :class="{ on: pressed(3) }" />
              <text x="306" y="105" class="face-label">Y</text>
              <circle class="face" cx="290" cy="120" r="15" :class="{ on: pressed(2) }" />
              <text x="286" y="125" class="face-label">X</text>
              <circle class="face" cx="330" cy="120" r="15" :class="{ on: pressed(1) }" />
              <text x="326" y="125" class="face-label">B</text>
              <circle class="face" cx="310" cy="140" r="15" :class="{ on: pressed(0) }" />
              <text x="306" y="145" class="face-label">A</text>
            </g>

            <!-- center buttons -->
            <circle class="mini" cx="196" cy="112" r="7" :class="{ on: pressed(8) }" />
            <circle class="mini" cx="224" cy="112" r="7" :class="{ on: pressed(9) }" />
            <circle class="guide" cx="210" cy="88" r="12" :class="{ on: pressed(16) }" />
            <text x="210" y="92" class="guide-label">✕</text>
          </svg>
        </section>
      </div>

      <div v-else class="gp-empty">
        <div class="gp-empty-icon">🎮</div>
        <div class="gp-empty-title">{{ t('gamepad.noneDetected') }}</div>
        <div class="gp-note">{{ t('gamepad.testerNoPad') }}</div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { useI18n } from '../i18n/index.js';

const { t } = useI18n();
const emit = defineEmits(['close']);
/* standalone=true：独立 Tauri 窗口（tester.html）铺满窗口，X 关闭窗口本身；
 * 否则作为主窗口内的右侧抽屉使用。 */
const props = defineProps({
  standalone: { type: Boolean, default: false },
});
const api = window.driverApi;
/* Tauri：WebView2 没有 Gamepad API，优先走 Rust 的 XInput 事件；
 * 浏览器/网页版仍回退到 navigator.getGamepads()。 */
const driverMode = !!(api && typeof api.on === 'function' && typeof api.send === 'function');

const MAX_PADS = 4;
const pads = ref(new Array(MAX_PADS).fill(null));
const selected = ref(0);
const circularity = ref(false);
const traceL = ref([]);
const traceR = ref([]);
const vibrating = ref(false);

let raf = 0;
let vibTimer = null;
let lastTrace = 0;

const pad = computed(() => pads.value[selected.value] || null);

const axes = computed(() => {
  const a = (pad.value && pad.value.axes) || [];
  return [Number(a[0]) || 0, Number(a[1]) || 0, Number(a[2]) || 0, Number(a[3]) || 0];
});

const buttonList = computed(() => {
  const out = [];
  for (let i = 0; i < 17; i++) {
    const b = pad.value && pad.value.buttons ? pad.value.buttons[i] : null;
    out.push({ pressed: !!(b && b.pressed), value: b ? Number(b.value) || 0 : 0 });
  }
  return out;
});

const padName = computed(() => {
  const id = (pad.value && pad.value.id) || '';
  return id.split('(')[0].trim() || t('gamepad.unknownMapping');
});

const mappingRaw = computed(() => (pad.value && pad.value.mapping) || '—');
const mappingLabel = computed(() => {
  const m = pad.value && pad.value.mapping;
  if (!m) return t('gamepad.unknownMapping');
  return m === 'standard' ? 'XInput STANDARD GAMEPAD' : m;
});

const hasVibration = computed(() => {
  const p = pad.value;
  return !!(p && (p.vibrationActuator || (p.hapticActuators && p.hapticActuators.length)));
});

function shortName(id) {
  const s = String(id || '').split('(')[0].trim();
  return s || t('gamepad.unknownMapping');
}
function pressed(i) {
  return !!(buttonList.value[i] && buttonList.value[i].pressed);
}
function buttonValue(i) {
  return buttonList.value[i] ? buttonList.value[i].value : 0;
}
function trigger(i) {
  return Math.max(0, Math.min(1, buttonValue(i)));
}
function stickActive(xi, yi) {
  return Math.abs(axes.value[xi]) > 0.05 || Math.abs(axes.value[yi]) > 0.05;
}
function tracePoints(points) {
  return points.map(([x, y]) => `${(x * 100).toFixed(1)},${(y * 100).toFixed(1)}`).join(' ');
}

function snapshot(gp) {
  if (!gp) return null;
  return {
    index: gp.index,
    id: gp.id,
    connected: gp.connected,
    mapping: gp.mapping,
    timestamp: gp.timestamp,
    axes: Array.from(gp.axes || []),
    buttons: Array.from(gp.buttons || []).map((b) => ({
      pressed: !!b.pressed,
      touched: !!b.touched,
      value: Number(b.value) || 0,
    })),
    vibrationActuator: gp.vibrationActuator || null,
    hapticActuators: gp.hapticActuators ? Array.from(gp.hapticActuators) : [],
  };
}

function poll(ts) {
  const list = (navigator.getGamepads && navigator.getGamepads()) || [];
  const next = [];
  for (let i = 0; i < MAX_PADS; i++) next.push(snapshot(list[i]));
  pads.value = next;

  if (!next[selected.value]) {
    const first = next.findIndex((p) => !!p);
    if (first >= 0) selected.value = first;
  }

  pushTrace(ts);
  raf = requestAnimationFrame(poll);
}

function pushTrace(ts) {
  if (!circularity.value || !pad.value || ts - lastTrace < 16) return;
  lastTrace = ts;
  const a = axes.value;
  traceL.value.push([a[0], a[1]]);
  traceR.value.push([a[2], a[3]]);
  if (traceL.value.length > 800) traceL.value.shift();
  if (traceR.value.length > 800) traceR.value.shift();
}

/* Rust XInput 事件：body = {type:"gamepad_state", pads:[...]} */
function onGamepadState(data) {
  if (!data || !Array.isArray(data.pads)) return;
  const list = data.pads.slice(0, MAX_PADS);
  while (list.length < MAX_PADS) list.push(null);
  pads.value = list;
  if (!list[selected.value]) {
    const first = list.findIndex((p) => !!p);
    if (first >= 0) selected.value = first;
  }
  pushTrace(performance.now());
}

function toggleCircularity() {
  circularity.value = !circularity.value;
  if (circularity.value) clearTraces();
}
function clearTraces() {
  traceL.value = [];
  traceR.value = [];
}

function clearVibTimer() {
  if (vibTimer) {
    clearTimeout(vibTimer);
    clearInterval(vibTimer);
    vibTimer = null;
  }
}

async function vibrate(ms) {
  const p = pad.value;
  if (!p) return;
  if (driverMode) {
    clearVibTimer();
    api.send('gamepad_vibrate', { index: p.index, left: 65535, right: 65535 });
    if (ms > 0) {
      vibTimer = setTimeout(() => {
        api.send('gamepad_vibrate', { index: p.index, left: 0, right: 0 });
        vibTimer = null;
      }, ms);
    }
    return;
  }
  const a = p.vibrationActuator;
  if (!a) return;
  try {
    if (typeof a.playEffect === 'function') {
      await a.playEffect('dual-rumble', {
        startDelay: 0,
        duration: ms,
        strongMagnitude: 1.0,
        weakMagnitude: 1.0,
      });
    } else if (typeof a.pulse === 'function') {
      await a.pulse(1.0, ms);
    }
  } catch (e) {
    /* WebView/browser may reject haptics; keep the tester usable. */
  }
}

function toggleInfiniteVibration() {
  if (vibrating.value) {
    stopVibration();
    return;
  }
  vibrating.value = true;
  if (driverMode) {
    clearVibTimer();
    const p = pad.value;
    if (p) api.send('gamepad_vibrate', { index: p.index, left: 65535, right: 65535 });
    return;
  }
  vibrate(60000);
  vibTimer = setInterval(() => {
    if (vibrating.value) vibrate(60000);
  }, 55000);
}

function stopVibration() {
  vibrating.value = false;
  clearVibTimer();
  if (driverMode) {
    const p = pad.value;
    if (p) api.send('gamepad_vibrate', { index: p.index, left: 0, right: 0 });
    return;
  }
  const a = pad.value && pad.value.vibrationActuator;
  try {
    if (a && typeof a.reset === 'function') a.reset();
    else if (a && typeof a.playEffect === 'function') {
      a.playEffect('dual-rumble', {
        startDelay: 0,
        duration: 0,
        strongMagnitude: 0,
        weakMagnitude: 0,
      });
    }
  } catch (e) {
    /* ignore */
  }
}

function requestClose() {
  if (props.standalone) {
    if (api && typeof api.closeGamepadTester === 'function') {
      api.closeGamepadTester();
      return;
    }
    window.close();
    return;
  }
  emit('close');
}

function onKeydown(e) {
  if (e.key === 'Escape') requestClose();
}

onMounted(() => {
  window.addEventListener('keydown', onKeydown);
  if (driverMode) {
    api.on('gamepad_state', onGamepadState);
    api.send('gamepad_watch', { on: true });
  } else {
    raf = requestAnimationFrame(poll);
  }
});

onUnmounted(() => {
  window.removeEventListener('keydown', onKeydown);
  if (driverMode) {
    api.send('gamepad_watch', { on: false });
    if (typeof api.off === 'function') api.off('gamepad_state', onGamepadState);
  } else if (raf) {
    cancelAnimationFrame(raf);
  }
  stopVibration();
});
</script>

<style scoped>
.gp-overlay {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  align-items: stretch;
  justify-content: flex-end;
  /* 顶部让出 36px 原生标题栏（最小化/最大化/关闭）+ 8px 间距 */
  padding: 44px 12px 12px;
  pointer-events: none;
}
/* 右侧竖向抽屉：在标题栏下方，左侧应用保持可见/可操作。 */
.gp-dialog {
  width: min(560px, 94vw);
  height: calc(100vh - 56px);
  overflow: auto;
  pointer-events: auto;
  border: 1px solid var(--border, #2a3140);
  border-left: 3px solid #50c878;
  border-radius: 12px;
  background: var(--bg-panel, #141922);
  color: var(--text-primary, #e6e9ef);
  box-shadow: -18px 0 60px rgba(0, 0, 0, 0.5);
}
/* 独立窗口模式：铺满整个窗口，顶部留一条品牌色，标题栏可拖动。 */
.gp-overlay.standalone {
  position: static;
  inset: auto;
  padding: 0;
  pointer-events: auto;
}
.gp-dialog.standalone {
  width: 100%;
  height: 100vh;
  border: 0;
  border-top: 3px solid #50c878;
  border-radius: 0;
  box-shadow: none;
}
.gp-head.drag {
  -webkit-app-region: drag;
  user-select: none;
}
.gp-head.drag .gp-close,
.gp-head.drag button {
  -webkit-app-region: no-drag;
}
.gp-head {
  position: sticky;
  top: 0;
  z-index: 2;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 14px 18px;
  border-bottom: 1px solid var(--border, #2a3140);
  background: var(--bg-panel, #141922);
}
.gp-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 15px;
  font-weight: 700;
  white-space: nowrap;
}
.gp-live {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #677086;
}
.gp-live.on {
  background: #50c878;
  box-shadow: 0 0 8px rgba(80, 200, 120, 0.8);
}
.gp-head-hint {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  color: var(--text-muted, #8891a4);
}
.gp-close {
  width: 30px;
  height: 30px;
  border: 1px solid var(--border, #2a3140);
  border-radius: 8px;
  background: transparent;
  color: inherit;
  cursor: pointer;
}
.gp-close:hover {
  border-color: #50c878;
  color: #50c878;
}
.gp-slots {
  display: flex;
  gap: 8px;
  padding: 12px 18px;
  overflow-x: auto;
  border-bottom: 1px solid var(--border, #2a3140);
}
.gp-slot {
  flex: 0 0 auto;
  min-width: 128px;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
  padding: 7px 10px;
  border: 1px solid var(--border, #2a3140);
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.02);
  color: inherit;
  cursor: pointer;
  text-align: left;
}
.gp-slot.active {
  border-color: #50c878;
  background: rgba(80, 200, 120, 0.08);
}
.gp-slot.on .gp-slot-state {
  color: #50c878;
}
.gp-slot-n {
  font-size: 10px;
  color: var(--text-muted, #8891a4);
}
.gp-slot-name {
  max-width: 150px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  font-weight: 600;
}
.gp-slot-state {
  font-size: 10px;
  color: var(--text-muted, #8891a4);
}
.gp-main {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 14px 16px 18px;
}
.gp-name {
  margin: 0;
  font-size: 22px;
  line-height: 1.15;
  letter-spacing: -0.02em;
}
.gp-subname {
  margin-top: 4px;
  color: var(--text-muted, #8891a4);
  font-size: 13px;
  letter-spacing: 0.04em;
}
.gp-info {
  display: flex;
  flex-wrap: wrap;
  gap: 10px 24px;
  margin: 16px 0 18px;
  padding-bottom: 14px;
  border-bottom: 1px solid var(--border, #2a3140);
}
.gp-info > div {
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.gp-info span {
  font-size: 10px;
  color: var(--text-muted, #8891a4);
  text-transform: uppercase;
  letter-spacing: 0.08em;
}
.gp-info b {
  font-size: 15px;
  font-weight: 600;
}
.gp-section-title {
  margin-bottom: 8px;
  font-size: 10px;
  font-weight: 700;
  color: var(--text-muted, #8891a4);
  text-transform: uppercase;
  letter-spacing: 0.1em;
}
.gp-buttons {
  display: grid;
  grid-template-columns: repeat(6, minmax(0, 1fr));
  gap: 4px;
}
.gp-btn {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 5px 6px;
  border: 1px solid var(--border, #2a3140);
  border-radius: 5px;
  background: rgba(255, 255, 255, 0.02);
}
.gp-btn.pressed {
  border-color: #50c878;
  background: rgba(80, 200, 120, 0.18);
}
.gp-btn.analog {
  border-color: #d9a441;
  background: rgba(217, 164, 65, 0.12);
}
.gp-btn-name {
  font-size: 9px;
  color: var(--text-muted, #8891a4);
}
.gp-btn-val {
  font-family: 'Consolas', 'Menlo', monospace;
  font-size: 12px;
  font-weight: 600;
}
.gp-axes {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 14px;
  margin-top: 18px;
}
.gp-stick-box {
  min-width: 0;
}
.gp-axis-vals {
  display: flex;
  gap: 16px;
  margin-bottom: 6px;
}
.gp-axis-vals > div {
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.gp-axis-vals span {
  font-size: 9px;
  color: var(--text-muted, #8891a4);
}
.gp-axis-vals b {
  font-family: 'Consolas', 'Menlo', monospace;
  font-size: 12px;
  font-weight: 600;
}
.gp-stick-svg {
  width: 100%;
  max-width: 210px;
  display: block;
  color: var(--text-muted, #8891a4);
}
.gp-stick-svg .ring,
.gp-stick-svg .cross {
  fill: none;
  stroke: var(--border, #2a3140);
  stroke-width: 2;
}
.gp-stick-svg .cross {
  stroke-width: 1;
  opacity: 0.55;
}
.gp-stick-svg .dot {
  fill: #677086;
  transition: fill 0.08s linear;
}
.gp-stick-svg .dot.on {
  fill: #50c878;
}
.gp-stick-svg .trace {
  fill: none;
  stroke: #50c878;
  stroke-width: 1.4;
  opacity: 0.7;
}
.gp-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 16px;
}
.gp-action {
  padding: 7px 11px;
  border: 1px solid var(--border, #2a3140);
  border-radius: 7px;
  background: rgba(255, 255, 255, 0.03);
  color: inherit;
  font-size: 12px;
  cursor: pointer;
}
.gp-action:hover:not(:disabled) {
  border-color: #50c878;
  color: #50c878;
}
.gp-action.active {
  border-color: #50c878;
  background: rgba(80, 200, 120, 0.16);
  color: #50c878;
}
.gp-action:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}
.gp-note {
  margin-top: 8px;
  font-size: 11px;
  color: var(--text-muted, #8891a4);
}
.gp-right {
  display: flex;
  order: -1;                 /* 抽屉顶部先显示手柄状态图 */
  align-items: center;
  justify-content: center;
  min-width: 0;
}
.gp-pad {
  width: 100%;
  max-width: 360px;
  color: var(--text-muted, #8891a4);
  overflow: visible;
}
.gp-pad .body {
  fill: rgba(255, 255, 255, 0.02);
  stroke: var(--border, #2a3140);
  stroke-width: 2.4;
}
.gp-pad .trigger {
  fill: #677086;
  stroke: var(--border, #2a3140);
}
.gp-pad .shoulder,
.gp-pad .stick-base,
.gp-pad .dpad-arm,
.gp-pad .face,
.gp-pad .mini,
.gp-pad .guide {
  fill: rgba(255, 255, 255, 0.03);
  stroke: var(--border, #2a3140);
  stroke-width: 2;
  transition: fill 0.08s linear, stroke 0.08s linear;
}
.gp-pad .stick-top {
  fill: #677086;
  stroke: none;
}
.gp-pad .dpad-dir {
  fill: #677086;
  stroke: none;
}
.gp-pad .on {
  fill: rgba(80, 200, 120, 0.3);
  stroke: #50c878;
}
.gp-pad .dpad-dir.on {
  fill: #50c878;
}
.gp-pad .pad-label,
.gp-pad .face-label,
.gp-pad .guide-label {
  fill: var(--text-muted, #8891a4);
  stroke: none;
  font-size: 10px;
  font-family: 'Consolas', 'Menlo', monospace;
  text-anchor: middle;
}
.gp-pad .face-label {
  font-size: 11px;
  font-weight: 700;
}
.gp-pad .guide-label {
  font-size: 12px;
}
.gp-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 70px 24px;
  text-align: center;
}
.gp-empty-icon {
  font-size: 52px;
  opacity: 0.55;
}
.gp-empty-title {
  font-size: 18px;
  font-weight: 700;
}
@media (max-width: 700px) {
  .gp-overlay {
    padding: 40px 8px 8px;
  }
  .gp-dialog {
    width: 96vw;
    height: calc(100vh - 48px);
  }
  .gp-buttons {
    grid-template-columns: repeat(4, minmax(0, 1fr));
  }
  .gp-pad {
    max-width: 300px;
  }
}
</style>
