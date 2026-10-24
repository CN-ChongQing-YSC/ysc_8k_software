<template>
  <div class="panel macro-page pg-page">
    <div class="panel-header">
      <svg class="panel-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <path d="M8 1.8 L13 3.6 V7.2 C13 10.4 10.9 12.7 8 14.2 C5.1 12.7 3 10.4 3 7.2 V3.6 Z" />
        <path d="M5.6 7.7 L7.2 9.3 L10.6 5.9" />
      </svg>
      <span>{{ t('physicalGuard.title') }}</span>
      <div class="macro-header-hint">{{ t('physicalGuard.hint') }}</div>
      <div style="flex:1" />
      <div class="macro-header-actions">
        <button class="btn btn-ghost" :disabled="!connected || loading" @click="refreshConfig(true)">
          {{ t('physicalGuard.refresh') }}
        </button>
        <button class="btn btn-accent" :disabled="!dirty || !connected || !supported || saving" @click="saveConfig">
          {{ t('physicalGuard.save') }}
        </button>
        <button class="btn btn-outline red" :disabled="!connected || !supported || saving" @click="resetConfig">
          {{ t('physicalGuard.reset') }}
        </button>
      </div>
    </div>

    <div v-if="toast" class="macro-toast" :class="toastType">{{ toast }}</div>

    <div v-if="!connected" class="pg-alert warn">{{ t('physicalGuard.notConnected') }}</div>
    <div v-else-if="!supported" class="pg-alert warn">{{ t('physicalGuard.unsupported') }}</div>
    <div v-else-if="devMode === 1" class="pg-alert warn">{{ t('physicalGuard.gamepadLine') }}</div>

    <!-- 状态 / 手动窗口 -->
    <div class="pg-card">
      <div class="pg-card-head">
        <span class="pg-card-title">{{ t('physicalGuard.statusTitle') }}</span>
        <div style="flex:1" />
        <button
          class="btn-toggle btn-xs"
          :class="{ active: cfg.enable }"
          :disabled="!connected || !supported || saving"
          @click="toggleEnable"
        >
          {{ cfg.enable ? 'ON' : 'OFF' }}
        </button>
      </div>

      <div class="pg-windows">
        <div class="pg-window">
          <span class="pg-label">{{ t('physicalGuard.windowX') }}</span>
          <div class="macro-num-input">
            <input
              type="number"
              class="input-select"
              min="0"
              max="500"
              step="1"
              v-model.number="cfg.wx"
              :disabled="!connected || !supported || saving"
              @input="markDirty"
              @change="normalizeWins"
            />
            <span class="macro-num-unit">ms</span>
          </div>
        </div>
        <div class="pg-window">
          <span class="pg-label">{{ t('physicalGuard.windowY') }}</span>
          <div class="macro-num-input">
            <input
              type="number"
              class="input-select"
              min="0"
              max="500"
              step="1"
              v-model.number="cfg.wy"
              :disabled="!connected || !supported || saving"
              @input="markDirty"
              @change="normalizeWins"
            />
            <span class="macro-num-unit">ms</span>
          </div>
        </div>
      </div>

      <div class="pg-counters">
        <span>{{ t('physicalGuard.suppressedX') }} <b>{{ counters.sx }}</b></span>
        <span>{{ t('physicalGuard.suppressedY') }} <b>{{ counters.sy }}</b></span>
        <span>{{ t('physicalGuard.lastSuppressed') }} <b>{{ counters.last ? counters.last + ' ms' : '—' }}</b></span>
      </div>
      <div class="pg-note">{{ t('physicalGuard.statusNote') }}</div>
    </div>

    <!-- 校准 -->
    <div class="pg-card pg-cal-card">
      <div class="pg-card-head">
        <span class="pg-card-title">{{ t('physicalGuard.calibrateTitle') }}</span>
        <div style="flex:1" />
        <span v-if="calPhase === 'running'" class="pg-badge">{{ t('physicalGuard.calibrating') }}</span>
      </div>

      <!-- idle / countdown -->
      <div v-if="calPhase === 'idle' || calPhase === 'countdown'">
        <p class="pg-hint">{{ t('physicalGuard.calibrateHint') }}</p>
        <div class="pg-cal-actions">
          <button class="btn btn-accent" :disabled="!canCalibrate" @click="startCalibration('x')">
            {{ t('physicalGuard.startX') }}
          </button>
          <button class="btn" :disabled="!canCalibrate" @click="startCalibration('y')">
            {{ t('physicalGuard.startY') }}
          </button>
        </div>

        <div v-if="calPhase === 'countdown'" class="pg-countdown">
          <div class="pg-countdown-num">{{ calCountdown }}</div>
          <div class="pg-countdown-text">{{ t('physicalGuard.getReady') }}</div>
        </div>
      </div>

      <!-- running -->
      <div v-else-if="calPhase === 'running'">
        <div class="pg-swing">
          <template v-if="calAxis === 'x'">
            <div class="pg-arrow" :class="{ hot: calDirX < 0 }">←</div>
            <div class="pg-swing-text">{{ t('physicalGuard.swingX') }}</div>
            <div class="pg-arrow" :class="{ hot: calDirX > 0 }">→</div>
          </template>
          <template v-else>
            <div class="pg-arrow" :class="{ hot: calDirY < 0 }">↑</div>
            <div class="pg-swing-text">{{ t('physicalGuard.swingY') }}</div>
            <div class="pg-arrow" :class="{ hot: calDirY > 0 }">↓</div>
          </template>
        </div>

        <div class="pg-progress">
          <div class="pg-progress-bar" :style="{ width: progressPct + '%' }"></div>
        </div>
        <div class="pg-live">
          <span>{{ t('physicalGuard.elapsed') }} {{ (calElapsed / 1000).toFixed(1) }}s</span>
          <span>{{ t('physicalGuard.frames') }} {{ calFrames }}</span>
          <span>{{ t('physicalGuard.liveMin') }} {{ fmtMs(calStat && calStat.min) }}</span>
          <span>{{ t('physicalGuard.liveCount') }} {{ (calStat && calStat.count) || 0 }}</span>
        </div>

        <div v-if="currentRecent.length" class="pg-bars">
          <div
            v-for="(v, i) in currentRecent"
            :key="i"
            class="pg-bar"
            :style="{ height: barHeight(v) + 'px' }"
            :title="v + ' ms'"
          ></div>
        </div>
        <div v-else-if="showNoData" class="pg-no-data">{{ t('physicalGuard.noData') }}</div>
        <div v-else class="pg-no-data">{{ t('physicalGuard.noReversals') }}</div>

        <div class="pg-cal-actions">
          <button class="btn btn-outline red" @click="cancelCalibration">{{ t('physicalGuard.stop') }}</button>
        </div>
      </div>

      <!-- result -->
      <div v-else-if="calPhase === 'result'">
        <div class="pg-result">
          <div class="pg-result-num">{{ fmtMs(calStat && calStat.min) }}</div>
          <div class="pg-result-label">
            {{ calAxis === 'x' ? t('physicalGuard.resultX') : t('physicalGuard.resultY') }}
          </div>
          <div class="pg-result-sub">
            P5 {{ fmtMs(calStat && calStat.p5) }}
            · {{ t('physicalGuard.median') }} {{ fmtMs(calStat && calStat.median) }}
            · {{ t('physicalGuard.samples') }} {{ (calStat && calStat.count) || 0 }}
            · {{ timebaseLabel }}
          </div>
          <div v-if="calReason === 'stopped'" class="pg-note">{{ t('physicalGuard.stoppedEarly') }}</div>
          <div v-else-if="calStat && !calStat.reliable" class="pg-note">{{ t('physicalGuard.tooFew') }}</div>
        </div>

        <div class="pg-result-actions">
          <button class="btn btn-accent" :disabled="!supported || !connected || saving" @click="applyCalibration">
            {{ t('physicalGuard.apply') }}
          </button>
          <button class="btn" :disabled="!canCalibrate" @click="startCalibration(calAxis)">
            {{ t('physicalGuard.recalibrate') }}
          </button>
          <button v-if="calAxis === 'x'" class="btn" :disabled="!canCalibrate" @click="startCalibration('y')">
            {{ t('physicalGuard.nextY') }}
          </button>
          <button v-else class="btn" :disabled="!canCalibrate" @click="startCalibration('x')">
            {{ t('physicalGuard.nextX') }}
          </button>
        </div>
      </div>
    </div>

    <!-- 已学习 -->
    <div class="pg-card">
      <div class="pg-card-head">
        <span class="pg-card-title">{{ t('physicalGuard.learnedTitle') }}</span>
      </div>
      <div class="pg-learned-row">
        <span class="pg-learned-axis">X</span>
        <b>{{ learnedWx ? learnedWx + ' ms' : '—' }}</b>
        <span class="pg-learned-axis">Y</span>
        <b>{{ learnedWy ? learnedWy + ' ms' : '—' }}</b>
      </div>
      <div class="pg-note">{{ t('physicalGuard.learnedNote') }}</div>
    </div>

    <!-- 说明 -->
    <div class="pg-card pg-how">
      <div class="pg-card-head">
        <span class="pg-card-title">{{ t('physicalGuard.howTitle') }}</span>
      </div>
      <ul class="pg-how-list">
        <li>{{ t('physicalGuard.how1') }}</li>
        <li>{{ t('physicalGuard.how2') }}</li>
        <li>{{ t('physicalGuard.how3') }}</li>
        <li>{{ t('physicalGuard.how4') }}</li>
        <li>{{ t('physicalGuard.how5') }}</li>
        <li>{{ t('physicalGuard.how6') }}</li>
        <li>{{ t('physicalGuard.how7') }}</li>
      </ul>
    </div>
  </div>
