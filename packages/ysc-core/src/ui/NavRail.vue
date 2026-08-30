<template>
  <nav class="navrail" :class="{ expanded: ui.railExpanded }">
    <!-- 收起态：外栏分组图标 + 内栏当前分组条目（双栏紧凑布局）。
         点击分组图标直达该组页面（上次访问页），不再只切换内栏。 -->
    <template v-if="!ui.railExpanded">
      <div class="rail-outer">
        <button
          v-for="g in groups"
          :key="g.key"
          class="group-btn"
          :class="{ active: activeGroup === g.key }"
          :title="groupBtnTitle(g)"
          @click="goToGroup(g)"
        >
          <Icon :name="g.icon" :size="20" />
        </button>
        <button class="group-btn rail-toggle" :title="t('nav.expand')" @click="ui.setRailExpanded(true)">
          <Icon name="menu" :size="20" />
        </button>
      </div>

      <div class="rail-inner">
        <div class="rail-inner-head">
          <span class="rail-title">{{ currentGroup ? groupLabel(currentGroup) : '' }}</span>
        </div>
        <button
          v-for="item in currentItems"
          :key="item.key"
          class="item-btn"
          :class="{ active: ui.activeView === item.key, disabled: isDesktopOnly(item) }"
          :disabled="isDesktopOnly(item)"
          :title="itemTitle(item)"
          @click="onSelect(item)"
        >
          <Icon :name="item.icon" :size="16" />
          <span class="item-label">{{ itemLabel(item) }}</span>
          <span v-if="isDesktopOnly(item)" class="tag">{{ t('nav.desktopTag') }}</span>
        </button>
      </div>
    </template>

    <!-- 展开态：单栏全量分组，每页显示名称 + 一行作用说明，分组头可点击直达 -->
    <template v-else>
      <div class="rail-full">
        <template v-for="g in groups" :key="g.key">
          <button
            class="grp-head"
            :class="{ active: activeGroup === g.key }"
            :title="t('nav.openGroup', { name: groupLabel(g) })"
            @click="goToGroup(g)"
          >
            <Icon :name="g.icon" :size="18" />
            <span class="grp-title">{{ groupLabel(g) }}</span>
          </button>
          <div class="grp-items">
            <button
              v-for="item in g.items"
              :key="g.key + '/' + item.key"
              class="item-btn has-desc"
              :class="{ active: ui.activeView === item.key, disabled: isDesktopOnly(item) }"
              :disabled="isDesktopOnly(item)"
              :title="itemTitle(item)"
              @click="onSelect(item)"
            >
              <Icon :name="item.icon" :size="16" />
              <span class="item-text">
                <span class="item-label">
                  {{ itemLabel(item) }}
                  <span v-if="isDesktopOnly(item)" class="tag">{{ t('nav.desktopTag') }}</span>
                </span>
                <span class="item-desc">{{ itemDesc(item) }}</span>
              </span>
            </button>
          </div>
        </template>
        <button class="rail-toggle-full" @click="ui.setRailExpanded(false)">
          <Icon name="menuOpen" :size="18" />
          <span>{{ t('nav.collapse') }}</span>
        </button>
      </div>
    </template>
  </nav>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import Icon from './Icon.vue';
import type { NavGroup, NavItem } from './nav-config';
import { useUiStore } from '../store/ui-store';
import { useI18n } from '../i18n';
import { getPlatform } from '../platform';

const props = defineProps<{ groups: NavGroup[] }>();

const ui = useUiStore();
ui.loadRailExpanded(); // 恢复上次展开/收起状态（默认展开）
const { t } = useI18n();
const isWeb = getPlatform() === 'web';

