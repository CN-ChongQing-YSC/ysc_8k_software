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
      <!-- 设备数据线路（固件 cmd 62/63，落盘持久化）：鼠标线路 / 手柄线路。
           两条线路显式互斥 —— 鼠标线路下不跑手柄钩子，手柄线路下不跑鼠标
           合成器；不匹配线路的数据照常透传。devMode 为 null（固件未回 cmd 62，
           一般是旧固件）时显示占位文字。 -->
      <div v-if="connected" class="field">
        <label>{{ t('serial.modeLabel') }}</label>
        <select v-if="devMode != null" class="input-select" :value="String(devMode)" @change="onModeChange">
          <option value="0">{{ t('serial.modeMouse') }}</option>
          <option value="1">{{ t('serial.modeGamepad') }}</option>
        </select>
        <input v-else class="input-text" type="text" :value="t('serial.modeUnknown')" disabled />
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
  /** 设备数据线路（固件 cmd 62 读回）：0=鼠标 1=手柄，null=未知 */
  devMode: { type: Number, default: null },
});

const emit = defineEmits(['connect', 'disconnect', 'refresh', 'switchBaud', 'setMode']);

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

function onModeChange(e) {
  const m = parseInt(e.target.value);
  if (m === 0 || m === 1) {
    emit('setMode', m);
  }
}
</script>
