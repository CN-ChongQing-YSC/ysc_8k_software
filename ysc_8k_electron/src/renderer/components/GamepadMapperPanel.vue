<template>
  <div class="panel gamepad-page">
    <div class="panel-header">
      <svg class="panel-icon" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
        <rect x="2" y="7" width="16" height="9" rx="4" />
        <circle cx="6" cy="11.5" r="1.2" />
        <circle cx="14" cy="11.5" r="1.2" />
        <line x1="9" y1="11.5" x2="11" y2="11.5" />
      </svg>
      <span>{{ t('gamepad.title') }}</span>
      <span class="header-hint">{{ t('gamepad.hint') }}</span>
      <span class="conn-state" :class="connected ? 'on' : 'off'">{{ connected ? '● ' + t('gamepad.connected') : '○ ' + t('gamepad.notConnected') }}</span>
      <div style="flex:1"></div>
      <label class="enable-toggle">
        <input type="checkbox" :checked="config.enabled" :disabled="!connected" @change="onToggleEnable($event.target.checked)" />
        <span>{{ config.enabled ? t('gamepad.enabled') : t('gamepad.disabled') }}</span>
      </label>
      <button class="btn-ghost" :disabled="!connected || loading" @click="loadFromDevice">{{ t('gamepad.refresh') }}</button>
      <button class="btn-accent" :disabled="!connected || loading" @click="saveAll">{{ t('gamepad.save') }}</button>
      <button class="btn-outline red" :disabled="!connected || loading" @click="onReset">{{ t('gamepad.reset') }}</button>
      <button class="btn-ghost" @click="exportConfig" :title="t('gamepad.exportTitle')">{{ t('gamepad.export') }}</button>
      <button class="btn-ghost" @click="importConfig" :title="t('gamepad.importTitle')">{{ t('gamepad.import') }}</button>
      <input ref="importInput" type="file" accept=".json,application/json" style="display:none" @change="onImportFile" />
    </div>

    <div v-if="toast" class="gm-toast" :class="toastType">{{ toast }}</div>

    <div class="gm-body">
      <section class="gm-card">
        <div class="gm-card-title">{{ t('gamepad.move') }}</div>
        <div class="gm-map-row">
          <span class="gm-label">{{ t('gamepad.moveMouse') }}</span>
          <input type="number" class="input-num" step="1" min="1" v-model.number="movePx" @input="onMoveChange" />
          <span>{{ t('gamepad.movePx') }}</span>
          <span class="gm-arrow">→</span>
          <span class="gm-label">{{ t('gamepad.movePct') }}</span>
          <input type="number" class="input-num" step="0.1" min="0.1" v-model.number="movePct" @input="onMoveChange" />
          <span>%</span>
          <label class="gm-field gm-stick-sel">
            <span class="gm-label">{{ t('gamepad.targetStick') }}</span>
            <select class="input-select" :value="config.move.target_stick" @change="onStick($event.target.value)">
              <option value="left">{{ t('gamepad.stickLeft') }}</option>
              <option value="right">{{ t('gamepad.stickRight') }}</option>
            </select>
          </label>
          <label class="gm-field gm-stick-sel">
            <span class="gm-label">{{ t('gamepad.times') }}</span>
            <input type="number" class="input-num sm" step="1" min="1" v-model.number="config.move.duration" @input="markDirty" />
            <span>{{ t('gamepad.packets') }}</span>
          </label>
        </div>
        <div class="gm-hint">{{ moveHint }}</div>
        <div class="gm-invert-row">
          <label class="gm-check"><input type="checkbox" v-model="config.move.invert_x" @change="markDirty" /> {{ t('gamepad.invertX') }}</label>
          <label class="gm-check"><input type="checkbox" v-model="config.move.invert_y" @change="markDirty" /> {{ t('gamepad.invertY') }}</label>
          <label class="gm-check">{{ t('gamepad.deadzone') }}
            <input type="number" class="input-num sm" min="0" max="100" step="1" v-model.number="config.move.deadzone" @input="markDirty" /> %
          </label>
        </div>
      </section>

      <section class="gm-card">
        <div class="gm-card-title-row">
          <div class="gm-card-title">{{ t('gamepad.keyRules') }}</div>
          <button class="btn-ghost sm" @click="addRule">{{ t('gamepad.addRule') }}</button>
        </div>
        <div v-if="!config.key_rules.length" class="gm-empty">{{ t('gamepad.noRules') }}</div>
        <div v-for="(r, i) in config.key_rules" :key="i" class="gm-rule">
          <select class="input-select" v-model="r.src" @change="markDirty">
            <option v-for="k in srcKeys" :key="k" :value="k">{{ k }}</option>
          </select>
          <span class="gm-arrow">→</span>
          <select class="input-select" v-model="r.target" @change="markDirty">
            <option v-for="tgt in targetKeys" :key="tgt" :value="tgt">{{ tgt.replace('mouse_', '') }}</option>
          </select>
          <label v-if="r.src === 'LT' || r.src === 'RT'" class="gm-thr">
            <span class="gm-label">{{ t('gamepad.threshold') }}</span>
            <input type="number" class="input-num sm" min="0" max="255" v-model.number="r.threshold" @input="markDirty" />
          </label>
          <select class="input-select sm" v-model="r.mode" @change="markDirty">
            <option value="augment">{{ t('gamepad.modeAugment') }}</option>
            <option value="replace">{{ t('gamepad.modeReplace') }}</option>
          </select>
          <button class="btn-outline red xs" @click="removeRule(i)">✕</button>
        </div>
      </section>

      <section class="gm-card">
        <div class="gm-card-title-row">
          <div class="gm-card-title">{{ t('gamepad.debugLog') }}</div>
          <button class="btn-ghost sm" @click="copyLog">📋 {{ t('gamepad.copyAll') }}</button>
          <button class="btn-ghost sm" @click="log = []">{{ t('gamepad.clear') }}</button>
        </div>
        <div class="gm-log">
          <div v-for="(l, i) in log" :key="i" :class="'log-line log-' + l.type">
            <span class="log-time">{{ l.time }}</span> {{ l.msg }}
          </div>
          <div v-if="!log.length" class="gm-empty">{{ t('gamepad.emptyLog') }}</div>
        </div>
      </section>
    </div>
  </div>