</template>

<script setup>
import { reactive, ref, computed, watch, onMounted, onUnmounted } from 'vue';
import { useI18n } from '../i18n/index.js';

const { t } = useI18n();
const api = window.driverApi;

const props = defineProps({
  connected: { type: Boolean, default: false },
  // 0=鼠标线路 1=手柄线路 null=未知
  devMode: { type: Number, default: null },
});

const cfg = reactive({ enable: 0, wx: 12, wy: 12 });
const counters = reactive({ sx: 0, sy: 0, last: 0 });

const loaded = ref(false);
const loading = ref(false);
const supported = ref(true);
const saving = ref(false);
const dirty = ref(false);

const toast = ref('');
const toastType = ref('ok');
let toastTimer = null;

const calPhase = ref('idle');       // idle | countdown | running | result
const calAxis = ref('x');
const calCountdown = ref(3);
const calDuration = ref(5000);
const calElapsed = ref(0);
const calFrames = ref(0);
const calX = ref(null);
const calY = ref(null);
const calDirX = ref(0);
const calDirY = ref(0);
const calTimebase = ref('none');
const calReason = ref('');

const resultX = ref(null);
const resultY = ref(null);
const lastSession = ref(0);
const lastPhase = ref('idle');
const startingCal = ref(false);

let countdownTimer = null;
let elapsedTimer = null;
let unsupportedTimer = null;
let saveTimer = null;
let pollTimer = null;
let disposed = false;
const cleanups = [];

