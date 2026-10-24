#requires -Version 5.1
<#
.SYNOPSIS
    构建 YSC 8K Tauri 单 exe 发布包（NSIS），并归档为 YSC_8K_Setup_<ver>.exe。
#>
$ErrorActionPreference = 'Stop'

$app = Split-Path $PSScriptRoot -Parent
$pkg = Get-Content (Join-Path $app 'package.json') -Raw | ConvertFrom-Json
$ver = [string]$pkg.version
if (-not $ver) { throw '无法读取 ysc_8k_tauri/package.json 的 version' }

$running = Get-Process -Name 'ysc_8k_desktop' -ErrorAction SilentlyContinue
if ($running) {
    throw 'YSC 8K 驱动正在运行，请先关闭 ysc_8k_desktop.exe 再构建（否则 target/release 下的 exe 被占用）。'
}

Write-Host "==> YSC 8K Tauri build v$ver" -ForegroundColor Cyan
Push-Location $app
try {
    npm run build:app
    if ($LASTEXITCODE -ne 0) { throw "tauri build 失败，exit=$LASTEXITCODE" }

    $nsis = Get-ChildItem (Join-Path $app 'src-tauri\target\release\bundle\nsis') -Filter '*.exe' |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $nsis) { throw '未找到 NSIS 安装包' }

    $outDir = Join-Path $app 'release'
    New-Item -ItemType Directory -Force -Path $outDir | Out-Null
    $dst = Join-Path $outDir "YSC_8K_Setup_$ver.exe"
    Copy-Item -LiteralPath $nsis.FullName -Destination $dst -Force

    $hash = Get-FileHash -LiteralPath $dst -Algorithm SHA256
    $size = (Get-Item -LiteralPath $dst).Length
    Write-Host "OK: $dst" -ForegroundColor Green
    Write-Host ("    size={0:N0} bytes  sha256={1}" -f $size, $hash.Hash) -ForegroundColor Green
    Write-Host "    源 NSIS: $($nsis.FullName)"
} finally {
    Pop-Location
}
