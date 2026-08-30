<template>
  <div class="panel macro-page minterp-page">
    <div class="panel-header">
      <svg class="panel-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <path d="M2 12 L5 12 L7 4 L9 4 L11 12 L14 12" />
      </svg>
      <span>{{ t('mouseInterp.title') }}</span>
      <div class="macro-header-hint">{{ t('mouseInterp.hint') }}</div>
      <div style="flex:1" />
      <div class="macro-header-actions">
        <button class="btn btn-ghost" @click="loadFromDevice">{{ t('mouseInterp.refresh') }}</button>
        <button class="btn btn-accent" :disabled="!dirty" @click="saveAll">{{ t('mouseInterp.save') }}</button>
        <button class="btn btn-outline red" @click="onReset">{{ t('mouseInterp.reset') }}</button>
      </div>
    </div>
    <div v-if="toast" class="macro-toast" :class="toastType">{{ toast }}</div>

    <div v-if="dirty" class="minterp-unsaved">
      <svg class="minterp-unsaved-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <path d="M8 2 L14 13 H2 Z" />
        <path d="M8 6 L8 9" />
        <circle cx="8" cy="11" r="0.5" fill="currentColor" />
      </svg>
      <span>{{ t('mouseInterp.unsaved') }}</span>
    </div>

    <div class="minterp-card">
      <div class="macro-slot-header">
        <span class="macro-slot-title">{{ t('mouseInterp.configTitle') }}</span>
        <div style="flex:1" />
        <button class="btn-toggle btn-xs" :class="{ active: cfg.enabled }" @click="toggleEnabled">
          {{ cfg.enabled ? 'ON' : 'OFF' }}
        </button>
      </div>

      <div class="minterp-body" v-show="cfg.enabled">
        <div class="macro-field">
          <span class="macro-field-label">{{ t('mouseInterp.profile') }}</span>
          <span class="macro-field-hint">{{ t('mouseInterp.profileHint') }}</span>
          <select class="input-select" v-model="cfg.profile" @change="markDirty">
            <option :value="0">{{ t('mouseInterp.profileLinear') }}</option>
            <option :value="1">{{ t('mouseInterp.profileEase') }}</option>
            <option :value="2">{{ t('mouseInterp.profileMinJerk') }}</option>
          </select>
        </div>

        <div class="macro-field minterp-field-row">
          <div class="minterp-col">
            <span class="macro-field-label">{{ t('mouseInterp.window') }}</span>
            <span class="macro-field-hint">{{ t('mouseInterp.windowHint') }}</span>
            <div class="macro-num-input">
              <input type="number" class="input-select" min="0" max="1000" step="1"
                :value="cfg.window" @input="setNum('window', $event, 0, 1000)" />
              <span class="macro-num-unit">{{ t('mouseInterp.msUnit') }}</span>
            </div>
          </div>
          <div class="minterp-col">
            <span class="macro-field-label">{{ t('mouseInterp.maxWindow') }}</span>
            <span class="macro-field-hint">{{ t('mouseInterp.maxWindowHint') }}</span>
            <div class="macro-num-input">
              <input type="number" class="input-select" min="1" max="200" step="1"
                :value="cfg.maxw" @input="setNum('maxw', $event, 1, 200)" />
              <span class="macro-num-unit">{{ t('mouseInterp.msUnit') }}</span>
            </div>
          </div>
        </div>

        <div class="minterp-ema" v-if="ema > 0">
          {{ t('mouseInterp.emaLabel') }}: <b>{{ ema }} ms</b>
          <span class="minterp-ema-sub">≈ {{ (1000 / ema).toFixed(0) }} Hz</span>
        </div>

        <div class="minterp-note">{{ t('mouseInterp.note') }}</div>
      </div>

      <div class="macro-slot-empty" v-show="!cfg.enabled">
        <span>{{ t('mouseInterp.notEnabled') }}</span>
      </div>
    </div>

    <div class="minterp-card minterp-test-card">
      <div class="macro-slot-header">
        <span class="macro-slot-title">{{ t('mouseInterp.testTitle') }}</span>
        <div style="flex:1" />
        <button
          class="btn"
          :class="testRunning ? 'btn-outline red' : 'btn-accent'"
          @click="toggleTest"
        >
          {{ testRunning ? t('mouseInterp.testStop') : t('mouseInterp.testStart') }}
        </button>
      </div>
      <div class="minterp-test-hint">{{ t('mouseInterp.testHint') }}</div>

      <div class="minterp-field-row">
        <div class="minterp-col">
          <span class="macro-field-label">{{ t('mouseInterp.testInterval') }}</span>
          <span class="macro-field-hint">{{ t('mouseInterp.testIntervalHint') }}</span>
          <div class="macro-num-input">
            <input
              type="number"
              class="input-select"
              min="1"
              max="1000"
              step="1"
              :value="testInterval"
              :disabled="testRunning"
              @input="setTestInterval"
            />
            <span class="macro-num-unit">{{ t('mouseInterp.msUnit') }}</span>
          </div>
        </div>
        <div class="minterp-col">
          <span class="macro-field-label">{{ t('mouseInterp.testAmplitude') }}</span>
          <span class="macro-field-hint">{{ t('mouseInterp.testAmplitudeHint') }}</span>
          <div class="macro-num-input">
            <input
              type="number"
              class="input-select"
              min="1"
              max="1000"
              step="1"
              :value="testAmplitude"
              :disabled="testRunning"
              @input="setTestAmplitude"
            />
            <span class="macro-num-unit">{{ t('mouseInterp.testAmplitudeUnit') }}</span>
          </div>
        </div>
      </div>

      <div class="minterp-test-rate">
        <span class="macro-field-label">{{ t('mouseInterp.testRateLabel') }}</span>
        <div class="minterp-test-rate-value">
          <b>{{ testCallsPerSecond }}</b> {{ t('mouseInterp.testRateUnit') }}
          <span class="minterp-test-rate-sub">{{ t('mouseInterp.testRateNote') }} {{ testInterval }} ms</span>
        </div>
      </div>

      <div class="minterp-test-status" :class="{ running: testRunning }">
        <span v-if="testRunning">
          {{ t('mouseInterp.testRunning') }} · {{ testTicks }}
          <span class="minterp-test-dir">{{ testDirection === 1 ? '→' : '←' }}</span>
        </span>
        <span v-else>{{ t('mouseInterp.testIdle') }}</span>
      </div>
    </div>
  </div>