const calStat = computed(() => (calAxis.value === 'x' ? calX.value : calY.value));
const currentRecent = computed(() => {
  const st = calStat.value;
  return st && Array.isArray(st.recent) ? st.recent : [];
});
const maxRecent = computed(() => {
  const list = currentRecent.value;
  let m = 0;
  for (let i = 0; i < list.length; i++) if (list[i] > m) m = list[i];
  return m;
});
const progressPct = computed(() => {
  if (!calDuration.value) return 0;
  const p = (calElapsed.value / calDuration.value) * 100;
  return p < 0 ? 0 : p > 100 ? 100 : p;
});
const showNoData = computed(() => calElapsed.value > 1000 && calFrames.value === 0);
const canCalibrate = computed(
  () => !!api && props.connected && props.devMode !== 1 && (calPhase.value === 'idle' || calPhase.value === 'result')
);
const learnedWx = computed(() => pickLearned(resultX.value, 0));
const learnedWy = computed(() => pickLearned(resultY.value, 0));
const timebaseLabel = computed(() => {
  const tb = calTimebase.value;
  if (tb === 'device') return t('physicalGuard.timebaseDevice');
  if (tb === 'host') return t('physicalGuard.timebaseHost');
  if (tb === 'mixed') return t('physicalGuard.timebaseMixed');
  return t('physicalGuard.timebaseNone');
});

