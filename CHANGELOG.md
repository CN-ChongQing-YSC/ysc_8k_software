# 更新说明 (CHANGELOG)

YSC 8K 上位机软件发布记录。旧栈版本号存在于 `VERSION`、各 `package.json` 与
`ysc_8k_electron/package.json`（冻结在 `1.17.0`）；Tauri 桌面端版本见
`ysc_8k_tauri/package.json` / `tauri.conf.json`（`2.1.0`）。
Rust 驱动在编译期读取 `VERSION` 自动上报；C++ 参照版仍需同步
`ysc_8k_driver/main.cpp` 的 `get_version`。

---

## 未发布（Unreleased）

## v2.1.0 — 2026-10-30

### 手柄测试器与手柄线路提示

- 新增手柄测试器：手柄映射页点击“监控手柄”，在**主窗口右侧弹出独立窗口**
  （`tester.html`，560 宽、与主窗口同高、置顶、不遮挡主界面），实时显示 PC
  手柄的按键 B0-B16、摇杆 AXIS 0-3、圆度轨迹、手柄状态图和震动测试。
  WebView2 不提供 Gamepad API，测试器改由 Rust XInput 60Hz 轮询
  （`gamepad_state` 事件 + `gamepad_vibrate` 命令）。独立窗口由
  `ysc_gamepad_tester_open/close`（async 命令）创建/关闭，关闭它只停 XInput
  推送、不退出主程序；切换主界面页面不受影响。网页版无 Tauri 窗口能力时
  回退为应用内右侧抽屉。
- 手柄映射页新增设备线路提示条（读 cmd 62 `devMode`）：设备处于鼠标线路时
  明确提示“手柄映射不会生效”并提供一键“切换到手柄线路”；处于手柄线路时
  显示绿色生效提示，避免映射配置后无反应却无从排查。

### NET API 活动预览

- 最近调用记录从 10 条扩到 32 条；驱动给服务启动时间 `t0`，每条记录带墙上
  时间偏移 `ms` 与同类命令间隔 `dt`，面板显示 `HH:MM:SS.mmm` / `+Δms`，
  悬停可看完整摘要，并附图例说明 `↓`/`↑`/`Δ` 含义。
- 键盘命令不再只显示 `ctrl/n` 计数，而是显示具体按键变化（如「↓A W ↑S」）：
  驱动把 kmboxnet 的 `(ctrl, keys)` 摊平成 HID 键码集合做状态差分，
  `send_keyboard_diff` 返回按下/松开/按住集合，按 HID 用法码映射为
  `A`/`F1`/`LCtrl` 等可读键名；记录 detail 缓冲区 40 → 128 字节并按 UTF-8
  边界截断，`↓`/`↑` 不会被切坏。

### V3

- YSC V3 USB CDC 波特率改为“逻辑值”：驱动识别 `YSCV3-CDC` 后允许任意正波特率，
  切换只更新显示值并让固件回 ACK，不再关端口/重开；串口面板与双设备面板均提示
  “波特率仅作显示，实际按 USB 全速通信”。普通串口仍保持白名单与重开回退。

### 构建

- `ysc_8k_tauri/tools/build.ps1` 新增 `-OutDir` 参数：可把归档好的安装包直接输出到
  任意目录（默认仍是 `ysc_8k_tauri/release`）。
- 发布：NSIS `YSC_8K_Setup_2.1.0.exe`（6,278,405 B，sha256
  `FA0C3A71D0DCFCC5C84FD24B076C807645D206EE19C73C98B1F624AF25746AAE`）。

## v2.0.0 — 2026-10-22

### 桌面端迁移：Electron + C++ → 纯 Rust + Tauri v2 单 exe

- 新增 `ysc_8k_tauri/`：Tauri v2 宿主 + WebView2，单进程/单 exe；
  不再启动 Electron、Node、`ysc_8k_driver.exe` 子进程，不再使用命名管道。
