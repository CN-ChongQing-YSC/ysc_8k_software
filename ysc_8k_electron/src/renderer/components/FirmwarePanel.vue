<template>
  <div class="panel firmware-panel">
    <div class="panel-header">
      <svg class="panel-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <rect x="3" y="1" width="10" height="14" rx="2" />
        <line x1="6" y1="5" x2="10" y2="5" />
        <line x1="6" y1="8" x2="10" y2="8" />
        <circle cx="8" cy="12" r="1" />
      </svg>
      <span>{{ t('firmware.title') }}</span>
    </div>
    <div class="firmware-body">
      <FirmwareStatusLed />

      <!-- Version list. Checking fetches the admin-listed catalog (metadata only);
           each row's CTA downloads that version + flashes it. -->
      <div class="firmware-section" v-if="listState === 'checking'">
        <div class="fw-version-list-head"><span class="fw-update-spinner"></span>{{ t('firmwareUpdate.checking') }}</div>
      </div>
      <div class="firmware-section" v-else-if="listState === 'error'">
        <div class="fw-version-list-head"><span class="fw-list-empty">{{ errorMsg || t('firmwareUpdate.unavailable') }}</span></div>
      </div>
      <div class="firmware-section" v-else-if="listState === 'ready' && versions.length">
        <div class="fw-version-list">
          <div class="fw-version-list-head">
            <span class="fw-list-title">{{ t('firmwareUpdate.availableTitle') }}</span>
            <span class="fw-list-count">{{ versions.length }} {{ t('firmwareUpdate.versionsUnit') }}</span>
          </div>
          <div v-for="v in versions" :key="v.version" class="fw-version-row"
               :class="{ 'is-latest': v.isLatest, 'is-installed': v.isInstalled, 'is-downloading': downloadingVer === v.version, 'is-quiet': !v.isLatest && !v.isInstalled }">
            <div class="fw-version-ident">
              <span class="fw-version-num">v{{ v.version }}</span>
              <div class="fw-version-badges">
                <span v-if="v.isLatest" class="fw-version-badge latest">{{ t('firmwareUpdate.badgeLatest') }}</span>
                <span v-if="v.isInstalled" class="fw-version-badge installed">{{ t('firmwareUpdate.badgeInstalled') }}</span>
              </div>
            </div>
            <div class="fw-version-body">
              <div class="fw-version-meta">
                <span>{{ (v.size / 1024).toFixed(1) }} KB</span>
                <span class="fw-meta-sep">·</span>
                <span v-if="v.releaseDate">{{ t('firmwareUpdate.released', { date: v.releaseDate }) }}</span>
              </div>
              <div class="fw-version-notes" v-if="v.releaseNotes">{{ v.releaseNotes }}</div>
            </div>
            <div class="fw-version-action">
              <div class="fw-update-progress" v-if="downloadingVer === v.version">
                <div class="fw-update-progress-bar">
                  <div class="fw-update-progress-fill" :style="{ width: dlPercent + '%' }"></div>
                </div>
                <div class="fw-update-progress-text">{{ dlPercent }}%</div>
              </div>
              <button v-else-if="v.isInstalled" class="fw-version-cta-quiet" @click="downloadAndUpgrade(v)" :disabled="state === 'upgrading'">{{ t('firmwareUpdate.reinstall') }}</button>
              <button v-else-if="v.isLatest" class="fw-update-cta" @click="downloadAndUpgrade(v)" :disabled="state === 'upgrading'">{{ t('firmwareUpdate.cta') }}</button>
              <button v-else class="fw-version-cta-quiet" @click="downloadAndUpgrade(v)" :disabled="state === 'upgrading'">{{ t('firmwareUpdate.cta') }}</button>
            </div>
          </div>
        </div>
      </div>

      <!-- Manual fallback: 本地 .bin 文件选择（与在线下载并列的独立入口） -->
      <div class="firmware-section">
        <div class="firmware-label">{{ t('firmware.fileLabel') }}</div>
        <div class="firmware-file-row">
          <input class="input-text firmware-file-input" :value="filePath" :placeholder="t('firmware.filePlaceholder')" readonly />
          <button class="btn btn-accent" @click="browseFile" :disabled="state === 'upgrading'">{{ t('firmware.browse') }}</button>
        </div>
        <div class="firmware-file-info" v-if="fileInfo">{{ fileInfo }}</div>
      </div>

      <!-- 下载波特率：默认 1.5M（兼容旧行为），允许用户保持当前或选其他值 -->
      <div class="firmware-section">
        <div class="firmware-label">{{ t('firmware.baudLabel') }}</div>
        <div class="firmware-file-row">
          <select class="input-text firmware-file-input" v-model.number="selectedBaud" :disabled="state === 'upgrading'">
            <option :value="0">{{ t('firmware.baudKeep') }}</option>
            <option v-for="b in baudOptions" :key="b" :value="b">{{ b.toLocaleString() }}</option>
          </select>
        </div>
      </div>

      <div class="firmware-actions">
        <button class="btn" @click="checkUpdate" :disabled="state === 'upgrading' || listState === 'checking'">
          {{ listState === 'checking' ? t('firmwareUpdate.checking') : t('firmwareUpdate.checkBtn') }}
        </button>
        <button class="btn btn-accent" @click="startUpgrade()" :disabled="!canStart">
          {{ state === 'upgrading' ? t('firmware.startPending') : t('firmware.start') }}
        </button>
        <button class="btn" @click="enterIAP" :disabled="!canEnterIAP">{{ t('firmware.enterIAP') }}</button>
        <button class="btn" v-if="state === 'upgrading'" @click="cancelUpgrade">{{ t('firmware.cancel') }}</button>
      </div>

      <div class="firmware-section" v-if="showProgress">
        <div class="firmware-label">{{ t('firmware.progress') }}</div>
        <div class="firmware-progress-bar">
          <div class="firmware-progress-fill" :style="{ width: progressPct + '%' }"></div>
        </div>
        <div class="firmware-progress-text">{{ progressStatus }}</div>
      </div>

      <!-- 校验失败（固件不匹配）恢复提示：断电再上电后重新更新 -->
      <div class="firmware-section" v-if="mismatchShown">
        <div class="fw-mismatch-card">
          <div class="fw-mismatch-title">⚠ {{ t('firmware.mismatchTitle') }}</div>
          <div class="fw-mismatch-tip">{{ t('firmware.mismatchTip') }}</div>
          <ol class="fw-mismatch-steps">
            <li>{{ t('firmware.mismatchStep1') }}</li>
            <li>{{ t('firmware.mismatchStep2') }}</li>
            <li>{{ t('firmware.mismatchStep3') }}</li>
          </ol>
          <div class="fw-mismatch-actions">
            <button class="btn btn-accent" @click="retryAfterMismatch">{{ t('firmware.mismatchRetry') }}</button>
            <button class="btn" @click="mismatchShown = false">{{ t('firmware.mismatchClose') }}</button>
          </div>
        </div>
      </div>

      <div class="firmware-section firmware-log-section">
        <div class="firmware-label">{{ t('firmware.logTitle') }}</div>
        <div class="firmware-log" ref="logContainer">
          <div class="firmware-log-item" v-for="(log, i) in logs" :key="i" :class="log.cls">
            <span class="firmware-log-time">[{{ log.time }}]</span>
            <span class="firmware-log-msg">{{ log.msg }}</span>
          </div>
          <div class="firmware-log-empty" v-if="!logs.length">{{ t('firmware.logWaiting') }}</div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, nextTick, onMounted, onUnmounted, toRaw } from 'vue';
