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
      <span>{{ t('towmcu.title') }}</span>
    </div>
    <div class="firmware-body">
      <FirmwareStatusLed />

      <!-- CDC port picker -->
      <div class="firmware-section">
        <div class="firmware-label">{{ t('towmcu.port') }} (VID 1A86 / PID FE0C)</div>
        <div class="firmware-file-row">
          <select class="input-text firmware-file-input" v-model="selectedPort" :disabled="state === 'upgrading'">
            <option value="" disabled>{{ t('towmcu.noPort') }}</option>
            <option v-for="p in ports" :key="p.port" :value="p.port">
              [{{ p.side || '?' }}] {{ p.port }} — {{ p.serial }}
            </option>
          </select>
          <button class="btn" @click="refreshPorts" :disabled="state === 'upgrading'">{{ t('towmcu.refresh') }}</button>
          <button class="btn" @click="queryVersion" :disabled="!canSelectPort">{{ t('towmcu.queryVersion') }}</button>
        </div>
        <div class="firmware-file-info" v-if="versionText">{{ versionText }}</div>
      </div>

      <!-- Per-side version lists. Checking fetches the listed catalog for both
           sides; each row's CTA downloads + flashes that side. -->
      <template v-for="side in ['left','right']" :key="side">
        <div class="firmware-section" v-if="list[side].state !== 'idle'">
          <div class="fw-version-list" v-if="list[side].state === 'ready' && list[side].versions.length">
            <div class="fw-version-list-head">
              <span class="fw-list-title">{{ sideName(side) }}</span>
              <span class="fw-list-count">{{ list[side].versions.length }} {{ t('firmwareUpdate.versionsUnit') }}</span>
            </div>
            <div v-for="v in list[side].versions" :key="v.version" class="fw-version-row"
                 :class="{ 'is-latest': v.isLatest, 'is-installed': v.isInstalled, 'is-downloading': list[side].downloadingVer === v.version, 'is-quiet': !v.isLatest && !v.isInstalled }">
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
                <div class="fw-update-progress" v-if="list[side].downloadingVer === v.version">
                  <div class="fw-update-progress-bar">
                    <div class="fw-update-progress-fill" :style="{ width: list[side].percent + '%' }"></div>
                  </div>
                  <div class="fw-update-progress-text">{{ list[side].percent }}%</div>
                </div>
                <button v-else-if="v.isInstalled" class="fw-version-cta-quiet" @click="downloadAndUpgrade(side, v)" :disabled="state === 'upgrading' || !canSelectPort">{{ t('firmwareUpdate.reinstall') }}</button>
                <button v-else-if="v.isLatest" class="fw-update-cta" @click="downloadAndUpgrade(side, v)" :disabled="state === 'upgrading' || !canSelectPort">{{ t('firmwareUpdate.cta') }}</button>
                <button v-else class="fw-version-cta-quiet" @click="downloadAndUpgrade(side, v)" :disabled="state === 'upgrading' || !canSelectPort">{{ t('firmwareUpdate.cta') }}</button>
              </div>
            </div>
          </div>
          <div class="fw-version-list-head" v-else-if="list[side].state === 'checking'">
            <span class="fw-update-spinner"></span>{{ sideName(side) }} · {{ t('firmwareUpdate.checking') }}
          </div>
          <div class="fw-version-list-head" v-else-if="list[side].state === 'error'">
            <span class="fw-list-empty">{{ sideName(side) }} · {{ t('firmwareUpdate.unavailable') }}</span>
          </div>
        </div>
      </template>

      <!-- Manual fallback file rows -->
      <div class="firmware-section">
        <div class="firmware-label">{{ t('towmcu.fileLeft') }}</div>
        <div class="firmware-file-row">
          <input class="input-text firmware-file-input" :value="leftPath" :placeholder="t('firmware.filePlaceholder')" readonly />
          <button class="btn btn-accent" @click="browse('left')" :disabled="state === 'upgrading'">{{ t('towmcu.browse') }}</button>
        </div>
        <div class="firmware-file-info" v-if="leftInfo">{{ leftInfo }}</div>
      </div>
      <div class="firmware-section">
        <div class="firmware-label">{{ t('towmcu.fileRight') }}</div>
        <div class="firmware-file-row">
          <input class="input-text firmware-file-input" :value="rightPath" :placeholder="t('firmware.filePlaceholder')" readonly />
          <button class="btn btn-accent" @click="browse('right')" :disabled="state === 'upgrading'">{{ t('towmcu.browse') }}</button>
        </div>
        <div class="firmware-file-info" v-if="rightInfo">{{ rightInfo }}</div>
      </div>

      <div class="firmware-actions">
        <button class="btn" @click="checkUpdate" :disabled="state === 'upgrading' || checking">{{ checking ? t('firmwareUpdate.checking') : t('firmwareUpdate.checkBtn') }}</button>
        <button class="btn" @click="enterIAP" :disabled="!canSelectPort || state === 'upgrading'">{{ t('towmcu.enterIAP') }}</button>
        <button class="btn btn-accent" @click="startUpgrade('left')" :disabled="!canStartLeft">
          {{ upgradingSide === 'left' ? t('firmware.startPending') : t('towmcu.upgradeLeft') }}
        </button>
        <button class="btn btn-accent" @click="startUpgrade('right')" :disabled="!canStartRight">
          {{ upgradingSide === 'right' ? t('firmware.startPending') : t('towmcu.upgradeRight') }}
        </button>
        <button class="btn" v-if="state === 'upgrading'" @click="cancelUpgrade">{{ t('towmcu.cancel') }}</button>
      </div>

      <div class="firmware-section" v-if="showProgress">
        <div class="firmware-label">{{ t('towmcu.progress') }}</div>
        <div class="firmware-progress-bar">
          <div class="firmware-progress-fill" :style="{ width: progressPct + '%' }"></div>
        </div>
        <div class="firmware-progress-text">{{ progressStatus }}</div>
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
import { ref, reactive, computed, nextTick, onMounted, onUnmounted, toRaw } from 'vue';
import { useI18n } from '../i18n/index.js';
import FirmwareStatusLed from './FirmwareStatusLed.vue';

