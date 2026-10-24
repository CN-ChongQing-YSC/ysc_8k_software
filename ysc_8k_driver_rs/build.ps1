<#
.SYNOPSIS
  构建 YSC 8K 驱动的 Rust 版本，并投放 Electron / SDK 期望的位置。

.DESCRIPTION
  默认行为（与仓库既有打包契约一致）：
    target\release\ysc_8k_driver.exe
      → ..\ysc_8k_driver\work\Release\ysc_8k_driver.exe   （electron-builder extraResources 的源）
    target\release\ysc_sdk.dll
      → ..\ysc_8k_driver\bin\x64\Release\ysc_sdk.dll      （Python/C# x64 示例自动定位）
    target\i686-pc-windows-msvc\release\ysc_sdk.dll
      → ..\ysc_8k_driver\bin\Win32\Release\ysc_sdk.dll    （易语言/32 位示例）
      → ..\Example\E\ysc_sdk.dll

.PARAMETER NoCopy
  只构建，不覆盖 ..\ysc_8k_driver 下的产物。

.PARAMETER DebugBuild
  构建 dev profile（带控制台，便于看 panic/诊断），仍按同样路径投放。
#>
param(
    [switch]$NoCopy,
    [switch]$DebugBuild
)

$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $here

$profile = if ($DebugBuild) { "debug" } else { "release" }
Write-Host "[ysc] cargo build ($profile) ..." -ForegroundColor Cyan
if ($DebugBuild) {
    cargo build -p ysc_driver -p ysc_sdk
} else {
    cargo build --release -p ysc_driver -p ysc_sdk
}
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

Write-Host "[ysc] cargo build (x86 SDK, $profile) ..." -ForegroundColor Cyan
if ($DebugBuild) {
    cargo build -p ysc_sdk --target i686-pc-windows-msvc
} else {
    cargo build --release -p ysc_sdk --target i686-pc-windows-msvc
}
if ($LASTEXITCODE -ne 0) { throw "cargo build (x86) failed" }

$targetRoot = Join-Path $here "target"
$destDriver = Join-Path $here "..\ysc_8k_driver\work\Release\ysc_8k_driver.exe"
$destSdkX64 = Join-Path $here "..\ysc_8k_driver\bin\x64\Release\ysc_sdk.dll"
$destSdkX86 = Join-Path $here "..\ysc_8k_driver\bin\Win32\Release\ysc_sdk.dll"
$destExampleE = Join-Path $here "..\Example\E\ysc_sdk.dll"

if (-not $NoCopy) {
    New-Item -ItemType Directory -Force -Path (Split-Path $destDriver) | Out-Null
    New-Item -ItemType Directory -Force -Path (Split-Path $destSdkX64) | Out-Null
    New-Item -ItemType Directory -Force -Path (Split-Path $destSdkX86) | Out-Null

    Copy-Item (Join-Path $targetRoot "$profile\ysc_8k_driver.exe") $destDriver -Force
    Copy-Item (Join-Path $targetRoot "$profile\ysc_sdk.dll") $destSdkX64 -Force
    Copy-Item (Join-Path $targetRoot "i686-pc-windows-msvc\$profile\ysc_sdk.dll") $destSdkX86 -Force
    if (Test-Path (Split-Path $destExampleE)) {
        Copy-Item $destSdkX86 $destExampleE -Force
    }

    Write-Host "[ysc] artifacts:" -ForegroundColor Green
    Get-Item $destDriver, $destSdkX64, $destSdkX86 |
        Select-Object FullName, Length, LastWriteTime | Format-Table -AutoSize
}

Write-Host "[ysc] done." -ForegroundColor Green
