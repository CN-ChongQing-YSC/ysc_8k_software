# YSC 8K 网页版（Web Serial）

零安装的浏览器版 YSC 驱动。通过 **Web Serial API** 直连设备的 CDC 串口，
与 Electron 版共享 `@ysc/core` 核心（协议/传输/命令层/store/UI/i18n）。

## 浏览器要求

仅 **Chromium 系**（Chrome / Edge 桌面端）支持 Web Serial，且须在**安全源**
（`https://` 或 `localhost`）打开。Firefox / Safari 不支持，会显示降级横幅。

## 运行

```bash
# 在仓库根目录（npm workspaces）
npm install                 # 首次：建立 @ysc/core 软链 + 安装 vue/pinia/vite
npm run dev:web             # = npm -w ysc-8k-web run dev → http://localhost:5174
```

用 Chrome 打开 `http://localhost:5174`：

1. 点「选择设备」→ 系统弹窗选 YSC 串口（VID `1A86` / PID `FE0C`）。
2. 选波特率 →「连接」。
3. 连接后自动读取固件版本；可开关实时监控、用键盘测试注入按键。

> Chrome 会按源记忆已授权端口，回访时可直接连接，无需再次弹窗。

## 功能范围（MVP）

✅ 设备授权/连接/断开 · 固件版本读取（cmd 132）· 实时监控（cmd 34）· 键盘注入（cmd 45/46）

后续阶段补齐：宏 / 抖动 / 鼠标曲线 / 手柄映射 / 固件 v1+v2 烧录（v2 走两次手势重授权）。

| 能力 | 网页版 | 说明 |
|---|---|---|
| 连接/版本/监控/宏/手柄/键盘/调试 | ✅ | 纯串口操作 |
| 固件 v1 烧录 | ✅（Phase 5） | 同口不重枚举 |
| 固件 v2 烧录 | ✅ 带重授权弹窗（Phase 6） | 设备重枚举需二次手势 |
| kmboxnet UDP 桥 | ❌ 仅桌面 | 浏览器不能监听 UDP |
| CH343 驱动检测/安装 | ❌ 仅桌面 | 需 SetupAPI + 提权 |

## 与 Electron 版的关系

两者都消费 `@ysc/core`：

- **网页版**：`WebSerialTransport`（`navigator.serial`）→ `YscDevice` → store → UI
- **Electron 版**：`DriverPipeTransport`（命名管道→`ysc_8k_driver.exe`）→ `YscDevice` → store → UI

`YscDevice` 与 store 传输无关，故功能面板两版共用（Phase 4 起）。
