<template>
  <div class="panel macro-page mcurve-page">
    <div class="panel-header">
      <svg class="panel-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <path d="M2 12 C4 12 5 4 8 4 C11 4 12 12 14 12" />
      </svg>
      <span>{{ t('mouseCurve.title') }}</span>
      <div class="macro-header-hint">{{ t('mouseCurve.hint') }}</div>
      <div style="flex:1" />
      <div class="macro-header-actions">
        <button class="btn btn-ghost" @click="loadFromDevice">{{ t('mouseCurve.refresh') }}</button>
        <button class="btn btn-accent" :disabled="!dirty" @click="saveAll">{{ t('mouseCurve.save') }}</button>
        <button class="btn btn-outline red" @click="onReset">{{ t('mouseCurve.reset') }}</button>
      </div>
    </div>
    <div v-if="toast" class="macro-toast" :class="toastType">{{ toast }}</div>

    <div class="mcurve-card">
      <div class="macro-slot-header">
        <span class="macro-slot-title">{{ t('mouseCurve.configTitle') }}</span>
        <div style="flex:1" />
        <button class="btn-toggle btn-xs" :class="{ active: cfg.enabled }" @click="toggleEnabled">
          {{ cfg.enabled ? 'ON' : 'OFF' }}
        </button>
      </div>

      <div class="mcurve-body" v-show="cfg.enabled">
        <div class="macro-field">
          <span class="macro-field-label">{{ t('mouseCurve.profile') }}</span>
          <span class="macro-field-hint">{{ t('mouseCurve.profileHint') }}</span>
          <select class="input-select" v-model="cfg.profile" @change="markDirty">
            <option :value="0">{{ t('mouseCurve.profileLinear') }}</option>
            <option :value="1">{{ t('mouseCurve.profileEase') }}</option>
            <option :value="2">{{ t('mouseCurve.profileMinJerk') }}</option>
            <option :value="3">{{ t('mouseCurve.profileRandom') }}</option>
          </select>
        </div>

        <div class="macro-field mcurve-field-row">
          <div class="mcurve-col">
            <span class="macro-field-label">{{ t('mouseCurve.segments') }}</span>
            <span class="macro-field-hint">{{ t('mouseCurve.segmentsHint') }}</span>
            <div class="macro-num-input">
              <input type="number" class="input-select" min="2" max="32" step="1"
                :value="cfg.segments" @input="setNum('segments', $event, 2, 32)" />
            </div>
          </div>
          <div class="mcurve-col">
            <span class="macro-field-label">{{ t('mouseCurve.duration') }}</span>
            <span class="macro-field-hint">{{ t('mouseCurve.durationHint') }}</span>
            <div class="macro-num-input">
              <input type="number" class="input-select" min="0" max="2000" step="1"
                :value="cfg.duration" @input="setNum('duration', $event, 0, 2000)" />
              <span class="macro-num-unit">{{ t('mouseCurve.msUnit') }}</span>
            </div>
          </div>
          <div class="mcurve-col">
            <span class="macro-field-label">{{ t('mouseCurve.jitter') }}</span>
            <span class="macro-field-hint">{{ t('mouseCurve.jitterHint') }}</span>
            <div class="macro-num-input">
              <input type="number" class="input-select" min="0" max="100" step="1"
                :value="cfg.jitter" @input="setNum('jitter', $event, 0, 100)" />
              <span class="macro-num-unit">%</span>
            </div>
          </div>
        </div>

        <div class="mcurve-note">{{ t('mouseCurve.note') }}</div>

        <div class="mcurve-viz">
          <div class="mcurve-viz-head">
            <span class="macro-field-label">曲线预览（逐报告复刻固件输出）</span>
            <div style="flex:1"></div>
            <button class="btn btn-ghost" @click="replay">▶ 重播</button>
          </div>

          <div class="mcurve-track" ref="trackEl">
            <div class="mcurve-tick start"></div>
            <div class="mcurve-dot" ref="dotEl"></div>
            <div class="mcurve-tick end"></div>
          </div>

          <div class="mcurve-bars">
            <div v-for="(h, i) in barsData" :key="i" class="mcurve-bar"
                 :class="{ active: i === activeSeg }" :style="{ height: h + '%' }"></div>
          </div>
          <div class="mcurve-bars-axis"><span>起步</span><span>中段</span><span>收尾</span></div>

          <div class="mcurve-readout">
            <span>进度 <b>{{ readout.progress }}%</b></span>
            <span>当前段 <b>{{ readout.seg + 1 }}/{{ readout.n }}</b></span>
            <span>本段占总位移 <b>{{ readout.share }}%</b></span>
            <span>USB 报告数 <b>{{ readout.totalReports }}</b></span>
            <span>实际设备时长(8KHz) <b>≈ {{ readout.realMs }} ms</b></span>
          </div>
          <div class="mcurve-viz-note">慢放演示(固定 ~2.2s)。设备按固定节拍逐 USB 报告吐位移，曲线体现在"每段走多远"——时间轴匀速、位移呈钟形，这就是"起步慢·中间快·收尾慢"。</div>
        </div>
      </div>

      <div class="macro-slot-empty" v-show="!cfg.enabled">
        <span>{{ t('mouseCurve.notEnabled') }}</span>
      </div>
    </div>
  </div>