const api = window.driverApi;
const { t } = useI18n();

const ports = ref([]);
const selectedPort = ref('');
const awaitingNewPort = ref(false);
let portPollTimer = null;
let iapWaitTimer = null;
const versionText = ref('');
const leftPath = ref('');
const leftInfo = ref('');
const rightPath = ref('');
const rightInfo = ref('');

const state = ref('idle');
const upgradingSide = ref('');
const progressPct = ref(0);
const progressStatus = ref('');
const logs = ref([]);
const logContainer = ref(null);
const checking = ref(false);

const DEV_TYPE = { left: 'ysc_towmcu_left', right: 'ysc_towmcu_right' };

// Per-side version-list state.
const list = reactive({
  left:  { state: 'idle', versions: [], downloadingVer: '', percent: 0 },
  right: { state: 'idle', versions: [], downloadingVer: '', percent: 0 },
});
const autoUpgrade = reactive({ left: false, right: false });
const pendingVersion = reactive({ left: '', right: '' });

const showProgress = computed(() => state.value === 'upgrading' || progressPct.value > 0);
const canSelectPort = computed(() => !!selectedPort.value && state.value !== 'upgrading');
const canStartLeft = computed(() => state.value !== 'upgrading' && !!selectedPort.value && !!leftPath.value);
const canStartRight = computed(() => state.value !== 'upgrading' && !!selectedPort.value && !!rightPath.value);

function sideName(side) { return side === 'left' ? t('towmcu.fileLeft') : t('towmcu.fileRight'); }
function sideOf(deviceType) {
  if (deviceType === DEV_TYPE.left) return 'left';
  if (deviceType === DEV_TYPE.right) return 'right';
  return null;
}

const cleanups = [];
function listen(event, handler) {
  api.on(event, handler);
  cleanups.push(function () { api.off(event, handler); });
}
function addLog(msg, cls) {
  const now = new Date();
  const time = [now.getHours(), now.getMinutes(), now.getSeconds()]
    .map(n => String(n).padStart(2, '0')).join(':');
  logs.value.push({ time, msg, cls: cls || '' });
  if (logs.value.length > 200) logs.value.splice(0, logs.value.length - 200);
  nextTick(() => { if (logContainer.value) logContainer.value.scrollTop = logContainer.value.scrollHeight; });
}