import { useI18n } from '../i18n/index.js';
import FirmwareStatusLed from './FirmwareStatusLed.vue';

const api = window.driverApi;
const { t } = useI18n();

const props = defineProps({
  filePath: { type: String, default: '' },
  fileInfo: { type: String, default: '' },
});
const emit = defineEmits(['update-file']);

const state = ref('idle'); // idle | upgrading | done
const progressPct = ref(0);
const progressStatus = ref('');
const logs = ref([]);
const logContainer = ref(null);

// 校验失败（固件不匹配）恢复提示：true 时显示「断电再上电」卡片
const mismatchShown = ref(false);
let lastStartWasMem = false; // 重新更新时复用同一条入口（本地文件 / 在线内存）

// Version-list state. listState idle = list hidden.
const listState = ref('idle');   // idle | checking | ready | error
const versions = ref([]);        // ListedVersion[]
const errorMsg = ref('');
const downloadingVer = ref('');  // version string currently downloading
const dlPercent = ref(0);

const DEVICE_TYPE = 'ysc_v2_8k_mouse';
let autoUpgradeOnDone = false;
let pendingVersion = '';

// 下载波特率。0 = 保持当前探测到的波特率不切换；默认 1.5M 兼容旧行为。
const baudOptions = [115200, 230400, 460800, 921600, 1000000, 1500000, 2000000, 3000000, 4000000];
const selectedBaud = ref(1500000);

