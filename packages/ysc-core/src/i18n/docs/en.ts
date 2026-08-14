// English documentation content for DocsPanel & CommandTester.
// Mirrors the Chinese version in ./zh.js — same structure, English text.

export const docs = {
  serial: {
    title: 'Serial Protocol',
    basicTitle: 'Basic Parameters',
    basicRows: [
      ['Baudrate', 'Firmware allowlist: 115200/230400/460800/921600/1M/1.5M/2M/3M. 4M is an experimental extension that the hardware may not reliably support. Switchable via cmd:133 or km.baud.'],
      ['Data bits', '8'],
      ['Stop bits', '1'],
      ['Parity', 'None'],
      ['Flow control', 'No hardware flow control'],
      ['DMA', 'USART1 has one TX DMA channel and one RX DMA channel; IDLE IRQ triggers frame reception'],
    ],
    frameTitle: 'Frame Format',
    frameFormat: '<START>[2-byte big-endian length][JSON payload]<END>',
    frameNote: 'YSC protocol frame: <START>(7B) + 2-byte big-endian total length + JSON payload + <END>(5B). MAKCU protocol: plain text km.command(args)\\r\\n, response terminated with \\r\\n>>>. A binary baud frame is also supported: 0xDE 0xAD 0x05 0x00 0xA5 + 4-byte little-endian baud (no text markers).',
    sections: [
      {
        title: 'YSC Commands',
        cards: [
          {
            title: 'CMD_MOUSE_MOVE (30)  Smooth mouse move',
            format: '{"cmd":30,"x":100,"y":50,"c":1,"rx":1.0,"ry":1.0,"rc":0}',
            note: 'Smooth move. x/y are total target displacement, c is step count (one chunk per step). Optional rx/ry are physical mouse X/Y axis reduction factors (float, default 1.0 = no reduction). rc is the reduction duration in <strong>milliseconds</strong> (int16, default 0 = off). The firmware auto-adapts to the current mouse polling rate (1K = 1ms/packet, 8K = 8 packets/ms), so rc=50 lasts 50ms at either rate. Reduction applies to <strong>physical mouse input</strong>, not to this command\'s displacement — useful for recoil compensation: scale down the pull-down input while firing. Firmware uses a lightweight JSON scanner (skips cJSON malloc/free) so this command is safe to call at high frequency.',
          },
          {
            title: 'CMD_MOUSE_MOVE_TOW (31)  Directional additive move',
            format: '{"cmd":31,"x":100,"y":50,"c":1,"rx":1.0,"ry":1.0,"rc":0}',
            note: 'Directional move, additive with cmd:30 (displacements sum in HID reports). Fields identical to cmd:30. When both cmd:30 and cmd:31 set reduction (rc&gt;0), <strong>cmd:31 takes priority</strong>.',
          },
          {
            title: 'CMD_MOUSE_BUTTON (32)  Auto-release button',
            format: '{"cmd":32,"b":1,"c":10}',
            note: 'Simulates a button press, auto-released after c reports. b is the button bit mask: 1=left, 2=right, 4=middle.',
          },
          {
            title: 'CMD_MOUSE_BUTTON_STATE (33)  Set button state directly',
            format: '{"cmd":33,"b":1,"s":1}',
            note: 'Sets button state directly (immediate, no report count). s=1 press, s=0 release. b uses the same bitmask as cmd:32.',
          },
          {
            title: 'CMD_ChangeUploadStatus (34)  Upload toggle',
            format: '{"cmd":34,"status":1}',
            note: 'Enable/disable HID data upload. status&gt;0 enables, status=0 disables.',
          },
          {
            title: 'CMD_MOUSE_WHEEL_HOLD (35)  Hold wheel',
            format: '{"cmd":35,"w":-1,"c":100}',
            note: 'Wheel hold. w is the wheel value (int8, -1=down, 1=up; other values are clamped to ±1), c is duration in milliseconds (uint32). Device emits the wheel value for the specified duration.',
          },
          {
            title: 'CMD_MACRO_SET (36)  Configure button macro',
            format: '{"cmd":36,"s":0,"e":1,"t":3,"p":1,"w":-1,"i":30,"d":10,"ij":5,"dj":2}',
            note: 'Configure a button macro slot. s=slot (0-7), e=enable (0/1), t=trigger bitmask, p=suppress bitmask, w=wheel value (int8), i=wheel pulse period ms (optional, 0=continuous), d=active duration per period ms (optional, <=i, 0=full cycle), ij=interval jitter ms (optional, each cycle actual interval randomized within i±ij), dj=duration jitter ms (optional, each cycle actual duration randomized within d±dj). Button bits: 1=left, 2=right, 4=middle, 8=back, 16=forward. When i>0 the wheel pulses on a period (e.g. i=30,d=10,ij=5,dj=2 = fire roughly every 30ms±5, active ~10ms±2 each). Response: {code:200,message:"macro_set",data:"<slot>"}.',
          },
          {
            title: 'CMD_MACRO_GET (37)  Query button macro',
            format: '{"cmd":37,"s":-1}',
            note: 'Query macro config. s=-1 returns all slots (array);s=0..7 returns a single slot. Response fields: s/e/t/p/w/i/d/ij/dj.',
          },
          {
            title: 'CMD_MACRO_RESET (38)  Reset macros',
            format: '{"cmd":38}',
            note: 'Reset all macro slots to firmware defaults (all disabled). Response: {code:200,message:"macro_reset"}.',
          },
          {
            title: 'CMD_JITTER_SET (39)  Jitter (recoil pattern) config',
            format: '{"cmd":39,"e":1,"t":1,"ax":10,"fx":0,"py":5,"fy":0}',
            note: 'Configure mouse jitter. e=enable(0/1), t=trigger(t=1 means fire/button), ax=X axis amplitude, fx=X frequency, py=Y amplitude, fy=Y frequency. Save/query result returned async via debug_response(message:"jitter").',
          },
          {
            title: 'CMD_JITTER_GET (40)  Query jitter',
            format: '{"cmd":40}',
            note: 'Query current jitter config. Response via debug_response(message:"jitter") returns {e,t,ax,fx,py,fy}.',
          },
          {
            title: 'CMD_JITTER_RESET (41)  Reset jitter',
            format: '{"cmd":41}',
            note: 'Reset jitter config to defaults (all zero, disabled).',
          },
          {
            title: 'CMD_MOUSE_CURVE_SET (42)  Mouse curve config',
            format: '{"cmd":42,"e":1,"p":2,"n":4,"d":0,"j":15}',
            note: 'Configure mouse movement curve. e=enable(0/1), p=profile, n=segments, d=duration(ms,0=profile default), j=jitter amplitude. Save/query result returned async via debug_response(message:"mouse_curve").',
          },
          {
            title: 'CMD_MOUSE_CURVE_GET (43)  Query mouse curve',
            format: '{"cmd":43}',
            note: 'Query current mouse curve. Response via debug_response(message:"mouse_curve") returns {enabled,profile,segments,duration,jitter}.',
          },
          {
            title: 'CMD_MOUSE_CURVE_RESET (44)  Reset mouse curve',
            format: '{"cmd":44}',
            note: 'Reset mouse curve to defaults (profile=2, segments=4, duration=0, jitter=15).',
          },
          {
            title: 'CMD_JUMP_TO_IAP (50)  Enter Bootloader',
            format: '{"cmd":50}',
            note: 'Erases the calibration word 0x5AA55AA5 to 0x0808FFFC and triggers a system reset. Device first replies {code:200,message:"entering_iap"}, then reboots into the IAP bootloader waiting for firmware upgrade.',
          },
          {
            title: 'CMD_GET_VERSION (132 / 0x84)  Query build date',
            format: '{"cmd":132}',
            note: 'Returns firmware compile date (__DATE__ macro). Response: {code:200,message:"version",data:"<compile date>"}.',
          },
          {
            title: 'CMD_SET_BAUDRATE (133 / 0x85)  Dynamic baudrate switch',
            format: '{"cmd":133,"baud":3000000}',
            note: 'Dynamically switch USART1 baudrate. Firmware skips allowlist validation and configures the hardware USART directly — recommended to only send rates from the 8-step allowlist (115200~3000000). Response: {code:200,message:"switching"}, then the device immediately reconfigures USART; the host must switch baudrate in lockstep.',
          },
        ],
      },
      {
        title: 'MAKCU Commands',
        cards: [
          {
            title: 'Mouse control',
            isTable: true,
            headers: ['Command', 'Format', 'Description'],
            rows: [
              ['Move', 'km.move(x,y[,seg])', 'Mouse move, segments 1..512 (clamped if out of range)'],
              ['Left', 'km.left(1/0)', '1=press, 0=release, 2=silent release (skips the button push queue)'],
              ['Right', 'km.right(1/0)', 'Same as left'],
              ['Middle', 'km.middle(1/0)', 'Same as left'],
              ['Side1', 'km.ms1(1/0)', 'Same as left'],
              ['Side2', 'km.ms2(1/0)', 'Same as left'],
              ['Wheel', 'km.wheel(-1/1)', 'Scroll one notch; only ±1 accepted (clamped)'],
            ],
          },
          {
            title: 'Axis / button lock',
            isTable: true,
            headers: ['Command', 'Description'],
            rows: [
              ['km.lock_mx(0/1) / km.lock_mx+ / km.lock_mx-', 'Lock X axis (positive/negative can be locked separately)'],
              ['km.lock_my(0/1) / km.lock_my+ / km.lock_my-', 'Lock Y axis, same semantics'],
              ['km.lock_ml/mr/mm/ms1/ms2(0/1)', 'Lock physical button so input is ignored'],
              ['km.invert_x/y(0/1)', 'Invert X or Y axis direction'],
              ['km.swap_xy(0/1)', 'Swap X/Y axis mapping'],
            ],
          },
          {
            title: 'Catch mode',
            isTable: true,
            headers: ['Command', 'Description'],
            rows: [
              ['km.catch_ml(0/1)', 'Left button catch mode: 0=auto, 1=manual'],
              ['km.catch_mm(0/1)', 'Middle button catch mode'],
              ['km.catch_mr(0/1)', 'Right button catch mode'],
              ['km.catch_ms1(0/1)', 'Side1 catch mode'],
              ['km.catch_ms2(0/1)', 'Side2 catch mode'],
            ],
          },
          {
            title: 'System commands',
            isTable: true,
            headers: ['Command', 'Description'],
            rows: [
              ['km.version()', 'Returns km.version(km.MAKCU)'],
              ['km.echo(0/1)', 'Command echo toggle (default on)'],
              ['km.baud(rate)', 'Switch baudrate (rate >= 115200, allowlist not enforced);no-arg GET returns current baud'],
              ['km.buttons(mode,period)', 'Button push mode: mode=0 off, mode>0 on; period cycle 1..1000 ms'],
              ['km.serial("xxx") / km.serial(0)', 'Set/reset device serial (max 32 bytes, escapes supported);no-arg GET returns current'],
              ['km.reboot()', 'Soft reset, replies km.reboot() then restarts'],
              ['km.scroll_side(0/1)', 'When enabled, side2 triggers wheel events'],
              ['km.scroll_lr(0/1)', 'When enabled, left+right/side pressed together triggers wheel'],
            ],
          },
          {
            title: 'Binary baudrate switch (special)',
            isTable: false,
            note: 'Besides text commands, the firmware accepts a 9-byte binary baud switch frame: 0xDE 0xAD 0x05 0x00 0xA5 + 4-byte little-endian baud. It bypasses the MAKCU text parser and reconfigures USART directly — useful for recovery when the current baudrate is unknown.',
          },
        ],
      },
    ],
  },

  protocols: [
    {
      id: 'ysc',
      name: 'YSC Protocol',
      commands: [
        {
          id: 'ysc_move', name: 'Mouse Move (cmd:30)',
          format: '{"cmd":30,"x":100,"y":50,"c":1,"rx":1.0,"ry":1.0,"rc":0}',
          note: 'Smooth mouse move. x/y are total target displacement, c is step count. Optional rx/ry/rc enable physical-mouse reduction (recoil compensation): rc is the reduction duration in ms; the firmware auto-adapts to polling rate (1K=1 packet/ms, 8K=8 packets/ms), so rc=N lasts N ms on both.',
          params: [
            { name: 'x', type: 'int16', desc: 'X target displacement (negative = left)' },
            { name: 'y', type: 'int16', desc: 'Y target displacement (negative = up)' },
            { name: 'c', type: 'int16', desc: 'Step count (1=instant, >1 smooth)' },
            { name: 'rx', type: 'float', desc: 'Optional. Physical mouse X reduction factor, default 1.0' },
            { name: 'ry', type: 'float', desc: 'Optional. Physical mouse Y reduction factor, default 1.0' },
            { name: 'rc', type: 'int16', desc: 'Optional. Reduction duration in ms (polling-rate adaptive), default 0=off' },
          ],
        },
        {
          id: 'ysc_move_tow', name: 'Directional Move (cmd:31)',
          format: '{"cmd":31,"x":100,"y":50,"c":1,"rx":1.0,"ry":1.0,"rc":0}',
          note: 'Directional move, additive with cmd:30 (displacements sum in HID reports). Fields identical to cmd:30. When both cmd:30 and cmd:31 set reduction (rc>0), cmd:31 takes priority.',
          params: [
            { name: 'x', type: 'int16', desc: 'X target displacement' },
            { name: 'y', type: 'int16', desc: 'Y target displacement' },
            { name: 'c', type: 'int16', desc: 'Step count' },
            { name: 'rx', type: 'float', desc: 'Optional. Physical mouse X reduction factor' },
            { name: 'ry', type: 'float', desc: 'Optional. Physical mouse Y reduction factor' },
            { name: 'rc', type: 'int16', desc: 'Optional. Reduction duration in ms (polling-rate adaptive)' },
          ],
        },
        {
          id: 'ysc_button', name: 'Mouse Button (cmd:32)',
          format: '{"cmd":32,"b":1,"c":10}',
          note: 'Simulated button press, auto-released after c reports. b is the button bit mask (1=left, 2=right, 4=middle).',
          params: [
            { name: 'b', type: 'uint8', desc: 'Button bitmask (1=left, 2=right, 4=middle)' },
            { name: 'c', type: 'uint32', desc: 'Report count before auto-release' },
          ],
        },
        {
          id: 'ysc_button_state', name: 'Button State (cmd:33)',
          format: '{"cmd":33,"b":1,"s":1}',
          note: 'Set button state directly (immediate, no report count). s=1 press, s=0 release.',
          params: [
            { name: 'b', type: 'uint8', desc: 'Button bitmask' },
            { name: 's', type: 'uint8', desc: '0=release, 1=press' },
          ],
        },
        {
          id: 'ysc_upload', name: 'Toggle Upload (cmd:34)',
          format: '{"cmd":34,"status":1}',
          note: 'Enable/disable HID data upload. When enabled, the device reports mouse data (buttons + motion) over serial.',
          actions: [
            { label: 'Enable', cmd: '{"cmd":34,"status":1}' },
            { label: 'Disable', cmd: '{"cmd":34,"status":0}' },
          ],
        },
        {
          id: 'ysc_wheel_hold', name: 'Wheel Hold (cmd:35)',
          format: '{"cmd":35,"w":-1,"c":100}',
          note: 'Wheel hold. w is the wheel value (only ±1 accepted), c is duration in ms.',
          params: [
            { name: 'w', type: 'int8', desc: 'Wheel value (-1=down, 1=up; others clamped)' },
            { name: 'c', type: 'uint32', desc: 'Duration in ms' },
          ],
          actions: [
            { label: 'Down 100ms', cmd: '{"cmd":35,"w":-1,"c":100}' },
            { label: 'Down 200ms', cmd: '{"cmd":35,"w":-1,"c":200}' },
            { label: 'Up 100ms', cmd: '{"cmd":35,"w":1,"c":100}' },
          ],
        },
        {
          id: 'ysc_macro_set', name: 'Set Macro (cmd:36)',
          format: '{"cmd":36,"s":0,"e":1,"t":3,"p":1,"w":-1,"i":30,"d":10}',
          note: 'Configure a button macro. s=slot (0-7), e=enable, t=trigger mask, p=suppress mask, w=wheel value, i=interval period ms (0=continuous), d=active duration per period ms. Button bits: 1=left, 2=right, 4=middle, 8=back, 16=forward.',
          params: [
            { name: 's', type: 'uint8', desc: 'Slot (0-7)' },
            { name: 'e', type: 'uint8', desc: 'Enable (0/1)' },
            { name: 't', type: 'uint8', desc: 'Trigger button bitmask' },
            { name: 'p', type: 'uint8', desc: 'Suppress button bitmask' },
            { name: 'w', type: 'int8', desc: 'Wheel value' },
            { name: 'i', type: 'uint16', desc: 'Wheel pulse period (ms), 0=continuous' },
            { name: 'd', type: 'uint16', desc: 'Active duration per period (ms), <=i, 0=full cycle' },
          ],
          actions: [
            { label: 'Left+Right -> suppress left + wheel', cmd: '{"cmd":36,"s":0,"e":1,"t":3,"p":1,"w":-1}' },
            { label: 'Forward -> wheel', cmd: '{"cmd":36,"s":2,"e":1,"t":16,"p":0,"w":-1}' },
            { label: 'Forward -> wheel (every 30ms / 10ms)', cmd: '{"cmd":36,"s":2,"e":1,"t":16,"p":0,"w":-1,"i":30,"d":10}' },
            { label: 'Disable slot 0', cmd: '{"cmd":36,"s":0,"e":0,"t":3,"p":1,"w":-1}' },
          ],
        },
        {
          id: 'ysc_macro_get', name: 'Query Macro (cmd:37)',
          format: '{"cmd":37,"s":-1}',
          note: 'Query macro config. s=-1 returns all, s=0-7 returns one slot.',
          actions: [
            { label: 'Query all', cmd: '{"cmd":37,"s":-1}' },
            { label: 'Query slot 0', cmd: '{"cmd":37,"s":0}' },
          ],
        },
        {
          id: 'ysc_macro_reset', name: 'Reset Macros (cmd:38)',
          format: '{"cmd":38}',
          note: 'Reset all macro config to firmware defaults.',
          actions: [
            { label: 'Reset', cmd: '{"cmd":38}' },
          ],
        },
        {
          id: 'ysc_kbd_key', name: 'Keyboard Key (cmd:45)',
          format: '{"cmd":45,"kc":4,"down":1}',
          note: 'Inject one keyboard press/release. kc=HID keycode (0x04-0xE7; modifiers 0xE0-0xE7=Left/Right Ctrl/Shift/Alt/GUI); down=1 press, 0 release. Only takes effect when the passthrough device has a keyboard interface — otherwise the device replies 404 no_keyboard. Press multiple keys by calling repeatedly; release with down:0 or cmd:46. Real key presses still pass through (merged with injected state).',
          params: [
            { name: 'kc',   type: 'uint8', desc: 'HID keycode (0x04-0xE7; modifiers 0xE0-0xE7)' },
            { name: 'down', type: 'uint8', desc: '1=press, 0=release' },
          ],
          actions: [
            { label: 'Press a',          cmd: '{"cmd":45,"kc":4,"down":1}' },
            { label: 'Release a',        cmd: '{"cmd":45,"kc":4,"down":0}' },
            { label: 'Press Left Shift', cmd: '{"cmd":45,"kc":225,"down":1}' },
            { label: 'Release Left Shift', cmd: '{"cmd":45,"kc":225,"down":0}' },
          ],
        },
        {
          id: 'ysc_kbd_release_all', name: 'Keyboard Release All (cmd:46)',
          format: '{"cmd":46}',
          note: 'Release every injected key. Ignored when there is no keyboard interface.',
          actions: [
            { label: 'Release all', cmd: '{"cmd":46}' },
          ],
        },
        {
          id: 'ysc_kbd_type_string', name: 'Keyboard Type String (cmd:47)',
          format: '{"cmd":47,"s":"Wasd123A123vciseC"}',
          note: 'Type a mixed-case ASCII string char-by-char. The firmware reads the CapsLock state the PC sends via the HID LED Output report (bit1) and, for each character, computes press-Shift = (char needs shift) XOR (CapsLock on AND char is a letter); when Shift is needed it auto-presses Left Shift (0xE1) and never toggles the CapsLock key itself. Symbols/digits/spaces are unaffected by CapsLock. Replies 404 no_keyboard, 400 too_long (>128 bytes)/empty_string, or 409 busy (already typing). On accept replies 200 typing_started, and sends an async 200 type_done on completion (data has typed/skipped/total). Unmappable characters (control chars, etc.) are skipped and counted. Real key presses still pass through (merged).',
          params: [
            { name: 's', type: 'string', desc: 'ASCII string to type, 1..128 bytes, JSON-escaped' },
          ],
          actions: [
            { label: 'Wasd123A123vciseC',        cmd: '{"cmd":47,"s":"Wasd123A123vciseC"}' },
            { label: 'Hello World!',             cmd: '{"cmd":47,"s":"Hello World!"}' },
            { label: 'test@123',                 cmd: '{"cmd":47,"s":"test@123"}' },
            { label: 'Shift symbols !@#$%^&*()', cmd: '{"cmd":47,"s":"!@#$%^&*()"}' },
            { label: 'Empty string (expect 400)', cmd: '{"cmd":47,"s":""}' },
          ],
        },
        {
          id: 'ysc_kbd_type_status', name: 'Keyboard Type Status (cmd:48)',
          format: '{"cmd":48}',
          note: 'Query the typing engine progress. Response data: busy(0/1) / remaining(chars not yet typed) / total / typed(done) / skipped(unmappable). busy=0 means idle or finished. Used to type strings longer than the 128-byte single-call limit: send a cmd:47 chunk -> poll cmd:48 until busy=0 -> send the next chunk. Synchronous query (send + wait for reply); the host flushes RX before each send, so async type_done / mouse-upload frames never desync the pairing.',
          params: [],
          actions: [
            { label: 'Query status', cmd: '{"cmd":48}' },
          ],
        },
        {
          id: 'ysc_jump_iap', name: 'Enter Bootloader (cmd:50)',
          format: '{"cmd":50}',
          note: 'Erase calibration word and reboot into IAP bootloader. Device first replies entering_iap, then drops USB and waits for firmware upgrade. Dual-MCU v2 (towmcu) CDC path: the device clears its anti-brick flag, writes BKP_DR1=0xA5A5, resets, and re-enumerates with serial TOWMCUIAP — the COM number may change, so rescan ports.',
          params: [],
        },
        {
          id: 'ysc_get_version', name: 'Get Version (cmd:0x84)',
          format: '{"cmd":132}',
          note: 'Query firmware build date. Returns {code:200,message:"version",data:"build date"}. Dual-MCU v2 (towmcu) CDC path: APP returns message="ysc-towmcu-L|R v1.0"; IAP returns data="YSC-IAP". Classify mode by the substring "-IAP".',
          params: [],
        },
        {
          id: 'ysc_set_baud', name: 'Switch Baudrate (cmd:0x85)',
          format: '{"cmd":133,"baud":3000000}',
          note: 'Dynamically switch serial baudrate. Firmware skips allowlist validation;recommended to send only allowlist rates (115200~3000000).',
          params: [
            { name: 'baud', type: 'uint32', desc: 'Target baudrate' },
          ],
        },
      ],
    },
    {
      id: 'makcu',
      name: 'MAKCU Protocol',
      commands: [
        {
          id: 'mk_move', name: 'Mouse Move',
          format: 'km.move(100,50)',
          note: 'Move the mouse. Supports an optional segments parameter for stepped motion (range 1..512).',
          params: [
            { name: 'x', type: 'int', desc: 'X offset' },
            { name: 'y', type: 'int', desc: 'Y offset' },
            { name: 'segments', type: 'int', desc: 'Optional, step count (1..512)' },
          ],
        },
        {
          id: 'mk_left', name: 'Left Button',
          format: 'km.left(1)',
          note: 'Left button. 1=press, 0=release, 2=silent release (no push-queue entry).',
          actions: [
            { label: 'Press', cmd: 'km.left(1)' },
            { label: 'Release', cmd: 'km.left(0)' },
            { label: 'Silent release', cmd: 'km.left(2)' },
          ],
        },
        {
          id: 'mk_right', name: 'Right Button',
          format: 'km.right(1)',
          note: 'Right button. Same params as left.',
          actions: [
            { label: 'Press', cmd: 'km.right(1)' },
            { label: 'Release', cmd: 'km.right(0)' },
            { label: 'Silent release', cmd: 'km.right(2)' },
          ],
        },
        {
          id: 'mk_middle', name: 'Middle Button',
          format: 'km.middle(1)',
          note: 'Middle button. Same params as left.',
          actions: [
            { label: 'Press', cmd: 'km.middle(1)' },
            { label: 'Release', cmd: 'km.middle(0)' },
            { label: 'Silent release', cmd: 'km.middle(2)' },
          ],
        },
        {
          id: 'mk_ms1', name: 'Side1 (ms1)',
          format: 'km.ms1(1)',
          note: 'Side button 1.',
          actions: [
            { label: 'Press', cmd: 'km.ms1(1)' },
            { label: 'Release', cmd: 'km.ms1(0)' },
          ],
        },
        {
          id: 'mk_ms2', name: 'Side2 (ms2)',
          format: 'km.ms2(1)',
          note: 'Side button 2.',
          actions: [
            { label: 'Press', cmd: 'km.ms2(1)' },
            { label: 'Release', cmd: 'km.ms2(0)' },
          ],
        },
        {
          id: 'mk_wheel', name: 'Wheel',
          format: 'km.wheel(1)',
          note: 'Scroll one notch. Only -1 (down) or 1 (up) accepted; other values are clamped.',
          actions: [
            { label: 'Scroll up', cmd: 'km.wheel(1)' },
            { label: 'Scroll down', cmd: 'km.wheel(-1)' },
          ],
        },
        {
          id: 'mk_lock_mx', name: 'Lock X Axis',
          format: 'km.lock_mx(1)',
          note: 'Lock X axis. 1=lock both directions, 0=unlock. Use lock_mx+ / lock_mx- to lock one direction.',
          actions: [
            { label: 'Lock', cmd: 'km.lock_mx(1)' },
            { label: 'Unlock', cmd: 'km.lock_mx(0)' },
          ],
        },
        {
          id: 'mk_lock_my', name: 'Lock Y Axis',
          format: 'km.lock_my(1)',
          note: 'Lock Y axis. Same as X;supports lock_my+ / lock_my-.',
          actions: [
            { label: 'Lock', cmd: 'km.lock_my(1)' },
            { label: 'Unlock', cmd: 'km.lock_my(0)' },
          ],
        },
        {
          id: 'mk_lock_btn', name: 'Lock Buttons',
          format: 'km.lock_ml(1)',
          note: 'Lock physical buttons so input is ignored. Supports lock_ml (left), lock_mr (right), lock_mm (middle), lock_ms1, lock_ms2.',
          actions: [
            { label: 'Lock left', cmd: 'km.lock_ml(1)' },
            { label: 'Unlock left', cmd: 'km.lock_ml(0)' },
            { label: 'Lock right', cmd: 'km.lock_mr(1)' },
            { label: 'Unlock right', cmd: 'km.lock_mr(0)' },
            { label: 'Lock middle', cmd: 'km.lock_mm(1)' },
            { label: 'Unlock middle', cmd: 'km.lock_mm(0)' },
            { label: 'Lock side1', cmd: 'km.lock_ms1(1)' },
            { label: 'Unlock side1', cmd: 'km.lock_ms1(0)' },
            { label: 'Lock side2', cmd: 'km.lock_ms2(1)' },
            { label: 'Unlock side2', cmd: 'km.lock_ms2(0)' },
          ],
        },
        {
          id: 'mk_catch', name: 'Catch Mode',
          format: 'km.catch_ml(1)',
          note: 'Button catch mode: 0=auto, 1=manual. Supports catch_ml/mm/mr/ms1/ms2.',
          actions: [
            { label: 'Left manual', cmd: 'km.catch_ml(1)' },
            { label: 'Left auto', cmd: 'km.catch_ml(0)' },
            { label: 'Middle manual', cmd: 'km.catch_mm(1)' },
            { label: 'Right manual', cmd: 'km.catch_mr(1)' },
            { label: 'Side1 manual', cmd: 'km.catch_ms1(1)' },
            { label: 'Side2 manual', cmd: 'km.catch_ms2(1)' },
          ],
        },
        {
          id: 'mk_invert', name: 'Invert Axis',
          format: 'km.invert_x(1)',
          note: 'Invert axis direction. Supports invert_x and invert_y.',
          actions: [
            { label: 'Invert X', cmd: 'km.invert_x(1)' },
            { label: 'Restore X', cmd: 'km.invert_x(0)' },
            { label: 'Invert Y', cmd: 'km.invert_y(1)' },
            { label: 'Restore Y', cmd: 'km.invert_y(0)' },
          ],
        },
        {
          id: 'mk_swap_xy', name: 'Swap XY Axes',
          format: 'km.swap_xy(1)',
          note: 'Swap X/Y axis mapping.',
          actions: [
            { label: 'Enable swap', cmd: 'km.swap_xy(1)' },
            { label: 'Disable swap', cmd: 'km.swap_xy(0)' },
          ],
        },
        {
          id: 'mk_scroll_side', name: 'Side Button Scroll',
          format: 'km.scroll_side(1)',
          note: 'When enabled, side2 triggers wheel events.',
          actions: [
            { label: 'Enable', cmd: 'km.scroll_side(1)' },
            { label: 'Disable', cmd: 'km.scroll_side(0)' },
          ],
        },
        {
          id: 'mk_scroll_lr', name: 'L+R Button Scroll',
          format: 'km.scroll_lr(1)',
          note: 'When enabled, pressing left+right/side simultaneously triggers wheel.',
          actions: [
            { label: 'Enable', cmd: 'km.scroll_lr(1)' },
            { label: 'Disable', cmd: 'km.scroll_lr(0)' },
          ],
        },
        {
          id: 'mk_buttons', name: 'Button Push Mode',
          format: 'km.buttons(1,10)',
          note: 'Enable button push mode. The device proactively reports physical button state changes. mode=0 off, period cycle 1..1000 ms.',
          params: [
            { name: 'mode', type: 'int', desc: '0=off, >0=on' },
            { name: 'period', type: 'int', desc: 'Push cycle (ms, 1..1000)' },
          ],
        },
        {
          id: 'mk_serial', name: 'Serial Number',
          format: 'km.serial("abc123")',
          note: 'Set/query/reset device serial (max 32 bytes). Quoted arg=set, no arg=query, arg=0=reset. Supports C-style escapes for newline, tab, backslash, quote, and hex byte (e.g. backslash-x followed by two hex digits).',
          actions: [
            { label: 'Query serial', cmd: 'km.serial()' },
            { label: 'Reset serial', cmd: 'km.serial(0)' },
          ],
        },
        {
          id: 'mk_echo', name: 'Echo Control',
          format: 'km.echo(1)',
          note: 'Toggle command echo. 1=on, 0=off. GET returns current state.',
          actions: [
            { label: 'On', cmd: 'km.echo(1)' },
            { label: 'Off', cmd: 'km.echo(0)' },
          ],
        },
        {
          id: 'mk_baud', name: 'Switch Baudrate',
          format: 'km.baud(3000000)',
          note: 'Dynamically switch serial baudrate (rate >= 115200, allowlist not enforced). No-arg GET returns current baud.',
          params: [
            { name: 'rate', type: 'int', desc: 'Target baudrate (>= 115200)' },
          ],
        },
        {
          id: 'mk_version', name: 'Get Version',
          format: 'km.version()',
          note: 'Query firmware version. Returns km.version(km.MAKCU).',
          params: [],
        },
        {
          id: 'mk_reboot', name: 'Reboot',
          format: 'km.reboot()',
          note: 'Soft reset the device.',
          params: [],
        },
      ],
    },
  ],
};
