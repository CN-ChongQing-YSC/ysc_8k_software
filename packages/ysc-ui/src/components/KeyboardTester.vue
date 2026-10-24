<template>
  <div class="kbd-tester">
    <div class="kbd-toolbar">
      <span class="kbd-title">{{ t('cmdTester.kbdTitle') }}</span>
      <button class="btn btn-action kbd-release" @click="releaseAll">{{ t('cmdTester.kbdReleaseAll') }}</button>
    </div>

    <div class="kbd-wrap">
      <!-- main block: Esc/F-row + main rows + modifier row -->
      <div class="kbd-main">
        <div class="kbd-row" v-for="(row, ri) in mainRows" :key="'m' + ri">
          <button
            v-for="k in row"
            :key="k.c"
            class="kbd-key"
            :class="{ pressed: !!pressed[k.c], mod: !!k.mod }"
            :style="{ flexGrow: k.w || 1, flexBasis: '0' }"
            @mousedown.prevent="press(k)"
            @mouseup="release(k)"
            @mouseleave="release(k)"
          >{{ k.l }}</button>
        </div>
      </div>

      <!-- nav + arrows cluster (3-col grid) -->
      <div class="kbd-nav">
        <template v-for="(k, i) in navKeys" :key="'n' + i">
          <div v-if="k.blank" class="kbd-spacer"></div>
          <button
            v-else
            class="kbd-key"
            :class="{ pressed: !!pressed[k.c], mod: !!k.mod }"
            @mousedown.prevent="press(k)"
            @mouseup="release(k)"
            @mouseleave="release(k)"
          >{{ k.l }}</button>
        </template>
      </div>

      <!-- numpad (4-col grid, + / Enter / 0 span) -->
      <div class="kbd-numpad">
        <button
          v-for="k in numpadKeys"
          :key="'k' + k.c"
          class="kbd-key"
          :class="{ pressed: !!pressed[k.c], mod: !!k.mod }"
          :style="k.rs ? { gridRow: 'span ' + k.rs } : (k.cs ? { gridColumn: 'span ' + k.cs } : null)"
          @mousedown.prevent="press(k)"
          @mouseup="release(k)"
          @mouseleave="release(k)"
        >{{ k.l }}</button>
      </div>
    </div>

    <p class="kbd-hint">{{ t('cmdTester.kbdHint') }}</p>
  </div>
</template>

<script setup>
import { reactive } from 'vue';
import { useI18n } from '../i18n/index.js';

const emit = defineEmits(['send']);
const { t } = useI18n();

/* pressed-state map keyed by HID keycode */
const pressed = reactive({});

/* ---- full 104-key layout. c = USB HID keycode (decimal). w = flex-grow for
 *      width. mod = modifier/special styling. rs/cs = numpad grid spans. ---- */

/* grave(`)=0x35, 1..0 = 0x1E..0x27, -=0x2D, ==0x2E, Bksp=0x2A, Tab=0x2B,
 * Enter=0x28, Esc=0x29, Space=0x2C, Caps=0x39, brackets 0x2F/0x30/0x31,
 * ;=0x33 '=0x34 ,=0x36 .=0x37 /=0x38, F1..F12=0x3A..0x45,
 * modifiers LCtrl/LShift/LAlt/LGui=0xE0..0xE3, RCtrl/RShift/RAlt/RGui=0xE4..0xE7,
 * arrows Right/Left/Down/Up=0x4F/0x50/0x51/0x52, nav Ins/Home/PgUp/Del/End/PgDn
 * =0x49..0x4E, PrtSc/ScrLk/Pause=0x46/0x47/0x48, App=0x65,
 * numpad NumLk..KP. = 0x53..0x63, KP+=0x57, KPEnter=0x58. */