const showProgress = computed(() => state.value === 'upgrading' || progressPct.value > 0);

const statusText = computed(() => {
  if (state.value === 'upgrading') return t('firmware.upgrading');
  if (state.value === 'done' && progressPct.value >= 100) return t('firmware.done');
  return t('firmware.ready');
});

const canStart = computed(() => state.value !== 'upgrading' && props.filePath.length > 0);
const canEnterIAP = computed(() => state.value === 'idle');

const cleanups = [];
function listen(event, handler) {
  api.on(event, handler);
  cleanups.push(function() { api.off(event, handler); });
}
function addLog(msg, cls) {
  const now = new Date();
  const time = [now.getHours(), now.getMinutes(), now.getSeconds()]
    .map(n => String(n).padStart(2, '0')).join(':');
  logs.value.push({ time, msg, cls: cls || '' });
  if (logs.value.length > 200) logs.value.splice(0, logs.value.length - 200);
  nextTick(() => {
    if (logContainer.value) logContainer.value.scrollTop = logContainer.value.scrollHeight;
  });
}

function browseFile() {
  if (!api || !api.openIAPFile) return;
  api.openIAPFile().then(function(result) {
    if (!result) return;
    autoUpgradeOnDone = false;
    pendingVersion = '';
    emit('update-file', { path: result.path, info: result.info });
    addLog(t('firmware.logSelected', { info: result.info }), 'info');
  });
}

// Fetch the listed-version catalog — METADATA ONLY, no download.
// Backend returns version-desc; index 0 is the latest.
function checkUpdate() {
  if (!api || !api.listFirmwareVersions) return;
  if (listState.value === 'checking') return;
  listState.value = 'checking';
  errorMsg.value = '';
  addLog(t('firmwareUpdate.checking'), 'info');
  api.listFirmwareVersions(DEVICE_TYPE).then(function(list) {
    versions.value = list || [];
    if (!versions.value.length) {
      listState.value = 'error';
      errorMsg.value = t('firmwareUpdate.unavailable');
      addLog(t('firmwareUpdate.unavailable'), 'warn');
      return;
    }
    listState.value = 'ready';
    const latest = versions.value[0];
    addLog(t('firmwareUpdate.foundNew') + ' v' + latest.version + ' (' + versions.value.length + ' ' + t('firmwareUpdate.versionsUnit') + ')', 'info');
  }).catch(function() {
    listState.value = 'error';
    errorMsg.value = t('firmwareUpdate.unavailable');
    addLog(t('firmwareUpdate.unavailable'), 'err');
  });
}

// CTA for one row: download that version to cache, then flash it.
function downloadAndUpgrade(ver) {
  if (!api || !api.downloadFirmware) return;
  if (state.value === 'upgrading') return;
  autoUpgradeOnDone = true;
  pendingVersion = ver.version;
  downloadingVer.value = ver.version;
  dlPercent.value = 0;
  addLog(t('firmwareUpdate.downloading') + ' v' + ver.version, 'info');
  api.downloadFirmware(toRaw(ver));
}

