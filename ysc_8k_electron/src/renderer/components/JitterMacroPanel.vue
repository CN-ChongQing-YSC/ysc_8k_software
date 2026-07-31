<template>
  <div class="panel macro-page jitter-page">
    <div class="panel-header">
      <svg class="panel-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <path d="M2 8 L5 8 L6.5 4 L9.5 12 L11 8 L14 8" />
      </svg>
      <span>{{ t('jitterMacro.title') }}</span>
      <div class="macro-header-hint">{{ t('jitterMacro.hint') }}</div>
      <div style="flex:1" />
      <div class="macro-header-actions">
        <button class="btn btn-ghost" @click="loadFromDevice">{{ t('jitterMacro.refresh') }}</button>
        <button class="btn btn-accent" :disabled="!dirty" @click="saveAll">{{ t('jitterMacro.save') }}</button>
        <button class="btn btn-outline red" @click="onReset">{{ t('jitterMacro.reset') }}</button>
      </div>
    </div>
    <div v-if="toast" class="macro-toast" :class="toastType">{{ toast }}</div>

    <div class="jitter-card">
      <div class="macro-slot-header">
        <span class="macro-slot-title">{{ t('jitterMacro.configTitle') }}</span>
        <div style="flex:1" />
        <button class="btn-toggle btn-xs" :class="{ active: cfg.enabled }" @click="toggleEnabled">
          {{ cfg.enabled ? 'ON' : 'OFF' }}
        </button>
      </div>

      <div class="jitter-body" v-show="cfg.enabled">
        <div class="macro-field">
          <span class="macro-field-label">{{ t('jitterMacro.trigger') }}</span>
          <span class="macro-field-hint">{{ t('jitterMacro.triggerHint') }}</span>
          <div class="macro-btn-group">
            <button v-for="b in buttonDefs" :key="'j'+b.bit"
              class="macro-btn-toggle"
              :class="{ active: (cfg.trigger & b.bit) !== 0 }"
              @click="toggleBit('trigger', b.bit)"
            >{{ b.name }}</button>
          </div>
        </div>

        <div class="macro-field jitter-field-row">
          <div class="jitter-col">
            <span class="macro-field-label">{{ t('jitterMacro.amplitudeX') }}</span>
            <span class="macro-field-hint">{{ t('jitterMacro.amplitudeXHint') }}</span>
            <div class="macro-num-input">
              <input type="number" class="input-select" min="-127" max="127" step="1"
                :value="cfg.ax" @input="setNum('ax', $event, -127, 127)" />
              <span class="macro-num-unit">{{ t('jitterMacro.unit') }}</span>
            </div>
          </div>
          <div class="jitter-col">
            <span class="macro-field-label">{{ t('jitterMacro.freqX') }}</span>
            <span class="macro-field-hint">{{ t('jitterMacro.freqXHint') }}</span>
            <div class="macro-num-input">
              <input type="number" class="input-select" min="0" max="65535" step="1"
                :value="cfg.fx" @input="setNum('fx', $event, 0, 65535)" />
              <span class="macro-num-unit">{{ t('jitterMacro.msUnit') }}</span>
            </div>
          </div>
        </div>

        <div class="macro-field jitter-field-row">
          <div class="jitter-col">
            <span class="macro-field-label">{{ t('jitterMacro.pullY') }}</span>
            <span class="macro-field-hint">{{ t('jitterMacro.pullYHint') }}</span>
            <div class="macro-num-input">
              <input type="number" class="input-select" min="-127" max="127" step="1"
                :value="cfg.py" @input="setNum('py', $event, -127, 127)" />
              <span class="macro-num-unit">{{ t('jitterMacro.unit') }}</span>
            </div>
          </div>
          <div class="jitter-col">
            <span class="macro-field-label">{{ t('jitterMacro.freqY') }}</span>
            <span class="macro-field-hint">{{ t('jitterMacro.freqYHint') }}</span>
            <div class="macro-num-input">
              <input type="number" class="input-select" min="0" max="65535" step="1"
                :value="cfg.fy" @input="setNum('fy', $event, 0, 65535)" />
              <span class="macro-num-unit">{{ t('jitterMacro.msUnit') }}</span>
            </div>
          </div>
        </div>

        <div class="jitter-note">{{ t('jitterMacro.freqZeroNote') }}</div>
      </div>

      <div class="macro-slot-empty" v-show="!cfg.enabled">
        <span>{{ t('jitterMacro.notEnabled') }}</span>
      </div>
    </div>
  </div>