- 新增 `packages/ysc-ui/`：现有 YSC 桌面 UI 从 `ysc_8k_electron/src/renderer` 迁入；
  `window.driverApi` 兼容层把旧管道命令/事件映射到 Tauri IPC，面板无需重写。
- 新增 `ysc_8k_driver_rs/crates/ysc_host/`：`device` / `commands` / `iap` / `iap2` /
  `kmbox` / `guard` / `monitor` 与宿主解耦，`HostSink`/`HostNotifier` 由 Tauri 实现；
  旧 `ysc_driver` 管道二进制保留为回归薄壳。
- 无边框窗口、托盘、单实例、文件对话框、窗口控制、桌面拖拽已接入；
  Vite + Vue 3 + Pinia + `@ysc/core` store 正常驱动串口/监控面板。
- 新增 V3 固件页 `firmware-v3` / `V3FirmwarePanel`：只列 `YSCV3-CDC`，
  查询 `YSC-V3-IAP`/`YSC-V3-APP`，手动加密镜像升级，END 后等回 `YSC-V3-APP`；
  真机已验证 `YSC-V3-IAP` 直刷和 APP→IAP→APP 两条升级路径，
  最终回到 `YSC-V3-APP Oct 21 2026`。
- 新增 V3 服务器在线固件：
  - `firmware_server` 新增 `ysc_v3_8k` 设备类型，公开页 `/download/ysc_v3_8k`；
  - 上传 `1.2.1` / `ysc_v3_8k_v1.2.1.bin`（69,800 B，SHA-256
    `090DFA5A599B827720C00629245FF1C92C55CC9502C272F8D693B712E275A2F7`）；
  - Tauri 驱动新增 Rust HTTP 固件服务（查询/下载/缓存/SHA-256）。
- V1/V2 在线固件恢复：同一 `firmware_server` 上 `ysc_v2_8k_mouse`（v4.0）、
  `ysc_towmcu_left`（v2.3）、`ysc_towmcu_right`（v1.7）均可检测、下载；
  下载后经 `firmware-cache` 读取，走既有 `iap_start_mem` / `towmcu_start_mem` 烧录。
- 修复 V3「进入下载模式」等待过短：双片 3 阶段握手实测可达 10–20s，
  宿主扫描窗口从 10s 放宽到 30s；失败时 UI 不再误报“已自动选中新串口”。
- 新增 V3 启动自动烧录：exe 同目录 `v3_firmware.bin` 或
  `--auto-flash-v3=<path>` 启用；启动后自动切到 V3 页，等待端口、先 probe 版本再
  复用 `iap2::start_file` 完成 APP→IAP→烧录→回 APP；`--no-auto-flash` 可关闭。
- 修复无边框窗口白边：`tauri.conf.json` 关闭 undecorated shadow，窗口客户区与
  `WindowRect` 完全重合（四边 inset 均为 0），不再露出 8px 白色非客户区。
- 发布：`ysc_8k_tauri/tools/build.ps1` 产出 NSIS
  `YSC_8K_Setup_2.0.0.exe`（6,250,709 B，sha256
  `DF3C9877CCFC9A5BE9AC5785DD6C809E622BE8875CA6777E701DDBEA6C4DF832`）。
- 清理 V3 固件说明/驱动文案中的芯片型号字样；服务器 V3 releaseNotes 已更新，
  V3 导航描述改为「YSC V3 加密镜像升级」。
- 旧 Electron/C++ 栈冻结在 `1.17.0`，只读回归；新功能只进 Tauri 栈。
- 测试：`cargo test --workspace` 通过（ysc_core 17、ysc_host 10）；
  `cargo test`（Tauri firmware 3 项）通过；
  `npm --prefix ysc_8k_tauri/web run typecheck` 通过；V3 真机升级两次成功；
  服务器 V3 `latest`/下载/页面校验通过；V1/V2 latest 未变。
  发布版只运行 `ysc_8k_desktop.exe` 一个进程，无管道。

