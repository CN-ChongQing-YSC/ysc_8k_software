<template>
  <div class="panel">
    <div class="panel-header">
      <svg class="panel-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <rect x="1" y="4" width="6" height="8" rx="1" /><rect x="9" y="4" width="6" height="8" rx="1" />
      </svg>
      <span>{{ t('device.title') }}</span>
      <button class="d-refresh" @click="emit('refresh')">{{ t('serial.refresh') }}</button>
    </div>
    <div class="panel-body">
      <div class="d-hint">{{ t('device.hint') }}</div>

      <div v-for="slot in slots" :key="slot.dev" class="d-card" :class="{ on: slot.connected }">
        <!-- 卡头：角色徽标 + 连接状态点 +（已连接时）端口 chip -->
        <div class="d-card-head">
          <span class="d-role-badge" :class="'role-' + roleKey(slot)">
            {{ roleLabel(slot) }}
          </span>
          <span class="d-status" :class="{ on: slot.connected }">
            <i class="d-dot" aria-hidden="true"></i>
            {{ slot.connected ? t('device.connected') : t('device.notConnected') }}
          </span>
          <span v-if="slot.connected" class="d-port">{{ slot.port }}<template v-if="slot.baud"> @ {{ slot.baud }}</template></span>
        </div>

        <!-- 参数区：标签 ← → 值 的右对齐行，整行可点。
             去掉输入框边框后，界面从「填表」变成「设置项」，视觉噪音大幅下降。
             字段顺序按用户操作顺序排：端口 → 波特率 → 角色。
             选端口是第一动作；角色放最后（多数时候由驱动自动识别，无需干预）。 -->
        <div class="d-rows">
          <template v-if="!slot.connected">
            <div class="d-row">
              <span class="d-k">{{ t('device.port') }}</span>
              <AppSelect
                v-model="pick[slot.dev].port"
                :options="portOptions"
                :placeholder="t('device.portPlaceholder')"
                :aria-label="t('device.port')"
              />
            </div>

            <div class="d-row">
              <span class="d-k">{{ t('device.baud') }}</span>
              <input
                class="d-input"
                v-model="pick[slot.dev].baud"
                :placeholder="t('device.baudHint')"
                :aria-label="t('device.baud')"
                inputmode="numeric"
                spellcheck="false"
              />
            </div>

            <div class="d-row">
              <span class="d-k">{{ t('device.roleLabel') }}</span>
              <AppSelect
                v-model="pick[slot.dev].role"
                :options="roleOptions"
                :aria-label="t('device.roleLabel')"
              />
            </div>
          </template>

          <template v-else>
            <div class="d-row">
              <span class="d-k">{{ t('device.upstream') }}</span>
              <span class="d-v wide">{{ upstreamText(slot) }}</span>
            </div>
            <!-- 已连接后的波特率切换：串口两端都要改，驱动会先让固件切、再重开
                 主机侧句柄。用于连上后升到高速率（透传吞吐/低延迟）。 -->
            <div class="d-row">
              <span class="d-k">{{ t('device.switchBaud') }}</span>
              <AppSelect
                v-if="!baudBusy[slot.dev]"
                :model-value="String(slot.baud || '')"
                :options="baudOptions(slot)"
                :aria-label="t('device.switchBaud')"
                @update:model-value="onSwitchBaud(slot, $event)"
              />
              <template v-else>
                <span class="d-v mono">{{ slot.baud }}</span>
                <span class="d-tag warn">{{ t('device.switching') }}</span>
              </template>
            </div>
            <!-- 角色同样放最后 -->
            <div class="d-row">
              <span class="d-k">{{ t('device.roleLabel') }}</span>
              <AppSelect
                :model-value="slot.role === 'none' ? 'auto' : slot.role"
                :options="roleOptions"
                :aria-label="t('device.roleLabel')"
                @update:model-value="onRoleChange(slot, $event)"
              />
              <span v-if="slot.role !== 'none' && slot.auto" class="d-tag">{{ t('device.auto') }}</span>
              <span v-else-if="slot.role === 'none' && !slot.probeDone" class="d-tag warn">{{ t('device.detecting') }}</span>
              <span v-else-if="slot.role === 'none' && slot.probeDone" class="d-tag warn">{{ t('device.detectFailed') }}</span>
            </div>
          </template>
        </div>

        <!-- 操作区：接入 / 拔出 合成一个分段控件。
             未连接时「拔出」不可选 —— 断开一个本来就没连上的设备是无意义操作，
             从界面结构上就消灭了这个误点可能。
             两个选项始终同时存在（只是禁用），用户看到的是「档位在、暂时推不动」，
             而不是控件凭空消失。 -->
        <div class="d-seg" role="group" :aria-label="t('device.actions')">
          <button
            v-if="!slot.connected"
            class="d-opt"
            data-kind="on"
            :disabled="!pick[slot.dev].port"
            :title="pick[slot.dev].port ? t('device.connect') : t('device.needPort')"
            @click="onConnect(slot)"
          >
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6.5 3H4a1 1 0 0 0-1 1v8a1 1 0 0 0 1 1h2.5" /><path d="M10.5 11 14 8l-3.5-3" /><path d="M14 8H6.5" /></svg>
            {{ t('device.connect') }}
          </button>
          <template v-else>
            <button class="d-opt" data-kind="on" :disabled="baudBusy[slot.dev]" @click="onConnect(slot)">
              <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6.5 3H4a1 1 0 0 0-1 1v8a1 1 0 0 0 1 1h2.5" /><path d="M10.5 11 14 8l-3.5-3" /><path d="M14 8H6.5" /></svg>
              {{ t('device.reconnect') }}
            </button>
            <button class="d-opt" data-kind="off" :disabled="baudBusy[slot.dev]" @click="onDisconnect(slot)">
              <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9.5 3H12a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1H9.5" /><path d="M5.5 5 2 8l3.5 3" /><path d="M2 8h7.5" /></svg>
              {{ t('device.disconnect') }}
            </button>
          </template>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed, reactive } from 'vue';
