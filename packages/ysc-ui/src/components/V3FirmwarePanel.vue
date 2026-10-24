<template>
  <div class="panel firmware-panel">
    <div class="panel-header">
      <svg class="panel-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <rect x="1.5" y="2" width="5.5" height="12" rx="1.2" />
        <rect x="9" y="2" width="5.5" height="12" rx="1.2" />
        <line x1="3" y1="5" x2="5.5" y2="5" />
        <line x1="3" y1="7.5" x2="5.5" y2="7.5" />
        <line x1="10.5" y1="5" x2="13" y2="5" />
        <line x1="10.5" y1="7.5" x2="13" y2="7.5" />
      </svg>
      <span>{{ t('firmwareV3.title') }}</span>
    </div>

    <div class="firmware-body">
      <div class="firmware-section">
        <div class="firmware-label">{{ t('towmcu.port') }} (YSCV3-CDC / V3)</div>
        <div class="firmware-file-row">
          <select class="input-text firmware-file-input" v-model="selectedPort" :disabled="busy">
            <option value="" disabled>{{ t('firmwareV3.noPort') }}</option>
            <option v-for="p in ports" :key="p.port" :value="p.port">
              [{{ p.side || 'V3' }}] {{ p.port }} — {{ p.serial }}
            </option>
          </select>
          <button class="btn" @click="refreshPorts" :disabled="busy">{{ t('towmcu.refresh') }}</button>
          <button class="btn" @click="queryVersion" :disabled="!canSelectPort">{{ t('towmcu.queryVersion') }}</button>
        </div>
        <div class="firmware-file-info" v-if="versionText">
          {{ t('firmwareV3.versionLabel') }}: <b>{{ versionText }}</b>
          <span class="v3-mode-pill" :class="{ 'is-iap': mode === 'IAP', 'is-app': mode === 'APP' }">{{ modeLabel }}</span>
        </div>
        <div class="v3-note">{{ t('firmwareV3.manualNote') }}</div>
      </div>

      <div class="firmware-section">
        <div class="firmware-label">{{ t('firmwareV3.file') }}</div>
        <div class="firmware-file-row">
          <input class="input-text firmware-file-input" :value="filePath" :placeholder="t('firmware.filePlaceholder')" readonly />
          <button class="btn btn-accent" @click="browse" :disabled="busy">{{ t('towmcu.browse') }}</button>
        </div>
        <div class="firmware-file-info" v-if="fileInfo">{{ fileInfo }}</div>
      </div>

      <div class="firmware-actions">
        <button class="btn" @click="enterIAP" :disabled="!canSelectPort">{{ t('towmcu.enterIAP') }}</button>
        <button class="btn btn-accent" @click="start" :disabled="!canStart">
          {{ busy ? t('firmwareV3.startPending') : t('firmwareV3.start') }}
        </button>
        <button class="btn" v-if="busy" @click="cancel">{{ t('towmcu.cancel') }}</button>
      </div>

      <div class="firmware-section" v-if="showProgress">
        <div class="firmware-label">{{ t('towmcu.progress') }}</div>
        <div class="firmware-progress-bar">
          <div class="firmware-progress-fill" :style="{ width: progressPct + '%' }"></div>
        </div>
        <div class="firmware-progress-text">{{ progressStatus }}</div>
      </div>

      <div class="firmware-section" v-if="appNotBooted">
        <div class="v3-warn">
          <div class="v3-warn-title">⚠ {{ t('firmwareV3.appNotBootedTitle') }}</div>
          <div class="v3-warn-tip">{{ t('firmwareV3.appNotBootedTip') }}</div>
          <button class="btn" @click="appNotBooted = false">{{ t('firmware.mismatchClose') }}</button>
        </div>
      </div>

      <div class="firmware-section firmware-log-section">
        <div class="firmware-label">{{ t('towmcu.logTitle') }}</div>
        <div class="firmware-log" ref="logContainer">
          <div class="firmware-log-item" v-for="(log, i) in logs" :key="i" :class="log.cls">
            <span class="firmware-log-time">[{{ log.time }}]</span>
            <span class="firmware-log-msg">{{ log.msg }}</span>
          </div>
          <div class="firmware-log-empty" v-if="!logs.length">{{ t('towmcu.logWaiting') }}</div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, nextTick, onMounted, onUnmounted } from 'vue';