### 后端 Rust 移植

- 新增 `ysc_8k_driver_rs/`：把 C++ 驱动完整移植到 Rust（`windows` crate），
  保持命名管道 `\\.\pipe\ysc_8k_driver`、逐行 JSON 事件与全部命令完全不变，
  Electron / 网页版 / 示例无需改动。
  - 覆盖：重叠串口 I/O、波特率探测/切换、双设备槽位与角色识别、监控聚合、
    物理护栏、kmboxnet UDP 服务、v1/v2 固件升级、托盘/单实例/调试窗口/热插拔。
  - `ysc_sdk.dll` 同步移植（x64 + x86，36 个 `Ysc_*` 导出），行为与错误文案对齐。
  - 驱动版本改为编译期读取根 `VERSION`，不再需要手工改 `main.cpp`。
  - 差分验证：管道 24 条命令、槽位、串口错误路径、kmnet 16 种 UDP 命令、
    IAP 错误/擦除超时路径均与 C++ 参照驱动逐事件一致；Python SDK 测试 8 项全过；
    Electron 实机启动并由 Rust 驱动填充端口列表。
  - 构建：`ysc_8k_driver_rs/build.ps1`（投放到原 `work/Release` 与 `bin/...` 路径）。
  - 详见 `ysc_8k_driver_rs/README.md`（含有意差异与真机验证边界）。

### 构建修复

- 修复 `ysc_8k_electron/src/renderer/i18n/docs/en.js` 中 Physical Guard 说明里的
  未转义单引号（`software's`）导致 `vite build` / electron-builder 打包失败的问题。

---

## v1.17.0 — 2026-10-19

### 新增功能

- **物理护栏（Physical Guard）—— 防异常移动**
  - 新增「物理护栏」面板（输入分组）：引导用户用最快速度左右摆动、再上下摆动鼠标，
    驱动按 X/Y 两轴分别记录相邻方向反转之间的最小毫秒间隔，作为“人类最快反向窗口”。
  - 驱动新增 `HumanGuard` 测量模块与管道命令 `human_guard_begin/stop/status`；
    结果通过 `human_guard` 事件推送，包含 min / p5 / median / 样本数 / 时基 / 实时柱状数据。
  - 固件新增 `PhysicalGuard` 模块与 `cmd 68/69/70`：逐轴拦截与物理方向相反、且低于窗口的
    软件移动命令（cmd 30/31、Makcu move、kmboxnet、SDK）；同向、物理停手 >50ms、超过窗口
    一律放行；物理鼠标透传完全不改。
  - 被拦截轴上 cmd 30/31 两条移动流的未发完余量会一起取消，该轴 Reduce 复位为 1.0
    （物理优先，注入位移不再守恒），避免旧余量继续与物理方向对抗。
  - 配置持久化在固件数据 Flash 逻辑偏移 `0x7000`，重启后仍生效；默认关闭、12/12ms。
  - 监控帧 `{"b","x","y","wh"}` 增加固件毫秒时基 `t`，且 1ms 合并窗口内的 X/Y 改为累加，
    修复 8K 鼠标高速移动时监控样本被覆盖丢失的问题。
  - 老固件无 `t` 时驱动自动退回主机时基并标记；无 cmd 68/69/70 时面板显示“不支持”，
    校准数据仍可查看，但禁用“应用”。

### 修复

- **手柄映射：导出的配置重新导入后「摇杆比例」变回 1%**
  - 界面上的「N 像素 → X%」只折算成固件的 `move.scale`（像素/1%）下发，导出文件里也只有
    `scale`；重新导入或从设备读回时按 `scale` 反推，百分比输入被硬写成 1%，像素还会被
    `Math.round` 取整（10 像素→3% 会变成 3 像素→1%），比例被悄悄改掉。
  - 导出文件新增 `ui.move_px` / `ui.move_pct`（仅界面用，不发给设备），导入时原样还原两个
    输入框；没有这对数（固件回读、旧备份）时按 `scale` 反推最接近的整数像素/百分比组合，
    保持比值不变（10px→3%、13px→7% 都能还原），不再回落成 1%、也没有取整误差。
  - 导入时 `scale` 兼容字符串形式；像素/百分比输入允许小数，极小比例不再被 `min=1` 卡住。

