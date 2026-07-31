<template>
  <TitleBar />
  <div class="app-body">
    <Sidebar :current="currentView" @navigate="onNavigate" />
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
      <div v-if="currentView === 'home'" class="main-content">
        <div class="left-col">
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
            :connected="serialConnected"
            @toggle-upload="onToggleUpload"
          />
        </div>
      </div>
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
import { ref, onMounted, onUnmounted, watch } from 'vue';
import TitleBar from './components/TitleBar.vue';
import Sidebar from './components/Sidebar.vue';
import StatusBar from './components/StatusBar.vue';
import SerialPanel from './components/SerialPanel.vue';
import KmnetPanel from './components/KmnetPanel.vue';
import MonitorPanel from './components/MonitorPanel.vue';
import MacroPanel from './components/MacroPanel.vue';
import JitterMacroPanel from './components/JitterMacroPanel.vue';
import MouseCurvePanel from './components/MouseCurvePanel.vue';
import GamepadMapperPanel from './components/GamepadMapperPanel.vue';
import DocsPanel from './components/DocsPanel.vue';
import FirmwarePanel from './components/FirmwarePanel.vue';
import TowmcuFirmwarePanel from './components/TowmcuFirmwarePanel.vue';
import DebugPanel from './components/DebugPanel.vue';

const api = window.driverApi;

const VALID_VIEWS = ['home', 'docs', 'macro', 'jitter', 'mouse-curve', 'gamepad', 'firmware', 'towmcu-firmware', 'debug'];
const VIEW_STORAGE_KEY = 'ysc_ui_current_view';

function loadStoredView() {
  try {
    const v = localStorage.getItem(VIEW_STORAGE_KEY);
    return VALID_VIEWS.indexOf(v) >= 0 ? v : 'home';
  } catch (e) { return 'home'; }
}

const currentView = ref(loadStoredView());

watch(currentView, function(v) {
  try { localStorage.setItem(VIEW_STORAGE_KEY, v); } catch (e) { /* ignore */ }
});
const serialConnected = ref(false);
const serialPort = ref('');
const serialBaud = ref(0);
const netRunning = ref(false);
const netPort = ref(5251);
const netIp = ref('');
const netMac = ref('');
const version = ref('1.11.0');
const ports = ref([]);

const firmwareFilePath = ref('');
const firmwareFileInfo = ref('');

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

const cleanups = [];

function listen(event, handler) {
  api.on(event, handler);
  cleanups.push(function() { api.off(event, handler); });
}

function onNavigate(view) {
  currentView.value = view;
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

let monitorTimer = null;
let portsRefreshAt = 0;

function refreshPortsThrottled() {
  if (serialConnected.value) return;
  const now = Date.now();
  if (now - portsRefreshAt < 800) return;
  portsRefreshAt = now;
  api.send('enum_ports');
}

function onWindowFocus() {
  refreshPortsThrottled();
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
  });

  listen('serial_disconnected', function() {
    serialConnected.value = false;
    if (kmnetSelfCheck.value.active && kmnetSelfCheck.value.step === 'disconnecting') {
      selfCheckBeginReconnect();
    }
  });

  listen('serial_error', function(data) {
    if (kmnetSelfCheck.value.active && kmnetSelfCheck.value.step === 'reconnecting') {
      var msg = (data && data.message) ? data.message : '未知错误';
      selfCheckScheduleReconnectRetry(msg);
      return;
    }
    if (kmnetSelfCheck.value.active) {
      selfCheckFail('串口错误:' + (data && data.message ? data.message : '未知错误'));
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
  });

  api.send('get_state');
  portsRefreshAt = Date.now();
  api.send('enum_ports');

  window.addEventListener('focus', onWindowFocus);

  monitorTimer = setInterval(function() {
    api.send('get_monitor');
  }, 100);
});

onUnmounted(function() {
  for (var i = 0; i < cleanups.length; i++) cleanups[i]();
  if (monitorTimer) clearInterval(monitorTimer);
  selfCheckCleanupTimers();
  window.removeEventListener('focus', onWindowFocus);
});
</script>
