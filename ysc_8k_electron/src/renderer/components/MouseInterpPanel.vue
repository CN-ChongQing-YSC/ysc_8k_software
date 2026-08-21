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
  </div>
</template>

<script setup>
import { reactive, ref, watch } from 'vue';
import { useI18n } from '../i18n/index.js';

const { t } = useI18n();

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
</script>

<style scoped>
.minterp-page .minterp-card {
  background: var(--bg-card, #1e2230);
  border: 1px solid var(--border-color, #2c3142);
  border-radius: 10px;
  padding: 18px 20px;
  margin-top: 14px;
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
</style>
