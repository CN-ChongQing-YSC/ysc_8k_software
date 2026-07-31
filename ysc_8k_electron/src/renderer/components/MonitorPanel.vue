<template>
  <div class="panel" style="flex: 1">
    <div class="panel-header">
      <svg class="panel-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><polyline points="1 12 4 7 7 9 10 4 15 4" /></svg>
      <span>{{ t('monitor.title') }}</span>
      <button
        class="mon-toggle"
        :class="{ active: uploadEnabled }"
        :disabled="!connected"
        @click="toggleUpload"
      >{{ uploadEnabled ? t('monitor.stop') : t('monitor.start') }}</button>
    </div>
    <div class="panel-body" style="flex: 1">
      <div class="mon-grid">
        <div class="mon-cell">
          <span class="mon-label">{{ t('monitor.buttons') }}</span>
          <span class="mon-val">{{ buttonLabels }}</span>
        </div>
        <div class="mon-cell">
          <span class="mon-label">X</span>
          <span class="mon-val">{{ x }}</span>
        </div>
        <div class="mon-cell">
          <span class="mon-label">Y</span>
          <span class="mon-val">{{ y }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed } from 'vue';
import { useI18n } from '../i18n/index.js';

const { t } = useI18n();

const props = defineProps({
  buttons: { type: Number, default: 0 },
  x: { type: Number, default: 0 },
  y: { type: Number, default: 0 },
  connected: { type: Boolean, default: false },
});

const emit = defineEmits(['toggle-upload']);

const uploadEnabled = ref(false);

function toggleUpload() {
  uploadEnabled.value = !uploadEnabled.value;
  emit('toggle-upload', uploadEnabled.value);
}

const BTN_MAP = computed(() => [
  [1, t('monitor.btn.left')],
  [2, t('monitor.btn.right')],
  [4, t('monitor.btn.middle')],
  [8, t('monitor.btn.back')],
  [16, t('monitor.btn.forward')],
]);

const buttonLabels = computed(() => {
  if (props.buttons === 0) return t('monitor.none');
  const labels = BTN_MAP.value.filter(([bit]) => props.buttons & bit).map(([, label]) => label);
  return labels.length > 0 ? labels.join('+') : String(props.buttons);
});
</script>

<style scoped>
.mon-toggle {
  margin-left: auto;
  padding: 2px 10px;
  font-size: 12px;
  border: 1px solid var(--border, #444);
  border-radius: 4px;
  background: transparent;
  color: var(--text, #ccc);
  cursor: pointer;
  transition: all 0.15s;
}
.mon-toggle:hover:not(:disabled) {
  border-color: var(--accent, #4a9);
}
.mon-toggle.active {
  background: var(--accent, #4a9);
  color: #fff;
  border-color: var(--accent, #4a9);
}
.mon-toggle:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
