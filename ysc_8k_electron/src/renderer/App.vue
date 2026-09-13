<template>
  <TitleBar />
  <div class="app-body">
    <!-- 共享核心双栏导航（@ysc/core/ui），替代扁平 Sidebar；viewKey 同步到 useUiStore -->
    <NavRail :groups="ELECTRON_NAV" />
    <div class="app-main">
      <StatusBar
        :serial-connected="serialConnected"
        :serial-port="serialPort"
        :serial-baud="serialBaud"
        :net-running="netRunning"
        :net-port="netPort"
        :net-ip="netIp"
        :version="version"
      />
      <!-- CH343/WCH 设备驱动异常（黄叹号）全局警告条 -->
      <div v-if="ch343Status.problem && !ch343Dismissed" class="ch343-warn-bar">
        <div class="ch343-warn-text">
          <span class="ch343-warn-title">⚠ {{ t('driver.missingTitle') }}</span>
          <span class="ch343-warn-tip">{{ t('driver.missingTip') }}</span>
        </div>
        <div class="ch343-warn-actions">
          <button class="btn btn-accent" :disabled="ch343Installing" @click="onInstallCh343Driver">
            {{ ch343Installing ? t('driver.installing') : t('driver.installBtn') }}
          </button>
          <button class="btn" @click="dismissCh343Warn">{{ t('driver.dismiss') }}</button>
        </div>
      </div>
      <template v-if="currentView === 'home'">
        <div class="main-content">
          <div class="left-col">
            <DevicePanel
              :slots="deviceSlots"
              :ports="ports"
              @connect="onDeviceConnect"
              @disconnect="onDeviceDisconnect"
              @set-role="onDeviceSetRole"
              @refresh="onRefreshPorts"
            />
            <SerialPanel
              :connected="serialConnected"
              :connected-port="serialPort"
              :ports="ports"
              :baud="serialBaud"
              @connect="onSerialConnect"
              @disconnect="onSerialDisconnect"
              @refresh="onRefreshPorts"
              @switch-baud="onSwitchBaud"
            />
            <KmnetPanel
              :running="netRunning"
              :ip="netIp"
              :mac="netMac"
              :port="netPort"
              :self-check="kmnetSelfCheck"
              :serial-connected="serialConnected"
              @start="onKmnetStart"
              @stop="onKmnetStop"
            />
          </div>
          <div class="right-col">
            <MonitorPanel
              :buttons="monitorButtons"
              :x="monitorX"
              :y="monitorY"
              :kbd="kbdState"
              :connected="anyDeviceConnected"
              @toggle-upload="onToggleUpload"
            />
          </div>
        </div>
        <!-- 下方一栏：透传鼠标 USB 序列号自定义 -->
        <SerialNumberPanel
          :connected="serialConnected"
          :info="snInfo"
          :loading="snLoading"
          @write="onSnWrite"
          @clear="onSnClear"
          @refresh="loadSerialNumber"
        />
      </template>
      <div v-else-if="currentView === 'macro'" class="main-content">
        <MacroPanel
          :macros="macros"
          @save-all="onMacroSaveAll"
          @reset="onMacroReset"
          @load="loadMacros"
        />
      </div>
      <JitterMacroPanel
        v-else-if="currentView === 'jitter'"
        :config="jitterConfig"
        @save="onJitterSave"
        @reset="onJitterReset"
        @load="loadJitter"
      />
      <MouseCurvePanel
        v-else-if="currentView === 'mouse-curve'"
        :config="mouseCurveConfig"
        @save="onMouseCurveSave"
        @reset="onMouseCurveReset"
        @load="loadMouseCurve"
      />
      <MouseInterpPanel
        v-else-if="currentView === 'mouse-interp'"
        :config="mouseInterpConfig"
        @save="onMouseInterpSave"
        @reset="onMouseInterpReset"
        @load="loadMouseInterp"
      />
      <GamepadMapperPanel v-else-if="currentView === 'gamepad'" />
      <DocsPanel v-else-if="currentView === 'docs'" />
      <FirmwarePanel
        v-else-if="currentView === 'firmware'"
        :file-path="firmwareFilePath"
        :file-info="firmwareFileInfo"
        @update-file="onFirmwareFileUpdate"
      />
      <TowmcuFirmwarePanel v-else-if="currentView === 'towmcu-firmware'" />
      <DebugPanel v-else-if="currentView === 'debug'" :serial-connected="serialConnected" />
      <DocsPanel v-else />
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import TitleBar from './components/TitleBar.vue';
import StatusBar from './components/StatusBar.vue';
import SerialPanel from './components/SerialPanel.vue';
import DevicePanel from './components/DevicePanel.vue';
import KmnetPanel from './components/KmnetPanel.vue';
import MonitorPanel from './components/MonitorPanel.vue';
import MacroPanel from './components/MacroPanel.vue';
import JitterMacroPanel from './components/JitterMacroPanel.vue';
import MouseCurvePanel from './components/MouseCurvePanel.vue';
import MouseInterpPanel from './components/MouseInterpPanel.vue';
import SerialNumberPanel from './components/SerialNumberPanel.vue';
import GamepadMapperPanel from './components/GamepadMapperPanel.vue';
import DocsPanel from './components/DocsPanel.vue';
import FirmwarePanel from './components/FirmwarePanel.vue';
import TowmcuFirmwarePanel from './components/TowmcuFirmwarePanel.vue';
import DebugPanel from './components/DebugPanel.vue';
import { useI18n } from './i18n/index.js';
import { NavRail } from '@ysc/core/ui';
import { useUiStore } from '@ysc/core/store';
import { ELECTRON_NAV } from './electron-nav';