import { AppSelect } from '@ysc/core/ui';
import { useI18n } from '../i18n/index.js';

const { t } = useI18n();

const props = defineProps({
  /** 设备槽位列表（来自驱动器 device_list 事件） */
  slots: { type: Array, default: () => [] },
  /** 可选串口列表 */
  ports: { type: Array, default: () => [] },
});

const emit = defineEmits(['connect', 'disconnect', 'set-role', 'set-baud', 'refresh']);

// 每个槽位的待连接端口/波特率/角色。
// role 默认 'auto'（交给驱动探测）；用户可先选好角色再连接 —— 因为
// device_attach 会把槽位角色重置为 none，不带上就会被清掉。
const pick = reactive({
  0: { port: '', baud: '0', role: 'auto' },
  1: { port: '', baud: '0', role: 'auto' },
});

/* 已连接后可切换的波特率，与驱动 SUPPORTED_BAUDS / 固件白名单对齐。
 * 最高档 4000000 供高速透传用。切到设备跑不起来的速率时驱动会自动回退到原
 * 速率并报错，不会把设备留在失联状态。 */
const BAUDS = [115200, 230400, 460800, 921600, 1000000, 1500000, 2000000, 3000000, 4000000];

/* 下拉选项。用 computed 而不是模块级常量：文案来自 i18n，
   写成常量会在语言切换后停留在旧语言。 */
const portOptions = computed(() => [
  // 空值项让用户可以把已选端口清掉（回到未选状态）
  { value: '', label: t('device.portPlaceholder') },
  ...props.ports.map((p) => ({ value: p, label: p })),
]);
const roleOptions = computed(() => [
  { value: 'auto', label: t('device.roleAuto') },
  { value: 'mouse', label: t('device.roleMouse') },
  { value: 'kbd', label: t('device.roleKbd') },
]);

/** 已连接时的波特率选项：把当前速率置顶并去重，避免列表里出现两个相同值 */
function baudOptions(slot) {
  const cur = String(slot.baud || '');
  const list = BAUDS.map(String);
  const ordered = cur && list.includes(cur) ? [cur, ...list.filter((b) => b !== cur)] : list;
  return ordered.map((b) => ({ value: b, label: b }));
}

// 切换进行中标记：切换要关端口重开，需要几百毫秒，期间禁用下拉与操作按钮
// 避免重复触发。注意「断开」在切换中也被禁用 —— 驱动侧切换是原子的，
// 中途插断开会让端口状态机错乱。
const baudBusy = reactive({ 0: false, 1: false });
let baudBusyTimer = null;