---

## v1.16.0 — 2026-10-19

### 新增功能

- **HID 抓包「按键链路诊断」：把"物理按键不上报"一步定位到具体环节**
  - 固件抓包改为**双通道 + 事件**：`dir=0` 上游（盒子 USB 主机口收到的物理设备报告）、
    `dir=1` 下发（盒子实际发给主机的报告，在端点发送处记录）、`dir=2` 事件（丢弃/屏蔽/发送失败）。
    每条记录带毫秒时间戳、端点、接口、按键字节（按鼠标布局提取）与分类/原因码。
  - 新增抓包过滤（固件 `cmd 716`）：`全部` / `仅变化` / **`仅按键轨迹`**。旧版"全部模式"
    在 8K 鼠标上约 12ms 就把 100 条缓冲写满，按键帧根本不在里面；「仅按键轨迹」只在按键
    字节变化时记录，闲置换帧冲不掉，缓冲满时覆盖最旧的（保证"刚做的操作"一定在）。
  - 新增诊断快照（固件 `cmd 717`）：上游/下发的报告数、鼠标报告数、按键非零包数、按下/释放
    次数，以及 RID 不符、接口不符、布局缺失、按键被屏蔽、下发失败、键盘引擎吞掉、
    **合成缓冲溢出丢包**等计数；附鼠标布局快照（len/RID/接口/端点/按键字节偏移与位宽、X/Y/滚轮）。
  - 新增分页记录读取（固件 `cmd 718`，16 条/次，含原始字节）。
  - 调试面板新增「按键链路诊断」卡片：模式切换、**一键抓按键**（清空→仅按键轨迹→8 秒后自动
    读回并给出判定）、计数器、布局快照、轨迹表；并直接给出结论：
    上游就没有按键位（不是盒子的问题）/ 上游有而下发没有（丢在盒子内部，附原因计数）/
    已下发到主机（问题在 PC 侧）。导出 TXT 同步带上「Button Path Diagnosis」整段。

- **驱动 / SDK 集成「按键弹起」「强制松开」（固件 cmd 58 / cmd 67）**
  - 管道新增 `release_button` / `force_release` / `force_release_query` / `force_restore`，
    带 `b` 位图、`s`（弹起/恢复）、`t`（自动恢复毫秒）参数；`send_result` 只表示已写入串口，
    固件应答仍通过 `debug_response` 异步返回。
  - SDK 1.1.0 新增 `Ysc_ReleaseButton` / `Ysc_ForceRelease` / `Ysc_ForceReleaseQuery` /
    `Ysc_ForceRestore`，x64/x86 均已导出；`Ysc_ForceRelease` 同步返回
    `{"code":200,"message":"force_release|force_restore","data":"{\"b\":..,\"remain\":..}"}`。
  - 真机验证：`force_release` → `{b:1,remain:600}`；查询 → `remain:585`；
    `force_restore` → `{b:0,remain:0}`；SDK 调用返回一致。

### 修复