</template>

<script setup>
import { reactive, ref, computed, onMounted, onUnmounted } from 'vue';
import { useI18n } from '../i18n/index.js';

const { t } = useI18n();
const api = window.driverApi;

const srcKeys = ['A','B','X','Y','UP','DOWN','LEFT','RIGHT','LB','RB','LT','RT','CREATE','OPTIONS','L3','R3','PS'];
const targetKeys = ['mouse_left','mouse_right','mouse_middle','mouse_side1','mouse_side2'];

const config = reactive({
  enabled: false,
  key_rules: [],
  move: { scale: 10.0, target_stick: 'left', duration: 10, invert_x: false, invert_y: false, deadzone: 0 },
});
const dirty = ref(false);
const loading = ref(false);
const connected = ref(false);
const toast = ref('');
const toastType = ref('ok');
const log = ref([]);
let toastTimer = null;
let pending = null;   // { code, resolve, timer }

function addLog(msg, type = 'info') {
  const time = new Date().toLocaleTimeString();
  log.value.unshift({ msg, type, time });
  if (log.value.length > 80) log.value.pop();
  console.log('[GMap]', msg);
}

function markDirty() { dirty.value = true; }
/* move conversion UI: two inputs (px, pct) → scale = px/pct (pixels per 1%) */
const movePx = ref(10);
const movePct = ref(1);
const moveHint = computed(() => {
  if (!movePx.value || movePx.value <= 0) return '';
  const perPx = movePct.value / movePx.value;
  return t('gamepad.moveHint', {
    perPx: perPx.toFixed(2),
    px: movePx.value,
    pct: movePct.value,
  });
});
function onMoveChange() {
  if (movePct.value > 0 && movePx.value > 0) {
    config.move.scale = movePx.value / movePct.value;
    markDirty();
  }
}
function onStick(v) { config.move.target_stick = v; markDirty(); }
function addRule() {
  config.key_rules.push({ src: 'A', target: 'mouse_left', threshold: 128, mode: 'augment' });
  markDirty();
}
function removeRule(i) { config.key_rules.splice(i, 1); markDirty(); }