function showToast(msg, type) {
  toast.value = msg;
  toastType.value = type || 'ok';
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(function () { toast.value = ''; }, 2400);
}

function listen(event, handler) {
  if (!api || !api.on) return;
  // preload 的 on() 返回的清理函数才是移除“包装后 handler”的正解；
  // api.off(event, handler) 移除的是原函数，和实际注册的 wrapper 对不上。
  const off = api.on(event, handler);
  cleanups.push(function () {
    if (typeof off === 'function') off();
    else if (api.off) api.off(event, handler);
  });
}

function send(type, params) {
  if (api && api.send) api.send(type, params || {});
}

function markDirty() { dirty.value = true; }

function clampWin(v) {
  let n = parseInt(v, 10);
  if (isNaN(n)) n = 0;
  if (n < 0) n = 0;
  if (n > 500) n = 500;
  return n;
}

function normalizeWins() {
  cfg.wx = clampWin(cfg.wx);
  cfg.wy = clampWin(cfg.wy);
  if (!dirty.value) dirty.value = true;
}

function fmtMs(v) {
  return (v && v > 0) ? (v + ' ms') : '—';
}

function barHeight(v) {
  const m = maxRecent.value;
  if (!m || !v) return 6;
  const h = 8 + Math.round((v / m) * 48);
  return h < 6 ? 6 : h;
}

function pickLearned(stat, fallback) {
  if (!stat) return fallback || 0;
  if (stat.suggest && stat.reliable) return stat.suggest;
  if (stat.min) return stat.min;
  return fallback || 0;
}

function reasonLabel(reason) {
  if (reason === 'device_lost') return t('physicalGuard.reasonDeviceLost');
  if (reason === 'shutdown') return t('physicalGuard.reasonShutdown');
  if (reason === 'stopped') return t('physicalGuard.reasonStopped');
  if (reason === 'restarted') return t('physicalGuard.reasonRestarted');
  return reason || t('physicalGuard.reasonTimeout');
}

function toggleEnable() {
  cfg.enable = cfg.enable ? 0 : 1;
  dirty.value = true;
}

function applyServerConfig(j) {
  if (!j) return;
  loaded.value = true;
  if (!dirty.value) {
    cfg.enable = j.e ? 1 : 0;
    cfg.wx = (j.wx != null) ? clampWin(j.wx) : cfg.wx;
    cfg.wy = (j.wy != null) ? clampWin(j.wy) : cfg.wy;
  }
  counters.sx = j.sx || 0;
  counters.sy = j.sy || 0;
  counters.last = j.last || 0;
}

