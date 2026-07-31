# 检查固件更新 (自动探测最新版本)

驱动 v1 面板（「固件更新」）和 v2 面板（「8K V2固件更新」）都内置了「检查更新」
按钮，连接到 YSC 固件管理后台（`ysc-order-2026.top/fwapi`），自动比对版本、下载
最新固件到本地缓存、填入升级流程，**不用再每次手动浏览选文件**。

## 工作原理

1. 点「检查更新」→ 驱动向后台 `GET /fwapi/api/firmware/{deviceType}/latest` 查询
2. 后台返回最新 active 固件（版本号、sha256、下载 URL）
3. 驱动对比「后台版本」与「本地记录的已装版本」（见下），判断是否更新
4. 若有新版：自动下载到 `%APPDATA%\YSC 8K 驱动\firmware-cache\`，sha256 校验
5. 下载完成后路径自动填入固件文件栏，「开始升级」/「升级左/右」按钮亮起
6. 升级成功后，驱动把「这次装到的版本」写入本地 `installed.json`，下次检查就据此判断

## 版本比对策略

后台固件用**语义版本号**（如 `1.0.0`、`1.0.1`），admin 上传时录入。

驱动**不依赖设备上报的版本串**做比对（8K V2是 `ysc-towmcu-L v1.0`、单芯片是编译
日期 `Jul 3 2026`——后者无法语义比较）。改为记录「上次成功装到的后台版本」到
`firmware-cache/installed.json`，对照后台最新版判断。这样三个产品线逻辑统一。

## 用法

### v1 面板（单芯片键鼠）
- deviceType = `ysc_v2_8k_mouse`
- 「检查更新」按钮在固件文件栏右侧
- 一次只查一个固件

### v2 面板（8K V2）
- deviceType = `ysc_towmcu_left` + `ysc_towmcu_right`
- 「检查更新」按钮在操作行（左/右并行查询）
- 哪侧有新版就自动下载哪侧，互不阻塞
- 两边都已是最新时提示「左右两侧均已是最新版本」

## 排错

| 现象 | 原因 / 处理 |
|---|---|
| 「无法连接固件服务器」 | 网络不通 / 域名解析失败 / 后台服务未启动。先 `curl http://ysc-order-2026.top/fwapi/api/firmware/health` 自测 |
| 「无可用固件」 | 后台没有该 deviceType 的 active 固件。联系管理员上传 |
| 始终提示「发现新版本」即使已装 | `installed.json` 损坏或丢失，或上次升级未成功。删 `%APPDATA%\YSC 8K 驱动\firmware-cache\installed.json` 重置 |
| 下载失败 / sha256 校验失败 | 网络中断或文件被篡改。重试；仍失败则联系管理员核对后台 sha256 |
| 想用手动文件不走自动 | 直接点「浏览」选本地 `.bin` 即可，自动路径会被覆盖，且不会写入 `installed.json` |

## 本地调试

开发时想让驱动指向本地后台（`./gradlew bootRun` 跑在 `localhost:8088`）：

```bash
# Windows PowerShell
$env:YSC_FW_BASE_URL="http://localhost:8088"; npm run dev

# Git Bash
YSC_FW_BASE_URL=http://localhost:8088 npm run dev
```

生产构建不设此环境变量，自动用 `http://ysc-order-2026.top/fwapi`。

## 缓存清理

下载的固件缓存在 `%APPDATA%\YSC 8K 驱动\firmware-cache\`：
- `<deviceType>_v<version>.bin` — 下载的固件（同版本 + sha256 匹配会复用，不重下）
- `installed.json` — 各 deviceType 已装版本记录

清理：直接删该目录即可，下次检查会重建。
