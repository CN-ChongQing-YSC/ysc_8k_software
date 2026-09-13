<template>
  <div class="asel" :class="{ disabled, open }">
    <button
      ref="triggerRef"
      type="button"
      class="asel-trigger"
      role="combobox"
      aria-haspopup="listbox"
      :aria-expanded="open"
      :aria-controls="listId"
      :aria-activedescendant="open && activeIndex >= 0 ? optionId(activeIndex) : undefined"
      :aria-label="ariaLabel"
      :disabled="disabled"
      @click="toggle"
      @keydown="onKeydown"
    >
      <span class="asel-value" :class="{ empty: !hasValue }">{{ display }}</span>
      <svg class="asel-chev" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M4 6.5 8 10.5 12 6.5" />
      </svg>
    </button>

    <!-- 弹层 Teleport 到 body：面板本身 overflow:hidden，留在原地会被裁掉。
         定位用 fixed + 触发器矩形，右边缘对齐，宽度按内容自适应。 -->
    <Teleport to="body">
      <div
        v-if="open"
        :id="listId"
        ref="panelRef"
        class="asel-panel"
        role="listbox"
        :style="panelStyle"
      >
        <button
          v-for="(opt, i) in options"
          :id="optionId(i)"
          :key="String(opt.value)"
          type="button"
          class="asel-opt"
          role="option"
          :aria-selected="isSelected(opt.value)"
          :class="{ active: i === activeIndex, selected: isSelected(opt.value) }"
          :disabled="opt.disabled"
          @click="choose(opt)"
          @mousemove="activeIndex = i"
        >
          <span class="asel-opt-label">{{ opt.label }}</span>
          <svg v-if="isSelected(opt.value)" class="asel-tick" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M3.5 8.5 6.5 11.5 12.5 5" />
          </svg>
        </button>
      </div>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue';

export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

const props = withDefaults(
  defineProps<{
    modelValue: string;
    options: SelectOption[];
    /** 未选中时显示的占位文案 */
    placeholder?: string;
    disabled?: boolean;
    /** 无障碍名称（触发器本身没有可见文本时必填） */
    ariaLabel?: string;
    /** 弹层最小宽度，默认跟随触发器宽度 */
    panelMinWidth?: number;
  }>(),
  { placeholder: '', disabled: false, ariaLabel: undefined, panelMinWidth: 0 },
);

const emit = defineEmits<{ 'update:modelValue': [string] }>();

const triggerRef = ref<HTMLButtonElement | null>(null);
const panelRef = ref<HTMLElement | null>(null);
const open = ref(false);
const activeIndex = ref(-1);
const panelStyle = ref<Record<string, string>>({});

// 每个实例一个唯一 id 前缀，供 aria-activedescendant / aria-controls 关联
const uid = 'asel-' + Math.random().toString(36).slice(2, 9);
const listId = uid + '-list';
const optionId = (i: number) => uid + '-opt-' + i;

const currentLabel = computed(() => {
  const hit = props.options.find((o) => o.value === props.modelValue);
  return hit ? hit.label : '';
});
const hasValue = computed(() => currentLabel.value !== '');
const display = computed(() => (hasValue.value ? currentLabel.value : props.placeholder));

function positionPanel(): void {
  const el = triggerRef.value;
  if (!el) return;
  const r = el.getBoundingClientRect();
  const gap = 4;
  const vw = window.innerWidth;
  const vh = window.innerHeight;

  // 宽度按「最长选项文案」估算，而不是跟随触发器。
  // 触发器在 DevicePanel 里是铺满整行的（可达上千 px），若直接跟随，
  // 弹层会变成一条横贯整卡的巨幅列表 —— 既难看也难扫读。
  // 估宽 + 下限 + 上限，再按视口收边。
  const maxLabelChars = props.options.reduce((m, o) => Math.max(m, o.label.length), 0);
  const estTextW = maxLabelChars * 7.4; // 12px 字号下中英混排的粗略字宽
  const estW = estTextW + 16 /* padding */ + 21 /* 勾选图标 */;
  const width = Math.round(Math.max(140, props.panelMinWidth, Math.min(estW, 260, vw - 16)));

  const estH = Math.min(props.options.length * 30 + 8, 260);
  const below = r.bottom + gap;
  const openUp = below + estH > vh - 8 && r.top - gap - estH > 8;

  panelStyle.value = {
    position: 'fixed',
    // 右边缘与触发器对齐；贴到视口右边界时回拉
    left: Math.round(Math.min(r.right - width, vw - width - 8)) + 'px',
    top: Math.round(openUp ? r.top - gap - estH : below) + 'px',
    width: width + 'px',
    maxHeight: '260px',
  };
}

function openPanel(): void {
  if (props.disabled) return;
  activeIndex.value = Math.max(
    0,
    props.options.findIndex((o) => o.value === props.modelValue),
  );
  open.value = true;
  nextTick(positionPanel);
  window.addEventListener('scroll', close, true);
  window.addEventListener('resize', close);
}