function onDebugResponse(data) {
  if (!data || typeof data.message !== 'string') return;

  if (data.message === 'guard') {
    clearTimeout(unsupportedTimer);
    supported.value = true;
    loading.value = false;
    if (typeof data.data === 'string') {
      try { applyServerConfig(JSON.parse(data.data)); } catch (e) { /* ignore */ }
    }
    return;
  }

  if (data.message === 'guard_set' || data.message === 'guard_reset') {
    clearTimeout(saveTimer);
    saving.value = false;
    supported.value = true;
    dirty.value = false;
    if (typeof data.data === 'string') {
      try {
        const j = JSON.parse(data.data);
        if (j) {
          cfg.enable = j.e ? 1 : 0;
          cfg.wx = (j.wx != null) ? clampWin(j.wx) : cfg.wx;
          cfg.wy = (j.wy != null) ? clampWin(j.wy) : cfg.wy;
        }
      } catch (e) { /* ignore */ }
    }
    if (data.message === 'guard_reset') {
      counters.sx = 0;
      counters.sy = 0;
      counters.last = 0;
      resultX.value = null;
      resultY.value = null;
      calX.value = null;
      calY.value = null;
      calReason.value = '';
      calPhase.value = 'idle';
      showToast(t('physicalGuard.resetDone'), 'ok');
    } else {
      // 固件每次 cmd 68 都会重置拦截计数
      counters.sx = 0;
      counters.sy = 0;
      counters.last = 0;
      showToast(t('physicalGuard.saved'), 'ok');
    }
  }
}

function onSendResult(data) {
  if (!data || typeof data.cmd !== 'string') return;
  if (data.cmd.indexOf('guard_') !== 0) return;
  if (data.ok === false) {
    if (data.cmd === 'guard_get') {
      loading.value = false;
      clearTimeout(unsupportedTimer);
    }
    if (data.cmd === 'guard_set' || data.cmd === 'guard_reset') {
      saving.value = false;
      clearTimeout(saveTimer);
    }
    showToast(data.error || t('physicalGuard.sendFailed'), 'err');
  }
}

function onGuardEvent(data) {
  if (!data || !data.phase) return;

  if (data.session != null) {
    if (lastSession.value && data.session < lastSession.value) return;
    if (
      data.session === lastSession.value &&
      (lastPhase.value === 'done' || lastPhase.value === 'error') &&
      data.phase === 'running'
    ) return;
    lastSession.value = data.session;
    lastPhase.value = data.phase;
  }

  if (data.phase === 'running') {
    if (calPhase.value !== 'running') return;
    calElapsed.value = Math.max(calElapsed.value, data.elapsed || 0);
    calFrames.value = data.frames || 0;
    calX.value = data.x || null;
    calY.value = data.y || null;
    calDirX.value = data.dirX || 0;
    calDirY.value = data.dirY || 0;
    calTimebase.value = data.timebase || 'none';
    return;
  }

  if (data.phase === 'done') {
    stopCalTimers();
    calPhase.value = 'result';
    calElapsed.value = data.elapsed || calDuration.value;
    calFrames.value = data.frames || 0;
    calX.value = data.x || null;
    calY.value = data.y || null;
    calTimebase.value = data.timebase || 'none';
    calReason.value = data.reason || '';
    if (data.axis === 'y') resultY.value = data.y || null;
    else resultX.value = data.x || null;

    const st = data.axis === 'y' ? data.y : data.x;
    if (st && st.count < 5) showToast(t('physicalGuard.tooFew'), 'warn');
    return;
  }

  if (data.phase === 'error') {
    stopCalTimers();
    calPhase.value = 'idle';
    calReason.value = data.reason || 'error';
    showToast(t('physicalGuard.aborted', { reason: reasonLabel(data.reason) }), 'err');
  }
}

function stopCalTimers() {
  if (countdownTimer) { clearInterval(countdownTimer); countdownTimer = null; }
  if (elapsedTimer) { clearInterval(elapsedTimer); elapsedTimer = null; }
}

function refreshConfig(initial) {
  if (!props.connected) return;
  if (initial) {
    loaded.value = false;
    loading.value = true;
    clearTimeout(unsupportedTimer);
    unsupportedTimer = setTimeout(function () {
      if (!loaded.value) {
        supported.value = false;
        loading.value = false;
      }
    }, 2000);
  }
  send('get_guard_config');
}

function saveConfig() {
  normalizeWins();
  if (!props.connected) { showToast(t('physicalGuard.notConnected'), 'err'); return; }
  if (!supported.value) { showToast(t('physicalGuard.unsupported'), 'err'); return; }

  // 先保持 dirty，等固件 guard_set 确认后再清；下发失败时用户仍能看到“未保存”。
  saving.value = true;
  send('set_guard_config', {
    enable: cfg.enable ? 1 : 0,
    wx: cfg.wx,
    wy: cfg.wy,
  });

  clearTimeout(saveTimer);
  saveTimer = setTimeout(function () {
    saving.value = false;
    showToast(t('physicalGuard.saveTimeout'), 'warn');
  }, 1800);
}