function showToast(msg, type = 'ok') {
  toast.value = msg; toastType.value = type;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { toast.value = ''; }, 3000);
}

/* ---- copy whole log to clipboard (so user can paste it back) ---- */
async function copyLog() {
  // log.value is newest-first (unshift); reverse for chronological order
  const text = log.value.slice().reverse()
    .map(l => `[${l.time}] [${l.type}] ${l.msg}`)
    .join('\n');
  try {
    await navigator.clipboard.writeText(text);
    showToast(t('gamepad.logCopied', { n: log.value.length }), 'ok');
  } catch (e) {
    // fallback hint — user can still manually select the log text
    showToast(t('gamepad.logCopyFail'), 'err');
    addLog(t('gamepad.clipboardFail', { error: (e && e.message ? e.message : e) }), 'err');
  }
}

/* ---- config validation (审查) ---- */
function validateConfig() {
  const errs = [];
  if (typeof config.enabled !== 'boolean') errs.push(t('gamepad.errEnabledBool'));
  if (!Array.isArray(config.key_rules)) errs.push(t('gamepad.errKeyRulesArray'));
  else {
    if (config.key_rules.length > 32) errs.push(t('gamepad.errTooManyRules'));
    config.key_rules.forEach((r, i) => {
      if (!r.src) errs.push(t('gamepad.errSrcEmpty', { n: i + 1 }));
      if (!r.target) errs.push(t('gamepad.errTargetEmpty', { n: i + 1 }));
      if ((r.src === 'LT' || r.src === 'RT') && (r.threshold < 0 || r.threshold > 255))
        errs.push(t('gamepad.errThreshold', { n: i + 1 }));
    });
  }
  const sc = config.move.scale;
  if (typeof sc !== 'number' || isNaN(sc) || sc < 0.1 || sc > 100)
    errs.push(t('gamepad.errScale', { value: sc }));
  if (!['left','right'].includes(config.move.target_stick))
    errs.push(t('gamepad.errStick'));
  const dz = config.move.deadzone;
  if (typeof dz !== 'number' || isNaN(dz) || dz < 0 || dz > 100)
    errs.push(t('gamepad.errDeadzone', { value: dz }));
  return errs;
}

function buildCfgJson() {
  return {
    magic: 'GMAP', version: 1, enabled: !!config.enabled,
    key_rules: config.key_rules.map(r => {
      const o = { src: r.src, target: r.target, mode: r.mode };
      if (r.src === 'LT' || r.src === 'RT') o.threshold = r.threshold;
      return o;
    }),
    move: { scale: config.move.scale, target_stick: config.move.target_stick, duration: config.move.duration, invert_x: !!config.move.invert_x, invert_y: !!config.move.invert_y, deadzone: Number(config.move.deadzone) || 0 },
  };
}

function applyCfg(cfg) {
  if (!cfg || typeof cfg !== 'object') throw new Error('cfg not object');
  config.enabled = !!cfg.enabled;
  config.key_rules = (Array.isArray(cfg.key_rules) ? cfg.key_rules : []).map(r => ({
    src: r.src, target: r.target,
    threshold: r.threshold != null ? r.threshold : 128,
    mode: r.mode || 'augment',
  }));
  if (cfg.move) {
    config.move.scale = typeof cfg.move.scale === 'number' ? cfg.move.scale : 10.0;
    config.move.target_stick = cfg.move.target_stick || 'left';
    config.move.duration = cfg.move.duration || 10;
    config.move.invert_x = !!cfg.move.invert_x;
    config.move.invert_y = !!cfg.move.invert_y;
    config.move.deadzone = (typeof cfg.move.deadzone === 'number') ? cfg.move.deadzone : 0;
    // sync the px/pct inputs from scale (default pct=1, px=scale)
    movePct.value = 1;
    movePx.value = Math.max(1, Math.round(config.move.scale));
  }
  dirty.value = false;
}

