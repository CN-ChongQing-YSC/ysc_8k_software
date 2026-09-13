<template>
  <div class="panel" style="flex: 1">
    <div class="panel-header">
      <svg class="panel-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><polyline points="1 12 4 7 7 9 10 4 15 4" /></svg>
      <span>{{ t('monitor.title') }}</span>
      <button
        class="mon-toggle"
        :class="{ active: uploadEnabled }"
        :disabled="!connected"
        @click="toggleUpload"
      >{{ uploadEnabled ? t('monitor.stop') : t('monitor.start') }}</button>
    </div>
    <div class="panel-body" style="flex: 1">
      <div class="mon-grid">
        <div class="mon-cell">
          <span class="mon-label">{{ t('monitor.mouseDev') }}</span>
          <span class="mon-val">{{ buttonLabels }}</span>
        </div>
        <div class="mon-cell">
          <span class="mon-label">X</span>
          <span class="mon-val">{{ x }}</span>
        </div>
        <div class="mon-cell">
          <span class="mon-label">Y</span>
          <span class="mon-val">{{ y }}</span>
        </div>
      </div>

      <!-- 键盘机物理状态：显示用户真实按下的键（不含上位机注入的键） -->
      <div class="kbd-block">
        <div class="kbd-head">
          <span class="mon-label">{{ t('monitor.kbdDev') }}</span>
          <span v-if="!kbd || !kbd.valid" class="mon-sub">{{ t('monitor.noData') }}</span>
          <span v-else class="mon-sub mon-live">{{ t('monitor.live') }}</span>
          <span class="mon-sub" style="margin-left:auto">
            {{ t('monitor.rawCount') }}: {{ rawKbdCount }}
          </span>
        </div>
        <div class="mon-grid">
          <div class="mon-cell">
            <span class="mon-label">{{ t('monitor.kbdMods') }}</span>
            <span class="mon-val">{{ modLabels }}</span>
          </div>
          <div class="mon-cell wide">
            <span class="mon-label">{{ t('monitor.kbdKeys') }}</span>
            <span class="mon-val">{{ keyLabels }}</span>
          </div>
        </div>
      </div>

      <!-- 串口原始帧：驱动收到的每一帧原文。
           排障用途：界面上「看不到键盘数据」有两种完全不同的原因 —— 固件没发，
           或被某一层吃掉了。这里只要出现 message:"kbd" 就说明帧到了驱动，
           问题在后端消费/渲染；完全没有就说明固件侧没上报。 -->
      <div class="raw-block">
        <div class="raw-head">
          <span class="mon-label">{{ t('monitor.rawTitle') }}</span>
          <button class="raw-clear" @click="$emit('clear-raw')">{{ t('monitor.rawClear') }}</button>
        </div>
        <div v-if="!rawFrames || rawFrames.length === 0" class="raw-empty">
          {{ t('monitor.rawEmpty') }}
        </div>
        <div v-else class="raw-list">
          <div
            v-for="(f, i) in rawFrames.slice().reverse().slice(0, 12)"
            :key="i"
            class="raw-row"
            :class="{ 'raw-kbd': f.kind === 'kbd' || f.kind === 'kbd_raw' }"
          >
            <span class="raw-t">{{ f.t }}</span>
            <span class="raw-kind">{{ f.kind }}</span>
            <span class="raw-text" :title="f.text">{{ f.text }}</span>
          </div>
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
  /** 驱动实际收到的串口原始帧 [{t,kind,len,text}]（排障用） */
  rawFrames: { type: Array, default: () => [] },
});

const emit = defineEmits(['toggle-upload', 'clear-raw']);

/* 收到过多少条真实按键帧（kind=kbd）。这个计数是「固件有没有把按键上报出来」
 * 最直接的证据：一直为 0 说明固件侧没发，>0 但面板无数据就是后端/渲染问题。 */
const rawKbdCount = computed(() =>
  (props.rawFrames || []).filter(function (f) { return f.kind === 'kbd'; }).length
);

const uploadEnabled = ref(false);

function toggleUpload() {
  uploadEnabled.value = !uploadEnabled.value;
  emit('toggle-upload', uploadEnabled.value);
}

const BTN_MAP = computed(() => [
  [1, t('monitor.btn.left')],
  [2, t('monitor.btn.right')],
  [4, t('monitor.btn.middle')],
  [8, t('monitor.btn.back')],
  [16, t('monitor.btn.forward')],
]);

const buttonLabels = computed(() => {
  if (props.buttons === 0) return t('monitor.none');
  const labels = BTN_MAP.value.filter(([bit]) => props.buttons & bit).map(([, label]) => label);
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
.kbd-block {
  margin-top: 10px;
  padding-top: 8px;
  border-top: 1px dashed var(--border, #444);
}
.kbd-head {
  display: flex;
  align-items: baseline;
  gap: 8px;
  margin-bottom: 6px;
}
.mon-sub { font-size: 11px; opacity: 0.5; }
.mon-live { opacity: 1; color: var(--accent, #4a9); }

/* ---- 串口原始帧列表 ---- */
.raw-block {
  margin-top: 10px;
  padding-top: 8px;
  border-top: 1px dashed var(--border, #444);
}
.raw-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 4px;
}
.raw-clear {
  margin-left: auto;
  padding: 1px 8px;
  font-size: 11px;
  border: 1px solid var(--border, #444);
  border-radius: 3px;
  background: transparent;
  color: var(--text, #ccc);
  cursor: pointer;
}
.raw-empty { font-size: 11px; opacity: 0.45; padding: 4px 0; }
.raw-list {
  max-height: 190px;
  overflow-y: auto;
  font-family: Consolas, "Courier New", monospace;
  font-size: 10.5px;
  line-height: 1.45;
}
.raw-row {
  display: flex;
  gap: 6px;
  padding: 1px 0;
  white-space: nowrap;
}
.raw-row.raw-kbd { color: var(--accent, #4a9); font-weight: 600; }
.raw-t { opacity: 0.5; flex: 0 0 auto; }
.raw-kind { flex: 0 0 48px; opacity: 0.8; }
.raw-text {
  flex: 1 1 auto;
  overflow: hidden;
  text-overflow: ellipsis;
}
.mon-cell.wide { flex: 2; }
</style>

<style scoped>
.mon-toggle {
  margin-left: auto;
  padding: 2px 10px;
  font-size: 12px;
  border: 1px solid var(--border, #444);
  border-radius: 4px;
  background: transparent;
  color: var(--text, #ccc);
  cursor: pointer;
  transition: all 0.15s;
}
.mon-toggle:hover:not(:disabled) {
  border-color: var(--accent, #4a9);
}
.mon-toggle.active {
  background: var(--accent, #4a9);
  color: #fff;
  border-color: var(--accent, #4a9);
}
.mon-toggle:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