function resetConfig() {
  if (!props.connected) return;
  saving.value = true;
  dirty.value = false;
  send('reset_guard_config');
  clearTimeout(saveTimer);
  saveTimer = setTimeout(function () {
    saving.value = false;
    showToast(t('physicalGuard.saveTimeout'), 'warn');
  }, 1800);
}

async function startCalibration(axis) {
  if (!api) { showToast(t('physicalGuard.driverDown'), 'err'); return; }
  if (!props.connected) { showToast(t('physicalGuard.notConnected'), 'err'); return; }
  if (props.devMode === 1) { showToast(t('physicalGuard.gamepadLine'), 'err'); return; }
  if (calPhase.value !== 'idle' && calPhase.value !== 'result') return;
  if (startingCal.value) return;

  startingCal.value = true;
  try {
    if (api.isConnected) {
      const ok = await api.isConnected();
      if (!ok) { showToast(t('physicalGuard.driverDown'), 'err'); return; }
    }
  } catch (e) {
    /* ignore */
  } finally {
    startingCal.value = false;
  }

  // 等待 isConnected() 期间用户可能已经切走；unmount 后不要再起定时器。
  if (disposed) return;

  stopCalTimers();
  calAxis.value = axis === 'y' ? 'y' : 'x';
  calPhase.value = 'countdown';
  calCountdown.value = 3;
  calDuration.value = 5000;
  calElapsed.value = 0;
  calFrames.value = 0;
  calX.value = null;
  calY.value = null;
  calDirX.value = 0;
  calDirY.value = 0;
  calReason.value = '';
  calTimebase.value = 'none';

  countdownTimer = setInterval(function () {
    calCountdown.value -= 1;
    if (calCountdown.value <= 0) {
      clearInterval(countdownTimer);
      countdownTimer = null;
      beginSession();
    }
  }, 1000);
}

function beginSession() {
  if (disposed) return;
  calPhase.value = 'running';
  calElapsed.value = 0;
  send('human_guard_begin', { axis: calAxis.value, duration: calDuration.value });

  if (elapsedTimer) clearInterval(elapsedTimer);
  elapsedTimer = setInterval(function () {
    if (disposed) { stopCalTimers(); return; }
    if (calPhase.value !== 'running') return;
    // 驱动进度事件通常每 100ms 到一次；本地兜底只用于进度条不倒退。
    calElapsed.value += 100;
    // 驱动/管道异常时不会再有 done 事件：给一个宽限，避免界面永远停在“校准中”。
    if (calElapsed.value > calDuration.value + 5000) {
      stopCalTimers();
      calPhase.value = 'idle';
      showToast(t('physicalGuard.driverDown'), 'err');
    }
  }, 100);
}

function cancelCalibration() {
  send('human_guard_stop');
  stopCalTimers();
  calPhase.value = 'idle';
}

function applyCalibration() {
  const wxFallback = (cfg.wx != null) ? cfg.wx : 12;
  const wyFallback = (cfg.wy != null) ? cfg.wy : wxFallback;
  let wx = pickLearned(resultX.value, wxFallback);
  let wy = pickLearned(resultY.value, wyFallback);
  // 两个轴都没测到有效值时，用 12ms 默认值，避免“启用了一个 0/0 的空护栏”。
  if (!wx) wx = 12;
  if (!wy) wy = wx;
  cfg.enable = 1;
  cfg.wx = clampWin(wx);
  cfg.wy = clampWin(wy);
  saveConfig();
}

function onConnectedChanged(v) {
  if (v) {
    supported.value = true;
    loaded.value = false;
    refreshConfig(true);
  } else {
    stopCalTimers();
    calPhase.value = 'idle';
    supported.value = true;
    loaded.value = false;
    loading.value = false;
  }
}