</template>

<script setup>
import { reactive, ref, computed, watch } from 'vue';
import { useI18n } from '../i18n/index.js';

const { t } = useI18n();

const props = defineProps({
  config: { type: Object, default: () => ({ enabled: 0, trigger: 0, ax: 0, fx: 0, py: 0, fy: 0 }) }
});
const emit = defineEmits(['save', 'reset', 'load']);

const buttonDefs = computed(() => [
  { bit: 0x01, name: t('monitor.btn.left') },
  { bit: 0x02, name: t('monitor.btn.right') },
  { bit: 0x04, name: t('monitor.btn.middle') },
  { bit: 0x08, name: t('monitor.btn.back') },
  { bit: 0x10, name: t('monitor.btn.forward') },
]);

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

const cfg = reactive({
  enabled: 0, trigger: 0, ax: 10, fx: 0, py: 5, fy: 0
});

watch(() => props.config, (newCfg) => {
  if (!newCfg) return;
  cfg.enabled = newCfg.enabled || 0;
  cfg.trigger = newCfg.trigger || 0;
  cfg.ax = newCfg.ax || 0;
  cfg.fx = newCfg.fx || 0;
  cfg.py = newCfg.py || 0;
  cfg.fy = newCfg.fy || 0;
  dirty.value = false;
}, { deep: true, immediate: true });

function markDirty() { dirty.value = true; }

function toggleEnabled() {
  cfg.enabled = cfg.enabled ? 0 : 1;
  markDirty();
}

function toggleBit(field, bit) {
  cfg[field] = cfg[field] ^ bit;
  markDirty();
}

function setNum(field, ev, minVal, maxVal) {
  var v = parseInt(ev.target.value, 10);
  if (isNaN(v)) v = 0;
  if (v < minVal) v = minVal;
  if (v > maxVal) v = maxVal;
  cfg[field] = v;
  markDirty();
}

function saveAll() {
  emit('save', {
    enabled: cfg.enabled,
    trigger: cfg.trigger,
    ax: cfg.ax,
    fx: cfg.fx,
    py: cfg.py,
    fy: cfg.fy
  });
  dirty.value = false;
  showToast(t('jitterMacro.saved'), 'ok');
}

function onReset() {
  cfg.enabled = 0;
  cfg.trigger = 0;
  cfg.ax = 0;
  cfg.fx = 0;
  cfg.py = 0;
  cfg.fy = 0;
  emit('reset');
  dirty.value = false;
  showToast(t('jitterMacro.resetDone'), 'ok');
}

function loadFromDevice() {
  emit('load');
  showToast(t('jitterMacro.refreshed'), 'ok');
}
</script>

<style scoped>
.jitter-page .jitter-card {
  background: var(--bg-card, #1e2230);
  border: 1px solid var(--border-color, #2c3142);
  border-radius: 10px;
  padding: 18px 20px;
  margin-top: 14px;
}
.jitter-page .jitter-body {
  margin-top: 12px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.jitter-page .jitter-field-row {
  display: flex;
  gap: 24px;
  align-items: flex-start;
}
.jitter-page .jitter-col {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.jitter-page .jitter-note {
  font-size: 11px;
  color: var(--text-muted, #7a8294);
  line-height: 1.5;
  padding: 8px 10px;
  border-radius: 6px;
  background: var(--bg-elevated, #181b26);
}
</style>