- **抖枪宏 / 鼠标曲线没有板载：断电或重启后回到默认**
  - 固件原来只在 RAM 保存抖枪宏（cmd 39/41），鼠标曲线则**完全没有实现**
    （cmd 42 被"一次性滚轮"占用、43/44 无处理）。
  - 现固件新增两份带 magic/version/checksum 的 Flash 记录：
    `JITTER_OFFSET=0x5000`（抖枪）、`CURVE_OFFSET=0x6000`（曲线），上电自动恢复；
    cmd 39/41/42/44 只置延迟落盘标志，主循环擦写，ISR 不做 Flash 操作。
  - 鼠标曲线按 towmcu 同款算法实现：定点 Q=32768，profile 0=线性 / 1=smoothstep /
    2=min-jerk / 3=random，N 段（2..32）× H 报告分段发送，全整数（ISR 不碰 FPU）。
    cmd 42 带 `w` 仍是 kmboxnet 一次性滚轮，带 `e/p/n/d/j` 则是曲线设置；
    cmd 43 回读、cmd 44 恢复默认。
  - 固件版本 `Oct 18 2026`，已用 WCH-Link 烧录并校验（IAP + APP 均验过）。

- **手柄映射「读取配置」偶发超时/设备读取失败**
  - 面板原来用 `driverApi.isConnected()`（只代表和驱动进程的管道连着）判断“已连接”，
    串口没连时也会自动读配置，必然等满超时。改为监听驱动的
    `state` / `serial_connected` / `serial_disconnected`，串口未连时按钮禁用、不自动读；
    串口连上（含断开重连）后自动读一次。
  - 读取/保存/重置增加超时自动重试；重置后由 400ms 改为 1s 再读，避开固件延迟落盘
    （擦 4KB 扇区会阻塞主循环，期间返回的应答可能被遥测挤掉）。
  - 规则上限由 32 改为固件实际支持的 **24**（`GMAP_MAX_RULES`）。超过 24 条时固件会截断，
    而 2048B 的落盘缓冲也会装不下；导入超过 24 条的配置直接拒绝并提示。
  - 驱动 `SendJsonCommand` 的栈缓冲由 2048 增至 4096，并在拷贝前检查长度。此前 25 条以上
    规则的命令（约 2KB+）会栈溢出，驱动直接崩溃（WER `0xc0000409`），之后所有读写都失败。

- **中英混排：切换英文后仍有整块中文**
  - 补全此前写死在组件里的中文文案：`App.vue` KmNet 自检全流程提示、手柄映射面板（导出/导入、
    移动换算、死区、调试日志、校验/读写/重置/导入导出日志与错误）、HID 抓包面板（操作日志、
    导出的 TXT 报告表头与诊断段）、文档面板（导出 PDF 的生成中/成功/失败/取消提示）、
    鼠标曲线可视化（预览、重播、进度/段数/位移占比/报告数）。
  - 命令测试（Command Tester）在切换语言后按 id 重新绑定协议与命令对象，详情不再停留在旧语言。
  - 侧栏导航补上 `km-focus`（键鼠集火）的英文词条；分组悬停标题的分隔符改为按语言取用。
  - 主进程对话框（选择加密固件、左右固件、导出 PDF、默认文件名、文件大小单位、下载校验错误）
    随界面语言切换：渲染进程通过新增的 `app:setLang` IPC 同步语言。
  - 导出 PDF 的静态章节（盒子使用、运行模式、版本字符串、V1/V2 一致性核对等）改为中英双语，
    PDF 内容与界面语言一致，默认文件名也随语言变化。
  - 驱动（C++）回传的中文日志/错误（升级进度、串口错误、未连接设备等）在主进程转发前按语言
    翻译，英文界面下不再出现中文日志。
  - 标题栏与侧栏在英文长文案下允许换行、按钮不收缩，修复按钮文字被截断/标题重叠的问题；
    宏与鼠标曲线的字段改为竖向排布，避免英文提示被固定宽度挤成逐字竖排。

- **抓包记录会被闲置帧冲掉**：旧版 HID 抓包固定存"最先 100 帧"且不覆盖，8K 鼠标下 100 帧
  约 12ms 即写满，之后再操作什么都看不到（这正是"导出的抓包全是移动帧、看不到按键"的原因）。
  改为环形缓冲 + 可过滤模式，最近的操作始终可见。

## v1.15.0 — 2026-09-14

### 新增功能

