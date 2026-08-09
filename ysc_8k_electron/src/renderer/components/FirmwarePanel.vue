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

      <!-- 自动批量烧录：检测到新插入的 YSC 设备即用最新固件自动烧录，循环 -->
      <div class="firmware-section firmware-auto-section" :class="{ 'is-active': autoMode }">
        <div class="firmware-auto-head">
          <label class="firmware-auto-toggle">
            <input type="checkbox" v-model="autoMode" @change="onAutoToggle" />
            <span class="firmware-auto-toggle-text">{{ t('firmware.autoTitle') }}</span>
          </label>
          <span class="firmware-auto-status" :class="'auto-st-' + autoState">{{ autoStatusText }}</span>
        </div>
        <div class="firmware-auto-meta">
          <span class="firmware-auto-count">{{ t('firmware.autoCount', { n: autoCount }) }}</span>
          <button class="btn btn-tiny" @click="autoCount = 0" :disabled="autoMode">{{ t('firmware.autoReset') }}</button>
        </div>
        <div class="firmware-auto-hint" v-if="!firmwareReady && !autoMode">{{ t('firmware.autoNeedFw') }}</div>
        <div class="firmware-auto-warn" v-if="autoMode">{{ t('firmware.autoWarnManual') }}</div>
      </div>

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
              <button v-else-if="v.isInstalled" class="fw-version-cta-quiet" @click="downloadAndUpgrade(v)" :disabled="state === 'upgrading' || autoMode">{{ t('firmwareUpdate.reinstall') }}</button>
              <button v-else-if="v.isLatest" class="fw-update-cta" @click="downloadAndUpgrade(v)" :disabled="state === 'upgrading' || autoMode">{{ t('firmwareUpdate.cta') }}</button>
              <button v-else class="fw-version-cta-quiet" @click="downloadAndUpgrade(v)" :disabled="state === 'upgrading' || autoMode">{{ t('firmwareUpdate.cta') }}</button>
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

// ============ 自动批量烧录状态机 ============
// autoState: off → arming → armed → connecting → enteringIAP → flashing → cooldown → armed（循环）
const autoMode = ref(false);
const autoState = ref('off');
const autoCount = ref(0);
const firmwareReady = ref(false); // 最新固件已下载到主进程 pendingFirmware

// 模块级（非响应式）状态
let knownPorts = new Set();   // armed 时刻端口快照，用于 diff 出「新增口」
let lastPortList = [];        // 最近一次端口列表（endCooldown 重置 knownPorts 用）
let flashingPort = '';        // 当前正在连接/烧录的 COM 口
let armingVersion = '';       // arming 下载的版本号（日志用）
let cooldownTimer = null;
let enterIAPDelayTimer = null;

const autoStatusText = computed(function() {
  switch (autoState.value) {
    case 'arming':       return t('firmware.autoStArming');
    case 'armed':        return t('firmware.autoStArmed');
    case 'connecting':   return t('firmware.autoStConnecting');
    case 'enteringIAP':  return t('firmware.autoStEntering');
    case 'flashing':     return t('firmware.autoStFlashing');
    case 'cooldown':     return t('firmware.autoStCooldown');
    default:             return t('firmware.autoStOff');
  }
});

const statusText = computed(() => {
  if (state.value === 'upgrading') return t('firmware.upgrading');
  if (state.value === 'done' && progressPct.value >= 100) return t('firmware.done');
  return t('firmware.ready');
});

// 自动模式开启时禁用手动按钮，避免双路径冲突
const canStart = computed(() => state.value !== 'upgrading' && props.filePath.length > 0 && !autoMode.value);
const canEnterIAP = computed(() => state.value === 'idle' && !autoMode.value);

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

// ============ 自动批量烧录：状态机实现 ============
function clearTimer(h) { if (h) { clearTimeout(h); } return null; }
function clearAutoTimers() {
  cooldownTimer = clearTimer(cooldownTimer);
  enterIAPDelayTimer = clearTimer(enterIAPDelayTimer);
}

function onAutoToggle() {
  if (autoMode.value) enableAutoMode();
  else disableAutoMode();
}