</template>

<script setup>
import { reactive, ref, watch, onMounted, onUnmounted, nextTick } from 'vue';
import gsap from 'gsap';
import { useI18n } from '../i18n/index.js';

const { t } = useI18n();

const props = defineProps({
  config: { type: Object, default: () => ({ enabled: 0, profile: 2, segments: 4, duration: 0, jitter: 15 }) }
});
const emit = defineEmits(['save', 'reset', 'load']);

const dirty = ref(false);
const toast = ref('');
const toastType = ref('ok');
let toastTimer = null;

function showToast(msg, type) {
  toast.value = msg;
  toastType.value = type || 'ok';
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(function() { toast.value = ''; }, 2000);
}

const cfg = reactive({
  enabled: 0, profile: 2, segments: 4, duration: 0, jitter: 15
});

watch(() => props.config, (newCfg) => {
  if (!newCfg) return;
  cfg.enabled  = newCfg.enabled ? 1 : 0;
  cfg.profile  = newCfg.profile != null ? newCfg.profile : 2;
  cfg.segments = newCfg.segments != null ? newCfg.segments : 4;
  cfg.duration = newCfg.duration != null ? newCfg.duration : 0;
  cfg.jitter   = newCfg.jitter != null ? newCfg.jitter : 15;
  dirty.value = false;
}, { deep: true, immediate: true });

function markDirty() { dirty.value = true; }

function toggleEnabled() {
  cfg.enabled = cfg.enabled ? 0 : 1;
  markDirty();
}

function setNum(field, ev, minVal, maxVal) {
  var v = parseInt(ev.target.value, 10);
  if (isNaN(v)) v = minVal;
  if (v < minVal) v = minVal;
  if (v > maxVal) v = maxVal;
  cfg[field] = v;
  markDirty();
}

function saveAll() {
  emit('save', {
    enabled: cfg.enabled ? 1 : 0,
    profile: cfg.profile,
    segments: cfg.segments,
    duration: cfg.duration,
    jitter: cfg.jitter
  });
  dirty.value = false;
  showToast(t('mouseCurve.saved'), 'ok');
}

function onReset() {
  cfg.enabled = 0;
  cfg.profile = 2;
  cfg.segments = 4;
  cfg.duration = 0;
  cfg.jitter = 15;
  emit('reset');
  dirty.value = false;
  showToast(t('mouseCurve.resetDone'), 'ok');
}

function loadFromDevice() {
  emit('load');
  showToast(t('mouseCurve.refreshed'), 'ok');
}

/* ===== 曲线动画可视化（GSAP，逐字节复刻固件 mouse_curve_build）===== */
const CURVE_Q = 32768;
const trackEl = ref(null);
const dotEl = ref(null);
const barsData = ref([]);           // 每段柱高 %(归一化到最大段)
const activeSeg = ref(-1);
const readout = reactive({ progress: 0, seg: 0, n: 4, share: 0, totalReports: 0, realMs: 0 });

let tween = null;
let proxy = { p: 0 };
let curN = 4, curH = 1, curTotal = 4, curCum = null, seedCounter = 1;
let reduceMotion = false;