function roleKey(slot) {
  if (!slot.connected) return 'none';
  if (slot.role === 'mouse') return 'mouse';
  if (slot.role === 'kbd') return 'kbd';
  return 'none';
}

function roleLabel(slot) {
  const k = roleKey(slot);
  if (k === 'mouse') return t('device.roleMouse');
  if (k === 'kbd') return t('device.roleKbd');
  return t('device.roleNone');
}

function upstreamText(slot) {
  if (slot.vid == null || slot.vid < 0) return t('device.upstreamUnknown');
  const vid = slot.vid.toString(16).toUpperCase().padStart(4, '0');
  const pid = (slot.pid >= 0 ? slot.pid : 0).toString(16).toUpperCase().padStart(4, '0');
  return (slot.product ? slot.product + '  ' : '') + vid + ':' + pid;
}

function onConnect(slot) {
  const p = pick[slot.dev];
  emit('connect', slot.dev, p.port, parseInt(p.baud) || 0, p.role || 'auto');
}

function onDisconnect(slot) {
  emit('disconnect', slot.dev);
}

function onRoleChange(slot, v) {
  // auto = 交回驱动自动识别（置为 none，驱动会重新探测）
  // 记下来：未连接时它会在 next connect 里带上去；已连接时立即生效
  pick[slot.dev].role = v;
  emit('set-role', slot.dev, v === 'auto' ? 'none' : v);
}

function onSwitchBaud(slot, raw) {
  const baud = parseInt(raw);
  if (!baud || baud === slot.baud) return;
  baudBusy[slot.dev] = true;
  emit('set-baud', slot.dev, baud);
  // 切换含关端口/重开/重探角色，给足时间；驱动成功或失败都会刷 device_list
  if (baudBusyTimer) clearTimeout(baudBusyTimer);
  baudBusyTimer = setTimeout(function () { baudBusy[slot.dev] = false; }, 3000);
}
</script>

<style scoped>
/* ── 面板标题栏的「刷新」：小尺寸次级动作 ─────────────────── */
.d-refresh {
  margin-left: auto;
  padding: 2px 10px;
  font-size: 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  transition: color var(--transition-fast), border-color var(--transition-fast);
}
.d-refresh:hover { border-color: var(--border-hover); color: var(--text-primary); }

.d-hint {
  font-size: 11px;
  color: var(--text-dim);
  margin-bottom: 8px;
  line-height: 1.5;
}

/* ── 设备卡片 ─────────────────────────────────────────────── */
.d-card {
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: 9px 10px 10px;
  margin-bottom: 8px;
  background: rgba(255, 255, 255, 0.02);
  transition: border-color var(--transition-fast);
}
/* 已连接：左侧一道 2px 状态脊 + 边框转绿。
   颜色之外还有「形状」这条通道（有脊/无脊），不依赖单一颜色表达状态。 */
.d-card.on {
  border-color: var(--accent-green);
  border-left: 2px solid var(--accent-green);
  padding-left: 9px;
}

.d-card-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 4px;
}

.d-role-badge {
  font-size: 11px;
  padding: 1px 8px;
  border-radius: 10px;
  border: 1px solid var(--border);
  color: var(--text-muted);
}
.d-role-badge.role-mouse { border-color: #4a9; color: #6cb; }
.d-role-badge.role-kbd { border-color: #c96; color: #dba; }

.d-status {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 11px;
  color: var(--text-dim);
}
/* 状态圆点：未连接 6px 空心，已连接 6px 实心 + 辉光。
   空心/实心是形状差异，色弱用户也能分辨。 */
.d-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  border: 1px solid currentColor;
  background: transparent;
  flex-shrink: 0;
}
.d-status.on { color: var(--accent-green); }
.d-status.on .d-dot { background: currentColor; box-shadow: 0 0 6px var(--accent-green); }

.d-port {
  font-size: 11px;
  font-family: var(--font-mono);
  color: var(--text-muted);
  margin-left: auto;
  padding: 1px 7px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--bg-input);
}

/* ── 参数行：标签 ←→ 控件 ──────────────────────────────────
   原生 select 已换成 AppSelect（自定义下拉）—— 原生弹层用的是系统配色，
   深色主题下会弹出白色列表，且弹层宽度跟随那个铺满整行的透明 select，
   与右侧小值完全不匹配。这些都无法用 CSS 定制，只能换掉。
   行的 hover 只作用于真正含交互控件的行。 */
