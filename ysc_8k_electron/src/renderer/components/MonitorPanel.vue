<template>
  <div class="panel" style="flex: 1">
    <div class="panel-header">
      <svg class="panel-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><polyline points="1 12 4 7 7 9 10 4 15 4" /></svg>
      <span>{{ t('monitor.title') }}</span>
      <!-- 监控开关：把「开始监控 / 停止监控」这个动作改成明确的 on/off 开关。
           原地不动地留在标题栏右侧，用户不需要在界面里找按钮。 -->
      <button
        class="mon-switch"
        :aria-pressed="uploadEnabled ? 'true' : 'false'"
        :disabled="!connected"
        :title="uploadEnabled ? t('monitor.stop') : t('monitor.start')"
        @click="toggleUpload"
      >
        <span class="mon-switch-label">
          {{ uploadEnabled ? t('monitor.monitoring') : t('monitor.notMonitoring') }}
        </span>
        <span class="mon-track" aria-hidden="true"><span class="mon-knob"></span></span>
      </button>
    </div>
    <div class="panel-body" style="flex: 1">
      <!-- 仪表盘：X / Y 是两个核心读数，给最大字号；鼠标按键作为整宽一行。
           值为 0 时降级为弱化字重与颜色 —— 0 不该和 1 一样醒目。 -->
      <div class="mon-tiles">
        <div class="mon-tile" :class="{ hot: x !== 0 }" :data-zero="x === 0 ? 'true' : 'false'">
          <span class="mon-tile-k">X</span>
          <span class="mon-tile-v">{{ x }}</span>
          <span class="mon-tile-s">{{ t('monitor.count') }}</span>
        </div>
        <div class="mon-tile" :class="{ hot: y !== 0 }" :data-zero="y === 0 ? 'true' : 'false'">
          <span class="mon-tile-k">Y</span>
          <span class="mon-tile-v">{{ y }}</span>
          <span class="mon-tile-s">{{ t('monitor.count') }}</span>
        </div>
        <div class="mon-tile wide" :class="{ hot: buttons !== 0 }">
          <span class="mon-tile-k">{{ t('monitor.mouseDev') }} · {{ t('monitor.buttons') }}</span>
          <span class="mon-tile-v small" :class="{ zero: buttons === 0 }">{{ buttonLabels }}</span>
        </div>
      </div>

      <!-- 键盘机物理状态：显示用户真实按下的键（不含上位机注入的键）。
           仅在「键鼠集火」板块显示（showKbd）；主页是单串口路径，没有键盘机概念。 -->
      <div v-if="showKbd" class="kbd-block">
        <div class="kbd-head">
          <span class="mon-tile-k">{{ t('monitor.kbdDev') }}</span>
          <span v-if="!kbd || !kbd.valid" class="mon-sub">{{ t('monitor.noData') }}</span>
          <span v-else class="mon-sub mon-live"><i class="mon-pulse"></i>{{ t('monitor.live') }}</span>
        </div>
        <div class="kbd-kv">
          <span class="kbd-k">{{ t('monitor.kbdMods') }}</span>
          <span class="kbd-v" :class="{ empty: modLabels === t('monitor.kbdNone') }">{{ modLabels }}</span>
        </div>
        <div class="kbd-kv">
          <span class="kbd-k">{{ t('monitor.kbdKeys') }}</span>
          <span class="kbd-v" :class="{ empty: keyLabels === t('monitor.kbdNone') }">{{ keyLabels }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed } from 'vue';
import { useI18n } from '../i18n/index.js';

const { t } = useI18n();

const props = defineProps({
  buttons: { type: Number, default: 0 },
  x: { type: Number, default: 0 },
  y: { type: Number, default: 0 },
  connected: { type: Boolean, default: false },
  /** 键盘物理状态快照 {valid, mod, count, overflow, keys[]} */
  kbd: { type: Object, default: null },
  /** 是否显示键盘机区块（双设备「键鼠集火」板块才需要） */
  showKbd: { type: Boolean, default: false },
});

const emit = defineEmits(['toggle-upload']);

const uploadEnabled = ref(false);

function toggleUpload() {
  uploadEnabled.value = !uploadEnabled.value;
  emit('toggle-upload', uploadEnabled.value);
}