/* ---- connection check ---- */
async function checkConn() {
  try { connected.value = await api.isConnected(); }
  catch (e) { connected.value = false; }
  if (!connected.value) {
    addLog(t('gamepad.serialNotConnected'), 'err');
    showToast(t('gamepad.serialNotConnectedToast'), 'err');
    return false;
  }
  return true;
}

/* ---- send + wait for debug_response ---- */
function _sendAndWait(type, params, expectCode, timeoutMs = 3000) {
  return new Promise((resolve) => {
    pending = {
      code: expectCode,
      resolve,
      timer: setTimeout(() => {
        addLog(t('gamepad.timeout', { type: type, code: expectCode }), 'err');
        pending = null;
        resolve(null);
      }, timeoutMs),
    };
    const paramStr = JSON.stringify(params);
    addLog(`TX ${type} ${paramStr.length > 120 ? paramStr.slice(0, 120) + '...' : paramStr}`, 'tx');
    api.send(type, params);
  });
}
// reassign onDebugResponse to use pending.resolve
function _onDebugResponse(resp) {
  if (!resp || typeof resp.code !== 'number') return;
  if (resp.code < 100 || resp.code > 104) return;
  const dataPreview = resp.data ? (resp.data.length > 100 ? resp.data.slice(0, 100) + '...' : resp.data) : '(null)';
  addLog(`RX code=${resp.code} msg=${resp.message || '(null)'} data=${dataPreview}`,
         (resp.message && resp.message.indexOf('ok') === 0) ? 'rx' : 'err');
  if (pending && pending.code === resp.code) {
    clearTimeout(pending.timer);
    const p = pending; pending = null;
    p.resolve(resp);
  } else if (resp.code === 101) {
    try { applyCfg(JSON.parse(resp.data)); addLog(t('gamepad.configFilled'), 'ok'); }
    catch (e) { addLog(t('gamepad.fillParseFail', { error: e.message }), 'err'); }
  }
}

/* driver send_result listener — confirms the driver actually forwarded the
 * command to the serial port (vs. rejected it: not connected / empty data).
 * send_result arrives almost instantly; firmware debug_response follows. */
function _onSendResult(data) {
  const ok = data && data.ok;
  const cmd = data && data.cmd;
  const err = data && data.error;
  addLog(`driver send_result: ok=${ok} cmd=${cmd||''} ${err?'error='+err:''}`,
         ok ? 'rx' : 'err');
}

/* ---- actions ---- */
async function saveAll() {
  if (loading.value) return;
  if (!await checkConn()) return;
  const errs = validateConfig();
  if (errs.length) {
    addLog(t('gamepad.validateFail', { errors: errs.join(' | ') }), 'err');
    showToast(t('gamepad.configInvalid', { error: errs[0] }), 'err');
    return;
  }
  loading.value = true;
  const cfg = buildCfgJson();
  addLog(t('gamepad.validatePass', { cfg: JSON.stringify(cfg).slice(0, 140) }));
  const resp = await _sendAndWait('send_ysc', { cmd: JSON.stringify({cmd:100, data: cfg}) }, 100);
  loading.value = false;
  if (resp && resp.message && resp.message.indexOf('ok') === 0) {
    dirty.value = false;
    showToast(t('gamepad.saved'), 'ok');
    addLog(t('gamepad.saveOk', { msg: resp.message }), 'ok');
  } else {
    const reason = resp ? (resp.message || t('gamepad.fwRejected')) : t('gamepad.noResponse');
    showToast(t('gamepad.saveFail', { reason: reason }), 'err');
    addLog(t('gamepad.saveFail', { reason: reason }), 'err');
  }
}

