<template>
  <div class="cmd-tester">
    <div class="cmd-tree">
      <div
        class="tree-group"
        v-for="proto in protocols"
        :key="proto.id"
      >
        <div
          class="tree-root"
          :class="{ active: expandedProto === proto.id }"
          @click="toggleProto(proto.id)"
        >
          <svg class="tree-arrow" :class="{ open: expandedProto === proto.id }" viewBox="0 0 8 8"><path d="M2 1l4 3-4 3" /></svg>
          <span class="tree-label">{{ proto.name }}</span>
        </div>
        <div class="tree-children" v-show="expandedProto === proto.id">
          <div
            class="tree-item"
            v-for="cmd in proto.commands"
            :key="cmd.id"
            :class="{ active: selectedCmd && selectedCmd.id === cmd.id }"
            @click="selectCmd(proto, cmd)"
          >
            {{ cmd.name }}
          </div>
        </div>
      </div>
    </div>
    <div class="cmd-detail" v-if="selectedCmd">
      <div class="cmd-detail-header">
        <span class="cmd-proto-tag" :class="selectedProto.id">{{ selectedProto.name }}</span>
        <span class="cmd-name-tag">{{ selectedCmd.name }}</span>
      </div>
      <div class="cmd-detail-section">
        <div class="cmd-section-label">{{ t('cmdTester.format') }}</div>
        <div class="code-block">
          <code>{{ selectedCmd.format }}</code>
        </div>
      </div>
      <div class="cmd-detail-section" v-if="selectedCmd.note">
        <div class="cmd-section-label">{{ t('cmdTester.note') }}</div>
        <p class="cmd-note" v-html="selectedCmd.note"></p>
      </div>
      <div class="cmd-detail-section" v-if="selectedCmd.params && selectedCmd.params.length">
        <div class="cmd-section-label">{{ t('cmdTester.params') }}</div>
        <table class="doc-table">
          <thead>
            <tr>
              <th>{{ t('cmdTester.paramName') }}</th>
              <th>{{ t('cmdTester.paramType') }}</th>
              <th>{{ t('cmdTester.paramDesc') }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="p in selectedCmd.params" :key="p.name">
              <td><code>{{ p.name }}</code></td>
              <td>{{ p.type }}</td>
              <td v-html="p.desc"></td>
            </tr>
          </tbody>
        </table>
      </div>
      <div class="cmd-detail-section" v-if="selectedCmd.actions">
        <div class="cmd-section-label">{{ t('cmdTester.quickActions') }}</div>
        <div class="cmd-action-row">
          <button
            class="btn btn-action"
            v-for="act in selectedCmd.actions"
            :key="act.label"
            @click="sendAction(act.cmd)"
            :disabled="sending"
          >{{ act.label }}</button>
        </div>
        <div class="cmd-send-log" v-if="sendLog.length">
          <div class="cmd-log-item" v-for="(log, i) in sendLog" :key="i" :class="log.ok ? 'ok' : 'err'">
            <span class="log-dir">{{ log.dir }}</span>
            <span class="log-proto">{{ log.proto }}</span>
            <span class="log-text">{{ log.text }}</span>
          </div>
        </div>
      </div>
      <div class="cmd-detail-section" v-else-if="selectedCmd.editable !== false">
        <div class="cmd-section-label">{{ t('cmdTester.sendTest') }}</div>
        <div class="cmd-send-row">
          <input class="input-text cmd-input" v-model="sendText" :placeholder="t('cmdTester.sendPlaceholder')" />
          <button class="btn btn-accent" @click="sendCommand" :disabled="sending">{{ t('cmdTester.send') }}</button>
        </div>
        <div class="cmd-send-log" v-if="sendLog.length">
          <div class="cmd-log-item" v-for="(log, i) in sendLog" :key="i" :class="log.ok ? 'ok' : 'err'">
            <span class="log-dir">{{ log.dir }}</span>
            <span class="log-proto">{{ log.proto }}</span>
            <span class="log-text">{{ log.text }}</span>
          </div>
        </div>
      </div>
      <!-- Visual keyboard: click a key to inject press/release (cmd 45/46).
           Press/release commands flow through doSend and appear in the log. -->
      <div class="cmd-detail-section" v-if="selectedCmd && selectedCmd.id === 'ysc_kbd_key'">
        <KeyboardTester @send="doSend" />
      </div>
    </div>
    <div class="cmd-detail cmd-empty" v-else>
      <div class="cmd-empty-text">
        <svg viewBox="0 0 24 24" width="32" height="32" fill="none" stroke="currentColor" stroke-width="1.2">
          <polyline points="4 17 10 11 4 5" />
          <line x1="12" y1="19" x2="20" y2="19" />
        </svg>
        <span>{{ t('cmdTester.selectHint') }}</span>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed } from 'vue';