- **设备序列号自定义（透传鼠标 USB iSerialNumber）**
  - 固件新命令 `cmd 54/55/56/57`：回读（`custom`/`physical`/`active`）、写入（1-30 位可打印 ASCII，持久化到设备数据 flash）、清除恢复透传原序列号、软重启（重新枚举使新序列号生效）。
  - 写入后固件在每个枚举周期把 `USB_DESC_ID_SERIAL` 重定向到自定义字符串；源设备没有序列号时自动把设备描述符 `iSerialNumber` 指到字符串 3，保证 PC 能读到。
  - 首页下方新增"设备序列号"栏：显示当前/物理序列号，输入框 + 「写入并重启」/「恢复默认」，重启约 3 秒后自动回读。走既有 `send_ysc` 通用通道，驱动 C++ 无需改动。

- **Net API 活动预览（键鼠集火板块）**
  - 「键鼠集火」页新增 KmNet 服务面板：启停、端口、通讯地址（本机可用 `127.0.0.1:<端口>`），以及 **API 活动**实时预览 —— 客户端连接状态（IP:端口 / 连接次数 / 最近一次调用距今）、分类调用计数（鼠标 / 键盘 / 控制 / 未实现）、最近 10 条调用记录（命令名、明细、是否真的转发到对应盒子）。首屏/重连另用 `get_kmnet_stat` 拉取兜底。

- **对齐官方 kmboxnet API 的鼠标命令**
  - `mouse_left/right/middle` 改为"整份按键位图"语义（官方 `mouse_side1/side2` 借 `cmd_mouse_right` 下发 bit3/bit4），固件新增 `cmd 61` 一次覆盖按键位图；`cmd_mouse_wheel` 同时处理 button+x+y+wheel（官方 `mouse_all` 与 `mouse_wheel` 共用这条命令，此前 `mouse_all` 整条被丢弃）。
  - 滚轮改用固件新命令 `cmd 42`（一次性滚轮，区别于 `cmd 35` 的 hold 语义）；新增 `cmd_mask_mouse`/`cmd_unmask_all`（固件 `cmd 60`）与 `cmd_reboot`（固件 `cmd 57`）。未实现的命令会记录到活动预览里，不再静默 ACK 了事。

### 修复

- **刷完固件后"已连接却完全没反应"**：固件重新刷写/复位后 USART1 回到默认 115200，而驱动按上次选择的 4000000 连接 —— 句柄能打开、界面显示已连接，但一条回包都收不到（监控、按键、Net 全部无响应且看不出原因）。新增 `SerialPort::PingOnce()`：显式波特率下先握手一次，失败则自动扫描真实速率并重连。
- **驱动管道可能被永久卡死**：客户端异常退出（或被强制结束）时，断开路径上的 `FlushFileBuffers` 会在缓冲仍有数据时永久阻塞，之后任何客户端都连不上（必须重启驱动）；另外 `DebugLogger` 把每行日志都推管道，而前端没有消费者，推送还发生在串口读线程上（客户端一慢就阻塞串口）。两处均已移除。
- **Net API 活动面板永远空白**：`kmnet_stat` 载荷多了一个 `}`，整条事件不是合法 JSON，前端解析失败后静默丢弃；主进程同时改为**默认转发所有驱动事件**（原先只转发白名单，驱动新增事件忘了同步白名单就会静默丢失）。
- **双设备角色未指定时会发错盒子**：`SlotByRole`/`CommandBridge::Resolve` 原先在没有匹配角色时退回"第一个已连接设备"，两台都连着时键盘命令会静默发到鼠标盒（固件回 404，调用方却看到"成功"）。现在两台都连着且角色未指定时直接拒发；单设备与 `DEV_ROLE_NONE` 行为不变。
- 下拉选择器在自身选项列表里滚动时弹层会当场消失（window 捕获阶段的 scroll 监听把弹层自己的滚动也算进去了）。
- 串口日志加目标端口前缀 `TX[COMxx]`，便于一眼确认命令分流到哪台盒子。