async function loadFromDevice() {
  if (loading.value) return;
  if (!await checkConn()) return;
  loading.value = true;
  addLog(t('gamepad.readingConfig'));
  const resp = await _sendAndWait('send_ysc', { cmd: '{"cmd":101}' }, 101);
  loading.value = false;
  if (resp && resp.message === 'ok' && resp.data) {
    try { applyCfg(JSON.parse(resp.data)); showToast(t('gamepad.refreshed'), 'ok'); addLog(t('gamepad.readOk'), 'ok'); }
    catch (e) { showToast(t('gamepad.parseFail'), 'err'); addLog(t('gamepad.parseFailDetail', { error: e.message }), 'err'); }
  } else {
    const reason = resp ? (resp.message || t('gamepad.fwRejected')) : t('gamepad.noResponse');
    showToast(t('gamepad.readFail', { reason: reason }), 'err');
    addLog(t('gamepad.readFail', { reason: reason }), 'err');
  }
}

function onToggleEnable(on) {
  config.enabled = !!on;
  markDirty();
}

async function onReset() {
  if (loading.value) return;
  if (!await checkConn()) return;
  loading.value = true;
  addLog(t('gamepad.resetting'));
  const resp = await _sendAndWait('send_ysc', { cmd: '{"cmd":104}' }, 104);
  loading.value = false;
  if (resp && resp.message === 'ok') {
    showToast(t('gamepad.resetDone'), 'ok');
    addLog(t('gamepad.resetDoneReload'), 'ok');
    setTimeout(loadFromDevice, 400);
  } else {
    showToast(t('gamepad.resetFail'), 'err');
    addLog(t('gamepad.resetFail'), 'err');
  }
}