// 用普通函数而不是模块级 computed：computed 在 setup 里创建时只求值一次，
// 语言切换时不会重算，按键名会停留在旧语言。放进 buttonLabels 里现算。
function btnMap() {
  return [
    [1, t('monitor.btn.left')],
    [2, t('monitor.btn.right')],
    [4, t('monitor.btn.middle')],
    [8, t('monitor.btn.back')],
    [16, t('monitor.btn.forward')],
  ];
}

const buttonLabels = computed(() => {
  if (props.buttons === 0) return t('monitor.none');
  const labels = btnMap().filter(([bit]) => props.buttons & bit).map(([, label]) => label);
  return labels.length > 0 ? labels.join('+') : String(props.buttons);
});

// HID 修饰键位图 bit0..7 = 0xE0..0xE7（左Ctrl/左Shift/左Alt/左GUI/右Ctrl/右Shift/右Alt/右GUI）
const MOD_NAMES = ['LCtrl', 'LShift', 'LAlt', 'LGUI', 'RCtrl', 'RShift', 'RAlt', 'RGUI'];

const modLabels = computed(() => {
  const k = props.kbd;
  if (!k || !k.valid || !k.mod) return t('monitor.kbdNone');
  const out = MOD_NAMES.filter((_, i) => k.mod & (1 << i));
  return out.length ? out.join('+') : t('monitor.kbdNone');
});

// HID 键码 -> 可读名（常用键；未收录的直接显示 0xNN）
const HID_NAMES = {
  0x04: 'A', 0x05: 'B', 0x06: 'C', 0x07: 'D', 0x08: 'E', 0x09: 'F', 0x0A: 'G',
  0x0B: 'H', 0x0C: 'I', 0x0D: 'J', 0x0E: 'K', 0x0F: 'L', 0x10: 'M', 0x11: 'N',
  0x12: 'O', 0x13: 'P', 0x14: 'Q', 0x15: 'R', 0x16: 'S', 0x17: 'T', 0x18: 'U',
  0x19: 'V', 0x1A: 'W', 0x1B: 'X', 0x1C: 'Y', 0x1D: 'Z',
  0x1E: '1', 0x1F: '2', 0x20: '3', 0x21: '4', 0x22: '5', 0x23: '6', 0x24: '7',
  0x25: '8', 0x26: '9', 0x27: '0',
  0x28: 'Enter', 0x29: 'Esc', 0x2A: 'Backspace', 0x2B: 'Tab', 0x2C: 'Space',
  0x2D: '-', 0x2E: '=', 0x2F: '[', 0x30: ']', 0x31: '\\', 0x33: ';', 0x34: "'",
  0x35: '`', 0x36: ',', 0x37: '.', 0x38: '/', 0x39: 'CapsLock',
  0x3A: 'F1', 0x3B: 'F2', 0x3C: 'F3', 0x3D: 'F4', 0x3E: 'F5', 0x3F: 'F6',
  0x40: 'F7', 0x41: 'F8', 0x42: 'F9', 0x43: 'F10', 0x44: 'F11', 0x45: 'F12',
  0x49: 'Insert', 0x4A: 'Home', 0x4B: 'PgUp', 0x4C: 'Delete', 0x4D: 'End',
  0x4E: 'PgDn', 0x4F: 'Right', 0x50: 'Left', 0x51: 'Down', 0x52: 'Up',
};

function keyName(kc) {
  return HID_NAMES[kc] || ('0x' + kc.toString(16).toUpperCase().padStart(2, '0'));
}

const keyLabels = computed(() => {
  const k = props.kbd;
  if (!k || !k.valid || !k.keys || k.keys.length === 0) return t('monitor.kbdNone');
  let s = k.keys.map(keyName).join('+');
  if (k.overflow > 0) s += ' ' + t('monitor.kbdMore').replace('{n}', String(k.overflow));
  return s;
});
</script>