import { useI18n } from '../i18n/index.js';

const api = window.driverApi;
const { t } = useI18n();

const ports = ref([]);
const selectedPort = ref('');
const versionText = ref('');
const mode = ref('');
const filePath = ref('');
const fileInfo = ref('');
const busy = ref(false);
const progressPct = ref(0);
const progressStatus = ref('');
const logs = ref([]);
const appNotBooted = ref(false);
const logContainer = ref(null);

let portTimer = null;
const cleanups = [];

const canSelectPort = computed(() => !!selectedPort.value && !busy.value);
const canStart = computed(() => !!selectedPort.value && !!filePath.value && !busy.value);
const showProgress = computed(() => busy.value || progressPct.value > 0);
const modeLabel = computed(() => {
  if (mode.value === 'IAP') return t('firmwareV3.modeIap');
  if (mode.value === 'APP') return t('firmwareV3.modeApp');
  return t('firmwareV3.modeUnknown');
});

function listen(event, handler) {
  if (!api) return;
  api.on(event, handler);
  cleanups.push(() => api.off(event, handler));
}

function addLog(msg, cls) {
  if (!msg) return;
  const now = new Date();
  const time = [now.getHours(), now.getMinutes(), now.getSeconds()]
    .map((n) => String(n).padStart(2, '0'))
    .join(':');
  logs.value.push({ time, msg, cls: cls || '' });
  if (logs.value.length > 200) logs.value.splice(0, logs.value.length - 200);
  nextTick(() => {
    if (logContainer.value) logContainer.value.scrollTop = logContainer.value.scrollHeight;
  });
}

function normalizeVersion(raw) {
  const s = String(raw || '');
  try {
    const obj = JSON.parse(s);
    if (obj && typeof obj === 'object') return String(obj.raw || obj.v || s);
  } catch {
    /* not JSON */
  }
  return s;
}

function refreshPorts() {
  if (!api) return;
  api.send('towmcu_list_ports');
}

function queryVersion() {
  if (!api || !selectedPort.value) return;
  versionText.value = '';
  api.send('towmcu_query_version', { port: selectedPort.value });
}

function enterIAP() {
  if (!api || !selectedPort.value) return;
  addLog(t('towmcu.logIAPSent'), 'info');
  api.send('towmcu_enter_iap', { port: selectedPort.value });
}

async function browse() {
  if (!api || typeof api.openV3File !== 'function') return;
  const result = await api.openV3File();
  if (!result) return;
  filePath.value = result.path;
  fileInfo.value = result.info;
  addLog(t('firmwareV3.logSelected', { info: result.info }), 'info');
}

function start() {
  if (!api || !selectedPort.value || !filePath.value) return;
  if (versionText.value && !versionText.value.includes('YSC-V3')) {
    addLog(t('firmwareV3.wrongFamily', { version: versionText.value }), 'err');
    return;
  }
  appNotBooted.value = false;
  busy.value = true;
  progressPct.value = 0;
  progressStatus.value = t('towmcu.logStart');
  addLog(t('towmcu.logStart'), 'info');
  if (portTimer) {
    clearInterval(portTimer);
    portTimer = null;
  }
  api.send('towmcu_start', { port: selectedPort.value, path: filePath.value });
}

function cancel() {
  if (!api) return;
  addLog(t('towmcu.logCancel'), 'warn');
  api.send('towmcu_cancel');
}

