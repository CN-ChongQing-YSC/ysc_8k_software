<template>
  <div class="panel debug-panel">
    <div class="panel-header">
      <svg class="panel-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="7" cy="7" r="4.5" />
        <line x1="10.5" y1="10.5" x2="14" y2="14" />
        <line x1="5" y1="7" x2="9" y2="7" />
      </svg>
      <span>{{ t('debug.title') }}</span>
    </div>

    <div class="panel-body">
      <!-- 状态条 -->
      <div class="status-bar">
        <div class="status-left">
          <span class="dot" :class="status.connected ? 'on' : 'off'"></span>
          <span class="status-text">
            {{ status.connected ? t('debug.deviceConnected') : t('debug.deviceDisconnected') }}
          </span>
          <span v-if="status.connected && status.vid" class="vid-pid">
            VID:{{ hexVid }} PID:{{ hexPid }}
          </span>
          <span v-if="status.connected && status.product" class="product">
            {{ status.product }}
          </span>
        </div>
        <div class="status-right">
          <span v-if="status.debug" class="badge debug-on">{{ t('debug.badgeDebug') }}</span>
          <span v-else class="badge debug-off">{{ t('debug.badgePass') }}</span>
        </div>
      </div>

      <!-- 未连接提示 -->
      <div v-if="!serialConnected" class="hint-card">
        {{ t('debug.hintSerial') }}
      </div>
      <div v-else-if="!status.connected" class="hint-card">
        {{ t('debug.hintPlug') }}
      </div>

      <template v-else>
        <!-- 引导卡片 -->
        <div class="guide-card" :class="step">
          <div class="guide-step">
            <span class="step-num active">1</span>
            <div class="step-body">
              <div class="step-title">{{ t('debug.step1Title') }}</div>
              <div class="step-desc">
                {{ status.connected ? t('debug.step1DescOk') : t('debug.step1DescNo') }}
              </div>
            </div>
          </div>
          <div class="guide-step">
            <span class="step-num" :class="{ active: status.connected }">2</span>
            <div class="step-body">
              <div class="step-title">{{ t('debug.step2Title') }}</div>
              <div class="step-desc">{{ t('debug.step2Desc') }}</div>
            </div>
          </div>
          <div class="guide-step">
            <span class="step-num" :class="{ active: status.debug }">3</span>
            <div class="step-body">
              <div class="step-title">{{ t('debug.step3Title') }}</div>
              <div class="step-desc">{{ t('debug.step3Desc') }}</div>
            </div>
          </div>
          <div class="guide-step">
            <span class="step-num" :class="{ active: status.reportCount > 0 }">4</span>
            <div class="step-body">
              <div class="step-title">{{ t('debug.step4Title') }}</div>
              <div class="step-desc">{{ t('debug.step4Desc') }}</div>
            </div>
          </div>
        </div>

        <!-- 操作按钮组 -->
        <div class="actions">
          <button
            class="btn primary"
            :disabled="!status.connected || status.debug || busy !== null"
            @click="enterDebug"
          >
            {{ busy === 'enter' ? t('debug.entering') : t('debug.enter') }}
          </button>
          <button
            class="btn danger"
            :disabled="!status.debug || busy !== null"
            @click="exitDebug"
          >
            {{ busy === 'exit' ? t('debug.exiting') : t('debug.exit') }}
          </button>
        </div>

        <!-- 捕获进度 -->
        <div class="capture-card" v-if="status.debug || status.reportCount > 0">
          <div class="capture-head">
            <span class="capture-title">{{ t('debug.capture') }}</span>
            <span class="capture-count">{{ status.reportCount }} / {{ status.reportMax }}</span>
          </div>
          <div class="capture-bar">
            <div
              class="capture-fill"
              :class="{ full: status.reportCount >= status.reportMax }"
              :style="{ width: capturePercent + '%' }"
            ></div>
          </div>
          <div class="capture-hint" v-if="status.debug && status.reportCount < status.reportMax">
            {{ t('debug.captureHint') }}
          </div>
          <div class="capture-hint done" v-else-if="status.reportCount >= status.reportMax">
            {{ t('debug.captureDone', { max: status.reportMax }) }}
          </div>
        </div>

        <!-- 按键链路诊断：定位"物理按键不上报"这类问题 -->
        <div class="diag-card" v-if="status.debug || status.reportCount > 0">
          <div class="diag-head">
            <span class="diag-title">{{ t('debug.diagTitle') }}</span>
            <span class="diag-mode">{{ modeLabel }}</span>
          </div>
          <div class="diag-desc">{{ t('debug.diagDesc') }}</div>

          <div class="diag-modes">
            <button
              v-for="m in modeOptions"
              :key="m.v"
              class="chip"
              :class="{ on: mode === m.v }"
              :disabled="busy !== null || catching"
              @click="setMode(m.v)"
            >{{ m.label }}</button>
          </div>

          <div class="diag-actions">
            <button class="btn primary" :disabled="busy !== null || catching" @click="grabButtons">
              {{ catching ? t('debug.grabRunning') : t('debug.grabBtn') }}
            </button>
            <button class="btn" :disabled="busy !== null" @click="refreshDiag">{{ t('debug.diagRead') }}</button>
          </div>

          <div class="capture-hint" v-if="catchHint">{{ catchHint }}</div>

          <div class="verdict" :class="verdict.level" v-if="verdict">
            <div class="verdict-text">{{ verdict.text }}</div>
            <div class="verdict-detail">{{ verdict.detail }}</div>
          </div>

          <div class="stat-grid" v-if="diag">
            <div class="stat" v-for="s in statCells" :key="s.k">
              <span class="stat-k">{{ s.k }}</span>
              <span class="stat-v" :class="{ zero: !s.v, bad: s.bad && s.v > 0 }">{{ s.v }}</span>
            </div>
          </div>

          <div class="lay-line" v-if="diag && diag.lay">
            {{ t('debug.layLabel') }}
            len={{ diag.lay.len }} · rid={{ diag.lay.hasRid ? diag.lay.rid : '-' }} ·
            iface={{ diag.lay.iface }} · ep={{ diag.lay.ep }} ·
            btn(off={{ diag.lay.btnOff }},{{ diag.lay.btnBits }}bit) ·
            x({{ diag.lay.xOff }},{{ diag.lay.xBits }}) · y({{ diag.lay.yOff }},{{ diag.lay.yBits }}) ·
            wh({{ diag.lay.whOff }},{{ diag.lay.whBits }})
          </div>

          <div class="evt-box" v-if="events.length">
            <div class="evt-row evt-head">
              <span class="c-i">#</span>
              <span class="c-t">{{ t('debug.colTime') }}</span>
              <span class="c-d">{{ t('debug.colDir') }}</span>
              <span class="c-e">EP</span>
              <span class="c-b">{{ t('debug.colBtn') }}</span>
              <span class="c-c">{{ t('debug.colCls') }}</span>
              <span class="c-h">RAW</span>
            </div>
            <div v-for="e in events" :key="e.i" class="evt-row" :class="dirClass(e.d)">
              <span class="c-i">{{ e.i }}</span>
              <span class="c-t">{{ fmtT(e.t) }}</span>
              <span class="c-d">{{ dirLabel(e.d) }}</span>
              <span class="c-e">{{ e.e }}</span>
              <span class="c-b">{{ btnText(e.b) }}</span>
              <span class="c-c">{{ clsText(e) }}</span>
              <span class="c-h">{{ hexPretty(e.h) }}</span>
            </div>
          </div>
        </div>

        <!-- 导出按钮 -->
        <div class="export-row" v-if="status.reportCount > 0">
          <button
            class="btn export"
            :class="{ highlight: status.reportCount >= status.reportMax && busy === null }"
            :disabled="busy !== null"
            @click="exportTxt"
          >
            {{ busy === 'export' ? t('debug.exporting', { n: exportProgress, total: status.reportCount }) : t('debug.export') }}
          </button>
          <button class="btn" :disabled="busy !== null" @click="clearReports">{{ t('debug.clear') }}</button>
        </div>

        <!-- 日志 -->
        <details class="log-details">
          <summary>
            <span>{{ t('debug.logs') }}</span>
            <span class="log-count">{{ logs.length }}</span>
          </summary>
          <div class="log-box">
            <div class="log-empty" v-if="logs.length === 0">{{ t('debug.logsEmpty') }}</div>
            <div v-for="(l, i) in logs" :key="i" class="log-row" :class="l.type">
              <span class="log-time">{{ l.time }}</span>
              <span class="log-msg">{{ l.message }}</span>
            </div>
          </div>
        </details>
      </template>
    </div>
  </div>