</template>

<script setup>
import { reactive, ref, watch, onUnmounted, computed } from 'vue';
import { useI18n } from '../i18n/index.js';

const { t } = useI18n();
const api = window.driverApi;

const props = defineProps({
  config: { type: Object, default: () => ({ enabled: 1, profile: 0, window: 0, maxw: 50, ema: 0 }) }
});
const emit = defineEmits(['save', 'reset', 'load']);

const dirty = ref(false);
const toast = ref('');
const toastType = ref('ok');
let toastTimer = null;

function showToast(msg, type) {
  toast.value = msg;
  toastType.value = type || 'ok';
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(function() { toast.value = ''; }, 2000);
}

const cfg = reactive({ enabled: 1, profile: 0, window: 0, maxw: 50 });
const ema = ref(0);

watch(() => props.config, (newCfg) => {
  if (!newCfg) return;
  cfg.enabled = newCfg.enabled != null ? (newCfg.enabled ? 1 : 0) : 1;
  cfg.profile = newCfg.profile != null ? newCfg.profile : 0;
  cfg.window  = newCfg.window != null ? newCfg.window : 0;
  cfg.maxw    = newCfg.maxw != null ? newCfg.maxw : 50;
  ema.value   = newCfg.ema || 0;
  dirty.value = false;
}, { deep: true, immediate: true });

function markDirty() { dirty.value = true; }

function toggleEnabled() {
  cfg.enabled = cfg.enabled ? 0 : 1;
  markDirty();
}

function setNum(field, ev, minVal, maxVal) {
  var v = parseInt(ev.target.value, 10);
  if (isNaN(v)) v = minVal;
  if (v < minVal) v = minVal;
  if (v > maxVal) v = maxVal;
  cfg[field] = v;
  markDirty();
}

function saveAll() {
  emit('save', {
    enabled: cfg.enabled ? 1 : 0,
    profile: cfg.profile,
    window: cfg.window,
    maxw: cfg.maxw
  });
  dirty.value = false;
  showToast(t('mouseInterp.saved'), 'ok');
}

function onReset() {
  cfg.enabled = 1;
  cfg.profile = 0;
  cfg.window = 0;
  cfg.maxw = 50;
  emit('reset');
  dirty.value = false;
  showToast(t('mouseInterp.resetDone'), 'ok');
}

function loadFromDevice() {
  emit('load');
  showToast(t('mouseInterp.refreshed'), 'ok');
}

const testRunning = ref(false);
const testInterval = ref(10);
const testAmplitude = ref(200);
const testTicks = ref(0);
const testDirection = ref(1);
let testTimer = null;

