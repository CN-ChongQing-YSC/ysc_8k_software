<template>
  <div class="panel sn-panel">
    <div class="panel-header">
      <svg class="panel-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <rect x="3" y="2.5" width="10" height="11" rx="1.5" />
        <path d="M6 6h4" />
        <path d="M6 9h1.5" />
        <path d="M6 11.5h2.5" />
        <path d="M10.5 9.5l-1 1 1 1" />
      </svg>
      <span>{{ t('serialNumber.title') }}</span>
      <div class="macro-header-hint">{{ t('serialNumber.hint') }}</div>
      <div style="flex:1" />
      <button class="btn btn-ghost sn-refresh-btn" :disabled="loading" @click="$emit('refresh')">
        <span v-if="loading" class="sn-spinner" />
        <span>{{ loading ? t('serialNumber.refreshing') : t('serialNumber.refresh') }}</span>
      </button>
    </div>
    <div v-if="toast" class="macro-toast" :class="toastType">{{ toast }}</div>

    <div class="panel-body" :class="{ 'sn-loading': loading }">
      <div class="sn-info-row">
        <div class="sn-info-item">
          <span class="sn-info-label">{{ t('serialNumber.current') }}</span>
          <span class="sn-info-value" :class="{ overridden: !!info.active }">{{ info.active || t('serialNumber.none') }}</span>
        </div>
        <div class="sn-info-item">
          <span class="sn-info-label">{{ t('serialNumber.physical') }}</span>
          <span class="sn-info-value">{{ info.physical || t('serialNumber.none') }}</span>
        </div>
      </div>

      <div class="sn-input-row">
        <label class="sn-input-label">{{ t('serialNumber.customLabel') }}</label>
        <input
          v-model="snInput"
          class="input-text sn-input"
          type="text"
          maxlength="30"
          spellcheck="false"
          :placeholder="t('serialNumber.inputPlaceholder')"
          :disabled="busy || !connected"
          @keyup.enter="onWrite"
        />
        <button class="btn btn-accent" :disabled="busy || !connected" @click="onWrite">
          {{ busy ? t('serialNumber.writePending') : t('serialNumber.write') }}
        </button>
        <button class="btn btn-outline red" :disabled="busy || !connected || !info.custom" @click="$emit('clear')">
          {{ t('serialNumber.clear') }}
        </button>
      </div>
      <div class="sn-hint">{{ t('serialNumber.customHint') }}</div>
    </div>
  </div>
</template>

<script setup>
import { ref, watch } from 'vue';
import { useI18n } from '../i18n/index.js';

const { t } = useI18n();

const props = defineProps({
  connected: Boolean,
  loading: { type: Boolean, default: false },
  info: { type: Object, default: () => ({ custom: '', physical: '', active: '' }) },
});

const emit = defineEmits(['write', 'clear', 'refresh']);

const snInput = ref('');
const busy = ref(false);
const toast = ref('');
const toastType = ref('ok');
let toastTimer = null;

// 设备回读 custom 后同步输入框（未自定义时留空）
watch(() => props.info.custom, (v) => {
  snInput.value = v || '';
}, { immediate: true });

function showToast(msg, type) {
  toast.value = msg;
  toastType.value = type || 'ok';
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(function() { toast.value = ''; }, 3000);
}

function onWrite() {
  const s = snInput.value.trim();
  if (!props.connected) {
    showToast(t('serialNumber.notConnected'), 'err');
    return;
  }
  if (!s || s.length > 30 || !/^[\x20-\x7e]+$/.test(s)) {
    showToast(t('serialNumber.invalid'), 'err');
    return;
  }
  busy.value = true;
  emit('write', s);
  showToast(t('serialNumber.writeSaved'), 'ok');
  setTimeout(function() { busy.value = false; }, 3500);
}
</script>

<style scoped>
.sn-panel .sn-refresh-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.sn-panel .sn-spinner {
  width: 12px;
  height: 12px;
  border: 2px solid rgba(255, 255, 255, 0.25);
  border-top-color: currentColor;
  border-radius: 50%;
  display: inline-block;
  flex: none;
  animation: sn-rotate 0.8s linear infinite;
}
@keyframes sn-rotate {
  to { transform: rotate(360deg); }
}
.sn-panel .sn-loading .sn-info-value {
  opacity: 0.45;
  transition: opacity 0.2s;
}
.sn-panel .sn-info-row {
  display: flex;
  gap: 24px;
  flex-wrap: wrap;
}
.sn-panel .sn-info-item {
  display: flex;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
}
.sn-panel .sn-info-label {
  font-size: 12px;
  color: var(--text-muted, #7a8294);
  flex: none;
}
.sn-panel .sn-info-value {
  font-size: 13px;
  color: var(--text, #e6e9f0);
  font-family: Consolas, Menlo, monospace;
  word-break: break-all;
}
.sn-panel .sn-info-value.overridden {
  color: var(--accent, #4f8cff);
}
.sn-panel .sn-input-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 12px;
  flex-wrap: wrap;
}
.sn-panel .sn-input-label {
  font-size: 12px;
  color: var(--text-muted, #7a8294);
  flex: none;
}
.sn-panel .sn-input {
  flex: 1;
  min-width: 180px;
  max-width: 360px;
  font-family: Consolas, Menlo, monospace;
}
.sn-panel .sn-hint {
  font-size: 11px;
  color: var(--text-muted, #7a8294);
  line-height: 1.5;
  margin-top: 8px;
}
</style>
