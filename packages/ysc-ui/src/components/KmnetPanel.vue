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

      <!-- ── NET API 活动预览（键鼠集火用；主页不传 showActivity 就不显示）──
           能看到：是否被连上、累计调用分类计数、最近若干条调用（含"有没有真的
           转到盒子上"）。数据来自驱动的 kmnet_stat 事件 + get_kmnet_stat 拉取。 -->
      <div v-if="showActivity" class="kmnet-act">
        <div class="kmnet-act-head">
          <span class="kmnet-act-label">{{ t('kmnet.act.title') }}</span>
          <span class="kmnet-act-dot" :class="{ on: clientActive }" />
          <span class="kmnet-act-who">{{ clientLabel }}</span>
        </div>
        <div v-if="total > 0" class="kmnet-act-counts">
          <span class="pill m">{{ t('kmnet.act.cMouse') }} {{ counts.mouse }}</span>
          <span class="pill k">{{ t('kmnet.act.cKbd') }} {{ counts.kbd }}</span>
          <span class="pill c">{{ t('kmnet.act.cCtrl') }} {{ counts.ctrl }}</span>
          <span v-if="counts.other" class="pill u">{{ t('kmnet.act.cOther') }} {{ counts.other }}</span>
        </div>
        <div class="kmnet-act-list">
          <div v-if="!ring.length" class="kmnet-act-empty">{{ t('kmnet.act.empty') }}</div>
          <div v-for="r in ring" :key="r.i" class="kmnet-act-row" :title="rowTitle(r)">
            <span class="kmnet-act-t">{{ timeLabel(r) }}</span>
            <span class="kmnet-act-dt">{{ dtLabel(r) }}</span>
            <span class="kmnet-act-n" :class="'k' + r.k">{{ r.n }}</span>
            <span class="kmnet-act-d">{{ r.d }}</span>
            <span class="kmnet-act-ok" :class="{ bad: !r.ok, dash: r.k === 2 }">{{ targetLabel(r) }}</span>
          </div>
        </div>
        <div v-if="running" class="kmnet-act-legend">{{ t('kmnet.act.legend') }}</div>
        <div v-if="running" class="kmnet-hint">{{ t('kmnet.act.hint') }}</div>
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
  /** 驱动侧 NET 活动快照 {total,c:{mouse,kbd,ctrl,other},client:{...},ring:[...]} */
  stat: {
    type: Object,
    default: function() {
      return { total: 0, c: {}, client: {}, ring: [] };
    },
  },
  /** 是否显示 API 活动预览（键鼠集火面板才需要） */
  showActivity: { type: Boolean, default: false },
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

/* ── API 活动预览 ── */
const total = computed(() => (props.stat && props.stat.total) || 0);
const counts = computed(() => (props.stat && props.stat.c) || {});
const ring = computed(() => (props.stat && props.stat.ring) || []);
const clientActive = computed(() => !!(props.stat && props.stat.client && props.stat.client.active));

const clientLabel = computed(function() {
  const c = (props.stat && props.stat.client) || {};
  if (!c.ip) return t('kmnet.act.noClient');
  let s = t('kmnet.act.client', { ip: c.ip, port: c.port });
  if (c.sessions > 1) s += ' · ' + t('kmnet.act.sessions', { n: c.sessions });
  if (c.seenMs) s += ' · ' + t('kmnet.act.seen', { n: Math.round(c.seenMs / 1000) });
  return s;
});

/** 距 NET 服务启动的时间，秒（一位小数）——旧驱动没有 t0 时的兜底。 */
function atLabel(ms) {
  if (ms == null) return '';
  return (ms / 1000).toFixed(1) + 's';
}