const api = window.driverApi;
const { t } = useI18n();

const VALID_VIEWS = ['home', 'docs', 'macro', 'jitter', 'mouse-curve', 'mouse-interp', 'gamepad', 'firmware', 'towmcu-firmware', 'debug'];

// 共享核心 ui-store 驱动导航视图（与 @ysc/core NavRail 共用 activeView，持久化由 store 负责）
const ui = useUiStore();
ui.loadView();
if (!VALID_VIEWS.includes(ui.activeView)) ui.setView('home');
const currentView = computed(() => ui.activeView);
const serialConnected = ref(false);
const serialPort = ref('');
const serialBaud = ref(0);
const netRunning = ref(false);
const netPort = ref(5251);
const netIp = ref('');
const netMac = ref('');
const version = ref('');
const ports = ref([]);

// ── 双设备（一鼠标机 + 一键盘机）──
// 驱动把两台设备抽象成两个槽位；角色可自动识别（探测上游是鼠标还是键盘），
// 也可由用户手动指定。这里的状态全部来自驱动的 device_list / kbd_state 事件。
const deviceSlots = ref([
  { dev: 0, role: 'none', auto: false, connected: false, port: '', baud: 0, vid: -1, pid: -1, product: '', ifaceMask: 0 },
  { dev: 1, role: 'none', auto: false, connected: false, port: '', baud: 0, vid: -1, pid: -1, product: '', ifaceMask: 0 },
]);
const kbdState = ref({ valid: 0, mod: 0, count: 0, overflow: 0, keys: [] });

/* 是否已连上任意设备。双设备面板连的设备不算在 serialConnected 里
 * （那是旧单串口路径的状态），所以监控开关/面板的可用性要用这个综合判断，
 * 否则用「双设备」连上后「开始监控」按钮会一直是灰的。 */
const anyDeviceConnected = computed(() =>
  serialConnected.value ||
  deviceSlots.value.some(function (d) { return d && d.connected; })
);

const firmwareFilePath = ref('');
const firmwareFileInfo = ref('');

// CH343/WCH 驱动健康度：present=有 VID 1A86 设备；problem=有异常(黄叹号)。
// problem 为真时在顶部全局显示「缺驱动」警告条 + 一键安装按钮。
const ch343Status = ref({ present: false, problem: false, problems: [] });
const ch343Installing = ref(false);
const ch343Dismissed = ref(false); // 用户手动关闭后本次不再弹（点安装或重启会重置）