</template>

<script setup>
import { ref, reactive, computed, watch, onMounted, onUnmounted } from 'vue';
import { useI18n, getLang } from '../i18n/index.js';

const api = window.driverApi;
const { t } = useI18n();
const locale = () => (getLang() === 'zh' ? 'zh-CN' : 'en-US');

const props = defineProps({
  serialConnected: { type: Boolean, default: false },
});

const status = reactive({
  debug: false,
  connected: false,
  devDescrLen: 0,
  cfgDescrLen: 0,
  repDescrLen: 0,
  reportCount: 0,
  reportMax: 100,
  vid: 0,
  pid: 0,
  product: '',
});
const descriptors = ref({ device: [], config: [], report: [] });
const busy = ref(null);
const exportProgress = ref(0);
const logs = ref([]);
let pollTimer = null;

/* ---------- 按键链路诊断状态 ---------- */
const diag = ref(null);        // 固件 717 返回的计数器 + 鼠标布局快照
const events = ref([]);        // 固件 718 分页取回的记录（含方向/按键/分类）
const mode = ref(1);           // 抓包模式：0=全部 1=仅变化 2=仅按键轨迹
const catching = ref(false);
const catchHint = ref('');
let catchTimer = null;

const MODES = [
  { v: 2, key: 'modeBtn' },
  { v: 1, key: 'modeChanged' },
  { v: 0, key: 'modeAll' },
];

/* 分类码（dir=0 上游）/ 原因码（dir=2 事件）—— 与固件 DebugCapture.h 对齐 */
const CLS_LABELS = {
  0: 'debug.clsMouse', 1: 'debug.clsRid', 2: 'debug.clsIface', 3: 'debug.clsLayout',
  4: 'debug.clsKbd', 5: 'debug.clsRaw', 6: 'debug.clsOther',
  7: 'debug.clsGamepad', 8: 'debug.clsMouseRaw',
};
const NOTE_LABELS = {
  1: 'debug.noteEpBusy', 2: 'debug.noteNoContent', 3: 'debug.noteMasked',
  4: 'debug.noteDropRid', 5: 'debug.noteDropIface', 6: 'debug.noteDropLayout',
  7: 'debug.clsKbd', 8: 'debug.noteSendFail', 9: 'debug.noteQueueDrop',
};

/* ---------- helpers ---------- */
const addLog = (type, message) => {
  const time = new Date().toLocaleTimeString(locale());
  logs.value.unshift({ type, time, message });
  if (logs.value.length > 80) logs.value = logs.value.slice(0, 80);
};

const hexToBytes = (hex) => {
  if (!hex) return [];
  // Firmware returns hex with no separators (e.g. "12010002..."). Strip any
  // whitespace and chunk by 2 chars so we tolerate both formats.
  const clean = String(hex).replace(/\s+/g, '');
  const out = [];
  for (let i = 0; i + 1 < clean.length; i += 2) {
    out.push(parseInt(clean.substr(i, 2), 16));
  }
  return out;
};

const bytesToHex = (bytes) => {
  if (!bytes || !bytes.length) return '';
  return bytes.map((b) => b.toString(16).toUpperCase().padStart(2, '0')).join(' ');
};
const formatHex = bytesToHex;

const PAGE_NAMES = { 0x01: 'Generic Desktop', 0x07: 'Keyboard/Keypad', 0x0C: 'Consumer', 0x08: 'LEDs' };
const USAGE_NAMES = {
  0x01: { 0x02: 'Mouse', 0x06: 'Keyboard', 0x07: 'Keypad', 0x80: 'System Control' },
  0x0C: { 0x01: 'Consumer Control' },
};
const PROTOCOL_NAMES = { 0: 'Generic', 1: 'Keyboard', 2: 'Mouse' };