/** 墙上时间 HH:MM:SS.mmm：驱动给服务启动时间 t0，加上记录偏移 ms。 */
function timeLabel(r) {
  const t0 = (props.stat && props.stat.t0) || 0;
  if (!t0 || r.ms == null) return atLabel(r.ms);
  const d = new Date(t0 + r.ms);
  const p = function(n, w) { return String(n).padStart(w, '0'); };
  return p(d.getHours(), 2) + ':' + p(d.getMinutes(), 2) + ':' + p(d.getSeconds(), 2) + '.' + p(d.getMilliseconds(), 3);
}

/** 与上一条同类命令的毫秒间隔（驱动算好；dt < 0 表示没有上一条）。 */
function dtLabel(r) {
  if (r.dt == null || r.dt < 0) return '';
  return '+' + r.dt + 'ms';
}

function rowTitle(r) {
  let s = timeLabel(r) + '  ' + r.n + '  ' + r.d;
  if (r.dt != null && r.dt >= 0) s += '  (+' + r.dt + 'ms)';
  return s;
}

/** 命令落到哪台盒子；k=2 是控制类（连接/监控/屏蔽/重启），没有目标设备 */
function targetLabel(r) {
  if (!r.ok) return t('kmnet.act.notRouted');
  if (r.k === 0) return t('kmnet.act.tMouse');
  if (r.k === 1) return t('kmnet.act.tKbd');
  if (r.k === 2) return t('kmnet.act.tDash');
  return t('kmnet.act.tUnsupported');
}
</script>

<style scoped>
.kmnet-act {
  margin-top: 10px;
  padding-top: 8px;
  border-top: 1px dashed var(--border, #444);
}
.kmnet-act-head {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  margin-bottom: 6px;
}
.kmnet-act-label { opacity: 0.75; }
.kmnet-act-dot {
  width: 7px; height: 7px; border-radius: 50%;
  background: #666; flex: none;
}
.kmnet-act-dot.on { background: var(--accent, #4a9); box-shadow: 0 0 6px var(--accent, #4a9); }
.kmnet-act-who {
  opacity: 0.8;
  font-family: ui-monospace, Consolas, monospace;
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.kmnet-act-counts {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-bottom: 6px;
}
.kmnet-act-counts .pill {
  font-size: 11px;
  padding: 1px 6px;
  border-radius: 8px;
  border: 1px solid var(--border, #444);
  opacity: 0.9;
}
.pill.m { border-color: #4a9; }
.pill.k { border-color: #c96; }
.pill.c { border-color: #69c; }
.pill.u { border-color: #c66; color: #e88; }
.kmnet-act-list {
  max-height: 210px;
  overflow-y: auto;
  border: 1px solid var(--border, #333);
  border-radius: 4px;
  background: rgba(0, 0, 0, 0.15);
}
.kmnet-act-empty { font-size: 11px; opacity: 0.5; padding: 6px 8px; }
.kmnet-act-row {
  display: flex;
  align-items: baseline;
  gap: 6px;
  padding: 2px 6px;
  font-size: 11px;
  font-family: ui-monospace, Consolas, monospace;
  border-bottom: 1px solid rgba(255, 255, 255, 0.04);
}
.kmnet-act-row:last-child { border-bottom: none; }
.kmnet-act-t { opacity: 0.45; width: 84px; flex: none; text-align: right; }
.kmnet-act-dt { width: 46px; flex: none; text-align: right; color: #8bd; opacity: 0.8; }
.kmnet-act-n { width: 62px; flex: none; }
.kmnet-act-n.k0 { color: #6cb; }
.kmnet-act-n.k1 { color: #dba; }
.kmnet-act-n.k2 { color: #9cf; }
.kmnet-act-n.k3 { color: #e88; }
.kmnet-act-d {
  flex: 1;
  opacity: 0.7;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.kmnet-act-ok { flex: none; opacity: 0.7; }
.kmnet-act-ok.dash { opacity: 0.35; }
.kmnet-act-ok.bad { color: #e88; opacity: 1; }
.kmnet-act-legend {
  margin-top: 5px;
  font-size: 10px;
  opacity: 0.5;
  letter-spacing: 0.2px;
}
</style>