function onFirmwareFileUpdate(payload) {
  firmwareFilePath.value = payload.path;
  firmwareFileInfo.value = payload.info;
}

const kmnetSelfCheck = ref({
  active: false,
  step: '',
  message: '',
  result: '',
});

const TARGET_BAUD = 4000000;
const SELF_CHECK_RECONNECT_RETRIES = 3;
let selfCheckTimer = null;
let selfCheckReconnectTimer = null;
let selfCheckPort = '';
let selfCheckReconnectAttempts = 0;

function selfCheckCleanupTimers() {
  if (selfCheckTimer) {
    clearTimeout(selfCheckTimer);
    selfCheckTimer = null;
  }
  if (selfCheckReconnectTimer) {
    clearTimeout(selfCheckReconnectTimer);
    selfCheckReconnectTimer = null;
  }
}

function selfCheckFail(message) {
  kmnetSelfCheck.value.active = false;
  kmnetSelfCheck.value.step = 'failed';
  kmnetSelfCheck.value.result = 'failed';
  kmnetSelfCheck.value.message = message;
  selfCheckCleanupTimers();
}

function selfCheckSuccess(message) {
  kmnetSelfCheck.value.active = false;
  kmnetSelfCheck.value.step = 'done';
  kmnetSelfCheck.value.result = 'success';
  kmnetSelfCheck.value.message = message;
  selfCheckCleanupTimers();
}

function selfCheckScheduleTimeout() {
  if (selfCheckTimer) clearTimeout(selfCheckTimer);
  selfCheckTimer = setTimeout(function() {
    selfCheckFail('自检超时,请确认设备状态');
  }, 10000);
}

function selfCheckBeginReconnect() {
  kmnetSelfCheck.value.step = 'reconnecting';
  kmnetSelfCheck.value.message = '正在以 ' + TARGET_BAUD + ' 重新连接进行验证...';
  selfCheckScheduleTimeout();
  selfCheckReconnectAttempts = 0;
  selfCheckReconnectTimer = setTimeout(function() {
    selfCheckReconnectTimer = null;
    api.send('serial_connect', { port: selfCheckPort, baud: TARGET_BAUD });
  }, 600);
}

function selfCheckScheduleReconnectRetry(message) {
  selfCheckReconnectAttempts += 1;
  if (selfCheckReconnectAttempts > SELF_CHECK_RECONNECT_RETRIES) {
    selfCheckFail(message);
    return false;
  }
  kmnetSelfCheck.value.message = '第 ' + selfCheckReconnectAttempts + ' 次重连失败(' + message + '),等待后重试...';
  if (selfCheckReconnectTimer) clearTimeout(selfCheckReconnectTimer);
  selfCheckReconnectTimer = setTimeout(function() {
    selfCheckReconnectTimer = null;
    api.send('serial_connect', { port: selfCheckPort, baud: TARGET_BAUD });
  }, 1200);
  return true;
}

const monitorButtons = ref(0);
const monitorX = ref(0);
const monitorY = ref(0);

const macros = ref([]);

const jitterConfig = ref({ enabled: 0, trigger: 1, ax: 10, fx: 0, py: 5, fy: 0 });

const mouseCurveConfig = ref({ enabled: 0, profile: 2, segments: 4, duration: 0, jitter: 15 });
const mouseInterpConfig = ref({ enabled: 1, profile: 0, ema: 0 });

// 透传鼠标 USB 序列号（cmd 54 回读 {custom, physical, active}）
const snInfo = ref({ custom: '', physical: '', active: '' });
const snLoading = ref(false);   // 回读进行中：刷新按钮转圈防重复点击
let snLoadingTimer = null;

const cleanups = [];

function listen(event, handler) {
  api.on(event, handler);
  cleanups.push(function() { api.off(event, handler); });
}

function onNavigate(view) {
  ui.setView(view);
}

function onSerialConnect(port, baud) {
  api.send('serial_connect', { port: port, baud: baud });
}

function onToggleUpload(enable) {
  api.send('upload_enable', { enable: enable });
}

function onSerialDisconnect() {
  api.send('serial_disconnect');
}