const collectionLabel = (sec) => {
  const up = sec.usagePage || 0;
  if ((up & 0xff00) === 0xff00) return `Vendor (0x${up.toString(16).toUpperCase().padStart(4, '0')})`;
  const pn = PAGE_NAMES[up] || `Page 0x${up.toString(16).toUpperCase()}`;
  const un = (USAGE_NAMES[up] && USAGE_NAMES[up][sec.usage]) || `Usage 0x${(sec.usage || 0).toString(16).toUpperCase().padStart(2, '0')}`;
  return `${pn} / ${un}`;
};

const parseReportDescriptor = (bytes) => {
  const sections = [];
  let i = 0;
  let depth = 0;
  let current = null;
  let usagePage = 0;
  let usage = 0;
  let headerBuf = [];

  while (i < bytes.length) {
    const header = bytes[i];
    if (header === 0xfe) {
      const longLen = bytes[i + 1] || 0;
      i += 3 + longLen;
      continue;
    }
    const size = header & 0x03;
    const type = (header >> 2) & 0x03;
    const tag = (header >> 4) & 0x0f;
    const dataLen = size === 3 ? 4 : size;
    const data = [];
    for (let j = 0; j < dataLen; j++) data.push(bytes[i + 1 + j]);
    const item = [header, ...data];

    if (type === 1 && tag === 0x0) {
      usagePage = dataLen === 1 ? data[0] : data[0] | (data[1] << 8);
    } else if (type === 2 && tag === 0x0) {
      usage = data[data.length - 1];
    }

    if (type === 0 && tag === 0xa) {
      if (depth === 0 && data[0] === 0x01) {
        current = { usagePage, usage, reportIds: [], bytes: [...headerBuf, ...item] };
        headerBuf = [];
      } else if (current) {
        current.bytes.push(...item);
      }
      depth++;
      i += 1 + dataLen;
      continue;
    }

    if (type === 0 && tag === 0xc) {
      depth--;
      if (current) current.bytes.push(header);
      if (depth === 0 && current) {
        if (current.reportIds.length === 0) current.reportIds.push(0);
        current.label = collectionLabel(current);
        sections.push(current);
        current = null;
        usage = 0;
        headerBuf = [];
      }
      i += 1 + dataLen;
      continue;
    }

    if (type === 1 && tag === 0x8 && current) {
      current.reportIds.push(data[0]);
    }

    if (current) current.bytes.push(...item);
    else headerBuf.push(...item);
    i += 1 + dataLen;
  }
  return sections;
};

const parseConfigInterfaces = (cfgBytes) => {
  const interfaces = [];
  let i = 0;
  while (i + 2 <= cfgBytes.length) {
    const bLength = cfgBytes[i];
    const bDescriptorType = cfgBytes[i + 1];
    if (bLength === 0) break;
    if (bDescriptorType === 0x04) {
      interfaces.push({
        number: cfgBytes[i + 2],
        protocol: cfgBytes[i + 7],
        reportDescLen: 0,
      });
    } else if (bDescriptorType === 0x21) {
      const wDescLen = cfgBytes[i + 7] | (cfgBytes[i + 8] << 8);
      if (interfaces.length > 0) interfaces[interfaces.length - 1].reportDescLen = wDescLen;
    }
    i += bLength;
  }
  return interfaces;
};

const reportByInterface = computed(() => {
  const ifaces = parseConfigInterfaces(descriptors.value.config);
  const reportBytes = descriptors.value.report;
  const result = [];
  let offset = 0;
  for (const iface of ifaces) {
    const len = iface.reportDescLen;
    if (len === 0) continue;
    const slice = reportBytes.slice(offset, offset + len);
    offset += len;
    result.push({
      number: iface.number,
      protocol: PROTOCOL_NAMES[iface.protocol] || `Proto ${iface.protocol}`,
      bytes: slice,
      collections: parseReportDescriptor(slice),
    });
  }
  return result;
});

const step = computed(() => {
  if (!status.connected) return 's1';
  if (!status.debug) return 's2';
  if (status.reportCount < status.reportMax) return 's3';
  return 's4';
});
const capturePercent = computed(() => {
  const m = status.reportMax || 100;
  return Math.min(100, Math.round((status.reportCount / m) * 100));
});
const hexVid = computed(() => '0x' + (status.vid || 0).toString(16).toUpperCase().padStart(4, '0'));
const hexPid = computed(() => '0x' + (status.pid || 0).toString(16).toUpperCase().padStart(4, '0'));

/* ---------- 诊断展示辅助 ---------- */
const modeOptions = computed(() => MODES.map((m) => ({ v: m.v, label: t('debug.' + m.key) })));
const modeLabel = computed(() => {
  const hit = MODES.find((m) => m.v === mode.value) || MODES[1];
  return t('debug.' + hit.key);
});

const clsName = (e) => {
  if (e.d === 2) return t(NOTE_LABELS[e.c] || 'debug.clsOther');
  if (e.d === 1) return t('debug.dirDownShort');
  return t(CLS_LABELS[e.c] || 'debug.clsOther');
};
const dirLabel = (d) => (d === 1 ? t('debug.dirDown') : d === 2 ? t('debug.dirNote') : t('debug.dirUp'));
const dirClass = (d) => (d === 1 ? 'down' : d === 2 ? 'note' : 'up');
const clsText = (e) => clsName(e);
const BTN_BITS = ['L', 'R', 'M', 'S1', 'S2'];
const btnText = (b) => {
  if (b === undefined || b === null || b === 255) return '—';
  const names = BTN_BITS.filter((_, i) => b & (1 << i)).join('+');
  return '0x' + b.toString(16).toUpperCase().padStart(2, '0') + (names ? ' ' + names : '');
};
const tBase = computed(() => (events.value.length ? events.value[0].t || 0 : 0));
const fmtT = (t) => '+' + Math.max(0, ((t || 0) - tBase.value)) + 'ms';
const hexPretty = (h) => String(h || '').replace(/(..)/g, '$1 ').trim().toUpperCase();