/* ---- export / import (避免刷新固件丢失配置) ---- */
function exportConfig() {
  const cfg = buildCfgJson();
  const text = JSON.stringify(cfg, null, 2);
  const blob = new Blob([text], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = 'gamepad_config.json';
  a.click();
  URL.revokeObjectURL(url);
  showToast(t('gamepad.configExported'), 'ok');
  addLog(t('gamepad.configExporting', { text: text.slice(0, 100) }), 'ok');
}

function importConfig() {
  importInput.value?.click();
}

function onImportFile(event) {
  const file = event.target.files?.[0];
  if (!file) return;
  const reader = new FileReader();
  reader.onload = (e) => {
    try {
      const cfg = JSON.parse(e.target.result);
      applyCfg(cfg);
      showToast(t('gamepad.configImported'), 'ok');
      addLog(t('gamepad.importOk'), 'ok');
    } catch (err) {
      showToast(t('gamepad.importFail', { error: err.message }), 'err');
      addLog(t('gamepad.importFail', { error: err.message }), 'err');
    }
  };
  reader.readAsText(file);
  event.target.value = '';  /* reset so same file can be re-selected */
}

const importInput = ref(null);

onMounted(async () => {
  api.on('debug_response', _onDebugResponse);
  api.on('send_result', _onSendResult);
  addLog(t('gamepad.panelLoaded'), 'info');
  await checkConn();
  if (connected.value) {
    addLog(t('gamepad.autoLoading'), 'info');
    loadFromDevice();
  }
});
onUnmounted(() => {
  api.off('debug_response', _onDebugResponse);
  api.off('send_result', _onSendResult);
});
</script>

<style scoped>
.gamepad-page { display: flex; flex-direction: column; height: 100%; }
.header-hint { margin-left: 10px; opacity: .6; font-size: 13px; }
/* 窄窗口/英文长文案下标题栏换行，避免标题与按钮重叠 */
.gamepad-page .panel-header { align-content: center; }
.gamepad-page .panel-header > span:first-of-type { white-space: nowrap; }
.gamepad-page .header-hint {
  flex: 1 1 220px;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.conn-state { font-size: 12px; padding: 2px 8px; border-radius: 4px; }
.gamepad-page .conn-state { margin-left: 4px; white-space: nowrap; }
.conn-state.on { background: rgba(80,200,120,.15); color: #50c878; }
.conn-state.off { background: rgba(220,80,80,.15); color: #dc5050; }
.enable-toggle { display: flex; align-items: center; gap: 6px; margin-right: 8px; cursor: pointer; font-size: 13px; }
.enable-toggle input { width: 16px; height: 16px; }
.gm-toast { margin: 8px 16px; padding: 8px 14px; border-radius: 6px; font-size: 13px; }
.gm-toast.ok  { background: rgba(80,200,120,.15); color: #50c878; }
.gm-toast.err { background: rgba(220,80,80,.15); color: #dc5050; }
.gm-body { flex: 1; overflow-y: auto; padding: 8px 16px 16px; display: flex; flex-direction: column; gap: 14px; }
.gm-card { background: rgba(255,255,255,.03); border: 1px solid rgba(255,255,255,.08); border-radius: 8px; padding: 14px; }
.gm-card-title { font-weight: 600; font-size: 14px; margin-bottom: 10px; }
.gm-card-title-row { display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px; }
.gm-card-title-row .gm-card-title { margin-bottom: 0; }
.gm-row { display: flex; gap: 18px; flex-wrap: wrap; }
.gm-map-row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.gm-map-row .input-num { width: 70px; }
.gm-stick-sel { margin-left: 18px; }
.gm-invert-row { margin-top: 8px; display: flex; gap: 18px; }
.gm-check { display: flex; align-items: center; gap: 4px; font-size: 12px; cursor: pointer; }
.gm-check input { width: 14px; height: 14px; }
.gm-field { display: flex; flex-direction: column; gap: 4px; font-size: 13px; }
.gm-label { opacity: .75; }
.gm-hint { opacity: .5; font-size: 11px; }
.gm-empty { opacity: .5; font-size: 13px; padding: 8px 0; }
.gm-rule { display: flex; align-items: center; gap: 8px; padding: 6px 0; flex-wrap: wrap; }
.gm-arrow { opacity: .5; }
.gm-thr { display: flex; align-items: center; gap: 4px; font-size: 12px; }
.input-num { width: 90px; padding: 4px 8px; background: rgba(0,0,0,.25); border: 1px solid rgba(255,255,255,.1); border-radius: 4px; color: inherit; }
.input-num.sm { width: 70px; }
.input-select { padding: 4px 8px; background: rgba(0,0,0,.25); border: 1px solid rgba(255,255,255,.1); border-radius: 4px; color: inherit; }
.input-select.sm { padding: 4px 6px; }
.btn-ghost, .btn-accent, .btn-outline { padding: 6px 14px; border-radius: 6px; font-size: 13px; cursor: pointer; border: 1px solid transparent; }
.btn-ghost { background: transparent; border-color: rgba(255,255,255,.15); color: inherit; }
.btn-ghost:hover { background: rgba(255,255,255,.05); }
.btn-accent { background: #50c878; color: #000; }
.btn-accent:disabled { opacity: .4; cursor: not-allowed; }
.btn-outline { background: transparent; border-color: rgba(255,255,255,.15); color: inherit; }
.btn-outline.red { color: #dc5050; border-color: rgba(220,80,80,.4); }
.btn-outline.red:hover { background: rgba(220,80,80,.1); }
.btn-ghost.sm { padding: 4px 10px; font-size: 12px; }
.btn-outline.red.xs { padding: 2px 8px; font-size: 12px; }
.gm-log { max-height: 200px; overflow-y: auto; font-family: 'Consolas','Menlo',monospace; font-size: 11px; line-height: 1.5; user-select: text; -webkit-user-select: text; cursor: text; }
.log-line { padding: 1px 0; white-space: pre-wrap; word-break: break-all; }
.log-time { opacity: .5; margin-right: 6px; }
.log-tx { color: #6aa6ff; }
.log-rx { color: #50c878; }
.log-ok  { color: #50c878; }
.log-err { color: #dc5050; }
.log-info{ opacity: .8; }
</style>