function enterIAP() {
  addLog(t('firmware.logEnterIAP'), 'info');
  api.send('iap_enter');
}

function startUpgrade(overridePath) {
  const path = overridePath || props.filePath;
  if (!path) return;
  mismatchShown.value = false;
  lastStartWasMem = false;
  state.value = 'upgrading';
  progressPct.value = 0;
  progressStatus.value = t('firmware.logStart');
  addLog(t('firmware.logStart'), 'info');
  api.send('iap_start', { path, baud: selectedBaud.value });
}

// 在线下载路径：固件已暂存在主进程内存中（pendingFirmware），通过
// iap_start_mem 把 deviceType 传过去，main 进程会 Base64 编码后发给 C++。
function startUpgradeMem(deviceType) {
  mismatchShown.value = false;
  lastStartWasMem = true;
  state.value = 'upgrading';
  progressPct.value = 0;
  progressStatus.value = t('firmware.logStart');
  addLog(t('firmware.logStart'), 'info');
  api.send('iap_start_mem', { deviceType, baud: selectedBaud.value });
}

// 校验失败（固件不匹配）后：用户已按提示「断电再上电」，点此重新更新。
function retryAfterMismatch() {
  mismatchShown.value = false;
  if (lastStartWasMem) startUpgradeMem(DEVICE_TYPE);
  else startUpgrade();
}

function cancelUpgrade() {
  addLog(t('firmware.logCancel'), 'warn');
  api.send('iap_cancel');
}

// Refresh the version list (after a successful install, badges update).
function refreshList() {
  if (listState.value === 'ready' || listState.value === 'error') checkUpdate();
}

onMounted(function() {
  listen('iap_log', function(data) { addLog(data.message, data.cls || ''); });
  listen('iap_progress', function(data) {
    progressPct.value = Math.round((data.current / data.total) * 100);
    progressStatus.value = data.status || '';
  });
  listen('iap_done', function(data) {
    if (data.success) {
      mismatchShown.value = false;
      state.value = 'done';
      progressPct.value = 100;
      progressStatus.value = t('firmware.done');
      addLog(t('firmware.logDone'), 'ok');
      if (pendingVersion && api.recordFirmwareInstalled) {
        api.recordFirmwareInstalled(DEVICE_TYPE, pendingVersion);
      }
      pendingVersion = '';
      // Refresh badges: the installed row flips to the one we just flashed.
      const wasReady = listState.value === 'ready';
      refreshList();
      if (!wasReady) listState.value = 'idle';
    } else {
      state.value = 'idle';
      addLog(t('firmware.logFailed', { error: data.error || '' }), 'err');
      // 校验失败（固件不匹配）：弹出「断电再上电」恢复提示卡
      if (data.code === 'FIRMWARE_MISMATCH') mismatchShown.value = true;
    }
  });

  listen('firmware:download_progress', function(data) {
    dlPercent.value = data.percent || 0;
  });
  listen('firmware:download_done', function(data) {
    if (!data) return;
    downloadingVer.value = '';
    if (data.success) {
      addLog(t('firmwareUpdate.downloadDone'), 'ok');
      // 固件已暂存在主进程内存中（无 path），只更新显示信息
      emit('update-file', { info: data.info || '' });
      if (autoUpgradeOnDone) {
        autoUpgradeOnDone = false;
        startUpgradeMem(DEVICE_TYPE);
      }
    } else {
      addLog(t('firmwareUpdate.downloadFailed', { error: data.error || '' }), 'err');
      autoUpgradeOnDone = false;
      pendingVersion = '';
    }
  });

  addLog(t('firmware.logReady'));
});

onUnmounted(function() {
  for (var i = 0; i < cleanups.length; i++) cleanups[i]();
});
</script>