const statCells = computed(() => {
  const d = diag.value;
  if (!d) return [];
  return [
    { k: t('debug.stUpMouse'), v: d.upMouse || 0 },
    { k: t('debug.stUpBtnOn'), v: d.upBtnOn || 0 },
    { k: t('debug.stUpPress'), v: d.upPress || 0 },
    { k: t('debug.stDownMouse'), v: d.downMouse || 0 },
    { k: t('debug.stDownBtnOn'), v: d.downBtnOn || 0 },
    { k: t('debug.stDownPress'), v: d.downPress || 0 },
    { k: t('debug.stRid'), v: d.nRid || 0, bad: true },
    { k: t('debug.stIface'), v: d.nIface || 0, bad: true },
    { k: t('debug.stLayout'), v: d.nLayout || 0, bad: true },
    { k: t('debug.stMasked'), v: d.nMasked || 0, bad: true },
    { k: t('debug.stSendFail'), v: d.nSendFail || 0, bad: true },
    { k: t('debug.stQueueDrop'), v: d.nQueueDrop || 0, bad: true },
    { k: t('debug.stKbd'), v: d.nKbd || 0 },
    { k: t('debug.stRawFwd'), v: d.nRawFwd || 0 },
    { k: t('debug.stRec'), v: d.recTotal || 0 },
  ];
});

/* 判定：把"上游有没有按键位、下发有没有按键位"直接翻译成结论 */
const verdict = computed(() => {
  const d = diag.value;
  if (!d) return null;
  const lay = d.lay || {};
  const upBtn = d.upBtnOn || 0;
  const downBtn = d.downBtnOn || 0;
  if (!lay.valid) {
    return { level: 'bad', text: t('debug.vNoLayout'), detail: t('debug.vNoLayoutD') };
  }
  if ((d.upTotal || 0) === 0) {
    return { level: 'warn', text: t('debug.vNoUpstream'), detail: t('debug.vNoUpstreamD') };
  }
  if (upBtn === 0) {
    return {
      level: 'bad',
      text: t('debug.vUpNoBtn'),
      detail: t('debug.vUpNoBtnD', { mouse: d.upMouse || 0, rid: d.nRid || 0, iface: d.nIface || 0, layout: d.nLayout || 0 }),
    };
  }
  if (downBtn === 0) {
    return {
      level: 'bad',
      text: t('debug.vDownLost'),
      detail: t('debug.vDownLostD', {
        masked: d.nMasked || 0, fail: d.nSendFail || 0, kbd: d.nKbd || 0, drop: d.nQueueDrop || 0,
      }),
    };
  }
  return {
    level: 'ok',
    text: t('debug.vOk'),
    detail: t('debug.vOkD', { up: upBtn, down: downBtn, press: d.downPress || 0, drop: d.nQueueDrop || 0 }),
  };
});

/* ---------- command queue ---------- */
// Each firmware response carries the request cmd code (700..715). Build a
// pending map keyed by code so the asynchronous response can resolve the
// most recent request of that code. Commands are serialized via sendChain
// so only one request is in flight per code at any time.
const pendingByCode = new Map();
let serialListenerInstalled = false;

function ensureListener() {
  if (serialListenerInstalled) return;
  serialListenerInstalled = true;
  api.on('debug_response', (resp) => {
    if (!resp || typeof resp.code !== 'number') return;
    const entry = pendingByCode.get(resp.code);
    if (!entry) return;
    pendingByCode.delete(resp.code);
    clearTimeout(entry.timer);
    entry.resolve(resp);
  });
}

let sendChain = Promise.resolve();
function sendCmd(cmd, payload = {}, timeoutMs = 2000) {
  ensureListener();
  const result = sendChain.then(async () => {
    return new Promise((resolve) => {
      let done = false;
      const finalize = (val) => {
        if (done) return;
        done = true;
        resolve(val);
      };
      const timer = setTimeout(() => {
        pendingByCode.delete(cmd);
        finalize(null);
      }, timeoutMs);
      pendingByCode.set(cmd, { resolve: finalize, timer });
      api.send('debug_' + CMD_NAME[cmd], payload);
    });
  });
  sendChain = result.catch(() => {});
  return result;
}

const CMD_NAME = {
  700: 'enter',
  701: 'exit',
  702: 'status',
  710: 'get_dev_descr',
  711: 'get_cfg_descr',
  712: 'get_rep_descr',
  713: 'get_report',
  714: 'clear_reports',
  715: 'get_device_info',
  716: 'set_mode',
  717: 'get_diag',
  718: 'get_events',
};

const delay = (ms) => new Promise((r) => setTimeout(r, ms));

/* ---------- business ---------- */
const pollStatus = async () => {
  if (!props.serialConnected) return;
  const r = await sendCmd(702, {}, 1200);
  if (r && r.data) {
    const data = parseDataObject(r.data);
    if (data) {
      Object.assign(status, data);
    }
  }
};

function parseDataObject(s) {
  if (!s) return null;
  try { return JSON.parse(s); } catch (e) { return null; }
}

const enterDebug = async () => {
  busy.value = 'enter';
  addLog('info', t('debug.log.enter'));
  const r = await sendCmd(700, {}, 2000);
  if (r && r.message === 'debug_enter') {
    addLog('success', t('debug.log.entered'));
    await pollStatus();
    await fetchDescriptors();
  } else {
    addLog('error', t('debug.log.enterFail'));
  }
  busy.value = null;
};

const exitDebug = async () => {
  busy.value = 'exit';
  addLog('info', t('debug.log.exit'));
  const r = await sendCmd(701, {}, 2000);
  if (r && r.message === 'debug_exit') {
    addLog('success', t('debug.log.exited'));
    await pollStatus();
  } else {
    addLog('error', t('debug.log.exitFail'));
  }
  busy.value = null;
};

