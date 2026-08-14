<template>
  <nav class="navrail">
    <!-- 外栏：分组图标 -->
    <div class="rail-outer">
      <button
        v-for="g in groups"
        :key="g.key"
        class="group-btn"
        :class="{ active: activeGroup === g.key }"
        :title="g.label"
        @click="activeGroup = g.key"
      >
        <Icon :name="g.icon" :size="20" />
      </button>
    </div>

    <!-- 内栏：当前分组的条目 -->
    <div class="rail-inner">
      <div class="rail-inner-head">
        <span class="rail-title">{{ currentGroup?.label }}</span>
      </div>
      <button
        v-for="item in currentItems"
        :key="item.key"
        class="item-btn"
        :class="{ active: ui.activeView === item.key, disabled: isDesktopOnly(item) }"
        :disabled="isDesktopOnly(item)"
        :title="isDesktopOnly(item) ? '仅桌面版可用' : item.label"
        @click="onSelect(item)"
      >
        <Icon :name="item.icon" :size="16" />
        <span class="item-label">{{ item.label }}</span>
        <span v-if="isDesktopOnly(item)" class="tag">仅桌面</span>
      </button>
    </div>
  </nav>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import Icon from './Icon.vue';
import type { NavGroup, NavItem } from './nav-config';
import { useUiStore } from '../store/ui-store';
import { getPlatform } from '../platform';

const props = defineProps<{ groups: NavGroup[] }>();

const ui = useUiStore();
const isWeb = getPlatform() === 'web';

// 初始展开包含当前 activeView 的分组（基于传入的 groups）
const groupOf = (view: string): string =>
  props.groups.find((g) => g.items.some((i) => i.key === view))?.key ?? props.groups[0]?.key;
const activeGroup = ref(groupOf(ui.activeView));

watch(
  () => ui.activeView,
  (v) => {
    activeGroup.value = groupOf(v);
  },
);

const currentGroup = computed(() => props.groups.find((g) => g.key === activeGroup.value) ?? props.groups[0]);
const currentItems = computed(() => currentGroup.value?.items ?? []);

function isDesktopOnly(item: NavItem): boolean {
  return !!item.desktopOnly && isWeb;
}

function onSelect(item: NavItem): void {
  if (isDesktopOnly(item)) return;
  ui.setView(item.key);
}
</script>

<style scoped>
.navrail {
  display: flex;
  flex-shrink: 0;
  background: var(--rail-bg);
  border-right: 1px solid var(--border);
}
.rail-outer {
  width: var(--space-rail-outer);
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 10px 0;
  gap: 4px;
  background: var(--rail-outer-bg);
  border-right: 1px solid var(--border-soft);
}
.group-btn {
  width: 44px;
  height: 44px;
  border-radius: 10px;
  border: none;
  background: transparent;
  color: var(--rail-icon);
  display: flex;
  align-items: center;
  justify-content: center;
  transition: var(--transition-fast);
}
.group-btn:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}
.group-btn.active {
  background: var(--bg-selected-soft);
  color: var(--accent-selected);
}
.group-btn:focus-visible {
  outline: none;
  box-shadow: var(--ring-focus);
}
.rail-inner {
  width: var(--space-rail-inner);
  padding: 12px 8px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  overflow-y: auto;
  background: var(--rail-inner-bg);
}
.rail-inner-head {
  padding: 4px 8px 10px;
}
.rail-title {
  font-size: var(--text-xs);
  font-weight: var(--weight-bold);
  letter-spacing: 0.6px;
  text-transform: uppercase;
  color: var(--text-dim);
}
.item-btn {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 10px;
  border-radius: 7px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: var(--text-sm);
  font-weight: var(--weight-semibold);
  text-align: left;
  transition: var(--transition-fast);
}
.item-btn:hover:not(.disabled) {
  background: var(--bg-hover);
  color: var(--text-primary);
}
.item-btn.active {
  background: var(--bg-selected-soft);
  color: var(--accent-selected);
}
.item-btn.active :deep(.ysc-icon) {
  color: var(--accent-selected);
}
.item-btn:focus-visible {
  outline: none;
  box-shadow: var(--ring-focus);
}
.item-btn.disabled {
  color: var(--text-disabled);
  cursor: not-allowed;
}
.item-btn.disabled :deep(.ysc-icon) {
  color: var(--icon-disabled);
}
.item-label {
  flex: 1;
}
.tag {
  font-size: 10px;
  font-weight: var(--weight-bold);
  padding: 2px 6px;
  border-radius: var(--radius-sm);
  background: var(--bg-selected-soft);
  color: var(--text-muted);
  letter-spacing: 0.3px;
}
</style>