watch(() => props.connected, onConnectedChanged);
watch(() => props.devMode, function (v) {
  if (v === 1 && calPhase.value !== 'idle') cancelCalibration();
});

onMounted(function () {
  listen('human_guard', onGuardEvent);
  listen('debug_response', onDebugResponse);
  listen('send_result', onSendResult);
  listen('serial_connected', function () { supported.value = true; loaded.value = false; refreshConfig(true); });
  listen('serial_disconnected', function () { stopCalTimers(); calPhase.value = 'idle'; });
  listen('device_attached', function () { refreshConfig(true); });
  listen('device_detached', function () { stopCalTimers(); calPhase.value = 'idle'; });

  if (props.connected) refreshConfig(true);

  pollTimer = setInterval(function () {
    if (props.connected && supported.value && calPhase.value === 'idle') {
      send('get_guard_config');
    }
  }, 3000);
});

onUnmounted(function () {
  disposed = true;
  for (let i = 0; i < cleanups.length; i++) cleanups[i]();
  cleanups.length = 0;
  stopCalTimers();
  if (unsupportedTimer) clearTimeout(unsupportedTimer);
  if (saveTimer) clearTimeout(saveTimer);
  if (toastTimer) clearTimeout(toastTimer);
  if (pollTimer) clearInterval(pollTimer);
  if (calPhase.value === 'running' || calPhase.value === 'countdown') {
    send('human_guard_stop');
  }
});
</script>