<style scoped>
/* ── 监控开关（标题栏右侧）──────────────────────────────── */
.mon-switch {
  margin-left: auto;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 0;
  background: none;
  border: none;
  cursor: pointer;
  font-family: var(--font-ui);
  font-size: 11px;
  color: var(--text-dim);
  transition: color var(--transition-fast);
}
.mon-switch-label { letter-spacing: 0.02em; }
.mon-track {
  width: 38px;
  height: 20px;
  padding: 2px;
  display: flex;
  align-items: center;
  border-radius: 999px;
  background: var(--bg-input);
  border: 1px solid var(--border);
  /* 只用 tokens.css 里确实存在的变量：--transition-base 未定义，
     var() 失效会让 transition 整条声明作废（滑块瞬移） */
  transition: background var(--transition-fast), border-color var(--transition-fast);
}
.mon-knob {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--text-dim);
  transition: transform var(--transition-fast), background var(--transition-fast);
}
/* 开启态：轨道填充强调色，滑块右移。
   位置变化（形状）+ 颜色双通道，不依赖单一颜色表达状态。 */
.mon-switch[aria-pressed="true"] { color: var(--accent-green); font-weight: 600; }
.mon-switch[aria-pressed="true"] .mon-track {
  background: var(--accent-green);
  border-color: var(--accent-green);
}
.mon-switch[aria-pressed="true"] .mon-knob {
  transform: translateX(18px);
  background: #ffffff;
}
.mon-switch:disabled { opacity: 0.4; cursor: not-allowed; }

/* ── 仪表盘 ─────────────────────────────────────────────── */
.mon-tiles {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
}
.mon-tile {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 10px 12px;
  background: var(--bg-input);
  border: 1px solid var(--border);
  border-radius: var(--btn-radius);
  min-width: 0;
}
.mon-tile.wide { grid-column: span 2; }
.mon-tile.hot { border-color: var(--accent-blue); }

.mon-tile-k {
  font-family: var(--font-mono);
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.08em;
  color: var(--text-dim);
  text-transform: uppercase;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.mon-tile-v {
  font-family: var(--font-mono);
  font-size: 26px;
  font-weight: 700;
  line-height: 1.15;
  color: var(--text-primary);
  font-variant-numeric: tabular-nums;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.mon-tile-v.small { font-size: 17px; }
/* 归零：降为弱化字重与颜色，降低视觉权重但依然可读 */
.mon-tile[data-zero="true"] .mon-tile-v { color: var(--text-dim); font-weight: 400; }
.mon-tile-v.zero { color: var(--text-dim); font-weight: 400; }
.mon-tile.hot .mon-tile-v { color: var(--accent-blue); }

.mon-tile-s {
  font-size: 10px;
  color: var(--text-dim);
  letter-spacing: 0.04em;
}

/* ── 键盘机 ─────────────────────────────────────────────── */
.kbd-block {
  margin-top: 10px;
  padding-top: 10px;
  border-top: 1px solid var(--border-soft);
}
.kbd-head {
  display: flex;
  align-items: baseline;
  gap: 8px;
  margin-bottom: 6px;
}
.mon-sub { font-size: 11px; color: var(--text-dim); display: inline-flex; align-items: center; gap: 5px; }
.mon-live { color: var(--accent-green); font-weight: 600; }
/* 实时脉冲：仅用 transform/opacity，GPU 合成，不触发布局 */
.mon-pulse {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--accent-green);
  animation: mon-breathe 1.6s ease-in-out infinite;
}
@keyframes mon-breathe {
  0%, 100% { transform: scale(1); opacity: 1; }
  50%      { transform: scale(0.55); opacity: 0.45; }
}

.kbd-kv {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 6px 8px;
  border-radius: var(--radius-sm);
  background: rgba(0, 0, 0, 0.18);
}
.kbd-kv + .kbd-kv { margin-top: 4px; }
.kbd-k {
  font-size: 11px;
  color: var(--text-muted);
  min-width: 46px;
  flex-shrink: 0;
}
.kbd-v {
  margin-left: auto;
  font-family: var(--font-mono);
  font-size: 12px;
  font-weight: 600;
  color: var(--text-primary);
  text-align: right;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.kbd-v.empty { color: var(--text-dim); font-weight: 400; }

/* 尊重系统「减少动态效果」：脉冲停用，改为常亮（不是放慢） */
@media (prefers-reduced-motion: reduce) {
  .mon-pulse { animation: none; }
  .mon-knob, .mon-track { transition: none; }
}
</style>