onMounted(() => {
  addLog(t('firmwareV3.logReady'), 'info');
  refreshPorts();
  portTimer = setInterval(refreshPorts, 1000);

  listen('towmcu_ports', (data) => {
    const list = (data.ports || []).filter(
      (p) => p.side === 'V3' || (p.serial && String(p.serial).toUpperCase().includes('YSCV3')),
    );
    ports.value = list;
    if (!selectedPort.value && list.length) selectedPort.value = list[0].port;
    if (selectedPort.value && !list.some((p) => p.port === selectedPort.value)) {
      selectedPort.value = list.length ? list[0].port : '';
    }
  });

  listen('auto_flash_file', (data) => {
    if (!data.path) return;
    filePath.value = data.path;
    fileInfo.value = data.info || '';
    addLog(t('firmwareV3.logSelected', { info: data.info || data.path }), 'info');
  });

  listen('towmcu_version', (data) => {
    if (data.port && data.port !== selectedPort.value) return;
    const display = normalizeVersion(data.version);
    versionText.value = display;
    mode.value =
      data.mode || (/IAP/i.test(display) ? 'IAP' : /APP/i.test(display) ? 'APP' : '');
  });

  listen('iap_entered', (data) => {
    if (!data.port) {
      addLog(t('firmwareV3.enterFailed'), 'err');
      return;
    }
    selectedPort.value = data.port;
    addLog(t('towmcu.iapPortAutoSelected', { port: data.port }), 'info');
    refreshPorts();
    setTimeout(queryVersion, 500);
  });

  listen('iap2_log', (data) => addLog(data.message, data.cls));
  listen('iap2_progress', (data) => {
    const total = Number(data.total || 0);
    const current = Number(data.current || 0);
    progressPct.value = total > 0 ? Math.min(100, Math.round((current / total) * 100)) : 0;
    progressStatus.value = data.status || '';
  });
  listen('iap2_done', (data) => {
    busy.value = false;
    if (data.success) {
      const display = normalizeVersion(data.version);
      versionText.value = display || 'YSC-V3-APP';
      mode.value = 'APP';
      progressPct.value = 100;
      progressStatus.value = t('firmwareV3.logDone');
      addLog(t('firmwareV3.logDone'), 'ok');
      setTimeout(queryVersion, 800);
    } else {
      const err = data.error || data.code || 'unknown';
      addLog(t('firmwareV3.logFailed', { error: err }), 'err');
      if (String(data.code || data.error || '').includes('APP_NOT_BOOTED')) {
        appNotBooted.value = true;
      }
    }
    portTimer = setInterval(refreshPorts, 1000);
  });
});

onUnmounted(() => {
  if (portTimer) clearInterval(portTimer);
  for (const off of cleanups) off();
});
</script>

<style scoped>
.v3-mode-pill {
  display: inline-block;
  margin-left: 8px;
  padding: 1px 7px;
  border-radius: 999px;
  border: 1px solid var(--border);
  color: var(--text-secondary);
  font-size: 11px;
}
.v3-mode-pill.is-iap {
  color: #ffc857;
  border-color: rgba(255, 200, 87, 0.35);
  background: rgba(255, 200, 87, 0.08);
}
.v3-mode-pill.is-app {
  color: #4ade80;
  border-color: rgba(74, 222, 128, 0.35);
  background: rgba(74, 222, 128, 0.08);
}
.v3-note {
  margin-top: 8px;
  color: var(--text-dim);
  font-size: 11px;
  line-height: 1.6;
}
.v3-warn {
  border: 1px solid rgba(255, 200, 87, 0.35);
  background: rgba(255, 200, 87, 0.06);
  border-radius: 8px;
  padding: 10px 12px;
}
.v3-warn-title {
  color: #ffc857;
  font-weight: 700;
  font-size: 12px;
  margin-bottom: 4px;
}
.v3-warn-tip {
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.6;
  margin-bottom: 8px;
}
</style>