.d-rows {
  display: flex;
  flex-direction: column;
  gap: 1px;
  margin-bottom: 8px;
}
.d-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 30px;
  padding: 0 8px;
  border-radius: var(--radius-sm);
  transition: background var(--transition-fast);
}
.d-row:has(.asel), .d-row:has(.d-input) { cursor: pointer; }
.d-row:has(.asel):hover, .d-row:has(.d-input):hover { background: rgba(255, 255, 255, 0.045); }
/* 切换波特率进行中：整行锁住，不响应 hover */
.d-row:has(.d-tag.warn) { cursor: default; }

.d-k {
  font-size: 11px;
  color: var(--text-muted);
  min-width: 46px;
  flex-shrink: 0;
  pointer-events: none;
}

/* 波特率是自由数值（驱动允许 0=自动 或任意白名单速率），保持文本输入。
   右对齐 + 等宽，视觉上与右侧的 AppSelect 值同一列。 */
.d-input {
  flex: 1;
  min-width: 0;
  height: 24px;
  padding: 0;
  background: none;
  border: none;
  outline: none;
  font-family: var(--font-mono);
  font-size: 12px;
  font-weight: 600;
  color: var(--text-primary);
  text-align: right;
}
.d-input::placeholder { color: var(--text-muted); font-weight: 400; font-family: var(--font-ui); }
.d-input:focus-visible { color: var(--text-heading); }

.d-v {
  margin-left: auto;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 66%;
  pointer-events: none;
}
.d-v.mono { font-family: var(--font-mono); letter-spacing: 0.02em; }
.d-v.wide { font-weight: 400; color: var(--text-secondary); font-size: 11px; }
/* 空值提示（-- 选择端口 --）是真实需要读到的提示文字，用 --text-muted */
.d-v.empty { color: var(--text-muted); font-weight: 400; }

.d-tag {
  font-size: 10px;
  padding: 0 5px;
  border-radius: 3px;
  border: 1px solid var(--border);
  color: var(--text-muted);
  white-space: nowrap;
  flex-shrink: 0;
  margin-left: auto;
  pointer-events: none;
}
.d-tag.warn { border-color: #c96; color: #dba; }
/* 带标签时值不再顶到最右，由标签占据剩余空间 */
.d-v + .d-tag { margin-left: 6px; }

/* ── 分段控件：接入 / 拔出 ─────────────────────────────────
   3px 内凹凹槽 + 两个等宽档位，像物理拨档开关。
   选中态实心填充（接入=品牌绿，拔出=危险红），未选中态纯文字。 */
.d-seg {
  display: flex;
  gap: 3px;
  padding: 3px;
  background: var(--bg-input);
  border: 1px solid var(--border);
  border-radius: var(--btn-radius);
}
.d-opt {
  flex: 1;
  height: 28px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-dim);
  font-family: var(--font-ui);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  transition: background var(--transition-fast), color var(--transition-fast);
}
.d-opt svg { width: 13px; height: 13px; }

/* 接入 = 主操作：实心强调色 */
.d-opt[data-kind="on"]:not(:disabled) {
  background: var(--accent-blue);
  color: var(--bg-primary);
}
.d-opt[data-kind="on"]:not(:disabled):hover { background: #62d0fa; }

/* 拔出 = 危险操作：默认只要描边，hover 才满血，静态界面里不再到处是红色 */
.d-opt[data-kind="off"] {
  border: 1px solid transparent;
}
.d-opt[data-kind="off"]:not(:disabled) {
  color: var(--accent-red);
  border-color: #9b2020;
}
.d-opt[data-kind="off"]:not(:disabled):hover {
  background: var(--accent-red);
  border-color: var(--accent-red);
  color: #fff;
}

/* 禁用档位：底凹下去、文字压暗到 --text-dim，保留档位形状让用户知道「它存在」。
   注意：这里不要用 --text-ghost —— tokens.css 并未定义该变量，
   var() 未定义会让整条 color 声明失效、字色回落到继承值（白），
   实测会变成「禁用档位比可用档位还亮」。 */
.d-opt:disabled {
  color: var(--text-dim);
  background: rgba(0, 0, 0, 0.18);
  cursor: not-allowed;
}
</style>
