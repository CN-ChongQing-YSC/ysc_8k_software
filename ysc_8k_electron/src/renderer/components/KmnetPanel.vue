<template>
  <div class="panel">
    <div class="panel-header">
      <svg class="panel-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="8" r="6" /><ellipse cx="8" cy="8" rx="2.5" ry="6" /><line x1="2" y1="8" x2="14" y2="8" /></svg>
      <span>{{ t('kmnet.title') }}</span>
    </div>
    <div class="panel-body">
      <div class="field">
        <label>{{ t('kmnet.port') }}</label>
        <input class="input-text" type="text" v-model.number="portInput" :disabled="running" style="width: 80px" />
      </div>
      <div class="btn-row">
        <button
          class="btn btn-accent"
          :disabled="running || selfCheck.active || !serialConnected"
          :title="!serialConnected ? t('kmnet.warnTip') : ''"
          @click="$emit('start', portInput)"
        >
          {{ t('kmnet.start') }}
        </button>
        <button class="btn btn-outline red" :disabled="!running && !selfCheck.active" @click="$emit('stop')">
          {{ t('kmnet.stop') }}
        </button>
      </div>
      <div v-if="!serialConnected && !running" class="kmnet-warn">
        {{ t('kmnet.warn') }}
      </div>
      <div v-if="running && ip" class="kmnet-info">
        <div class="field">
          <label>{{ t('kmnet.addr') }}</label>
          <span class="kmnet-addr">{{ ip }}:{{ port }}</span>
        </div>
        <div class="field" v-if="mac">
          <label>{{ t('kmnet.mac') }}</label>
          <span class="kmnet-addr">{{ mac }}</span>
        </div>
        <div class="kmnet-hint">{{ t('kmnet.hint', { port: port }) }}</div>
      </div>
      <div
        v-if="selfCheck.active || selfCheck.result"
        class="kmnet-selfcheck"
        :class="selfCheckClass"
      >
        <div class="kmnet-selfcheck-title">
          <span v-if="selfCheck.active" class="kmnet-spinner" />
          <span v-else-if="selfCheck.result === 'success'" class="kmnet-icon-ok">✓</span>
          <span v-else-if="selfCheck.result === 'failed'" class="kmnet-icon-err">!</span>
          <span>{{ selfCheckTitle }}</span>
        </div>
        <div v-if="selfCheck.message" class="kmnet-selfcheck-msg">{{ selfCheck.message }}</div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed } from 'vue';
import { useI18n } from '../i18n/index.js';

const { t } = useI18n();

const props = defineProps({
  running: Boolean,
  ip: { type: String, default: '' },
  mac: { type: String, default: '' },
  port: { type: Number, default: 0 },
  selfCheck: {
    type: Object,
    default: function() {
      return { active: false, step: '', message: '', result: '' };
    },
  },
  serialConnected: { type: Boolean, default: false },
});

defineEmits(['start', 'stop']);

const portInput = ref(5251);

const stepLabelMap = computed(() => ({
  switching: t('kmnet.selfCheck.switching'),
  disconnecting: t('kmnet.selfCheck.disconnecting'),
  reconnecting: t('kmnet.selfCheck.reconnecting'),
}));

const selfCheckTitle = computed(function() {
  if (props.selfCheck.active) {
    return t('kmnet.selfCheck.running', { step: stepLabelMap.value[props.selfCheck.step] || props.selfCheck.step });
  }
  if (props.selfCheck.result === 'success') return t('kmnet.selfCheck.success');
  if (props.selfCheck.result === 'failed') return t('kmnet.selfCheck.failed');
  return '';
});

const selfCheckClass = computed(function() {
  if (props.selfCheck.active) return 'is-active';
  if (props.selfCheck.result === 'success') return 'is-success';
  if (props.selfCheck.result === 'failed') return 'is-failed';
  return '';
});
</script>