const fetchDescriptors = async () => {
  // Note: caller owns `busy` state so this can be invoked from enter/export
  // flows without clobbering their UI indicator.
  addLog('info', t('debug.log.fetchDesc'));

  // 设备描述符
  const dev = await sendCmd(710, {}, 2000);
  if (dev && dev.data) {
    const d = parseDataObject(dev.data);
    if (d && d.hex) {
      descriptors.value.device = hexToBytes(d.hex);
      addLog('success', t('debug.log.devDesc', { n: descriptors.value.device.length }));
    }
  }
  await delay(120);

  // 配置描述符
  const cfg = await sendCmd(711, {}, 2000);
  if (cfg && cfg.data) {
    const d = parseDataObject(cfg.data);
    if (d && d.hex) {
      descriptors.value.config = hexToBytes(d.hex);
      addLog('success', t('debug.log.cfgDesc', { n: descriptors.value.config.length }));
    }
  }
  await delay(120);

  // 报告描述符（分页拉取）
  const rep = await sendCmd(712, { offset: 0, max: 400 }, 3000);
  if (rep && rep.data) {
    const d = parseDataObject(rep.data);
    if (d) {
      const total = d.total || 0;
      let acc = hexToBytes(d.hex || '');
      let off = acc.length;
      addLog('info', t('debug.log.repFirst', { n: acc.length, total: total }));
      while (off < total) {
        await delay(120);
        const more = await sendCmd(712, { offset: off, max: 400 }, 3000);
        if (!more || !more.data) {
          addLog('error', t('debug.log.repChunk', { offset: off }));
          break;
        }
        const md = parseDataObject(more.data);
        if (!md) break;
        const chunk = hexToBytes(md.hex || '');
        if (!chunk.length) break;
        acc = acc.concat(chunk);
        off += chunk.length;
      }
      descriptors.value.report = acc.slice(0, total);
      addLog('success', t('debug.log.repDone', { n: descriptors.value.report.length, total: total }));
    }
  } else {
    addLog('error', t('debug.log.repFail'));
  }
};

const clearReports = async () => {
  const r = await sendCmd(714, {}, 2000);
  if (r && r.message === 'debug_cleared') {
    addLog('info', t('debug.log.cleared'));
    events.value = [];
    diag.value = null;
    await pollStatus();
  }
};

/* ---------- 按键链路诊断 ---------- */
/* 切抓包模式（固件会同时清空缓冲与计数器） */
const applyMode = async (m) => {
  mode.value = m;
  const r = await sendCmd(716, { mode: m }, 2500);
  const ok = r && r.message === 'debug_cfg';
  if (!ok) addLog('error', t('debug.log.modeFail'));
  await pollStatus();
  return ok;
};

const fetchDiag = async () => {
  const r = await sendCmd(717, {}, 3000);
  if (r && r.data) {
    const d = parseDataObject(r.data);
    if (d) {
      diag.value = d;
      if (typeof d.mode === 'number') mode.value = d.mode;
      return d;
    }
  }
  addLog('error', t('debug.log.diagFail'));
  return null;
};

/* 分页取记录：一页 16 条，比逐条 713 快得多 */
const fetchEvents = async (total) => {
  const acc = [];
  const n = total || 0;
  for (let off = 0; off < n; off += 12) {
    const r = await sendCmd(718, { from: off, n: 12 }, 3000);
    if (!r || !r.data) break;
    const d = parseDataObject(r.data);
    if (!d || !Array.isArray(d.items) || d.items.length === 0) break;
    acc.push(...d.items);
    if (d.items.length < 12) break;
  }
  events.value = acc;
  return acc;
};

const setMode = async (m) => {
  if (busy.value !== null) return;
  busy.value = 'mode';
  events.value = [];
  diag.value = null;
  await applyMode(m);
  addLog('info', t('debug.log.mode', { mode: modeLabel.value }));
  busy.value = null;
};

const refreshDiag = async () => {
  busy.value = 'diag';
  const d = await fetchDiag();
  const total = d ? d.recCount || 0 : status.reportCount || 0;
  await fetchEvents(total);
  addLog('info', t('debug.log.diagRefreshed', { n: events.value.length }));
  busy.value = null;
};

/* 一键抓按键：清空 -> 仅按键轨迹 -> 等 8 秒 -> 自动读回并判定。
 * 8K 鼠标"全部模式"约 16ms 就写满缓冲，按键那一帧一定被冲掉，
 * 所以这里强制切到"仅按键轨迹"，只记按键字节发生变化的帧。 */
const grabButtons = async () => {
  busy.value = 'grab';
  events.value = [];
  diag.value = null;
  catching.value = true;
  catchHint.value = t('debug.grabHint');
  await applyMode(2);
  busy.value = null;
  addLog('info', t('debug.log.grabArmed'));

  if (catchTimer) clearTimeout(catchTimer);
  catchTimer = setTimeout(async () => {
    catching.value = false;
    catchHint.value = '';
    const d = await fetchDiag();
    await fetchEvents(d ? d.recCount || 0 : 0);
    const v = verdict.value;
    addLog(v && v.level === 'ok' ? 'success' : 'error', t('debug.log.verdict', { text: v ? v.text : t('debug.log.noData') }));
  }, 8000);
};