function refreshPorts() { api.send('towmcu_list_ports'); }
function startPortPolling() {
  if (portPollTimer) return;
  portPollTimer = setInterval(refreshPorts, 1000);
}
function stopPortPolling() {
  if (portPollTimer) { clearInterval(portPollTimer); portPollTimer = null; }
}
function queryVersion() {
  versionText.value = '';
  if (selectedPort.value) api.send('towmcu_query_version', { port: selectedPort.value });
}
function enterIAP() {
  if (!selectedPort.value) return;
  awaitingNewPort.value = true;
  addLog(t('towmcu.logIAPSent'), 'info');
  api.send('towmcu_enter_iap', { port: selectedPort.value });
  if (iapWaitTimer) clearTimeout(iapWaitTimer);
  // C++ EnterIAP 内部 Sleep 1500 + FindIAPPort 最长 10s；正常 3s 内会推
  // iap_entered 事件自动选中 IAP 端口。60s 兜底覆盖 USB 重新枚举的边界情况。
  // 不再依赖 baseline 端口字符串比对 —— USB 转串口 CDC 在 IAP/APP 切换后 COM 号
  // 经常不变，字符串比对会永远找不到"新"端口，触发 30s 超时（用户体感断连）。
  iapWaitTimer = setTimeout(function () {
    if (awaitingNewPort.value) {
      awaitingNewPort.value = false;
      addLog(t('towmcu.iapPortWaitTimeout'), 'warn');
    }
    iapWaitTimer = null;
  }, 60000);
}
function startUpgrade(side) {
  const path = side === 'left' ? leftPath.value : rightPath.value;
  if (!path || !selectedPort.value) return;
  state.value = 'upgrading';
  upgradingSide.value = side;
  progressPct.value = 0;
  progressStatus.value = t('towmcu.logStart');
  addLog(t('towmcu.logStart'), 'info');
  // 升级期间停止 1 秒一次的端口轮询 —— 每次 towmcu_list_ports 会触发 C++ 端
  // SetupAPI 全枚举，可能让 Windows USB 栈返回脏状态、COM 号短暂消失，
  // 进而触发 selectedPort 被清空（towmcu_ports 监听器），UI 体感"断连"。
  // C++ 端 main.cpp 同步加了 IsRunning() 守卫，这里是双保险。
  stopPortPolling();
  api.send('towmcu_start', { port: selectedPort.value, path });
}
// 在线下载路径：固件已暂存在主进程内存中（pendingFirmware），通过
// towmcu_start_mem 把 deviceType 传过去，main 进程会 Base64 编码后发给 C++。
function startUpgradeMem(side) {
  if (!selectedPort.value) return;
  state.value = 'upgrading';
  upgradingSide.value = side;
  progressPct.value = 0;
  progressStatus.value = t('towmcu.logStart');
  addLog(t('towmcu.logStart'), 'info');
  stopPortPolling();
  api.send('towmcu_start_mem', { port: selectedPort.value, deviceType: DEV_TYPE[side] });
}
function cancelUpgrade() {
  addLog(t('towmcu.logCancel'), 'warn');
  api.send('towmcu_cancel');
}
function browse(side) {
  if (!api || !api.openTowmcuFile) return;
  api.openTowmcuFile(side).then(function (result) {
    if (!result) return;
    autoUpgrade[side] = false;
    pendingVersion[side] = '';
    if (side === 'left') { leftPath.value = result.path; leftInfo.value = result.info; }
    else { rightPath.value = result.path; rightInfo.value = result.info; }
    addLog(t('towmcu.logSelected', { side: sideName(side), info: result.info }), 'info');
  });
}

// Fetch the listed-version catalog for BOTH sides — METADATA ONLY.
function checkUpdate() {
  if (!api || !api.listFirmwareVersions || checking.value) return;
  checking.value = true;
  list.left.state = 'checking';
  list.right.state = 'checking';
  addLog(t('firmwareUpdate.checking'), 'info');
  Promise.all([
    api.listFirmwareVersions(DEV_TYPE.left).catch(() => []),
    api.listFirmwareVersions(DEV_TYPE.right).catch(() => []),
  ]).then(function (results) {
    checking.value = false;
    [['left', results[0]], ['right', results[1]]].forEach(function (entry) {
      const side = entry[0], vers = entry[1] || [];
      list[side].versions = vers;
      if (!vers.length) {
        list[side].state = 'error';
        addLog(sideName(side) + ': ' + t('firmwareUpdate.unavailable'), 'warn');
      } else {
        list[side].state = 'ready';
        addLog(sideName(side) + ': v' + vers[0].version + ' (' + vers.length + ' ' + t('firmwareUpdate.versionsUnit') + ')', 'info');
      }
    });
  });
}

// Row CTA: download this version to cache, then flash that side.
function downloadAndUpgrade(side, ver) {
  if (!api || !api.downloadFirmware) return;
  if (state.value === 'upgrading' || !selectedPort.value) return;
  autoUpgrade[side] = true;
  pendingVersion[side] = ver.version;
  list[side].downloadingVer = ver.version;
  list[side].percent = 0;
  addLog(sideName(side) + ': ' + t('firmwareUpdate.downloading') + ' v' + ver.version, 'info');
  api.downloadFirmware(toRaw(ver));
}

function refreshSide(side) {
  if (list[side].state !== 'ready' && list[side].state !== 'error') return;
  api.listFirmwareVersions(DEV_TYPE[side]).then(function (vers) {
    list[side].versions = vers || [];
    list[side].state = (vers && vers.length) ? 'ready' : 'error';
  }).catch(() => {});
}

