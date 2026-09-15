# YSC 8K 上位机软件

本仓库包含 YSC 8K 设备的 Windows 上位机软件,由两个对等子项目组成。已从固件仓库 `ysc_v2_8k_passthrough` 解耦,独立维护、独立发版。

| 目录 | 说明 | 技术栈 |
|---|---|---|
| `ysc_8k_driver/` | **后端**:Win32 托盘程序,独占全部串口 I/O,通过命名管道对外提供服务 | C++ / Visual Studio (v142) / x64 |
| `ysc_8k_electron/` | **前端**:UI 与业务逻辑,经命名管道与后端通信 | Electron + Vue 3 + Vite + TypeScript |

## 目录相对位置契约(重要)

前端以**相对路径**引用后端编译产物,因此两个子目录**必须并列存放于同一父目录**(即本仓库根):

- 开发模式:`ysc_8k_electron` 启动时 spawn `../ysc_8k_driver/work/Release/ysc_8k_driver.exe`
- 打包时(`electron-builder.yml`):把 `../ysc_8k_driver/work/Release/ysc_8k_driver.exe` 收进安装包
- 命名管道:`\\.\pipe\ysc_8k_driver`(前后端硬编码一致)

> 移动任一子目录到别处前,必须同步修改 `ysc_8k_electron/electron-builder.yml` 与 `ysc_8k_electron/src/main/index.ts` 中的相对路径。

## 快速开始

### 后端(ysc_8k_driver)
用 Visual Studio(带 v142 工具集)打开 `ysc_8k_driver/ysc_8k_driver.sln`,构建 `Release | x64`,产物在 `ysc_8k_driver/work/Release/ysc_8k_driver.exe`。依赖系统库:`ws2_32.lib`、`setupapi.lib`。

### 前端(ysc_8k_electron)
```bash
cd ysc_8k_electron
npm install          # 首次;国内镜像已配在 .npmrc
npm run dev          # 开发
npm run dist         # 打包安装包到 release/
```
> 打包前必须先得到后端 `ysc_8k_driver.exe`,否则 electron-builder 的 extraResources 找不到文件。
> 一键打包 + 静默安装见 `ysc_8k_electron/build-and-install.bat`。

## 各子项目详情
- 后端架构、命令、串口/管道协议:见 `ysc_8k_driver/README.md`
- 前端模块、固件升级流程:见 `ysc_8k_electron/docs/`

## 发布流程(Release)

**安装包只通过 GitHub Releases 分发,不提交进 git 仓库。**

1. **同步版本号**(必须一致):`VERSION`、根 `package.json` 与各 workspace 的 `package.json`(一条命令即可:`npm version <ver> --workspaces --include-workspace-root --no-git-tag-version`)、以及 `ysc_8k_driver/main.cpp` 里 `get_version` 上报的驱动版本。
2. **`CHANGELOG.md` 落版**:把「未发布(Unreleased)」改成 `## v<ver> — <日期>`,并在顶部新开一个空的「未发布」段。
3. **构建**:先构建后端得到 `ysc_8k_driver/work/Release/ysc_8k_driver.exe`,再在 `ysc_8k_electron` 里 `npm run dist`,产物为 `release/YSC 8K 驱动 Setup <ver>.exe`。
4. **打 tag + 发 Release**:tag `v<ver>`;发布说明 = CHANGELOG 对应段落 + 「安装说明」;把安装包作为 **Release 附件**上传,附件名用 ASCII:`YSC_8K_Setup_<ver>.exe`。直链形如
   `https://github.com/CN-ChongQing-YSC/ysc_8k_software/releases/download/v<ver>/YSC_8K_Setup_<ver>.exe`
5. **验证**:打开 Release 页面确认资产与 sha256,再从直链完整下载一次、比对哈希。

> ⚠️ **不要把安装包提交进仓库**:单文件约 85MB,超过 GitHub 50MB 的建议上限;`ysc_8k_electron/release/` 已在 `.gitignore` 里,**不要用 `git add -f` 强推** —— 那会让每次 clone 都多拉一份,而且历史很难瘦身。