function onRefreshPorts() {
  api.send('enum_ports');
}

function onSwitchBaud(baud) {
  api.send('switch_baudrate', { baud: baud });
}

// 触发 C++ 端检测 WCH/CH34x 设备驱动状态（结果经 ch343_driver_status 事件回推）
function checkCh343DriverNow() {
  if (api && api.checkCh343Driver) api.checkCh343Driver();
}

// 一键安装：以管理员权限启动随包 CH343SER.EXE，用户在 GUI 里点「安装」
function onInstallCh343Driver() {
  if (!api || !api.installCh343Driver || ch343Installing.value) return;
  ch343Installing.value = true;
  ch343Dismissed.value = false;
  api.installCh343Driver().then(function(res) {
    ch343Installing.value = false;
    // 安装器 GUI 已弹出（不等待其结束）；稍后重新检测刷新警告条
    setTimeout(checkCh343DriverNow, 1500);
  }).catch(function() {
    ch343Installing.value = false;
  });
}

function dismissCh343Warn() {
  ch343Dismissed.value = true;
}

function onKmnetStart(port) {
  if (!serialConnected.value) {
    selfCheckFail('请先连接串口后再启动 KmNet 服务');
    return;
  }
  kmnetSelfCheck.value = {
    active: false,
    step: '',
    message: '',
    result: '',
  };
  api.send('kmnet_start', { port: port });
}

function onKmnetStop() {
  api.send('kmnet_stop');
}

function onMacroSaveAll(allSlots) {
  macros.value = allSlots;
  for (var i = 0; i < allSlots.length; i++) {
    (function(idx) {
      setTimeout(function() {
        var s = allSlots[idx];
        var cmd = JSON.stringify({
          cmd: 36,
          s: idx,
          e: s.enabled,
          t: s.trigger,
          p: s.suppress,
          w: s.wheel,
          i: s.interval || 0,
          d: s.duration || 0,
          ij: s.ij || 0,
          dj: s.dj || 0
        });
        api.send('send_ysc', { cmd: cmd });
      }, idx * 50);
    })(i);
  }
}

function onMacroReset() {
  macros.value = Array.from({ length: 8 }, function() {
    return { enabled: 0, trigger: 0, suppress: 0, wheel: 0, interval: 0, duration: 0, ij: 0, dj: 0 };
  });
  api.send('send_ysc', { cmd: JSON.stringify({ cmd: 38 }) });
}

function loadMacros() {
  api.send('send_ysc', { cmd: JSON.stringify({ cmd: 37, s: -1 }) });
}

function onJitterSave(cfg) {
  jitterConfig.value = { ...cfg };
  api.send('send_ysc', { cmd: JSON.stringify({
    cmd: 39,
    e: cfg.enabled ? 1 : 0,
    t: cfg.trigger,
    ax: cfg.ax,
    fx: cfg.fx,
    py: cfg.py,
    fy: cfg.fy
  }) });
}

function onJitterReset() {
  jitterConfig.value = { enabled: 0, trigger: 0, ax: 0, fx: 0, py: 0, fy: 0 };
  api.send('send_ysc', { cmd: JSON.stringify({ cmd: 41 }) });
}

function loadJitter() {
  api.send('send_ysc', { cmd: JSON.stringify({ cmd: 40 }) });
}

function onMouseCurveSave(cfg) {
  mouseCurveConfig.value = { ...cfg };
  api.send('send_ysc', { cmd: JSON.stringify({
    cmd: 42,
    e: cfg.enabled ? 1 : 0,
    p: cfg.profile,
    n: cfg.segments,
    d: cfg.duration,
    j: cfg.jitter
  }) });
}

function onMouseCurveReset() {
  mouseCurveConfig.value = { enabled: 0, profile: 2, segments: 4, duration: 0, jitter: 15 };
  api.send('send_ysc', { cmd: JSON.stringify({ cmd: 44 }) });
}

function loadMouseCurve() {
  api.send('send_ysc', { cmd: JSON.stringify({ cmd: 43 }) });
}