<style scoped>
.pg-page .pg-card {
  background: var(--bg-card, #1e2230);
  border: 1px solid var(--border-color, #2c3142);
  border-radius: 10px;
  padding: 18px 20px;
  margin-top: 14px;
}
.pg-page .pg-card-head {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 14px;
}
.pg-page .pg-card-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text, #e6e9f0);
}
.pg-page .pg-badge {
  font-size: 11px;
  color: var(--accent, #4f8cff);
  border: 1px solid rgba(79, 140, 255, 0.35);
  background: rgba(79, 140, 255, 0.08);
  border-radius: 999px;
  padding: 2px 9px;
}
.pg-page .pg-alert {
  margin-top: 14px;
  padding: 10px 12px;
  border-radius: 8px;
  font-size: 13px;
  line-height: 1.5;
  border: 1px solid rgba(230, 174, 76, 0.35);
  border-left: 3px solid #e6ae4c;
  background: rgba(230, 174, 76, 0.10);
  color: #e8c98a;
}
.pg-page .pg-windows {
  display: flex;
  gap: 24px;
  flex-wrap: wrap;
}
.pg-page .pg-window {
  flex: 1;
  min-width: 180px;
  display: flex;
  align-items: center;
  gap: 10px;
}
.pg-page .pg-label {
  font-size: 12px;
  color: var(--text-muted, #7a8294);
  white-space: nowrap;
}
.pg-page .pg-counters {
  display: flex;
  gap: 20px;
  flex-wrap: wrap;
  margin-top: 14px;
  padding: 10px 12px;
  border-radius: 6px;
  background: var(--bg-elevated, #181b26);
  font-size: 12px;
  color: var(--text-muted, #7a8294);
}
.pg-page .pg-counters b {
  color: var(--text, #e6e9f0);
  font-variant-numeric: tabular-nums;
  margin-left: 4px;
}
.pg-page .pg-note {
  margin-top: 12px;
  padding: 8px 10px;
  border-radius: 6px;
  background: var(--bg-elevated, #181b26);
  font-size: 11px;
  line-height: 1.6;
  color: var(--text-muted, #7a8294);
}
.pg-page .pg-hint {
  margin: 0 0 14px;
  font-size: 13px;
  line-height: 1.6;
  color: var(--text-muted, #7a8294);
}
.pg-page .pg-cal-actions {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
  margin-top: 12px;
}
.pg-page .pg-countdown {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 18px 0 6px;
}
.pg-page .pg-countdown-num {
  font-size: 64px;
  font-weight: 700;
  line-height: 1;
  color: var(--accent, #4f8cff);
  font-variant-numeric: tabular-nums;
  animation: pg-pulse 0.9s ease-in-out infinite;
}
.pg-page .pg-countdown-text {
  margin-top: 10px;
  font-size: 13px;
  color: var(--text-muted, #7a8294);
}
@keyframes pg-pulse {
  0%, 100% { transform: scale(1); opacity: 1; }
  50% { transform: scale(0.92); opacity: 0.75; }
}
.pg-page .pg-swing {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 28px;
  padding: 20px 0 16px;
}
.pg-page .pg-arrow {
  font-size: 56px;
  line-height: 1;
  color: var(--text-muted, #7a8294);
  opacity: 0.28;
  transition: opacity 0.08s linear, transform 0.08s linear, color 0.08s linear;
}
.pg-page .pg-arrow.hot {
  opacity: 1;
  color: var(--accent, #4f8cff);
  transform: scale(1.16);
}
.pg-page .pg-swing-text {
  font-size: 16px;
  font-weight: 600;
  color: var(--text, #e6e9f0);
  text-align: center;
  min-width: 220px;
}
.pg-page .pg-progress {
  height: 8px;
  border-radius: 999px;
  background: var(--bg-elevated, #181b26);
  overflow: hidden;
}
.pg-page .pg-progress-bar {
  height: 100%;
  border-radius: 999px;
  background: var(--accent, #4f8cff);
  transition: width 0.12s linear;
}
.pg-page .pg-live {
  display: flex;
  gap: 18px;
  flex-wrap: wrap;
  margin-top: 12px;
  font-size: 12px;
  color: var(--text-muted, #7a8294);
  font-variant-numeric: tabular-nums;
}
.pg-page .pg-bars {
  display: flex;
  align-items: flex-end;
  gap: 3px;
  height: 62px;
  margin-top: 14px;
  padding: 6px 8px;
  border-radius: 6px;
  background: var(--bg-elevated, #181b26);
}
.pg-page .pg-bar {
  flex: 1;
  min-width: 4px;
  max-width: 12px;
  border-radius: 2px 2px 0 0;
  background: linear-gradient(180deg, var(--accent, #4f8cff), rgba(79, 140, 255, 0.35));
}
.pg-page .pg-no-data {
  margin-top: 14px;
  padding: 10px 12px;
  border-radius: 6px;
  background: var(--bg-elevated, #181b26);
  font-size: 12px;
  color: var(--text-muted, #7a8294);
}
.pg-page .pg-result {
  text-align: center;
  padding: 12px 0 4px;
}
.pg-page .pg-result-num {
  font-size: 46px;
  font-weight: 700;
  line-height: 1.1;
  color: var(--accent, #4f8cff);
  font-variant-numeric: tabular-nums;
}
.pg-page .pg-result-label {
  margin-top: 6px;
  font-size: 14px;
  font-weight: 600;
  color: var(--text, #e6e9f0);
}
.pg-page .pg-result-sub {
  margin-top: 8px;
  font-size: 12px;
  color: var(--text-muted, #7a8294);
  font-variant-numeric: tabular-nums;
}
.pg-page .pg-result-actions {
  display: flex;
  gap: 12px;
  justify-content: center;
  flex-wrap: wrap;
  margin-top: 16px;
}
.pg-page .pg-learned-row {
  display: flex;
  align-items: baseline;
  gap: 12px;
  flex-wrap: wrap;
}
.pg-page .pg-learned-axis {
  font-size: 12px;
  color: var(--text-muted, #7a8294);
}
.pg-page .pg-learned-row b {
  font-size: 20px;
  color: var(--accent, #4f8cff);
  font-variant-numeric: tabular-nums;
  margin-right: 18px;
}
.pg-page .pg-how-list {
  margin: 0;
  padding-left: 20px;
  font-size: 12px;
  line-height: 1.8;
  color: var(--text-muted, #7a8294);
}
</style>