## v1.14.0 — 2026-08-15

### 新增功能

- **鼠标插值（Mouse Interpolation，重点）**
  - 固件新命令 `cmd 51/52/53`（设置/读取/重置）：把软件低频移动指令（如 200Hz 的 cmd30/31）沿时间轴展开为设备实际回报率（高速 8 报文/ms，最高 8kHz）的平滑微增量，总位移严格守恒，轨迹接近原生高回报率人手移动。
  - **自适应窗口（默认）**：固件用 EMA 跟踪移动命令到达节奏，自动推导展开窗口；也可用固定窗口（`window_ms>0`）。cmd 52 回读带实时 `ema` 字段，UI 直接显示当前检测到的调用间隔。
  - **向后兼容**：显式指定步数 `c>1` 的调用（kmbox AUTOMOVE、MAKCU km.move segments）完全不受影响，无双重展开；与鼠标曲线叠加时曲线提供形状、插值提供时长。
  - **默认开启**，可在新"鼠标插值"面板关闭；配置持久化到设备 Flash（与曲线同扇区，曲线存储区由 2048 缩至 256 字节腾出空间）。
  - 驱动 pipe 新命令：`set_interp_config` / `get_interp_config` / `reset_interp_config`；SDK 新导出 `Ysc_InterpSet/Get/Reset`。
  - Electron 新增"鼠标插值"面板（输入分组），支持开关、形状（线性/缓入缓出/最小急动度）、固定窗口、自适应上限、EMA 实时显示。

## v1.13.0 — 2026-08-04

自上个发布版本 v1.11.0 以来的全部更新（7 个提交、45 个文件）。

### 新增功能

- **键盘端点打字控制**（重点）
  - `Ysc_KeyboardTypeString`：向设备注入一整段**混合大小写 ASCII 文本**（上限 128 字节），固件根据当前 CapsLock 状态自动决定每个字符的大小写并自动按下/释放 Shift，调用方无需关心大小写。
  - `Ysc_KeyboardKey`：单键**按下 / 抬起**注入（HID 键码 `0x04`–`0xE7`，修饰键 `0xE0`–`0xE7`）。
  - `Ysc_KeyboardReleaseAll`：一键释放所有按键。
- **鼠标移动接口封装完善**
  - `Ysc_MouseMove`（绝对移动）/ `Ysc_MouseMoveTow`（相对移动），支持步进插值。
- **CH343 / WCH 驱动一键安装**
  - 检测到 VID `1A86` 设备驱动异常（设备管理器黄色感叹号）时，界面顶部弹出告警条，支持一键以管理员权限拉起随包 `CH343SER.EXE` 安装器引导用户安装。

### 易语言（E）接入完善（面向开发者）

- 新增**完整的易语言调用 `ysc_sdk.dll` 示例**（`Example/E/`）：串口连接、命令调用、键盘字符串按下、释放全部按键。
- 配套提供 **32 位 / 64 位** `ysc_sdk.dll`（32 位为主流易语言，64 位供易语言 6.x / Python / C# 等使用），均静态 CRT 编译（`/MT`），目标机无需安装 VC 运行库。

### 其它

- 驱动自报版本、安装包版本同步至 `1.13.0`。
- 串口连接、命令调用在易语言侧的接入链路打通（连接 → 探测波特率 → 发送命令 → 解析返回）。

### 升级说明

- 终端用户：直接安装新版 `YSC 8K 驱动-Setup-1.13.0.exe` 覆盖旧版即可，配置与缓存保留在 `%APPDATA%\YSC 8K 驱动\`。
- 开发者：如需使用新的键盘打字接口，请替换为本次随附的 `ysc_sdk.dll`（旧版 DLL 不含 `Ysc_KeyboardTypeString` 等导出）。

---

## v1.11.0

上个正式发布版本（仓库 tag `v1.11.0`）。
