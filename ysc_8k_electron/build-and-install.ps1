# YSC 8K 驱动 - 一键构建并静默安装
# 用法:
#   powershell -ExecutionPolicy Bypass -File .\build-and-install.ps1              # 全流程(安装步骤会触发 UAC 提权)
#   powershell -ExecutionPolicy Bypass -File .\build-and-install.ps1 -DryRun      # 干跑, 不执行副作用
#   powershell -ExecutionPolicy Bypass -File .\build-and-install.ps1 -SkipInstall # 只打包不安装
#   powershell -ExecutionPolicy Bypass -File .\build-and-install.ps1 -SkipBuild   # 只安装不打包
# 或双击 build-and-install.bat
#
# 设计: npm run dist 在当前(用户级)PowerShell 中执行, 确保 npm/node 环境变量可用.
#       仅在最后调用 NSIS 安装包时通过 -Verb RunAs 触发 UAC 提权(perMachine 安装所需).

[CmdletBinding()]
param(
    [switch]$DryRun,
    [switch]$SkipBuild,
    [switch]$SkipInstall
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

# 强制 UTF-8 输出, 避免中文在非 Windows Terminal 中乱码
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding           = [System.Text.Encoding]::UTF8

$root = Split-Path -Parent $MyInvocation.MyCommand.Definition
Set-Location $root

function Write-Step($i, $total, $msg) {
    Write-Host "`n[$i/$total] $msg" -ForegroundColor Yellow
}
function Write-Ok($msg) {
    Write-Host "  [OK] $msg" -ForegroundColor Green
}
function Write-Info($msg) {
    Write-Host "  $msg" -ForegroundColor Gray
}
function Write-Warn($msg) {
    Write-Host "  [!] $msg" -ForegroundColor Yellow
}
function Write-Err($msg) {
    Write-Host "  [X] $msg" -ForegroundColor Red
}

Write-Host '========================================' -ForegroundColor Cyan
Write-Host '   YSC 8K 驱动 - 构建 & 安装'           -ForegroundColor Cyan
Write-Host '========================================' -ForegroundColor Cyan

if ($DryRun)        { Write-Warn 'DryRun 模式: 不执行实际副作用(打包/安装/杀进程)' }
if ($SkipBuild)     { Write-Info '已启用 -SkipBuild: 跳过打包步骤' }
if ($SkipInstall)   { Write-Info '已启用 -SkipInstall: 跳过安装步骤' }

$totalSteps = 5

# --- 1. 关闭正在运行的实例 ---
Write-Step 1 $totalSteps '关闭正在运行的实例...'
$names = @('YSC 8K 驱动', 'ysc_8k_driver')
$killed = $false
foreach ($n in $names) {
    $procs = Get-Process -Name $n -ErrorAction SilentlyContinue
    if ($procs) {
        foreach ($p in $procs) {
            if ($DryRun) {
                Write-Info "[DryRun] 将终止 $($p.Name) (PID $($p.Id))"
                $killed = $true
                continue
            }
            try {
                $p | Stop-Process -Force -ErrorAction Stop
                Write-Info "已终止 $($p.Name) (PID $($p.Id))"
                $killed = $true
            } catch {
                Write-Warn "无法终止 $($p.Name): $($_.Exception.Message)"
            }
        }
    }
}
if (-not $killed) { Write-Info '无运行实例' }
if (-not $DryRun) { Start-Sleep -Milliseconds 800 }

# --- 2. 检查 npm/node 可用性 ---
Write-Step 2 $totalSteps '检查 Node 工具链...'
$npmCmd = Get-Command npm -ErrorAction SilentlyContinue
if (-not $npmCmd) {
    Write-Err '当前 PowerShell 找不到 npm. 请确认 Node.js 已安装, 且 PATH 中包含 npm.cmd'
    Write-Err "PATH 中查找位置: $env:PATH"
    exit 1
}
Write-Ok "npm: $($npmCmd.Source)"

if (-not (Test-Path (Join-Path $root 'node_modules'))) {
    if ($DryRun) {
        Write-Info '[DryRun] node_modules 缺失, 将执行 npm install'
    } else {
        Write-Info 'node_modules 缺失, 执行 npm install...'
        npm install
        if ($LASTEXITCODE -ne 0) { throw "npm install 失败 (exit $LASTEXITCODE)" }
        Write-Ok '依赖安装完成'
    }
} else {
    Write-Ok 'node_modules 已存在, 跳过'
}

# --- 3. 打包(在当前 PowerShell 中执行, 确保 npm 环境变量可用) ---
if ($SkipBuild) {
    Write-Step 3 $totalSteps '跳过打包 (-SkipBuild)'
} else {
    Write-Step 3 $totalSteps '执行 npm run dist...'
    if ($DryRun) {
        Write-Info '[DryRun] 将执行 npm run dist'
    } else {
        $buildStart = Get-Date
        npm run dist
        if ($LASTEXITCODE -ne 0) { throw "npm run dist 失败 (exit $LASTEXITCODE)" }
        $buildSecs = [int]((Get-Date) - $buildStart).TotalSeconds
        Write-Ok "打包完成 (用时 ${buildSecs}s)"
    }
}

# --- 4. 查找最新安装包 ---
Write-Step 4 $totalSteps '查找安装包...'
$releaseDir = Join-Path $root 'release'
$setup = Get-ChildItem -Path $releaseDir -Filter '*Setup*.exe' -ErrorAction SilentlyContinue |
         Where-Object { $_.Name -notlike '*.blockmap' } |
         Sort-Object LastWriteTime -Descending |
         Select-Object -First 1
if (-not $setup) { throw "release 目录下未找到 *Setup*.exe" }
$sizeMB = [math]::Round($setup.Length / 1MB, 1)
Write-Ok "找到: $($setup.Name)  (${sizeMB} MB)"
Write-Info "生成时间: $($setup.LastWriteTime.ToString('yyyy-MM-dd HH:mm:ss'))"

# --- 5. 静默安装(通过 -Verb RunAs 触发 UAC, 仅此一步提权) ---
if ($SkipInstall) {
    Write-Step 5 $totalSteps '跳过安装 (-SkipInstall)'
} else {
    Write-Step 5 $totalSteps '静默安装到电脑...'
    if ($DryRun) {
        Write-Info "[DryRun] 将执行(提权): $($setup.FullName) /S /allusers"
    } else {
        Write-Info '将弹出 UAC 对话框请求管理员权限(perMachine 安装)...'
        Write-Info '使用 NSIS /S 静默参数 + /allusers, 无需手动点击下一步'
        $installStart = Get-Date
        try {
            # -Verb RunAs 触发 UAC 提权; -Wait 等待安装完成; -PassThru 拿到进程对象
            $proc = Start-Process -FilePath $setup.FullName -ArgumentList '/S', '/allusers' -Verb RunAs -Wait -PassThru
        } catch {
            Write-Err "UAC 提权失败或用户拒绝: $($_.Exception.Message)"
            exit 1
        }
        $installSecs = [int]((Get-Date) - $installStart).TotalSeconds
        if ($proc.ExitCode -ne 0) {
            Write-Err "静默安装返回非零退出码: $($proc.ExitCode)"
            Write-Warn "请尝试手动运行: $($setup.FullName)"
            exit $proc.ExitCode
        }
        Write-Ok "安装完成 (用时 ${installSecs}s)"
    }
}

if ($DryRun) {
    Write-Host ''
    Write-Host '========================================' -ForegroundColor Cyan
    Write-Host '  DryRun 完成, 未执行任何实际操作'        -ForegroundColor Cyan
    Write-Host '========================================' -ForegroundColor Cyan
    exit 0
}

# --- 从注册表读取真实安装路径 ---
function Get-InstallDir {
    $regPaths = @(
        'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*'
    )
    foreach ($p in $regPaths) {
        $items = Get-ItemProperty $p -ErrorAction SilentlyContinue |
                 Where-Object { $_.DisplayName -like '*YSC*8K*' }
        foreach ($item in $items) {
            if ($item.InstallLocation -and (Test-Path $item.InstallLocation)) {
                return $item.InstallLocation
            }
            if ($item.UninstallString -match '"([^"]+)"') {
                $uninst = $matches[1]
                $dir = Split-Path $uninst -Parent
                if (Test-Path $dir) { return $dir }
            }
        }
    }
    $fallback = Join-Path $env:ProgramFiles 'ysc-8k-driver'
    if (Test-Path $fallback) { return $fallback }
    return $null
}

$installDir = Get-InstallDir
$exe = $null
if ($installDir) {
    $exe = Get-ChildItem -Path $installDir -Filter '*.exe' -ErrorAction SilentlyContinue |
           Where-Object { $_.Name -notmatch 'Uninstall' -and $_.Name -notmatch 'elevate' } |
           Select-Object -First 1
}

Write-Host ''
Write-Host '========================================' -ForegroundColor Green
Write-Host '  全部完成!'                              -ForegroundColor Green
Write-Host '========================================' -ForegroundColor Green
Write-Host ''
if ($installDir) {
    Write-Host "  安装位置: $installDir" -ForegroundColor Cyan
    if ($exe) { Write-Host "  主程序:   $($exe.FullName)" -ForegroundColor Cyan }
} else {
    Write-Warn '未推断出安装目录, 可能 NSIS 安装到了非默认路径'
}

if (-not $SkipInstall) {
    Write-Host ''
    $launch = Read-Host '是否立即启动? [Y/n]'
    if ($launch -eq '' -or $launch -match '^[Yy]') {
        if ($exe) {
            Start-Process -FilePath $exe.FullName
            Write-Ok '已启动'
        } else {
            Write-Warn '未找到主程序, 请手动启动'
        }
    }
}