// 开启：固件未就绪则先 arming（拉最新版+下载），就绪则直接进 armed 并建端口基线
function enableAutoMode() {
  addLog(t('firmware.autoLogStarted'), 'info');
  if (!firmwareReady.value) {
    armFirmware();
  } else {
    autoState.value = 'armed';
    api.send('enum_ports'); // 建立 knownPorts 基线
  }
}

function disableAutoMode() {
  autoState.value = 'off';
  clearAutoTimers();
  addLog(t('firmware.autoLogStopped'), 'warn');
  // 不中断正在 flashing 的设备：iap_done 会检查 autoMode=false 走手动逻辑，不再循环
}

// arming：拉版本目录 → 下载最新版到主进程内存（pendingFirmware）
function armFirmware() {
  autoState.value = 'arming';
  addLog(t('firmware.autoLogArming'), 'info');
  api.listFirmwareVersions(DEVICE_TYPE).then(function(list) {
    if (!list || !list.length) {
      autoMode.value = false;
      autoState.value = 'off';
      addLog(t('firmwareUpdate.unavailable'), 'err');
      return;
    }
    const latest = list[0];
    armingVersion = latest.version;
    addLog(t('firmware.autoLogDownloading', { ver: latest.version }), 'info');
    api.downloadFirmware(toRaw(latest)); // 成功后 firmware:download_done 的 arming 分支接手
  }).catch(function() {
    autoMode.value = false;
    autoState.value = 'off';
    addLog(t('firmwareUpdate.unavailable'), 'err');
  });
}

// 端口列表变化（ports_changed 事件驱动 + ports_list 兜底）→ diff 出新增口
function onPortsForAuto(data) {
  const list = (data && data.ports) ? data.ports : [];
  lastPortList = list;
  if (autoState.value !== 'armed') {
    knownPorts = new Set(list); // 非 armed 态：只同步快照，不触发
    return;
  }
  const added = list.filter(function(p) { return !knownPorts.has(p); });
  knownPorts = new Set(list); // 立即吸收当前列表，防重复触发
  if (added.length > 0) startAutoConnect(added[0]);
}

// 单台时序第 1 步：连接新口（baud=0 让 driver 自动探测 = 识别 YSC 设备）
function startAutoConnect(port) {
  autoState.value = 'connecting';
  flashingPort = port;
  addLog(t('firmware.autoLogDetect', { port: port }), 'info');
  api.send('serial_connect', { port: port, baud: 0 });
}

// iap_done 后冷却 5s（覆盖设备重启回 APP 的 Sleep(2000)+重连），避免重启 blip 误判为新设备
function enterCooldown() {
  autoState.value = 'cooldown';
  flashingPort = '';
  cooldownTimer = setTimeout(endCooldown, 5000);
}
function endCooldown() {
  cooldownTimer = null;
  knownPorts = new Set(lastPortList); // 用最新列表重置，刚烧完的设备被吸收为「已知」
  autoState.value = 'armed';
  addLog(t('firmware.autoLogCooldownEnd'), 'info');
  api.send('enum_ports'); // 主动刷新一次，确保 knownPorts 与实际同步
}