const exportTxt = async () => {
  busy.value = 'export';
  exportProgress.value = 0;

  // Ensure descriptors are loaded (user may have entered debug mode before
  // they finished fetching, or reflashed the device mid-session).
  if (!descriptors.value.device.length ||
      !descriptors.value.config.length ||
      !descriptors.value.report.length) {
    addLog('info', t('debug.log.exportDesc'));
    await fetchDescriptors();
  }

  addLog('info', t('debug.log.pullReports'));
  const count = status.reportCount;
  const reports = [];

  /* 优先用 718 分页（16 条/次）；旧固件不支持时回退到 713 逐条读取 */
  let paged = false;
  const first = await sendCmd(718, { from: 0, n: 12 }, 3000);
  if (first && first.data) {
    const d0 = parseDataObject(first.data);
    if (d0 && Array.isArray(d0.items) && d0.items.length > 0) {
      paged = true;
      const push = (items) => items.forEach((it) => reports.push({
        idx: it.i, len: it.l, hex: it.h || '', t: it.t, dir: it.d,
        ep: it.e, intf: it.f, btn: it.b, cls: it.c,
      }));
      push(d0.items);
      const total = d0.total || count;
      while (reports.length < total) {
        const r = await sendCmd(718, { from: reports.length, n: 12 }, 3000);
        if (!r || !r.data) break;
        const d = parseDataObject(r.data);
        if (!d || !Array.isArray(d.items) || d.items.length === 0) break;
        push(d.items);
        exportProgress.value = reports.length;
        if (d.items.length < 12) break;
      }
    }
  }

  if (!paged) {
    for (let i = 0; i < count; i++) {
      const r = await sendCmd(713, { idx: i }, 1500);
      if (r && r.data) {
        const d = parseDataObject(r.data);
        if (d) {
          reports.push({
            idx: i, len: d.len, hex: d.hex || '', t: d.t, dir: d.dir,
            ep: d.ep, intf: d.intf, btn: d.btn, cls: d.cls,
          });
        }
      } else {
        addLog('error', t('debug.log.reportFail', { n: i }));
      }
      exportProgress.value = i + 1;
    }
  }
  exportProgress.value = reports.length;

  /* 诊断快照：没有就现取一份，保证导出文件自带"哪一段丢的"结论 */
  if (!diag.value) await fetchDiag();
  const d = diag.value;
  const lay = (d && d.lay) || {};

  const lines = [];
  lines.push(t('debug.exportDoc.title'));
  lines.push(t('debug.exportDoc.time', { time: new Date().toLocaleString(locale()) }));
  lines.push(t('debug.exportDoc.device', { vid: hexVid.value, pid: hexPid.value, product: status.product || '' }));
  lines.push(t('debug.exportDoc.reports', { n: reports.length, max: status.reportMax }));
  lines.push(t('debug.exportDoc.mode', { mode: modeLabel.value }));
  lines.push('='.repeat(60));
  lines.push('');
  lines.push(t('debug.exportDoc.devDesc'));
  lines.push(formatHex(descriptors.value.device));
  lines.push('');
  lines.push(t('debug.exportDoc.cfgDesc'));
  lines.push(formatHex(descriptors.value.config));
  lines.push('');
  lines.push(t('debug.exportDoc.repDesc', { bytes: descriptors.value.report.length, ifaces: reportByInterface.value.length }));
  reportByInterface.value.forEach((iface) => {
    lines.push('');
    lines.push('  ' + t('debug.exportDoc.iface', { n: iface.number, protocol: iface.protocol, bytes: iface.bytes.length }));
    iface.collections.forEach((sec, idx) => {
      lines.push(`    [${idx + 1}] ${sec.label}${sec.reportIds.length ? '  Report ID ' + sec.reportIds.join('/') : ''}  (${sec.bytes.length} B)`);
      lines.push(`      ${formatHex(sec.bytes)}`);
    });
    lines.push('  ' + t('debug.exportDoc.ifaceRaw', { n: iface.number }));
    lines.push('  ' + formatHex(iface.bytes));
  });
  lines.push('');
  lines.push(t('debug.exportDoc.repRaw'));
  lines.push(formatHex(descriptors.value.report));
  lines.push('');

  /* ---- 按键链路诊断 ---- */
  lines.push(t('debug.exportDoc.diag'));
  lines.push('  ' + t('debug.exportDoc.diagNote'));
  if (d) {
    lines.push('  ' + t('debug.exportDoc.diagStat', { mode: d.mode, rec: d.recCount, max: d.recMax, total: d.recTotal }));
    lines.push('  ' + t('debug.exportDoc.up', {
      total: d.upTotal, mouse: d.upMouse, btn: d.upBtnOn, press: d.upPress,
      release: d.upRelease, last: btnText(d.lastUpBtn),
    }));
    lines.push('  ' + t('debug.exportDoc.down', {
      total: d.downTotal, mouse: d.downMouse, btn: d.downBtnOn, press: d.downPress,
      release: d.downRelease, last: btnText(d.lastDownBtn),
    }));
    lines.push('  ' + t('debug.exportDoc.drop', {
      rid: d.nRid, iface: d.nIface, layout: d.nLayout, masked: d.nMasked, fail: d.nSendFail,
      kbd: d.nKbd, raw: d.nRawFwd, epbusy: d.nEpBusy, nocon: d.nNoContent, queue: d.nQueueDrop,
    }));
    lines.push('  ' + t('debug.exportDoc.layout', {
      valid: lay.valid, len: lay.len, rid: lay.hasRid ? lay.rid : '-', iface: lay.iface, ep: lay.ep,
      btnOff: lay.btnOff, btnBits: lay.btnBits, xOff: lay.xOff, xBits: lay.xBits,
      yOff: lay.yOff, yBits: lay.yBits, whOff: lay.whOff, whBits: lay.whBits,
    }));
    const v = verdict.value;
    if (v) lines.push('  ' + t('debug.exportDoc.verdict', { level: v.level, text: v.text, detail: v.detail }));
  } else {
    lines.push('  ' + t('debug.exportDoc.noDiag'));
  }
  lines.push('');
  lines.push(t('debug.exportDoc.captured', { n: reports.length }));
  lines.push('  ' + t('debug.exportDoc.columns'));
  reports.forEach((r) => {
    // Firmware hex has no separators; reformat with spaces for readability.
    const pretty = formatHex(hexToBytes(r.hex));
    const dir = r.dir === 1 ? 'down' : r.dir === 2 ? 'note' : 'up';
    const cls = clsText({ d: r.dir, c: r.cls });
    lines.push(
      `[${String(r.idx).padStart(3, ' ')}] t=${String(r.t === undefined ? 0 : r.t).padStart(8, ' ')} ` +
      `${dir.padEnd(4, ' ')} ep=${String(r.ep === undefined ? '-' : r.ep).padStart(2, ' ')} ` +
      `btn=${btnText(r.btn).padEnd(8, ' ')} ${cls.padEnd(14, ' ')} ${pretty}`
    );
  });

  const blob = new Blob([lines.join('\n')], { type: 'text/plain;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  const ts = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
  a.href = url;
  a.download = `hid_debug_${ts}.txt`;
  a.click();
  URL.revokeObjectURL(url);
  addLog('success', t('debug.log.exported', { n: reports.length }));
  busy.value = null;
};

const startPolling = () => {
  stopPolling();
  pollStatus();
  pollTimer = setInterval(pollStatus, 1500);
};
const stopPolling = () => {
  if (pollTimer) clearInterval(pollTimer);
  pollTimer = null;
};

onMounted(() => {
  ensureListener();
  if (props.serialConnected) {
    startPolling();
  }
});

onUnmounted(() => {
  stopPolling();
});

// React to serial connection changes from parent
watch(() => props.serialConnected, (v) => {
  if (v) startPolling();
  else {
    stopPolling();
    status.connected = false;
    status.debug = false;
  }
});
</script>

<style scoped>
.debug-panel {
  flex: 1;
  min-height: 0;
}

.panel-body {
  gap: 10px;
}

/* status bar */
.status-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  background: var(--bg-input);
  border: 1px solid var(--border);
  border-radius: var(--btn-radius);
  padding: 8px 12px;
}
.status-left { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
.dot.on { background: var(--accent-green); box-shadow: 0 0 6px rgba(52,211,153,0.5); }
.dot.off { background: #4b5563; }
.status-text { font-size: 12px; font-weight: 600; color: var(--text-primary); font-family: var(--font-mono); }
.vid-pid, .product {
  font-size: 11px;
  color: var(--text-secondary);
  font-family: var(--font-mono);
  padding: 1px 6px;
  background: var(--bg-tertiary);
  border-radius: 4px;
}
.badge { font-size: 11px; font-weight: 700; padding: 2px 10px; border-radius: 10px; font-family: var(--font-mono); }
.badge.debug-on { background: rgba(251,191,36,0.15); color: var(--accent-yellow); border: 1px solid rgba(251,191,36,0.3); }
.badge.debug-off { background: var(--bg-tertiary); color: var(--text-dim); }

.hint-card {
  background: var(--bg-input);
  border: 1px dashed var(--border);
  border-radius: var(--btn-radius);
  padding: 24px;
  text-align: center;
  font-size: 13px;
  color: var(--text-secondary);
}

/* guide */
.guide-card {
  background: var(--bg-input);
  border: 1px solid var(--border);
  border-radius: var(--btn-radius);
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.guide-step { display: flex; gap: 10px; align-items: flex-start; }
.step-num {
  width: 22px; height: 22px; border-radius: 50%;
  background: var(--bg-tertiary); color: var(--text-dim);
  font-size: 11px; font-weight: 700;
  display: flex; align-items: center; justify-content: center;
  flex-shrink: 0; font-family: var(--font-mono);
  transition: all 0.2s;
}
.step-num.active { background: var(--accent-green); color: #fff; }
.guide-card.s1 .guide-step:nth-child(1) .step-num,
.guide-card.s2 .guide-step:nth-child(2) .step-num,
.guide-card.s3 .guide-step:nth-child(3) .step-num,
.guide-card.s4 .guide-step:nth-child(4) .step-num {
  background: var(--accent-blue); color: #fff;
}
.step-body { flex: 1; }
.step-title { font-size: 12px; font-weight: 700; color: var(--text-primary); }
.step-desc { font-size: 11px; color: var(--text-secondary); margin-top: 2px; }

/* actions */
.actions { display: flex; gap: 8px; flex-wrap: wrap; }
.actions .btn, .export-row .btn {
  flex: 1; min-width: 90px;
}
.btn.primary { background: var(--accent-blue); color: #0b0d12; border: none; font-weight: 600; }
.btn.primary:hover:not(:disabled) { background: #62d0fa; }
.btn.danger { background: transparent; color: var(--accent-red); border: 1px solid #9b2020; }
.btn.danger:hover:not(:disabled) { background: var(--accent-red); color: #fff; }
.btn.export { background: var(--accent-green); color: #0b0d12; border: none; font-weight: 600; }
.btn.export:hover:not(:disabled) { background: #2bbd88; }
.btn.export.highlight {
  box-shadow: 0 0 0 2px rgba(52,211,153,0.4), 0 0 12px rgba(52,211,153,0.6);
  animation: pulse 1.6s ease-in-out infinite;
}
@keyframes pulse {
  0%, 100% { box-shadow: 0 0 0 2px rgba(52,211,153,0.4), 0 0 12px rgba(52,211,153,0.6); }
  50% { box-shadow: 0 0 0 2px rgba(52,211,153,0.6), 0 0 18px rgba(52,211,153,0.9); }
}

/* capture */
.capture-card {
  background: var(--bg-input);
  border: 1px solid var(--border);
  border-radius: var(--btn-radius);
  padding: 10px 14px;
}
.capture-head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px; }
.capture-title { font-size: 12px; font-weight: 700; color: var(--text-primary); font-family: var(--font-mono); }
.capture-count { font-size: 12px; font-weight: 700; color: var(--accent-blue); font-family: var(--font-mono); }
.capture-bar { height: 8px; background: var(--bg-tertiary); border-radius: 4px; overflow: hidden; }
.capture-fill { height: 100%; background: var(--accent-blue); border-radius: 4px; transition: width 0.3s; }
.capture-fill.full { background: var(--accent-green); }
.capture-hint { font-size: 11px; color: var(--text-secondary); margin-top: 6px; font-family: var(--font-mono); }
.capture-hint.done { color: var(--accent-green); font-weight: 600; }

/* descriptors */
.descr-group { display: flex; flex-direction: column; gap: 8px; }
.descr-card {
  background: var(--bg-input);
  border: 1px solid var(--border);
  border-radius: var(--btn-radius);
  overflow: hidden;
}
.descr-card summary {
  display: flex; align-items: center; justify-content: space-between;
  padding: 8px 12px;
  font-size: 12px; font-weight: 700;
  color: var(--text-primary);
  font-family: var(--font-mono);
  cursor: pointer;
  background: var(--bg-tertiary);
}
.descr-card summary:hover { background: var(--border); }
.descr-len {
  font-size: 10px; font-weight: 600;
  color: var(--text-secondary);
  padding: 1px 6px;
  background: var(--bg-tertiary);
  border-radius: 8px;
}
.hex-box {
  margin: 0;
  padding: 10px 12px;
  background: #0d1118;
  color: #93c5fd;
  font-family: var(--font-mono);
  font-size: 11px;
  line-height: 1.5;
  max-height: 180px;
  overflow: auto;
  white-space: pre-wrap;
  word-break: break-all;
}
.hex-box.small { padding: 6px 10px; max-height: 120px; font-size: 10px; }

/* report sections */
.report-sections { display: flex; flex-direction: column; padding: 8px; gap: 6px; }
.report-section { border: 1px solid var(--border); border-radius: 6px; overflow: hidden; }
.section-header {
  display: flex; align-items: center; gap: 8px;
  padding: 5px 10px;
  background: var(--bg-tertiary);
  font-family: var(--font-mono); font-size: 11px;
}
.section-idx {
  font-weight: 700; color: #fff;
  background: var(--accent-blue);
  padding: 1px 6px; border-radius: 3px; font-size: 10px;
}
.section-label { font-weight: 700; color: var(--text-primary); flex: 1; }
.section-len {
  font-size: 10px; color: var(--text-secondary);
  background: var(--bg-input); padding: 1px 6px; border-radius: 3px;
}
.collection-row { border-top: 1px solid var(--border); }
.collection-row:first-child { border-top: none; }
.collection-header {
  display: flex; align-items: center; gap: 6px;
  padding: 4px 10px 4px 28px;
  background: var(--bg-secondary);
  font-family: var(--font-mono); font-size: 10px;
}
.collection-name { font-weight: 600; color: var(--text-primary); flex: 1; }
.collection-tag {
  font-size: 9px; font-weight: 600; color: var(--accent-yellow);
  background: rgba(251,191,36,0.15); padding: 1px 5px; border-radius: 3px;
}
.collection-bytes {
  font-size: 9px; color: var(--text-dim);
  background: var(--bg-input); padding: 1px 5px; border-radius: 3px;
}

/* 按键链路诊断 */
.diag-card {
  background: var(--bg-input);
  border: 1px solid var(--border);
  border-radius: var(--btn-radius);
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.diag-head { display: flex; justify-content: space-between; align-items: center; }
.diag-title { font-size: 12px; font-weight: 700; color: var(--text-primary); font-family: var(--font-mono); }
.diag-mode { font-size: 10px; color: var(--accent-blue); font-family: var(--font-mono); }
.diag-desc { font-size: 10px; color: var(--text-dim); line-height: 1.5; }
.diag-modes { display: flex; gap: 6px; flex-wrap: wrap; }
.chip {
  font-size: 11px; font-family: var(--font-mono);
  padding: 3px 10px; border-radius: 10px;
  background: var(--bg-tertiary); color: var(--text-secondary);
  border: 1px solid var(--border); cursor: pointer;
}
.chip:hover:not(:disabled) { color: var(--text-primary); }
.chip.on { background: var(--accent-blue); color: #0b0d12; border-color: transparent; font-weight: 700; }
.diag-actions { display: flex; gap: 8px; }
.diag-actions .btn { flex: 1; min-width: 90px; }

.verdict { border-radius: 6px; padding: 8px 10px; border: 1px solid var(--border); }
.verdict.ok { background: rgba(52,211,153,0.10); border-color: rgba(52,211,153,0.35); }
.verdict.warn { background: rgba(251,191,36,0.10); border-color: rgba(251,191,36,0.35); }
.verdict.bad { background: rgba(248,113,113,0.10); border-color: rgba(248,113,113,0.35); }
.verdict-text { font-size: 12px; font-weight: 700; color: var(--text-primary); font-family: var(--font-mono); }
.verdict-detail { font-size: 10px; color: var(--text-secondary); margin-top: 3px; line-height: 1.5; word-break: break-all; }

.stat-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
  gap: 4px;
}
.stat {
  display: flex; justify-content: space-between; align-items: center;
  gap: 6px; padding: 3px 8px;
  background: var(--bg-tertiary); border-radius: 4px;
  font-family: var(--font-mono); font-size: 10px;
}
.stat-k { color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.stat-v { color: var(--accent-blue); font-weight: 700; }
.stat-v.zero { color: var(--text-dim); font-weight: 400; }
.stat-v.bad { color: var(--accent-red); }
.lay-line { font-size: 10px; color: var(--text-dim); font-family: var(--font-mono); line-height: 1.5; word-break: break-all; }

.evt-box { background: #0d1118; border-radius: 4px; max-height: 220px; overflow: auto; }
.evt-row {
  display: flex; gap: 8px; align-items: center;
  padding: 2px 8px;
  font-family: var(--font-mono); font-size: 10px;
  color: var(--text-secondary);
  border-bottom: 1px solid rgba(255,255,255,0.04);
}
.evt-row.evt-head { position: sticky; top: 0; background: #131a24; color: var(--text-dim); font-weight: 700; }
.evt-row.up .c-d { color: #60a5fa; }
.evt-row.down .c-d { color: var(--accent-green); }
.evt-row.note .c-d { color: var(--accent-yellow); }
.evt-row.note .c-c { color: var(--accent-yellow); }
.evt-row .c-i { width: 28px; color: var(--text-dim); flex-shrink: 0; }
.evt-row .c-t { width: 64px; flex-shrink: 0; }
.evt-row .c-d { width: 42px; flex-shrink: 0; }
.evt-row .c-e { width: 20px; flex-shrink: 0; }
.evt-row .c-b { width: 66px; flex-shrink: 0; }
.evt-row .c-c { width: 96px; flex-shrink: 0; }
.evt-row .c-h { flex: 1; color: #93c5fd; word-break: break-all; }

/* export row */
.export-row { display: flex; gap: 8px; }

/* logs */
.log-details { border: 1px solid var(--border); border-radius: var(--btn-radius); overflow: hidden; }
.log-details summary {
  display: flex; justify-content: space-between; align-items: center;
  padding: 8px 12px;
  font-size: 12px; font-weight: 700;
  color: var(--text-secondary);
  font-family: var(--font-mono);
  cursor: pointer;
  background: var(--bg-tertiary);
}
.log-count { font-size: 10px; background: var(--border); padding: 1px 6px; border-radius: 8px; color: var(--text-dim); }
.log-box {
  background: #0d1118;
  max-height: 180px;
  overflow-y: auto;
  padding: 4px 0;
}
.log-empty { padding: 12px 14px; font-size: 11px; color: var(--text-dim); font-family: var(--font-mono); }
.log-row { display: flex; gap: 8px; padding: 3px 14px; font-size: 11px; font-family: var(--font-mono); }
.log-time { color: var(--text-dim); flex-shrink: 0; }
.log-msg { color: var(--text-secondary); word-break: break-all; }
.log-row.info .log-msg { color: #60a5fa; }
.log-row.success .log-msg { color: #4ade80; }
.log-row.error .log-msg { color: #f87171; }
</style>
