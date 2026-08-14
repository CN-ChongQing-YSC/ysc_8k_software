// Chinese documentation content for DocsPanel & CommandTester.
// Synchronized with firmware command handlers in
//   hard/.../USART1Dma/USART1Dma.c  (YSC protocol)
//   hard/.../MakcuCommand/MakcuCommand.c  (MAKCU protocol)

export const docs = {
  serial: {
    title: '串口通信协议',
    basicTitle: '基本参数',
    basicRows: [
      ['波特率', '固件白名单: 115200/230400/460800/921600/1M/1.5M/2M/3M;4M 为实验性扩展,硬件不一定稳定支持。可通过 cmd:133 或 km.baud 动态切换'],
      ['数据位', '8'],
      ['停止位', '1'],
      ['校验位', '无'],
      ['流程控制', '无硬件流控'],
      ['DMA', 'USART1 TX/RX 各一条 DMA 通道,IDLE 中断触发收帧'],
    ],
    frameTitle: '帧格式',
    frameFormat: '<START>[2字节大端长度][JSON负载]<END>',
    frameNote: 'YSC 协议帧:<START>(7B) + 2 字节大端总长 + JSON 负载 + <END>(5B)。MAKCU 协议:纯文本 km.command(args)\\r\\n,响应以 \\r\\n>>> 结尾。还有二进制 baud 帧:0xDE 0xAD 0x05 0x00 0xA5 + 4 字节小端 baud(无任何文本标记)。',
    sections: [
      {
        title: 'YSC 命令列表',
        cards: [
          {
            title: 'CMD_MOUSE_MOVE (30)  鼠标平滑移动',
            format: '{"cmd":30,"x":100,"y":50,"c":1,"rx":1.0,"ry":1.0,"rc":0}',
            note: '平滑移动。x/y 为目标总位移,c 为分步次数(每步一份)。可选 rx/ry 为物理鼠标 X/Y 轴减速系数(float,默认 1.0=不减速),rc 为减速持续<strong>毫秒数</strong>(int16,默认 0=不减速)。固件按当前鼠标回报率自适应换算(1K=1ms/包,8K=1ms/8包),所以 rc=50 在两种回报率下都持续 50ms。减速作用于<strong>物理鼠标输入</strong>而非本命令的位移,常用于压枪:开火时缩小下拉输入以抵消后坐力。固件走轻量 JSON 扫描器,不走 cJSON 的 malloc/free,适合高频调用。',
          },
          {
            title: 'CMD_MOUSE_MOVE_TOW (31)  方向性叠加移动',
            format: '{"cmd":31,"x":100,"y":50,"c":1,"rx":1.0,"ry":1.0,"rc":0}',
            note: '方向性移动,可与 cmd:30 叠加(位移在 HID 报告中相加)。字段与 cmd:30 完全一致。当 cmd:30 和 cmd:31 同时设置减速(rc&gt;0)时,<strong>cmd:31 优先</strong>应用。',
          },
          {
            title: 'CMD_MOUSE_BUTTON (32)  按键自动释放',
            format: '{"cmd":32,"b":1,"c":10}',
            note: '模拟按键按下,持续上报 c 次后自动释放。b 为按键位掩码:1=左键, 2=右键, 4=中键。',
          },
          {
            title: 'CMD_MOUSE_BUTTON_STATE (33)  按键状态直设',
            format: '{"cmd":33,"b":1,"s":1}',
            note: '直接设置按键状态(立即生效,不上报次数)。s=1 按下,s=0 释放。b 同 cmd:32 的位掩码。',
          },
          {
            title: 'CMD_ChangeUploadStatus (34)  上传开关',
            format: '{"cmd":34,"status":1}',
            note: '开启/关闭 HID 数据上传。status&gt;0 开启,status=0 关闭。',
          },
          {
            title: 'CMD_MOUSE_WHEEL_HOLD (35)  滚轮按住',
            format: '{"cmd":35,"w":-1,"c":100}',
            note: '滚轮按住命令。w 为滚轮值(int8,-1=向下,1=向上,其他值会被裁剪到 ±1),c 为持续时长(毫秒 uint32)。设备在指定时长内持续发送滚轮值。',
          },
          {
            title: 'CMD_MACRO_SET (36)  配置按键宏',
            format: '{"cmd":36,"s":0,"e":1,"t":3,"p":1,"w":-1,"i":30,"d":10,"ij":5,"dj":2}',
            note: '配置按键宏槽位。s=槽位(0-7),e=启用(0/1),t=触发掩码,p=屏蔽掩码,w=滚轮值(int8),i=滚轮触发间隔周期ms(可选,0=持续无间隔),d=每周期内滚轮持续时长ms(可选,≤i,0=满周期),ij=间隔随机浮动幅度ms(可选,每周期实际间隔在 i±ij 内随机),dj=持续时长随机浮动幅度ms(可选,每周期实际时长在 d±dj 内随机)。按键位:1=左,2=右,4=中,8=侧后,16=侧前。i>0时滚轮按周期节拍脉冲输出(例如 i=30,d=10,ij=5,dj=2 表示约每30ms±5触发一次、每次持续约10ms±2)。响应 {code:200,message:"macro_set",data:"<slot>"}。',
          },
          {
            title: 'CMD_MACRO_GET (37)  查询按键宏',
            format: '{"cmd":37,"s":-1}',
            note: '查询宏配置。s=-1 返回全部(数组);s=0..7 返回单槽位。响应字段 s/e/t/p/w/i/d/ij/dj。',
          },
          {
            title: 'CMD_MACRO_RESET (38)  重置宏',
            format: '{"cmd":38}',
            note: '重置所有宏配置为固件默认值(全部禁用)。响应 {code:200,message:"macro_reset"}。',
          },
          {
            title: 'CMD_JITTER_SET (39)  抖动(压枪轨迹)配置',
            format: '{"cmd":39,"e":1,"t":1,"ax":10,"fx":0,"py":5,"fy":0}',
            note: '配置鼠标抖动。e=启用(0/1),t=触发(t=1 表示开火/按键时触发),ax=X 轴幅度,fx=X 轴频率,py=Y 轴幅度,fy=Y 轴频率。保存/查询结果经 debug_response(message:"jitter") 异步回传。',
          },
          {
            title: 'CMD_JITTER_GET (40)  查询抖动',
            format: '{"cmd":40}',
            note: '查询当前抖动配置。响应经 debug_response(message:"jitter") 回传 {e,t,ax,fx,py,fy}。',
          },
          {
            title: 'CMD_JITTER_RESET (41)  重置抖动',
            format: '{"cmd":41}',
            note: '重置抖动配置为默认(全部归零并禁用)。',
          },
          {
            title: 'CMD_MOUSE_CURVE_SET (42)  鼠标曲线配置',
            format: '{"cmd":42,"e":1,"p":2,"n":4,"d":0,"j":15}',
            note: '配置鼠标移动曲线。e=启用(0/1),p=曲线档位(profile),n=分段数,d=持续时间(ms,0=按档位默认),j=抖动幅度。保存/查询结果经 debug_response(message:"mouse_curve") 异步回传。',
          },
          {
            title: 'CMD_MOUSE_CURVE_GET (43)  查询鼠标曲线',
            format: '{"cmd":43}',
            note: '查询当前鼠标曲线配置。响应经 debug_response(message:"mouse_curve") 回传 {enabled,profile,segments,duration,jitter}。',
          },
          {
            title: 'CMD_MOUSE_CURVE_RESET (44)  重置鼠标曲线',
            format: '{"cmd":44}',
            note: '重置鼠标曲线为默认(profile=2, segments=4, duration=0, jitter=15)。',
          },
          {
            title: 'CMD_JUMP_TO_IAP (50)  进入 Bootloader',
            format: '{"cmd":50}',
            note: '擦除校准字 0x5AA55AA5 写入 0x0808FFFC 并触发系统复位。设备会先回复 {code:200,message:"entering_iap"} 然后重启进入 IAP Bootloader 模式等待固件升级。',
          },
          {
            title: 'CMD_GET_VERSION (132 / 0x84)  查询编译日期',
            format: '{"cmd":132}',
            note: '查询固件编译日期(__DATE__ 宏)。响应 {code:200,message:"version",data:"<compile date>"}。',
          },
          {
            title: 'CMD_SET_BAUDRATE (133 / 0x85)  动态切换波特率',
            format: '{"cmd":133,"baud":3000000}',
            note: '动态切换 USART1 波特率。固件不校验白名单,直接配置硬件 USART——所以建议只发送固件白名单内的 8 档(115200~3000000)。响应 {code:200,message:"switching"},之后设备立即重配 USART,上位机需同步换波特率。',
          },
        ],
      },
      {
        title: 'MAKCU 命令列表',
        cards: [
          {
            title: '鼠标控制',
            isTable: true,
            headers: ['命令', '格式', '说明'],
            rows: [
              ['移动', 'km.move(x,y[,seg])', '鼠标移动,seg 分步数 1..512(超出会被裁剪)'],
              ['左键', 'km.left(1/0)', '1=按下, 0=释放, 2=静默释放(不触发按键推送队列)'],
              ['右键', 'km.right(1/0)', '同上'],
              ['中键', 'km.middle(1/0)', '同上'],
              ['侧键1', 'km.ms1(1/0)', '同上'],
              ['侧键2', 'km.ms2(1/0)', '同上'],
              ['滚轮', 'km.wheel(-1/1)', '滚动一格,只接受 ±1(其他值会被裁剪)'],
            ],
          },
          {
            title: '轴/按键锁定',
            isTable: true,
            headers: ['命令', '说明'],
            rows: [
              ['km.lock_mx(0/1) / km.lock_mx+ / km.lock_mx-', '锁定 X 轴(正/负方向可分别锁)'],
              ['km.lock_my(0/1) / km.lock_my+ / km.lock_my-', '锁定 Y 轴,同上'],
              ['km.lock_ml/mr/mm/ms1/ms2(0/1)', '锁定物理按键使输入不生效'],
              ['km.invert_x/y(0/1)', '反转 X 或 Y 轴方向'],
              ['km.swap_xy(0/1)', '交换 X/Y 轴映射'],
            ],
          },
          {
            title: '捕获模式',
            isTable: true,
            headers: ['命令', '说明'],
            rows: [
              ['km.catch_ml(0/1)', '左键捕获模式:0=自动,1=手动'],
              ['km.catch_mm(0/1)', '中键捕获模式'],
              ['km.catch_mr(0/1)', '右键捕获模式'],
              ['km.catch_ms1(0/1)', '侧键 1 捕获模式'],
              ['km.catch_ms2(0/1)', '侧键 2 捕获模式'],
            ],
          },
          {
            title: '系统命令',
            isTable: true,
            headers: ['命令', '说明'],
            rows: [
              ['km.version()', '返回 km.version(km.MAKCU)'],
              ['km.echo(0/1)', '命令回显开关(默认开启)'],
              ['km.baud(rate)', '切换波特率(rate &gt;= 115200,不校验白名单);无参数时返回当前波特率'],
              ['km.buttons(mode,period)', '按键推送模式:mode=0 关闭,mode&gt;0 开启;period 周期 1..1000 ms'],
              ['km.serial("xxx") / km.serial(0)', '设置/重置设备序列号(最长 32 字节,支持转义);无参数时返回当前序列号'],
              ['km.reboot()', '软复位设备,响应 km.reboot() 后立即重启'],
              ['km.scroll_side(0/1)', '启用后,侧键 2 触发滚轮事件'],
              ['km.scroll_lr(0/1)', '启用后,左键+右键/侧键同时按下时触发滚轮'],
            ],
          },
          {
            title: '二进制波特率切换(特殊)',
            isTable: false,
            note: '除文本命令外,固件支持一个 9 字节的二进制 baud 切换帧:0xDE 0xAD 0x05 0x00 0xA5 + 4 字节小端 baud。该帧不经 MAKCU 文本解析,直接重配 USART。常用于不知道当前波特率时的恢复。',
          },
        ],
      },
    ],
  },

  protocols: [
    {
      id: 'ysc',
      name: 'YSC 协议',
      commands: [
        {
          id: 'ysc_move', name: '鼠标移动 (cmd:30)',
          format: '{"cmd":30,"x":100,"y":50,"c":1,"rx":1.0,"ry":1.0,"rc":0}',
          note: '平滑鼠标移动。x/y 为目标总位移,c 为分步次数。可选 rx/ry/rc 启用物理鼠标减速(压枪):rc 是减速持续毫秒数,固件按当前鼠标回报率自动换算(1K=1包/ms,8K=8包/ms),两种鼠标下 rc=N 都持续 N ms。',
          params: [
            { name: 'x', type: 'int16', desc: 'X 轴目标位移(负值向左)' },
            { name: 'y', type: 'int16', desc: 'Y 轴目标位移(负值向上)' },
            { name: 'c', type: 'int16', desc: '分步次数(1=立即,>1 平滑分摊)' },
            { name: 'rx', type: 'float', desc: '可选。物理鼠标 X 轴减速系数,默认 1.0' },
            { name: 'ry', type: 'float', desc: '可选。物理鼠标 Y 轴减速系数,默认 1.0' },
            { name: 'rc', type: 'int16', desc: '可选。减速持续毫秒数(自适应回报率),默认 0=不减速' },
          ],
        },
        {
          id: 'ysc_move_tow', name: '方向移动 (cmd:31)',
          format: '{"cmd":31,"x":100,"y":50,"c":1,"rx":1.0,"ry":1.0,"rc":0}',
          note: '方向性移动,与 cmd:30 叠加(位移在 HID 报告里相加)。字段与 cmd:30 完全一致。当 cmd:30 和 cmd:31 同时减速(rc>0)时,cmd:31 优先应用。',
          params: [
            { name: 'x', type: 'int16', desc: 'X 轴目标位移' },
            { name: 'y', type: 'int16', desc: 'Y 轴目标位移' },
            { name: 'c', type: 'int16', desc: '分步次数' },
            { name: 'rx', type: 'float', desc: '可选。物理鼠标 X 轴减速系数' },
            { name: 'ry', type: 'float', desc: '可选。物理鼠标 Y 轴减速系数' },
            { name: 'rc', type: 'int16', desc: '可选。减速持续毫秒数(自适应回报率)' },
          ],
        },
        {
          id: 'ysc_button', name: '鼠标按键 (cmd:32)',
          format: '{"cmd":32,"b":1,"c":10}',
          note: '模拟按键按下,持续 c 次后自动释放。b 为按键位掩码(1=左键,2=右键,4=中键)。',
          params: [
            { name: 'b', type: 'uint8', desc: '按键位掩码(1=左, 2=右, 4=中)' },
            { name: 'c', type: 'uint32', desc: '持续上报次数' },
          ],
        },
        {
          id: 'ysc_button_state', name: '按键状态 (cmd:33)',
          format: '{"cmd":33,"b":1,"s":1}',
          note: '直接设置按键状态(立即生效,无上报次数)。s=1 按下,s=0 释放。',
          params: [
            { name: 'b', type: 'uint8', desc: '按键位掩码' },
            { name: 's', type: 'uint8', desc: '0=释放, 1=按下' },
          ],
        },
        {
          id: 'ysc_upload', name: '切换上传状态 (cmd:34)',
          format: '{"cmd":34,"status":1}',
          note: '开启/关闭 HID 数据上传。开启后设备向串口回报鼠标数据(按键+移动)。',
          actions: [
            { label: '开启上传', cmd: '{"cmd":34,"status":1}' },
            { label: '关闭上传', cmd: '{"cmd":34,"status":0}' },
          ],
        },
        {
          id: 'ysc_wheel_hold', name: '滚轮按住 (cmd:35)',
          format: '{"cmd":35,"w":-1,"c":100}',
          note: '滚轮按住命令。w 为滚轮值(只接受 ±1),c 为持续时长(毫秒)。',
          params: [
            { name: 'w', type: 'int8', desc: '滚轮值(-1=向下, 1=向上,其他被裁剪)' },
            { name: 'c', type: 'uint32', desc: '持续时长(毫秒)' },
          ],
          actions: [
            { label: '向下100ms', cmd: '{"cmd":35,"w":-1,"c":100}' },
            { label: '向下200ms', cmd: '{"cmd":35,"w":-1,"c":200}' },
            { label: '向上100ms', cmd: '{"cmd":35,"w":1,"c":100}' },
          ],
        },
        {
          id: 'ysc_macro_set', name: '设置宏 (cmd:36)',
          format: '{"cmd":36,"s":0,"e":1,"t":3,"p":1,"w":-1,"i":30,"d":10}',
          note: '配置按键宏。s=槽位(0-7),e=启用,t=触发掩码,p=屏蔽掩码,w=滚轮值,i=间隔周期ms(0=持续),d=每周期持续ms。按键位:1=左,2=右,4=中,8=侧后,16=侧前。',
          params: [
            { name: 's', type: 'uint8', desc: '槽位 (0-7)' },
            { name: 'e', type: 'uint8', desc: '启用 (0/1)' },
            { name: 't', type: 'uint8', desc: '触发按键位掩码' },
            { name: 'p', type: 'uint8', desc: '屏蔽按键位掩码' },
            { name: 'w', type: 'int8', desc: '滚轮值' },
            { name: 'i', type: 'uint16', desc: '滚轮触发间隔周期(ms)，0=持续无间隔' },
            { name: 'd', type: 'uint16', desc: '每周期内滚轮持续时长(ms)，≤i，0=满周期' },
          ],
          actions: [
            { label: '左+右→屏蔽左+滚轮', cmd: '{"cmd":36,"s":0,"e":1,"t":3,"p":1,"w":-1}' },
            { label: '侧前→滚轮', cmd: '{"cmd":36,"s":2,"e":1,"t":16,"p":0,"w":-1}' },
            { label: '侧前→滚轮(每30ms/持续10ms)', cmd: '{"cmd":36,"s":2,"e":1,"t":16,"p":0,"w":-1,"i":30,"d":10}' },
            { label: '禁用槽位0', cmd: '{"cmd":36,"s":0,"e":0,"t":3,"p":1,"w":-1}' },
          ],
        },
        {
          id: 'ysc_macro_get', name: '查询宏 (cmd:37)',
          format: '{"cmd":37,"s":-1}',
          note: '查询宏配置。s=-1 查询全部,s=0-7 查询单个槽位。',
          actions: [
            { label: '查询全部', cmd: '{"cmd":37,"s":-1}' },
            { label: '查询槽位0', cmd: '{"cmd":37,"s":0}' },
          ],
        },
        {
          id: 'ysc_macro_reset', name: '重置宏 (cmd:38)',
          format: '{"cmd":38}',
          note: '重置所有宏配置为固件默认值。',
          actions: [
            { label: '重置默认', cmd: '{"cmd":38}' },
          ],
        },
        {
          id: 'ysc_kbd_key', name: '键盘按键 (cmd:45)',
          format: '{"cmd":45,"kc":4,"down":1}',
          note: '注入一次键盘按下/松开。kc=HID键码(0x04-0xE7；修饰键0xE0-0xE7=左右Ctrl/Shift/Alt/GUI)；down=1按下,0松开。仅当透传设备具有键盘接口时生效,否则设备回 404 no_keyboard。多键可多次调用;松开用 down:0 或 cmd:46。真实按键仍正常透传(与注入合并)。',
          params: [
            { name: 'kc',   type: 'uint8', desc: 'HID 键码 (0x04-0xE7；修饰键 0xE0-0xE7)' },
            { name: 'down', type: 'uint8', desc: '1=按下, 0=松开' },
          ],
          actions: [
            { label: '按下 a',      cmd: '{"cmd":45,"kc":4,"down":1}' },
            { label: '松开 a',      cmd: '{"cmd":45,"kc":4,"down":0}' },
            { label: '按下左Shift', cmd: '{"cmd":45,"kc":225,"down":1}' },
            { label: '松开左Shift', cmd: '{"cmd":45,"kc":225,"down":0}' },
          ],
        },
        {
          id: 'ysc_kbd_release_all', name: '键盘全释放 (cmd:46)',
          format: '{"cmd":46}',
          note: '释放所有注入按下的键。无键盘接口时忽略。',
          actions: [
            { label: '释放全部', cmd: '{"cmd":46}' },
          ],
        },
        {
          id: 'ysc_kbd_type_string', name: '键盘打字 (cmd:47)',
          format: '{"cmd":47,"s":"Wasd123A123vciseC"}',
          note: '逐字打出一段混合大小写 ASCII 字符串。固件读取 PC 通过 HID LED Output 报告下发的 CapsLock 状态(bit1),对每个字符计算:是否按 Shift = (字符需要 Shift) XOR (CapsLock 开启 且 为字母);需要时自动按下左 Shift(0xE1),绝不切换 CapsLock 键本身。符号/数字/空格不受 CapsLock 影响。无键盘接口回 404 no_keyboard;超 128 字节回 400 too_long;空串回 400 empty_string;正在打字时再发回 409 busy。接受时回 200 typing_started,完成异步回 200 type_done(data 含 typed/skipped/total)。不可映射字符被跳过并计入 skipped。真实按键仍正常透传(与注入合并)。',
          params: [
            { name: 's', type: 'string', desc: '要打字的 ASCII 字符串,1..128 字节,JSON 转义' },
          ],
          actions: [
            { label: 'Wasd123A123vciseC',     cmd: '{"cmd":47,"s":"Wasd123A123vciseC"}' },
            { label: 'Hello World!',          cmd: '{"cmd":47,"s":"Hello World!"}' },
            { label: 'test@123',              cmd: '{"cmd":47,"s":"test@123"}' },
            { label: 'Shift符号 !@#$%^&*()',  cmd: '{"cmd":47,"s":"!@#$%^&*()"}' },
            { label: '空串(应 400)',          cmd: '{"cmd":47,"s":""}' },
          ],
        },
        {
          id: 'ysc_kbd_type_status', name: '键盘打字状态 (cmd:48)',
          format: '{"cmd":48}',
          note: '查询打字引擎当前进度。响应 data: busy(0/1) / remaining(未打完字数) / total / typed(已打完) / skipped(不可映射已跳过)。busy=0 表示空闲或已打完。用于把超过单次 128 字节上限的长文本切片循环打印：打一片 cmd:47 → 轮询 cmd:48 直到 busy=0 → 再打下一片。同步查询：发送即等返回（宿主端每次发送前会清空接收缓冲，异步的 type_done/鼠标上报帧不会错位）。',
          params: [],
          actions: [
            { label: '查询状态', cmd: '{"cmd":48}' },
          ],
        },
        {
          id: 'ysc_jump_iap', name: '进入 Bootloader (cmd:50)',
          format: '{"cmd":50}',
          note: '擦除校准字并复位进入 IAP Bootloader 模式。设备先回复 entering_iap,然后断开 USB 等待固件升级。8K V2 路径:设备清 anti-brick flag、写 BKP_DR1=0xA5A5、复位后以序列号 TOWMCUIAP 重新枚举,COM 号可能改变,需重扫端口。',
          params: [],
        },
        {
          id: 'ysc_get_version', name: '获取版本 (cmd:0x84)',
          format: '{"cmd":132}',
          note: '查询固件编译日期。返回 {code:200,message:"version",data:"编译日期"}。8K V2 路径:APP 返回 message="ysc-towmcu-L|R v1.0";IAP 返回 data="YSC-IAP"。靠子串 "-IAP" 区分模式。',
          params: [],
        },
        {
          id: 'ysc_set_baud', name: '切换波特率 (cmd:0x85)',
          format: '{"cmd":133,"baud":3000000}',
          note: '动态切换串口波特率。固件不校验白名单直接配置,建议发送白名单内 8 档(115200~3000000)。',
          params: [
            { name: 'baud', type: 'uint32', desc: '目标波特率' },
          ],
        },
      ],
    },
    {
      id: 'makcu',
      name: 'MAKCU 协议',
      commands: [
        {
          id: 'mk_move', name: '鼠标移动',
          format: 'km.move(100,50)',
          note: '移动鼠标。支持可选的 segments 参数实现分步移动(范围 1..512)。',
          params: [
            { name: 'x', type: 'int', desc: 'X 轴偏移' },
            { name: 'y', type: 'int', desc: 'Y 轴偏移' },
            { name: 'segments', type: 'int', desc: '可选,分步数(1~512)' },
          ],
        },
        {
          id: 'mk_left', name: '左键',
          format: 'km.left(1)',
          note: '左键操作。1=按下,0=释放,2=静默释放(不入按键推送队列)。',
          actions: [
            { label: '按下', cmd: 'km.left(1)' },
            { label: '释放', cmd: 'km.left(0)' },
            { label: '静默释放', cmd: 'km.left(2)' },
          ],
        },
        {
          id: 'mk_right', name: '右键',
          format: 'km.right(1)',
          note: '右键操作。同左键参数。',
          actions: [
            { label: '按下', cmd: 'km.right(1)' },
            { label: '释放', cmd: 'km.right(0)' },
            { label: '静默释放', cmd: 'km.right(2)' },
          ],
        },
        {
          id: 'mk_middle', name: '中键',
          format: 'km.middle(1)',
          note: '中键操作。同左键参数。',
          actions: [
            { label: '按下', cmd: 'km.middle(1)' },
            { label: '释放', cmd: 'km.middle(0)' },
            { label: '静默释放', cmd: 'km.middle(2)' },
          ],
        },
        {
          id: 'mk_ms1', name: '侧键1 (ms1)',
          format: 'km.ms1(1)',
          note: '侧键 1 操作。',
          actions: [
            { label: '按下', cmd: 'km.ms1(1)' },
            { label: '释放', cmd: 'km.ms1(0)' },
          ],
        },
        {
          id: 'mk_ms2', name: '侧键2 (ms2)',
          format: 'km.ms2(1)',
          note: '侧键 2 操作。',
          actions: [
            { label: '按下', cmd: 'km.ms2(1)' },
            { label: '释放', cmd: 'km.ms2(0)' },
          ],
        },
        {
          id: 'mk_wheel', name: '滚轮',
          format: 'km.wheel(1)',
          note: '滚轮滚动一格。只接受 -1(向下)或 1(向上),其他值会被裁剪。',
          actions: [
            { label: '向上滚动', cmd: 'km.wheel(1)' },
            { label: '向下滚动', cmd: 'km.wheel(-1)' },
          ],
        },
        {
          id: 'mk_lock_mx', name: '锁定 X 轴',
          format: 'km.lock_mx(1)',
          note: '锁定 X 轴移动。1=锁定正向和负向,0=解锁。也可分别用 lock_mx+ / lock_mx- 锁定单向。',
          actions: [
            { label: '锁定', cmd: 'km.lock_mx(1)' },
            { label: '解锁', cmd: 'km.lock_mx(0)' },
          ],
        },
        {
          id: 'mk_lock_my', name: '锁定 Y 轴',
          format: 'km.lock_my(1)',
          note: '锁定 Y 轴移动。同 X 轴,支持 lock_my+ / lock_my-。',
          actions: [
            { label: '锁定', cmd: 'km.lock_my(1)' },
            { label: '解锁', cmd: 'km.lock_my(0)' },
          ],
        },
        {
          id: 'mk_lock_btn', name: '锁定按键',
          format: 'km.lock_ml(1)',
          note: '锁定按键使物理按键不生效。支持 lock_ml(左), lock_mr(右), lock_mm(中), lock_ms1, lock_ms2。',
          actions: [
            { label: '锁定左键', cmd: 'km.lock_ml(1)' },
            { label: '解锁左键', cmd: 'km.lock_ml(0)' },
            { label: '锁定右键', cmd: 'km.lock_mr(1)' },
            { label: '解锁右键', cmd: 'km.lock_mr(0)' },
            { label: '锁定中键', cmd: 'km.lock_mm(1)' },
            { label: '解锁中键', cmd: 'km.lock_mm(0)' },
            { label: '锁定侧键1', cmd: 'km.lock_ms1(1)' },
            { label: '解锁侧键1', cmd: 'km.lock_ms1(0)' },
            { label: '锁定侧键2', cmd: 'km.lock_ms2(1)' },
            { label: '解锁侧键2', cmd: 'km.lock_ms2(0)' },
          ],
        },
        {
          id: 'mk_catch', name: '捕获模式',
          format: 'km.catch_ml(1)',
          note: '按键捕获模式:0=自动,1=手动。支持 catch_ml/mm/mr/ms1/ms2。',
          actions: [
            { label: '左键手动', cmd: 'km.catch_ml(1)' },
            { label: '左键自动', cmd: 'km.catch_ml(0)' },
            { label: '中键手动', cmd: 'km.catch_mm(1)' },
            { label: '右键手动', cmd: 'km.catch_mr(1)' },
            { label: '侧键1手动', cmd: 'km.catch_ms1(1)' },
            { label: '侧键2手动', cmd: 'km.catch_ms2(1)' },
          ],
        },
        {
          id: 'mk_invert', name: '反转轴',
          format: 'km.invert_x(1)',
          note: '反转指定轴方向。支持 invert_x 和 invert_y。',
          actions: [
            { label: '反转 X', cmd: 'km.invert_x(1)' },
            { label: '恢复 X', cmd: 'km.invert_x(0)' },
            { label: '反转 Y', cmd: 'km.invert_y(1)' },
            { label: '恢复 Y', cmd: 'km.invert_y(0)' },
          ],
        },
        {
          id: 'mk_swap_xy', name: '交换 XY 轴',
          format: 'km.swap_xy(1)',
          note: '交换 X/Y 轴映射。',
          actions: [
            { label: '开启交换', cmd: 'km.swap_xy(1)' },
            { label: '关闭交换', cmd: 'km.swap_xy(0)' },
          ],
        },
        {
          id: 'mk_scroll_side', name: '侧键滚轮',
          format: 'km.scroll_side(1)',
          note: '启用后,侧键 2 将触发滚轮事件。',
          actions: [
            { label: '启用', cmd: 'km.scroll_side(1)' },
            { label: '禁用', cmd: 'km.scroll_side(0)' },
          ],
        },
        {
          id: 'mk_scroll_lr', name: '左右键滚轮',
          format: 'km.scroll_lr(1)',
          note: '启用后,左键+右键/侧键同时按下时触发滚轮。',
          actions: [
            { label: '启用', cmd: 'km.scroll_lr(1)' },
            { label: '禁用', cmd: 'km.scroll_lr(0)' },
          ],
        },
        {
          id: 'mk_buttons', name: '按键推送模式',
          format: 'km.buttons(1,10)',
          note: '开启按键推送模式。物理按键状态变化时主动上报。mode=0 关闭,period 周期 1..1000 ms。',
          params: [
            { name: 'mode', type: 'int', desc: '0=关闭, >0=开启' },
            { name: 'period', type: 'int', desc: '推送周期(ms,1..1000)' },
          ],
        },
        {
          id: 'mk_serial', name: '序列号',
          format: 'km.serial("abc123")',
          note: '设置/查询/重置设备序列号(最长 32 字节)。带引号参数=设置,无参数=查询,参数为 0=重置。支持 C 风格转义:换行/制表/反斜杠/引号/十六进制字节(反斜杠-x 后接两位 hex)。',
          actions: [
            { label: '查询序列号', cmd: 'km.serial()' },
            { label: '重置序列号', cmd: 'km.serial(0)' },
          ],
        },
        {
          id: 'mk_echo', name: '回显控制',
          format: 'km.echo(1)',
          note: '控制命令回显。1=开启,0=关闭。GET 时返回当前状态。',
          actions: [
            { label: '开启', cmd: 'km.echo(1)' },
            { label: '关闭', cmd: 'km.echo(0)' },
          ],
        },
        {
          id: 'mk_baud', name: '切换波特率',
          format: 'km.baud(3000000)',
          note: '动态切换串口波特率(rate >= 115200,不校验白名单)。无参数时返回当前波特率。',
          params: [
            { name: 'rate', type: 'int', desc: '目标波特率(>= 115200)' },
          ],
        },
        {
          id: 'mk_version', name: '获取版本',
          format: 'km.version()',
          note: '查询固件版本信息。返回 km.version(km.MAKCU)。',
          params: [],
        },
        {
          id: 'mk_reboot', name: '重启设备',
          format: 'km.reboot()',
          note: '软复位设备。',
          params: [],
        },
      ],
    },
  ],
};