const mainRows = [
  [ {l:'Esc',c:41}, {l:'F1',c:58},{l:'F2',c:59},{l:'F3',c:60},{l:'F4',c:61},{l:'F5',c:62},{l:'F6',c:63},{l:'F7',c:64},{l:'F8',c:65},{l:'F9',c:66},{l:'F10',c:67},{l:'F11',c:68},{l:'F12',c:69} ],
  [ {l:'`',c:53},{l:'1',c:30},{l:'2',c:31},{l:'3',c:32},{l:'4',c:33},{l:'5',c:34},{l:'6',c:35},{l:'7',c:36},{l:'8',c:37},{l:'9',c:38},{l:'0',c:39},{l:'-',c:45},{l:'=',c:46},{l:'Bksp',c:42,w:2,mod:1} ],
  [ {l:'Tab',c:43,w:1.5,mod:1},{l:'Q',c:20},{l:'W',c:26},{l:'E',c:8},{l:'R',c:21},{l:'T',c:23},{l:'Y',c:28},{l:'U',c:24},{l:'I',c:12},{l:'O',c:18},{l:'P',c:19},{l:'[',c:47},{l:']',c:48},{l:'\\',c:49,w:1.5,mod:1} ],
  [ {l:'Caps',c:57,w:1.75,mod:1},{l:'A',c:4},{l:'S',c:22},{l:'D',c:7},{l:'F',c:9},{l:'G',c:10},{l:'H',c:11},{l:'J',c:13},{l:'K',c:14},{l:'L',c:15},{l:';',c:51},{l:"'",c:52},{l:'Enter',c:40,w:2.25,mod:1} ],
  [ {l:'Shift',c:225,w:2.25,mod:1},{l:'Z',c:29},{l:'X',c:27},{l:'C',c:6},{l:'V',c:25},{l:'B',c:5},{l:'N',c:17},{l:'M',c:16},{l:',',c:54},{l:'.',c:55},{l:'/',c:56},{l:'Shift',c:229,w:2.75,mod:1} ],
  [ {l:'Ctrl',c:224,w:1.25,mod:1},{l:'Win',c:227,w:1.25,mod:1},{l:'Alt',c:226,w:1.25,mod:1},{l:'Space',c:44,w:6.25},{l:'Alt',c:230,w:1.25,mod:1},{l:'Win',c:231,w:1.25,mod:1},{l:'Menu',c:101,w:1.25,mod:1},{l:'Ctrl',c:228,w:1.25,mod:1} ],
];

/* nav + arrow cluster, read row-by-row into a 3-col grid. blank = empty cell. */
const navKeys = [
  {l:'PrtSc',c:70,mod:1},{l:'ScrLk',c:71,mod:1},{l:'Pause',c:72,mod:1},
  {l:'Ins',c:73},{l:'Home',c:74},{l:'PgUp',c:75},
  {l:'Del',c:76},{l:'End',c:77},{l:'PgDn',c:78},
  {blank:1},{blank:1},{blank:1},
  {blank:1},{l:'↑',c:82},{blank:1},
  {l:'←',c:80},{l:'↓',c:81},{l:'→',c:79},
];

/* numpad, flat — a 4-col grid with auto-flow places + (rs2), Enter (rs2),
 * and 0 (cs2) into the standard numpad shape. */
const numpadKeys = [
  {l:'Num',c:83,mod:1},{l:'/',c:84},{l:'*',c:85},{l:'-',c:86},
  {l:'7',c:95},{l:'8',c:96},{l:'9',c:97},{l:'+',c:87,rs:2},
  {l:'4',c:92},{l:'5',c:93},{l:'6',c:94},
  {l:'1',c:89},{l:'2',c:90},{l:'3',c:91},{l:'Ent',c:88,rs:2},
  {l:'0',c:98,cs:2},{l:'.',c:99},
];

function press(k) {
  pressed[k.c] = true;
  emit('send', '{"cmd":45,"kc":' + k.c + ',"down":1}');
}
function release(k) {
  if (!pressed[k.c]) return;
  pressed[k.c] = false;
  emit('send', '{"cmd":45,"kc":' + k.c + ',"down":0}');
}
function releaseAll() {
  for (const code in pressed) {
    if (pressed[code]) emit('send', '{"cmd":45,"kc":' + code + ',"down":0}');
    pressed[code] = false;
  }
  emit('send', '{"cmd":46}');
}
</script>

<style scoped>
.kbd-tester { margin-top: 10px; user-select: none; }
.kbd-toolbar { display: flex; align-items: center; justify-content: space-between; margin-bottom: 8px; }
.kbd-title { font-weight: 600; font-size: 13px; opacity: 0.85; }
.kbd-release { padding: 3px 10px; font-size: 12px; }

.kbd-wrap {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: flex-start;
  overflow-x: auto;
  padding-bottom: 4px;
}
.kbd-main { display: flex; flex-direction: column; gap: 4px; }
.kbd-row { display: flex; gap: 4px; }

.kbd-nav {
  display: grid;
  grid-template-columns: repeat(3, 34px);
  gap: 4px;
}
.kbd-numpad {
  display: grid;
  grid-template-columns: repeat(4, 34px);
  grid-auto-rows: 34px;
  grid-auto-flow: row;
  gap: 4px;
}

.kbd-key {
  height: 34px;
  min-width: 30px;
  padding: 0 4px;
  font-size: 11px;
  line-height: 1;
  color: var(--text, #e6e6e6);
  background: var(--panel-2, #2a2a2e);
  border: 1px solid var(--border, #3a3a40);
  border-radius: 5px;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  transition: background 0.05s, transform 0.05s;
}
.kbd-key:hover { background: var(--panel-3, #34343a); }
.kbd-key.mod { font-size: 10px; opacity: 0.92; }
.kbd-key.pressed {
  background: var(--accent, #4a93ff);
  color: #fff;
  border-color: var(--accent, #4a93ff);
  transform: translateY(1px);
}
.kbd-spacer { width: 34px; height: 34px; }

.kbd-hint { margin: 8px 0 0; font-size: 11px; opacity: 0.6; }
</style>