function onMouseInterpSave(cfg) {
  mouseInterpConfig.value = { ...cfg };
  api.send('send_ysc', { cmd: JSON.stringify({
    cmd: 51,
    e: cfg.enabled ? 1 : 0,
    p: cfg.profile
  }) });
}

function onMouseInterpReset() {
  mouseInterpConfig.value = { enabled: 1, profile: 0 };
  api.send('send_ysc', { cmd: JSON.stringify({ cmd: 53 }) });
}

function loadMouseInterp() {
  api.send('send_ysc', { cmd: JSON.stringify({ cmd: 52 }) });
}

function loadSerialNumber() {
  if (snLoading.value) return;   // 上一轮回读未结束，忽略重复触发
  snLoading.value = true;
  clearTimeout(snLoadingTimer);
  // 设备无响应（未升级新固件/已断开）时 3s 兜底解除，避免永远转圈
  snLoadingTimer = setTimeout(function() { snLoading.value = false; }, 3000);
  api.send('send_ysc', { cmd: JSON.stringify({ cmd: 54 }) });
}

// cmd 55 写入（固件延迟落盘）→ 800ms 后 cmd 57 软重启重新枚举生效 → 回读
function onSnWrite(s) {
  api.send('send_ysc', { cmd: JSON.stringify({ cmd: 55, s: s }) });
  setTimeout(function() {
    api.send('send_ysc', { cmd: JSON.stringify({ cmd: 57 }) });
  }, 800);
  setTimeout(loadSerialNumber, 3200);
}

function onSnClear() {
  api.send('send_ysc', { cmd: JSON.stringify({ cmd: 56 }) });
  setTimeout(function() {
    api.send('send_ysc', { cmd: JSON.stringify({ cmd: 57 }) });
  }, 800);
  setTimeout(loadSerialNumber, 3200);
}

let monitorTimer = null;
let kbdTimer = null;      // 键盘物理状态轮询
let deviceTimer = null;   // 设备槽位/角色轮询
let portsRefreshAt = 0;

// ── 双设备操作 ──
function onDeviceConnect(dev, port, baud) {
  api.send('device_attach', { dev: dev, port: port, baud: baud || 0 });
}

function onDeviceDisconnect(dev) {
  api.send('device_detach', { dev: dev });
}

function onDeviceSetRole(dev, role) {
  api.send('device_set_role', { dev: dev, role: role });
}

function refreshPortsThrottled() {
  if (serialConnected.value) return;
  const now = Date.now();
  if (now - portsRefreshAt < 800) return;
  portsRefreshAt = now;
  api.send('enum_ports');
}

function onWindowFocus() {
  refreshPortsThrottled();
  checkCh343DriverNow(); // 用户可能刚拔插/换口，重新检测驱动状态
}