/* 原样移植固件 mouse_curve_build（定点 Q=32768）。profile: 0=linear 1=ease 2=min-jerk 3=random */
function buildCum(n, profile, jitterPct, seed) {
  n = Math.max(2, Math.min(32, n | 0));
  const cum = new Array(n + 1);
  cum[0] = 0; cum[n] = CURVE_Q;
  if (profile === 3) {
    let lcg = (seed || 0x12345678) >>> 0;
    const w = new Array(n); let wsum = 0;
    for (let i = 0; i < n; i++) {
      const t0 = i / n, t1 = (i + 1) / n;
      const p0 = 3 * t0 * t0 - 2 * t0 * t0 * t0;
      const p1 = 3 * t1 * t1 - 2 * t1 * t1 * t1;
      const r01 = ((lcg >>> 17) & 0x7FFF) - 16384;          // ~[-16384,16383]
      let wv = (p1 - p0) * (1 + r01 / 16384 * (jitterPct / 100));
      lcg = (Math.imul(lcg, 1664525) + 1013904223) >>> 0;   // 对照固件 LCG 常数
      if (wv < 0.02) wv = 0.02;
      const wi = Math.round(wv * CURVE_Q); w[i] = wi; wsum += wi;
    }
    if (wsum === 0) wsum = 1;
    let acc = 0;
    for (let i = 0; i < n; i++) { acc += w[i]; let c = Math.round(acc * CURVE_Q / wsum); if (c > CURVE_Q) c = CURVE_Q; cum[i + 1] = c; }
    for (let i = 1; i <= n; i++) if (cum[i] < cum[i - 1]) cum[i] = cum[i - 1];   // 单调不减
    cum[n] = CURVE_Q;
  } else {
    for (let i = 1; i < n; i++) {
      const tt = i / n;
      let p;
      if (profile === 1)      p = 3 * tt * tt - 2 * tt * tt * tt;                  /* smoothstep */
      else if (profile === 2) p = tt * tt * tt * (10 - 15 * tt + 6 * tt * tt);     /* min-jerk */
      else                    p = tt;                                              /* linear */
      let pi = Math.round(p * CURVE_Q); if (pi > CURVE_Q) pi = CURVE_Q; cum[i] = pi;
    }
    cum[n] = CURVE_Q;
  }
  return cum;
}

function computeH(n, durationMs) {
  let h;
  if (durationMs > 0) { h = Math.max(1, Math.round(durationMs * 8 / n)); }  /* 时长模式(8K: 8 报告/ms) */
  else { h = 8; }                                                          /* 快速模式(8K): 每段 8 报告平摊，对照固件 */
  if (n * h > 30000) h = Math.max(1, Math.floor(30000 / n));               /* 对照固件 30000 钳位 */
  return h;
}

function draw() {
  const p = proxy.p;
  const r = Math.min(curTotal - 1, Math.max(0, Math.floor(p * curTotal)));
  const seg = Math.min(curN - 1, Math.floor(r / curH));
  const intra = r % curH;
  const pos = curCum[seg] + (curCum[seg + 1] - curCum[seg]) * (intra + 1) / curH;
  const frac = pos / CURVE_Q;
  if (dotEl.value && trackEl.value) {
    const trackW = trackEl.value.clientWidth;
    const dotW = dotEl.value.offsetWidth || 14;
    gsap.set(dotEl.value, { x: frac * Math.max(0, trackW - dotW) });
  }
  activeSeg.value = seg;
  readout.progress = Math.round(p * 100);
  readout.seg = seg;
  readout.share = Math.round((curCum[seg + 1] - curCum[seg]) / CURVE_Q * 1000) / 10;
}