function close(): void {
  if (!open.value) return;
  open.value = false;
  window.removeEventListener('scroll', close, true);
  window.removeEventListener('resize', close);
}

function toggle(): void {
  if (open.value) close();
  else openPanel();
}

function isSelected(v: string): boolean {
  return v === props.modelValue;
}

function choose(opt: SelectOption): void {
  if (opt.disabled) return;
  emit('update:modelValue', opt.value);
  close();
  nextTick(() => triggerRef.value?.focus());
}

/** 在可选项之间移动，跳过 disabled */
function move(delta: number): void {
  const n = props.options.length;
  if (!n) return;
  let i = activeIndex.value;
  for (let step = 0; step < n; step++) {
    i = (i + delta + n) % n;
    if (!props.options[i].disabled) {
      activeIndex.value = i;
      return;
    }
  }
}

function onKeydown(e: KeyboardEvent): void {
  switch (e.key) {
    case 'ArrowDown':
      e.preventDefault();
      if (!open.value) openPanel();
      else move(1);
      break;
    case 'ArrowUp':
      e.preventDefault();
      if (!open.value) openPanel();
      else move(-1);
      break;
    case 'Home':
      if (open.value) { e.preventDefault(); activeIndex.value = 0; move(1); }
      break;
    case 'End':
      if (open.value) { e.preventDefault(); activeIndex.value = props.options.length - 1; move(-1); }
      break;
    case 'Enter':
    case ' ':
      e.preventDefault();
      if (!open.value) openPanel();
      else if (activeIndex.value >= 0) choose(props.options[activeIndex.value]);
      break;
    case 'Escape':
      if (open.value) { e.preventDefault(); close(); }
      break;
    case 'Tab':
      // 不拦截：让焦点正常离开，但把弹层收起来
      close();
      break;
    default:
      break;
  }
}

// 打开时点面板以外的地方 → 关闭
function onDocPointerDown(e: PointerEvent): void {
  const t = e.target as Node | null;
  if (!t) return;
  if (triggerRef.value?.contains(t) || panelRef.value?.contains(t)) return;
  close();
}
watch(open, (v) => {
  if (v) document.addEventListener('pointerdown', onDocPointerDown, true);
  else document.removeEventListener('pointerdown', onDocPointerDown, true);
});

onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', onDocPointerDown, true);
  window.removeEventListener('scroll', close, true);
  window.removeEventListener('resize', close);
});
</script>

<style scoped>
.asel { position: relative; flex: 1; min-width: 0; display: flex; }

.asel-trigger {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0;
  background: none;
  border: none;
  font-family: var(--font-ui);
  cursor: pointer;
  text-align: left;
}
.asel.disabled .asel-trigger { cursor: not-allowed; }
.asel-trigger:focus-visible { outline: none; }
.asel:focus-within .asel-trigger .asel-value { color: var(--text-heading); }

.asel-value {
  margin-left: auto;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 100%;
}
/* 空值提示（-- 选择端口 --）是真实需要读到的提示文字 */
.asel-value.empty { color: var(--text-muted); font-weight: 400; }

.asel-chev {
  width: 12px;
  height: 12px;
  flex-shrink: 0;
  color: var(--text-dim);
  transition: transform var(--transition-fast);
}
.asel.open .asel-chev { transform: rotate(180deg); }
.asel.disabled .asel-chev { opacity: 0.4; }
</style>

<!-- 弹层被 Teleport 到 body，scoped 属性不生效，因此单独写在非 scoped 块里，
     类名统一加 asel- 前缀避免污染其它组件。 -->
<style>
.asel-panel {
  z-index: 9999;
  padding: 4px;
  overflow-y: auto;
  background: var(--bg-tertiary, #2b2e31);
  border: 1px solid var(--border);
  border-radius: var(--btn-radius, 6px);
  box-shadow: 0 12px 28px -8px rgba(0, 0, 0, 0.7), 0 2px 6px rgba(0, 0, 0, 0.4);
  animation: asel-in 120ms cubic-bezier(0.2, 0.8, 0.2, 1);
}
@keyframes asel-in {
  from { opacity: 0; transform: translateY(-4px); }
  to   { opacity: 1; transform: none; }
}

.asel-opt {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  min-height: 30px;
  padding: 5px 8px;
  border: none;
  border-radius: var(--radius-sm, 4px);
  background: transparent;
  color: var(--text-secondary);
  font-family: var(--font-ui);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}
/* 键盘高亮与鼠标悬停同一视觉，避免两套焦点样式 */
.asel-opt.active { background: var(--bg-hover, rgba(182, 196, 210, 0.16)); color: var(--text-heading); }
.asel-opt.selected { color: var(--accent-green, #4ade80); font-weight: 600; }
.asel-opt:disabled { color: var(--text-dim); cursor: not-allowed; background: transparent; }

.asel-opt-label { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.asel-tick { width: 13px; height: 13px; flex-shrink: 0; }

/* 尊重系统「减少动态效果」 */
@media (prefers-reduced-motion: reduce) {
  .asel-panel { animation: none; }
  .asel-chev { transition: none; }
}
</style>