const testCallsPerSecond = computed(() => {
  return testInterval.value > 0 ? Math.round(1000 / testInterval.value) : 0;
});

function setTestInterval(ev) {
  var v = parseInt(ev.target.value, 10);
  if (isNaN(v)) v = 10;
  if (v < 1) v = 1;
  if (v > 1000) v = 1000;
  testInterval.value = v;
}

function setTestAmplitude(ev) {
  var v = parseInt(ev.target.value, 10);
  if (isNaN(v)) v = 200;
  if (v < 1) v = 1;
  if (v > 1000) v = 1000;
  testAmplitude.value = v;
}

function sendTestMove() {
  api.send('send_ysc', {
    cmd: JSON.stringify({
      cmd: 30,
      x: testDirection.value * testAmplitude.value,
      y: 0,
      c: 1
    })
  });
}

function startTest() {
  if (!api) {
    showToast(t('mouseInterp.testNoDriver'), 'err');
    return;
  }
  testRunning.value = true;
  testTicks.value = 0;
  testDirection.value = 1;
  sendTestMove();
  testTimer = setInterval(function() {
    testTicks.value += 1;
    testDirection.value = testDirection.value === 1 ? -1 : 1;
    sendTestMove();
  }, testInterval.value);
}

function stopTest() {
  if (testTimer) {
    clearInterval(testTimer);
    testTimer = null;
  }
  testRunning.value = false;
}

function toggleTest() {
  if (testRunning.value) stopTest();
  else startTest();
}

onUnmounted(stopTest);
</script>

<style scoped>
.minterp-page .minterp-card {
  background: var(--bg-card, #1e2230);
  border: 1px solid var(--border-color, #2c3142);
  border-radius: 10px;
  padding: 18px 20px;
  margin-top: 14px;
}
.minterp-page .minterp-unsaved {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 14px;
  padding: 10px 12px;
  border: 1px solid rgba(230, 174, 76, 0.35);
  border-left: 3px solid #e6ae4c;
  border-radius: 8px;
  background: rgba(230, 174, 76, 0.10);
  color: #e8c98a;
  font-size: 13px;
  line-height: 1.4;
}
.minterp-page .minterp-unsaved-icon {
  width: 15px;
  height: 15px;
  flex: none;
}
.minterp-page .minterp-body {
  margin-top: 12px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.minterp-page .minterp-field-row {
  display: flex;
  gap: 24px;
  align-items: flex-start;
}
.minterp-page .minterp-col {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.minterp-page .minterp-ema {
  font-size: 12px;
  color: var(--text-muted, #7a8294);
  padding: 8px 10px;
  border-radius: 6px;
  background: var(--bg-elevated, #181b26);
}
.minterp-page .minterp-ema b { color: var(--text, #e6e9f0); font-variant-numeric: tabular-nums; }
.minterp-page .minterp-ema-sub { margin-left: 8px; opacity: .8; }
.minterp-page .minterp-note {
  font-size: 11px;
  color: var(--text-muted, #7a8294);
  line-height: 1.5;
  padding: 8px 10px;
  border-radius: 6px;
  background: var(--bg-elevated, #181b26);
}
.minterp-page .minterp-test-hint {
  font-size: 12px;
  color: var(--text-muted, #7a8294);
  line-height: 1.5;
  margin-top: 12px;
}
.minterp-page .minterp-test-rate {
  margin-top: 14px;
  padding: 10px 12px;
  border-radius: 6px;
  background: var(--bg-elevated, #181b26);
  display: flex;
  align-items: baseline;
  gap: 12px;
  flex-wrap: wrap;
}
.minterp-page .minterp-test-rate-value {
  font-size: 13px;
  color: var(--text, #e6e9f0);
}
.minterp-page .minterp-test-rate-value b {
  font-size: 18px;
  color: var(--accent, #4f8cff);
  font-variant-numeric: tabular-nums;
}
.minterp-page .minterp-test-rate-sub {
  margin-left: 8px;
  font-size: 12px;
  color: var(--text-muted, #7a8294);
}
.minterp-page .minterp-test-status {
  font-size: 12px;
  color: var(--text-muted, #7a8294);
  margin-top: 14px;
  padding: 8px 10px;
  border-radius: 6px;
  background: var(--bg-elevated, #181b26);
  font-variant-numeric: tabular-nums;
}
.minterp-page .minterp-test-status.running {
  color: var(--accent, #4f8cff);
}
.minterp-page .minterp-test-dir {
  display: inline-block;
  margin-left: 6px;
  color: var(--text, #e6e9f0);
}
</style>