function replay() {
  if (!trackEl.value || !dotEl.value) return;
  curN = Math.max(2, Math.min(32, cfg.segments || 4));
  curH = computeH(curN, cfg.duration || 0);
  curTotal = curN * curH;
  curCum = buildCum(curN, cfg.profile, cfg.jitter || 0, seedCounter++);   /* 每次新种子→random 每次不同 */
  let maxW = 1; const ws = [];
  for (let i = 0; i < curN; i++) { const wv = curCum[i + 1] - curCum[i]; ws.push(wv); if (wv > maxW) maxW = wv; }
  barsData.value = ws.map(w => Math.max(3, Math.round(w / maxW * 100)));
  readout.n = curN;
  readout.totalReports = curTotal;
  readout.realMs = Math.round(curTotal * 0.125 * 10) / 10;   /* 8KHz: 0.125ms/报告 */
  if (tween) { tween.kill(); tween = null; }
  proxy.p = 0;
  gsap.set(dotEl.value, { x: 0, yPercent: -50 });
  activeSeg.value = -1;
  if (reduceMotion) { proxy.p = 1; draw(); return; }
  /* 关键：ease 必须是 "none"——设备按固定节拍吐报告，曲线在"空间位移"里、时间轴匀速 */
  tween = gsap.to(proxy, { p: 1, duration: 2.2, ease: 'none', onUpdate: draw, onComplete: draw });
}

function scheduleReplay() { if (cfg.enabled) nextTick(replay); }

watch(cfg, scheduleReplay, { deep: true });

onMounted(function () {
  if (window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches) reduceMotion = true;
  nextTick(function () { if (cfg.enabled) replay(); });
});
onUnmounted(function () { if (tween) tween.kill(); });
</script>

<style scoped>
.mcurve-page .mcurve-card {
  background: var(--bg-card, #1e2230);
  border: 1px solid var(--border-color, #2c3142);
  border-radius: 10px;
  padding: 18px 20px;
  margin-top: 14px;
}
.mcurve-page .mcurve-body {
  margin-top: 12px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.mcurve-page .mcurve-field-row {
  display: flex;
  gap: 24px;
  align-items: flex-start;
}
.mcurve-page .mcurve-col {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.mcurve-page .mcurve-note {
  font-size: 11px;
  color: var(--text-muted, #7a8294);
  line-height: 1.5;
  padding: 8px 10px;
  border-radius: 6px;
  background: var(--bg-elevated, #181b26);
}
.mcurve-page .mcurve-viz { margin-top: 6px; display: flex; flex-direction: column; gap: 10px; }
.mcurve-page .mcurve-viz-head { display: flex; align-items: center; gap: 10px; }
.mcurve-page .mcurve-track {
  position: relative; height: 22px;
  background: var(--bg-elevated, #181b26);
  border: 1px solid var(--border-color, #2c3142); border-radius: 11px;
}
.mcurve-page .mcurve-tick {
  position: absolute; top: 50%; width: 8px; height: 8px; border-radius: 50%;
  transform: translate(-50%, -50%); background: var(--text-muted, #7a8294);
}
.mcurve-page .mcurve-tick.start { left: 4px; }
.mcurve-page .mcurve-tick.end { left: auto; right: 4px; transform: translate(50%, -50%); }
.mcurve-page .mcurve-dot {
  position: absolute; left: 0; top: 50%; width: 14px; height: 14px; border-radius: 50%;
  background: var(--accent, #5b9cff); box-shadow: 0 0 8px rgba(91, 156, 255, .7);
  will-change: transform;
}
.mcurve-page .mcurve-bars { display: flex; align-items: flex-end; gap: 3px; height: 64px; padding: 0 4px; }
.mcurve-page .mcurve-bar {
  flex: 1; min-height: 3px; background: var(--border-color, #2c3142);
  border-radius: 3px 3px 0 0; transition: background .12s, box-shadow .12s;
}
.mcurve-page .mcurve-bar.active {
  background: var(--accent, #5b9cff); box-shadow: 0 0 6px rgba(91, 156, 255, .6);
}
.mcurve-page .mcurve-bars-axis {
  display: flex; justify-content: space-between; font-size: 10px;
  color: var(--text-muted, #7a8294); padding: 0 4px;
}
.mcurve-page .mcurve-readout {
  display: flex; flex-wrap: wrap; gap: 14px; font-size: 12px; color: var(--text-muted, #7a8294);
}
.mcurve-page .mcurve-readout b { color: var(--text, #e6e9f0); font-variant-numeric: tabular-nums; }
.mcurve-page .mcurve-viz-note { font-size: 11px; color: var(--text-muted, #7a8294); line-height: 1.5; }
</style>