import { useI18n } from '../i18n/index.js';
import KeyboardTester from './KeyboardTester.vue';

const { t, docs } = useI18n();
const api = window.driverApi;

const protocols = computed(() => docs.value.protocols);

const PROTO_STORAGE_KEY = 'ysc_ui_cmd_tester_proto';
const CMD_STORAGE_KEY = 'ysc_ui_cmd_tester_cmd';

function loadStoredProto() {
  try { return localStorage.getItem(PROTO_STORAGE_KEY); } catch (e) { return null; }
}
function loadStoredCmdId() {
  try { return localStorage.getItem(CMD_STORAGE_KEY); } catch (e) { return null; }
}

const storedProto = loadStoredProto();
const storedCmdId = loadStoredCmdId();
const initialProtoList = storedProto && protocols.value
  ? protocols.value.filter(function(p) { return p.id === storedProto; })
  : [];
const initialCmd = (initialProtoList[0] && initialProtoList[0].commands || [])
  .filter(function(c) { return c.id === storedCmdId; })[0] || null;

const expandedProto = ref(initialCmd ? storedProto : null);
const selectedCmd = ref(initialCmd);
const selectedProto = ref(initialCmd ? initialProtoList[0] : null);
const sendText = ref(initialCmd ? initialCmd.format : '');
const sending = ref(false);
const sendLog = ref([]);

function toggleProto(id) {
  expandedProto.value = expandedProto.value === id ? null : id;
  selectedCmd.value = null;
  selectedProto.value = null;
  try { localStorage.removeItem(CMD_STORAGE_KEY); } catch (e) { /* ignore */ }
  try {
    if (expandedProto.value) localStorage.setItem(PROTO_STORAGE_KEY, expandedProto.value);
    else localStorage.removeItem(PROTO_STORAGE_KEY);
  } catch (e) { /* ignore */ }
}

function selectCmd(proto, cmd) {
  selectedProto.value = proto;
  selectedCmd.value = cmd;
  sendText.value = cmd.format;
  sendLog.value = [];
  try {
    localStorage.setItem(PROTO_STORAGE_KEY, proto.id);
    localStorage.setItem(CMD_STORAGE_KEY, cmd.id);
  } catch (e) { /* ignore */ }
}

function sendCommand() {
  if (!sendText.value || !selectedProto.value) return;
  doSend(sendText.value);
}

function sendAction(cmd) {
  if (!selectedProto.value) return;
  doSend(cmd);
}

function doSend(text) {
  sending.value = true;
  const protoType = selectedProto.value.id === 'ysc' ? 'send_ysc' : 'send_makcu';
  if (api) {
    api.send(protoType, { cmd: text });
  }
  sendLog.value.unshift({
    dir: 'TX',
    proto: selectedProto.value.name,
    text: text,
    ok: true,
  });
  if (sendLog.value.length > 20) sendLog.value.length = 20;
  setTimeout(function() { sending.value = false; }, 200);
}
</script>
