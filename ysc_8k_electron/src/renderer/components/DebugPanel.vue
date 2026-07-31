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
import { useI18n } from '../i18n/index.js';

const api = window.driverApi;
const { t } = useI18n();

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

/* ---------- helpers ---------- */
const addLog = (type, message) => {
  const time = new Date().toLocaleTimeString('zh-CN');
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
  addLog('info', '进入调试模式...');
  const r = await sendCmd(700, {}, 2000);
  if (r && r.message === 'debug_enter') {
    addLog('success', '已进入调试模式');
    await pollStatus();
    await fetchDescriptors();
  } else {
    addLog('error', '进入失败');
  }
  busy.value = null;
};

const exitDebug = async () => {
  busy.value = 'exit';
  addLog('info', '退出调试模式...');
  const r = await sendCmd(701, {}, 2000);
  if (r && r.message === 'debug_exit') {
    addLog('success', '已退出，恢复正常透传');
    await pollStatus();
  } else {
    addLog('error', '退出失败');
  }
  busy.value = null;
};

const fetchDescriptors = async () => {
  // Note: caller owns `busy` state so this can be invoked from enter/export
  // flows without clobbering their UI indicator.
  addLog('info', '获取描述符（后台）...');

  // 设备描述符
  const dev = await sendCmd(710, {}, 2000);
  if (dev && dev.data) {
    const d = parseDataObject(dev.data);
    if (d && d.hex) {
      descriptors.value.device = hexToBytes(d.hex);
      addLog('success', `设备描述符 ${descriptors.value.device.length}B`);
    }
  }
  await delay(120);

  // 配置描述符
  const cfg = await sendCmd(711, {}, 2000);
  if (cfg && cfg.data) {
    const d = parseDataObject(cfg.data);
    if (d && d.hex) {
      descriptors.value.config = hexToBytes(d.hex);
      addLog('success', `配置描述符 ${descriptors.value.config.length}B`);
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
      addLog('info', `报告描述符首批 ${acc.length}/${total}B`);
      while (off < total) {
        await delay(120);
        const more = await sendCmd(712, { offset: off, max: 400 }, 3000);
        if (!more || !more.data) {
          addLog('error', `报告描述符分片失败 @offset=${off}`);
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
      addLog('success', `报告描述符 ${descriptors.value.report.length}/${total}B`);
    }
  } else {
    addLog('error', '报告描述符获取失败');
  }
};

const clearReports = async () => {
  const r = await sendCmd(714, {}, 2000);
  if (r && r.message === 'debug_cleared') {
    addLog('info', '已清空报告缓冲');
    await pollStatus();
  }
};

const exportTxt = async () => {
  busy.value = 'export';
  exportProgress.value = 0;

  // Ensure descriptors are loaded (user may have entered debug mode before
  // they finished fetching, or reflashed the device mid-session).
  if (!descriptors.value.device.length ||
      !descriptors.value.config.length ||
      !descriptors.value.report.length) {
    addLog('info', '导出前重新获取描述符...');
    await fetchDescriptors();
  }

  addLog('info', '拉取全部报告...');
  const count = status.reportCount;
  const reports = [];
  for (let i = 0; i < count; i++) {
    const r = await sendCmd(713, { idx: i }, 1500);
    if (r && r.data) {
      const d = parseDataObject(r.data);
      if (d) {
        reports.push({ idx: i, len: d.len, hex: d.hex || '' });
      }
    } else {
      addLog('error', `读取报告 ${i} 失败`);
    }
    exportProgress.value = i + 1;
  }

  const lines = [];
  lines.push('YSC 8K HID Debug Capture');
  lines.push(`Time: ${new Date().toLocaleString('zh-CN')}`);
  lines.push(`Device VID=${hexVid.value} PID=${hexPid.value} product="${status.product || ''}"`);
  lines.push(`Reports: ${reports.length} / ${status.reportMax}`);
  lines.push('='.repeat(60));
  lines.push('');
  lines.push('== Device Descriptor ==');
  lines.push(formatHex(descriptors.value.device));
  lines.push('');
  lines.push('== Config Descriptor ==');
  lines.push(formatHex(descriptors.value.config));
  lines.push('');
  lines.push(`== HID Report Descriptor (${descriptors.value.report.length} bytes, ${reportByInterface.value.length} interfaces) ==`);
  reportByInterface.value.forEach((iface) => {
    lines.push('');
    lines.push(`  ---- Interface ${iface.number} (${iface.protocol}) — ${iface.bytes.length} B ----`);
    iface.collections.forEach((sec, idx) => {
      lines.push(`    [${idx + 1}] ${sec.label}${sec.reportIds.length ? '  Report ID ' + sec.reportIds.join('/') : ''}  (${sec.bytes.length} B)`);
      lines.push(`      ${formatHex(sec.bytes)}`);
    });
    lines.push(`  -- Full IF${iface.number} raw --`);
    lines.push('  ' + formatHex(iface.bytes));
  });
  lines.push('');
  lines.push('== Full Report Descriptor (raw, all interfaces concatenated) ==');
  lines.push(formatHex(descriptors.value.report));
  lines.push('');
  lines.push(`== Captured HID Reports (${reports.length}) ==`);
  reports.forEach((r) => {
    // Firmware hex has no separators; reformat with spaces for readability.
    const pretty = formatHex(hexToBytes(r.hex));
    lines.push(`[${String(r.idx).padStart(3, ' ')}] len=${String(r.len).padStart(2, ' ')}  ${pretty}`);
  });

  const blob = new Blob([lines.join('\n')], { type: 'text/plain;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  const ts = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
  a.href = url;
  a.download = `hid_debug_${ts}.txt`;
  a.click();
  URL.revokeObjectURL(url);
  addLog('success', `已导出 ${reports.length} 条报告`);
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
