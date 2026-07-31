<template>
  <div class="panel">
    <div class="panel-header">
      <svg class="panel-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M7 4l-4 4 4 4" /><path d="M12 4l-4 4 4 4" /><line x1="3" y1="8" x2="8" y2="8" /></svg>
      <span>{{ t('serial.title') }}</span>
    </div>
    <div class="panel-body">
      <div class="field">
        <label>{{ t('serial.port') }}</label>
        <input v-if="connected" class="input-text" type="text" :value="connectedPort" disabled />
        <select v-else class="input-select" v-model="selectedPort" @focus="refreshPorts" @click="refreshPorts">
          <option value="" disabled>{{ t('serial.portPlaceholder') }}</option>
          <option v-for="p in ports" :key="p" :value="p">{{ p }}</option>
        </select>
      </div>
      <div class="field">
        <label>{{ t('serial.baud') }}</label>
        <input class="input-text" type="text" :value="connected ? baud : baudInput" :disabled="connected" :placeholder="t('serial.baudHint')" />
      </div>
      <div class="btn-row">
        <button class="btn btn-accent" :disabled="connected || !selectedPort" @click="$emit('connect', selectedPort, parseInt(baudInput) || 0)">
          {{ t('serial.connect') }}
        </button>
        <button class="btn btn-outline red" :disabled="!connected" @click="$emit('disconnect')">
          {{ t('serial.disconnect') }}
        </button>
        <button class="btn btn-ghost" @click="$emit('refresh')" :disabled="connected">
          {{ t('serial.refresh') }}
        </button>
      </div>
      <div v-if="connected" class="field">
        <label>{{ t('serial.switch') }}</label>
        <select class="input-select" @change="onBaudSwitch">
          <option value="">{{ t('serial.switchPlaceholder') }}</option>
          <option v-for="b in baudRates" :key="b" :value="b" :selected="b === baud">{{ b }}</option>
        </select>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref } from 'vue';
import { useI18n } from '../i18n/index.js';

const { t } = useI18n();

const props = defineProps({
  connected: Boolean,
  connectedPort: { type: String, default: '' },
  ports: { type: Array, default: () => [] },
  baud: { type: Number, default: 0 },
});

const emit = defineEmits(['connect', 'disconnect', 'refresh', 'switchBaud']);

const selectedPort = ref('');
const baudInput = ref('0');

const baudRates = [115200, 230400, 460800, 921600, 1000000, 1500000, 2000000, 3000000, 4000000];

let lastRefreshAt = 0;
function refreshPorts() {
  if (props.connected) return;
  const now = Date.now();
  if (now - lastRefreshAt < 800) return;
  lastRefreshAt = now;
  emit('refresh');
}

function onBaudSwitch(e) {
  const val = parseInt(e.target.value);
  if (val > 0) {
    emit('switchBaud', val);
    e.target.value = '';
  }
}
</script>
