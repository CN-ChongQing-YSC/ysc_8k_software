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