// ===== 导航文案 i18n：按 group.key / item.key 查 nav.* 字典，缺失时回退导航表自带文案 =====
function tOr(path: string, fallback: string): string {
  const v = t(path);
  return v === path ? fallback : v;
}
function groupLabel(g: NavGroup): string {
  return tOr('nav.group.' + g.key, g.label);
}
function itemLabel(i: NavItem): string {
  return tOr('nav.item.' + i.key, i.label);
}
function itemDesc(i: NavItem): string {
  return tOr('nav.desc.' + i.key, i.desc ?? '');
}
function itemTitle(i: NavItem): string {
  if (isDesktopOnly(i)) return t('nav.desktopOnlyTip');
  return itemDesc(i) || itemLabel(i);
}
function groupBtnTitle(g: NavGroup): string {
  const target = groupTarget(g);
  return target ? groupLabel(g) + '：' + itemLabel(target) : groupLabel(g);
}

// item.key -> 所属分组 key（展开态全量渲染时，点击条目也要记录到正确的分组）
const groupKeyOfItem = computed<Record<string, string>>(() => {
  const m: Record<string, string> = {};
  for (const g of props.groups) for (const i of g.items) m[i.key] = g.key;
  return m;
});

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

/** 分组直达目标：组内上次访问页，无记录则第一个可用页。 */
function groupTarget(g: NavGroup): NavItem | undefined {
  const last = g.items.find((i) => i.key === ui.lastViewByGroup[g.key]);
  if (last && !isDesktopOnly(last)) return last;
  return g.items.find((i) => !isDesktopOnly(i)) ?? g.items[0];
}

/** 点击分组图标：直达该组页面（免二次点击），activeGroup 由 activeView 的 watch 同步。 */
function goToGroup(g: NavGroup): void {
  const target = groupTarget(g);
  if (target) ui.setView(target.key, g.key);
}

function onSelect(item: NavItem): void {
  if (isDesktopOnly(item)) return;
  ui.setView(item.key, groupKeyOfItem.value[item.key]);
}
</script>

<style scoped>
.navrail {
  display: flex;
  flex-shrink: 0;
  background: var(--rail-bg);
  border-right: 1px solid var(--border);
}
/* 展开态：单栏全量 */
.navrail.expanded {
  width: var(--space-rail-expanded, 264px);
  flex-direction: column;
  min-width: 0;
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
.rail-toggle {
  margin-top: auto;
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

/* ===== 展开态 ===== */
.rail-full {
  flex: 1;
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 12px 8px;
  overflow-y: auto;
  background: var(--rail-inner-bg);
}
.grp-head {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 8px;
  padding: 8px 10px 4px;
  border: none;
  border-radius: 7px;
  background: transparent;
  color: var(--text-dim);
  font-size: var(--text-xs);
  font-weight: var(--weight-bold);
  letter-spacing: 0.6px;
  text-transform: uppercase;
  text-align: left;
  transition: var(--transition-fast);
}
.grp-head:first-child {
  margin-top: 0;
}
.grp-head:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}
.grp-head.active {
  color: var(--accent-selected);
}
.grp-head:focus-visible {
  outline: none;
  box-shadow: var(--ring-focus);
}
.grp-title {
  flex: 1;
}
/* 子菜单缩进 + 竖向层级线，与父级分组头拉开落差 */
.grp-items {
  margin-left: 17px;
  padding-left: 9px;
  border-left: 2px solid var(--border-soft);
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.item-btn.has-desc {
  align-items: flex-start;
  padding: 7px 10px;
}
.item-btn.has-desc :deep(.ysc-icon) {
  margin-top: 2px;
}
.item-text {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}
.item-btn.has-desc .item-label {
  display: flex;
  align-items: center;
  gap: 6px;
}
.item-desc {
  font-size: 11px;
  font-weight: 400;
  color: var(--text-muted);
  line-height: 1.35;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.item-btn.active .item-desc {
  color: var(--accent-selected);
  opacity: 0.75;
}
.rail-toggle-full {
  margin-top: auto;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 10px;
  border: none;
  border-radius: 7px;
  background: transparent;
  color: var(--text-secondary);
  font-size: var(--text-sm);
  font-weight: var(--weight-semibold);
  text-align: left;
  cursor: pointer;
  transition: var(--transition-fast);
}
.rail-toggle-full:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}
.rail-toggle-full:focus-visible {
  outline: none;
  box-shadow: var(--ring-focus);
}
</style>
