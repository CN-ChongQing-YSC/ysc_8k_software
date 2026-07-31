<template>
  <div class="panel docs-split">
    <div class="panel-header">
      <svg class="panel-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <path d="M4 3h8a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2z" />
        <line x1="5" y1="6" x2="11" y2="6" />
        <line x1="5" y1="8.5" x2="11" y2="8.5" />
        <line x1="5" y1="11" x2="8" y2="11" />
      </svg>
      <span>{{ t('docs.title') }}</span>
      <div style="flex:1" />
      <button class="tab-btn" :class="{ active: activeTab === 'docs' }" @click="setTab('docs')">{{ t('docs.tabDocs') }}</button>
      <button class="tab-btn" :class="{ active: activeTab === 'tester' }" @click="setTab('tester')">{{ t('docs.tabTester') }}</button>
      <button class="tab-btn export-pdf-btn" @click="exportPdf" :disabled="exporting" :title="exportDetail">
        <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" style="margin-right:4px;vertical-align:-2px">
          <path d="M8 2v8" /><path d="M5 7l3 3 3-3" /><path d="M3 13h10" />
        </svg>{{ exportLabel }}
      </button>
    </div>
    <div v-if="exportDetail" class="export-status" :class="{ 'is-error': exportLabel.indexOf('失败') >= 0 }">{{ exportDetail }}</div>
    <div class="docs-scroll" v-show="activeTab === 'docs'">
      <section>
        <h3>{{ content.serial.title }}</h3>
        <div class="doc-card">
          <h4>{{ content.serial.basicTitle }}</h4>
          <table class="doc-table">
            <tbody>
              <tr v-for="(row, i) in content.serial.basicRows" :key="i">
                <td class="doc-label">{{ row[0] }}</td>
                <td v-html="row[1]"></td>
              </tr>
            </tbody>
          </table>
        </div>
        <div class="doc-card">
          <h4>{{ content.serial.frameTitle }}</h4>
          <div class="code-block"><code>{{ content.serial.frameFormat }}</code></div>
          <p class="doc-note" v-html="content.serial.frameNote"></p>
        </div>
      </section>

      <section v-for="(sec, si) in content.serial.sections" :key="si">
        <h3>{{ sec.title }}</h3>
        <div class="doc-card" v-for="(card, ci) in sec.cards" :key="ci">
          <h4>{{ card.title }}</h4>
          <template v-if="card.isTable">
            <table class="doc-table">
              <thead>
                <tr><th v-for="(h, hi) in card.headers" :key="hi">{{ h }}</th></tr>
              </thead>
              <tbody>
                <tr v-for="(row, ri) in card.rows" :key="ri">
                  <td v-for="(cell, di) in row" :key="di" v-html="cell"></td>
                </tr>
              </tbody>
            </table>
          </template>
          <template v-else>
            <div v-if="card.format" class="code-block"><code>{{ card.format }}</code></div>
            <p v-if="card.note" class="doc-note" v-html="card.note"></p>
          </template>
        </div>
      </section>
    </div>
    <CommandTester v-show="activeTab === 'tester'" />
  </div>
</template>

<script setup>
import { ref } from 'vue';
import CommandTester from './CommandTester.vue';
import { useI18n } from '../i18n/index.js';
import { buildPdfHtml } from '../pdf-doc.js';

const { t, docs } = useI18n();
const TAB_STORAGE_KEY = 'ysc_ui_docs_tab';
function loadStoredTab() {
  try {
    const v = localStorage.getItem(TAB_STORAGE_KEY);
    return v === 'tester' ? 'tester' : 'docs';
  } catch (e) { return 'docs'; }
}
const activeTab = ref(loadStoredTab());
function setTab(v) {
  activeTab.value = v;
  try { localStorage.setItem(TAB_STORAGE_KEY, v); } catch (e) { /* ignore */ }
}
const content = docs;

// ---- Export documentation as PDF ----
const api = window.driverApi;
const exporting = ref(false);
const exportLabel = ref('导出 PDF');
const exportDetail = ref('导出使用 / 协议 / 状态文档为 PDF');
let exportResetTimer = null;

async function exportPdf() {
  if (exporting.value) return;
  exporting.value = true;
  exportLabel.value = '生成中…';
  exportDetail.value = '';
  try {
    // First: confirm the preload actually exposes exportPdf. If not, the app is
    // running an OLD preload (must fully quit + reopen, or re-run dev/dist).
    if (!api || typeof api.exportPdf !== 'function') {
      exportLabel.value = '导出失败 ✗';
      exportDetail.value = '驱动 preload 未暴露 exportPdf —— 程序跑的是旧 preload。请【完全退出整个程序再重新打开】(打包版需重新 npm run dist;dev 版需停掉再 npm run dev)。';
      return;
    }
    let ver = '';
    try { ver = await api.getAppVersion(); } catch (e) { /* ignore */ }
    // `content` is a Vue computed ref (useI18n returns docs as computed); unwrap
    // with .value so buildPdfHtml sees the actual docs object, not the ref.
    const html = buildPdfHtml(content.value || content, ver);
    const res = await api.exportPdf(html, 'YSC-8K-使用与协议文档.pdf');
    if (res && res.success) {
      exportLabel.value = '已导出 ✓';
      exportDetail.value = res.path || '';
    } else if (res && res.canceled) {
      exportLabel.value = '已取消';
      exportDetail.value = '';
    } else {
      exportLabel.value = '导出失败 ✗';
      // Show the error + the log file path so the user can send us the full log.
      const err = (res && res.error) ? res.error : '未知错误';
      exportDetail.value = err + (res && res.log ? '  |  日志: ' + res.log : '');
    }
  } catch (e) {
    exportLabel.value = '导出失败 ✗';
    exportDetail.value = '渲染进程异常: ' + ((e && e.message) ? e.message : String(e));
  } finally {
    exporting.value = false;
    if (exportResetTimer) clearTimeout(exportResetTimer);
    exportResetTimer = setTimeout(function () {
      exportLabel.value = '导出 PDF';
    }, 8000);
  }
}
</script>

<style scoped>
.export-pdf-btn {
  display: inline-flex;
  align-items: center;
  color: #6ea8ff;
}
.export-pdf-btn:hover:not(:disabled) {
  color: #9cc2ff;
}
.export-pdf-btn:disabled {
  opacity: 0.6;
  cursor: default;
}
.export-status {
  padding: 5px 14px;
  font-size: 11px;
  color: #9cc2ff;
  background: rgba(46, 91, 215, 0.08);
  word-break: break-all;
}
.export-status.is-error {
  color: #ff9b9b;
  background: rgba(220, 53, 69, 0.10);
}
</style>