onMounted(function () {
  listen('towmcu_ports', function (data) {
    const newList = (data && data.ports) || [];
    const newSet = {};
    for (let i = 0; i < newList.length; i++) newSet[newList[i].port] = true;

    // 升级中不要因为端口短暂消失就清空 selectedPort —— C++ 端 IsRunning() 时
    // 已直接返回空列表，这里加守卫防止任何残留的非空推文扰动 UI。
    if (state.value !== 'upgrading' && selectedPort.value && !newSet[selectedPort.value]) {
      selectedPort.value = '';
    }

    // 等待进入 IAP：主路径是 iap_entered 事件（C++ 找到端口后直接推）。
    // 这里作为兼容兜底 —— 如果 towmcu_ports 推来的列表里有 side==='IAP'
    // 的端口（USB iSerialNumber == TOWMCUIAP），直接选中。基于 iSerialNumber
    // 的判断不受 COM 号复用影响（USB 转串口 CDC 切换 IAP/APP 后 COM 号常不变）。
    if (awaitingNewPort.value) {
      let iapPort = null;
      for (let i = 0; i < newList.length; i++) {
        if (newList[i].side === 'IAP') { iapPort = newList[i]; break; }
      }
      if (iapPort) {
        selectedPort.value = iapPort.port;
        awaitingNewPort.value = false;
        if (iapWaitTimer) { clearTimeout(iapWaitTimer); iapWaitTimer = null; }
        addLog(t('towmcu.iapPortAutoSelected', { port: iapPort.port }), 'ok');
      }
    }

    // 兜底：还没选且有端口 → 选第一个
    if (!selectedPort.value && newList.length) {
      selectedPort.value = newList[0].port;
    }

    ports.value = newList;
    if (!newList.length && !awaitingNewPort.value) addLog(t('towmcu.noPort'), 'warn');
  });
  listen('towmcu_version', function (data) {
    if (!data) return;
    versionText.value = '[' + data.mode + '] ' + data.version;
  });
  // C++ EnterIAP 找到 IAP 端口后推此事件。port 为空表示进入失败。
  // 前端基于此自动选中 IAP 端口，不再依赖 1 秒端口轮询 + 字符串比对。
  listen('iap_entered', function (data) {
    if (!data) return;
    if (iapWaitTimer) { clearTimeout(iapWaitTimer); iapWaitTimer = null; }
    awaitingNewPort.value = false;
    if (data.port) {
      selectedPort.value = data.port;
      addLog(t('towmcu.iapPortAutoSelected', { port: data.port }), 'ok');
    }
  });
  listen('iap2_log', function (data) { if (data) addLog(data.message, data.cls || ''); });
  listen('iap2_progress', function (data) {
    if (!data) return;
    progressPct.value = Math.round((data.current / data.total) * 100);
    progressStatus.value = data.status || '';
  });
  listen('iap2_done', function (data) {
    const side = upgradingSide.value;
    if (data && data.success) {
      state.value = 'done';
      progressPct.value = 100;
      progressStatus.value = t('towmcu.logDoneShort');
      addLog(t('towmcu.logDone'), 'ok');
      if (api.recordFirmwareInstalled && pendingVersion[side]) {
        api.recordFirmwareInstalled(DEV_TYPE[side], pendingVersion[side]);
        pendingVersion[side] = '';
      }
      autoUpgrade[side] = false;
      refreshSide(side);           // badges update: installed flips to this version
      refreshPorts();
    } else {
      state.value = 'idle';
      addLog(t('towmcu.logFailed', { error: (data && data.error) || '' }), 'err');
    }
    upgradingSide.value = '';
    // 升级结束，恢复端口轮询（startUpgrade/startUpgradeMem 时停了）
    startPortPolling();
  });

  listen('firmware:download_progress', function (data) {
    if (!data) return;
    const side = sideOf(data.deviceType);
    if (side) list[side].percent = data.percent || 0;
  });
  listen('firmware:download_done', function (data) {
    if (!data) return;
    const side = sideOf(data.deviceType);
    if (!side) return;
    list[side].downloadingVer = '';
    if (data.success) {
      addLog(sideName(side) + ': ' + t('firmwareUpdate.downloadDone'), 'ok');
      // 固件已暂存在主进程内存中（无 path），只更新显示信息
      if (side === 'left') { leftInfo.value = data.info || ''; }
      else { rightInfo.value = data.info || ''; }
      if (autoUpgrade[side]) {
        autoUpgrade[side] = false;
        startUpgradeMem(side);
      }
    } else {
      addLog(sideName(side) + ': ' + t('firmwareUpdate.downloadFailed', { error: data.error || '' }), 'err');
      autoUpgrade[side] = false;
      pendingVersion[side] = '';
    }
  });

  addLog(t('towmcu.logReady'));
  refreshPorts();
  startPortPolling();
});

onUnmounted(function () {
  stopPortPolling();
  if (iapWaitTimer) { clearTimeout(iapWaitTimer); iapWaitTimer = null; }
  for (var i = 0; i < cleanups.length; i++) cleanups[i]();
});
</script>