onMounted(function() {
  listen('iap_log', function(data) { addLog(data.message, data.cls || ''); });
  listen('iap_progress', function(data) {
    progressPct.value = Math.round((data.current / data.total) * 100);
    progressStatus.value = data.status || '';
  });
  listen('iap_done', function(data) {
    // 自动模式：成功计数+冷却，失败也冷却（避免立即重试同一台），都不弹恢复卡
    if (autoMode.value) {
      if (data.success) {
        autoCount.value++;
        addLog(t('firmware.autoLogDone', { n: autoCount.value }), 'ok');
      } else {
        addLog(t('firmware.autoLogFailed', { error: data.error || '' }), 'err');
        if (data.code === 'FIRMWARE_MISMATCH') addLog(t('firmware.autoLogMismatch'), 'err');
      }
      enterCooldown();
      return;
    }
    // 手动模式（现有逻辑）
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

  // 自动模式时序：serial_connect 成功 → iap_enter → 800ms 后 iap_start_mem
  listen('serial_connected', function() {
    if (autoState.value === 'connecting') {
      autoState.value = 'enteringIAP';
      addLog(t('firmware.autoLogConnected', { port: flashingPort }), 'info');
      api.send('iap_enter');
      enterIAPDelayTimer = setTimeout(function() {
        enterIAPDelayTimer = null;
        autoState.value = 'flashing';
        addLog(t('firmware.autoLogFlash', { port: flashingPort }), 'info');
        api.send('iap_start_mem', { deviceType: DEVICE_TYPE, baud: selectedBaud.value });
      }, 800);
    }
    // cooldown 态收到的 serial_connected（IAP Worker 烧完自动重连）忽略，不重新触发
  });
  listen('serial_error', function() {
    if (autoState.value === 'connecting') {
      // 探测失败 = 非 YSC 设备，忽略，回待命
      addLog(t('firmware.autoLogNotYsc', { port: flashingPort }), 'warn');
      flashingPort = '';
      autoState.value = 'armed';
    }
    // 其他态的 serial_error（如首页手动连接失败）忽略，不暂停自动模式。
  });
  // 不监听 serial_disconnected：烧完一台后设备会自动重连、随后被拔下换下一台，
  // 这都会触发 serial_disconnected，是批量烧录的正常环节，绝不应据此暂停。
  // （serial_disconnected 无法区分「换台拔插」与「首页手动断开」，靠它检测干扰必误伤。）

  listen('firmware:download_progress', function(data) {
    dlPercent.value = data.percent || 0;
  });
  listen('firmware:download_done', function(data) {
    if (!data) return;
    downloadingVer.value = '';
    // 自动模式前置下载（armFirmware 触发）
    if (autoState.value === 'arming') {
      if (data.success) {
        firmwareReady.value = true;
        autoState.value = 'armed';
        emit('update-file', { info: data.info || '' });
        addLog(t('firmware.autoLogArmed', { ver: armingVersion }), 'ok');
        api.send('enum_ports'); // 建立端口基线
      } else {
        autoMode.value = false;
        autoState.value = 'off';
        addLog(t('firmware.autoLogArmFailed', { error: data.error || '' }), 'err');
      }
      return;
    }
    // 手动模式（downloadAndUpgrade 触发的，autoUpgradeOnDone=true）
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

  // 端口热插拔：ports_changed（driver WM_DEVICECHANGE 推送）+ ports_list 兜底
  listen('ports_changed', onPortsForAuto);
  listen('ports_list', onPortsForAuto);

  addLog(t('firmware.logReady'));
});

onUnmounted(function() {
  clearAutoTimers();
  for (var i = 0; i < cleanups.length; i++) cleanups[i]();
});
</script>

<style scoped>
.firmware-auto-section {
  border: 1px solid var(--border-color, #2a2a2a);
  border-radius: 8px;
  padding: 10px 12px;
  background: var(--panel-bg-2, transparent);
}
.firmware-auto-section.is-active {
  border-color: var(--accent, #3a7afe);
}
.firmware-auto-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.firmware-auto-toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  user-select: none;
}
.firmware-auto-toggle input {
  width: 16px;
  height: 16px;
  cursor: pointer;
}
.firmware-auto-toggle-text {
  font-weight: 600;
}
.firmware-auto-status {
  font-size: 12px;
  padding: 2px 8px;
  border-radius: 10px;
  background: rgba(128, 128, 128, 0.18);
  color: var(--text-dim, #999);
}
.firmware-auto-status.auto-st-armed { color: #4caf50; background: rgba(76, 175, 80, 0.15); }
.firmware-auto-status.auto-st-flashing,
.firmware-auto-status.auto-st-enteringIAP,
.firmware-auto-status.auto-st-connecting { color: #ffa726; background: rgba(255, 167, 38, 0.15); }
.firmware-auto-status.auto-st-cooldown { color: #29b6f6; background: rgba(41, 182, 246, 0.15); }
.firmware-auto-status.auto-st-arming { color: #29b6f6; background: rgba(41, 182, 246, 0.15); }
.firmware-auto-meta {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 8px;
}
.firmware-auto-count {
  font-size: 13px;
  color: var(--text-dim, #bbb);
}
.btn-tiny {
  padding: 2px 10px;
  font-size: 12px;
}
.firmware-auto-hint {
  margin-top: 8px;
  font-size: 12px;
  color: var(--text-dim, #999);
}
.firmware-auto-warn {
  margin-top: 8px;
  font-size: 12px;
  color: #ffa726;
  background: rgba(255, 167, 38, 0.1);
  padding: 4px 8px;
  border-radius: 4px;
}
</style>