onMounted(function() {
  listen('serial_connected', function(data) {
    serialConnected.value = true;
    serialPort.value = data.port;
    serialBaud.value = data.baud;
    if (kmnetSelfCheck.value.active && kmnetSelfCheck.value.step === 'reconnecting') {
      if (data.baud === TARGET_BAUD) {
        selfCheckSuccess('自检通过,波特率 ' + data.baud + ' 工作正常');
      } else {
        selfCheckFail('重连成功但实际波特率为 ' + data.baud + ',与目标 ' + TARGET_BAUD + ' 不一致');
      }
      return;
    }
    setTimeout(loadMacros, 500);
    setTimeout(loadJitter, 600);
    setTimeout(loadMouseCurve, 650);
    setTimeout(loadMouseInterp, 700);
    setTimeout(loadSerialNumber, 800);
  });

  listen('serial_disconnected', function() {
    serialConnected.value = false;
    if (kmnetSelfCheck.value.active && kmnetSelfCheck.value.step === 'disconnecting') {
      selfCheckBeginReconnect();
      return;
    }
    // 主动/意外断开后刷新端口列表，让下拉框立即反映"端口已消失"
    api.send('enum_ports');
  });

  listen('serial_error', function(data) {
    if (kmnetSelfCheck.value.active && kmnetSelfCheck.value.step === 'reconnecting') {
      var msg = (data && data.message) ? data.message : '未知错误';
      selfCheckScheduleReconnectRetry(msg);
      return;
    }
    if (kmnetSelfCheck.value.active) {
      selfCheckFail('串口错误:' + (data && data.message ? data.message : '未知错误'));
    } else {
      // 非自检场景的串口错误：复位连接态并刷新端口（防御性兜底，覆盖物理断开等情形）
      serialConnected.value = false;
      api.send('enum_ports');
    }
  });

  listen('baudrate_switched', function(data) {
    serialBaud.value = data.baud;
    if (kmnetSelfCheck.value.active && kmnetSelfCheck.value.step === 'switching') {
      if (data.baud === TARGET_BAUD) {
        kmnetSelfCheck.value.step = 'disconnecting';
        kmnetSelfCheck.value.message = '波特率已切换,正在断开串口...';
        selfCheckScheduleTimeout();
        api.send('serial_disconnect');
      } else {
        selfCheckFail('波特率切换返回 ' + data.baud + ',与目标 ' + TARGET_BAUD + ' 不一致');
      }
    }
  });

  listen('baudrate_failed', function(data) {
    if (kmnetSelfCheck.value.active) {
      selfCheckFail('波特率切换失败:' + (data && data.message ? data.message : '设备可能已断开'));
    }
  });

  listen('kmnet_started', function(data) {
    netRunning.value = true;
    netPort.value = data.port;
    netIp.value = data.ip;
    netMac.value = data.mac || '';
    selfCheckPort = serialPort.value;
    if (serialBaud.value === TARGET_BAUD) {
      kmnetSelfCheck.value = {
        active: true,
        step: 'disconnecting',
        message: '当前已为 ' + TARGET_BAUD + ',断开重连进行自检...',
        result: '',
      };
      selfCheckScheduleTimeout();
      api.send('serial_disconnect');
    } else {
      kmnetSelfCheck.value = {
        active: true,
        step: 'switching',
        message: '正在将波特率切换为 ' + TARGET_BAUD + '...',
        result: '',
      };
      selfCheckScheduleTimeout();
      api.send('switch_baudrate', { baud: TARGET_BAUD });
    }
  });

  listen('kmnet_stopped', function() {
    netRunning.value = false;
    if (kmnetSelfCheck.value.active) {
      selfCheckFail('KmNet 服务已停止,自检中断');
    }
  });

  listen('monitor_data', function(data) {
    monitorButtons.value = data.buttons;
    monitorX.value = data.x;
    monitorY.value = data.y;
  });

  listen('debug_response', function(data) {
    if (!data || data.message !== 'jitter') return;
    if (typeof data.data !== 'string') return;
    try {
      var j = JSON.parse(data.data);
      jitterConfig.value = {
        enabled: j.e || 0,
        trigger: j.t || 0,
        ax: j.ax || 0,
        fx: j.fx || 0,
        py: j.py || 0,
        fy: j.fy || 0
      };
    } catch (e) { /* ignore parse errors */ }
  });

  listen('debug_response', function(data) {
    if (!data || data.message !== 'mouse_curve') return;
    if (typeof data.data !== 'string') return;
    try {
      var c = JSON.parse(data.data);
      mouseCurveConfig.value = {
        enabled: c.enabled ? 1 : 0,
        profile: c.profile != null ? c.profile : 2,
        segments: c.segments != null ? c.segments : 4,
        duration: c.duration != null ? c.duration : 0,
        jitter: c.jitter != null ? c.jitter : 15
      };
    } catch (e) { /* ignore parse errors */ }
  });

  listen('debug_response', function(data) {
    if (!data || data.message !== 'interp') return;
    if (typeof data.data !== 'string') return;
    try {
      var c = JSON.parse(data.data);
      mouseInterpConfig.value = {
        enabled: c.enabled ? 1 : 0,
        profile: c.profile != null ? c.profile : 0,
        ema: c.ema || 0
      };
    } catch (e) { /* ignore parse errors */ }
  });

  listen('debug_response', function(data) {
    if (!data || data.message !== 'sn') return;
    snLoading.value = false;
    clearTimeout(snLoadingTimer);
    if (typeof data.data !== 'string') return;
    try {
      var j = JSON.parse(data.data);
      snInfo.value = {
        custom: j.custom || '',
        physical: j.physical || '',
        active: j.active || ''
      };
    } catch (e) { /* ignore parse errors */ }
  });

  listen('ports_list', function(data) {
    ports.value = data.ports || [];
  });

  listen('version', function(data) {
    version.value = data.version;
  });

  listen('state', function(data) {
    serialConnected.value = data.serialConnected;
    serialPort.value = data.serialPort;
    serialBaud.value = data.serialBaud;
    netRunning.value = data.netRunning;
    netPort.value = data.netPort;
    // 键盘物理状态也随状态包回来，避免额外往返
    if (data.kbd) kbdState.value = data.kbd;
  });

  // ── 双设备事件 ──
  listen('device_list', function(data) {
    if (!data || !Array.isArray(data.devices)) return;
    var next = deviceSlots.value.slice();
    data.devices.forEach(function(d) {
      if (d.dev != null && d.dev >= 0 && d.dev < next.length) next[d.dev] = d;
    });
    deviceSlots.value = next;
  });

  listen('device_attached', function() { api.send('device_list'); });
  listen('device_detached', function() { api.send('device_list'); });
  listen('device_lost', function() { api.send('device_list'); });

  listen('kbd_state', function(data) {
    /* 驱动的载荷多一层包装：事件是 SendEvent("kbd_state", body)，
     * 而 body 本身就是 SerializeKbdJson() 的 {"kbd":{valid,mod,count,overflow,keys}}。
     * 也就是说渲染进程拿到的是 {kbd:{...}}，必须取 .kbd 再赋值。
     * 之前直接 kbdState.value = data，导致 kbdState.valid / .keys 全是 undefined，
     * 面板永远显示「无数据」—— 尽管事件和键码都是对的。 */
    if (!data || typeof data !== 'object') return;
    var payload = (data.kbd && typeof data.kbd === 'object') ? data.kbd : data;
    kbdState.value = payload;
  });

  listen('kbd_result', function() {
    // 键盘命令回执（成功/失败都刷一次状态，失败时 kbd 侧 valid 会反映出来）
    api.send('get_kbd_state');
  });

  listen('ch343_driver_status', function(data) {
    if (!data) return;
    ch343Status.value = {
      present: !!data.present,
      problem: !!data.problem,
      problems: data.problems || [],
    };
    // 状态恢复正常则自动取消「已关闭」标记
    if (!data.problem) ch343Dismissed.value = false;
  });

  api.send('get_state');
  api.send('device_list');   // 双设备槽位初始状态
  api.send('get_kbd_state'); // 键盘物理状态初始快照
  // 状态栏版本：直接取应用自身版本（app.getVersion）。
  // 原先只靠主进程启动时一次性的 driver 'version' 事件，回包常早于 renderer
  // 加载完成而丢失，界面永远停在写死的默认值（1.12.0）。
  if (api && api.getAppVersion) {
    api.getAppVersion().then(function(v) { if (v) version.value = v; });
  }
  portsRefreshAt = Date.now();
  api.send('enum_ports');
  checkCh343DriverNow(); // 启动时检测一次驱动状态

  window.addEventListener('focus', onWindowFocus);

  monitorTimer = setInterval(function() {
    api.send('get_monitor');
  }, 100);

  // 键盘物理状态 + 设备槽位（角色可能被驱动自动识别后变化）——低频轮询即可
  kbdTimer = setInterval(function() {
    api.send('get_kbd_state');
  }, 200);
  deviceTimer = setInterval(function() {
    api.send('device_list');
  }, 1000);
});

onUnmounted(function() {
  for (var i = 0; i < cleanups.length; i++) cleanups[i]();
  if (monitorTimer) clearInterval(monitorTimer);
  if (kbdTimer) clearInterval(kbdTimer);
  if (deviceTimer) clearInterval(deviceTimer);
  selfCheckCleanupTimers();
  clearTimeout(snLoadingTimer);
  window.removeEventListener('focus', onWindowFocus);
});
</script>
