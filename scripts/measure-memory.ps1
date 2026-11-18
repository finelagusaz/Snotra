<#
.SYNOPSIS
  Snotra のメモリ実測（#532 flip 基準 3）。PrivWS（物理私有 working set）を本命軸に
  snotra と設定サイドカーを合算する。

.DESCRIPTION
  採用ゲート（issue #532 comment 5011127487）と同じ手法を再現する:
  - 本命軸は Win32_PerfFormattedData_PerfProc_Process.WorkingSetPrivate（PrivWS）。
    WorkingSet64 は共有ページを合算で二重計上するため**出力しない**（採用ゲート当時に
    それを最も大きく歪めていたのは撤去済み WebView2 の共有ページだった）。
  - **合算するのは、ルートと、その直接の子の設定サイドカー snotra-settings だけである**
    （存命中のみ）。Snotra から起動したアプリも子孫に入るので、合算から除外し、件数と名前を
    別に出す（#1461）。選び方と理由の正本は `lib/SnotraMemoryScope.psm1`。
  - **プロセス件数を必ず出力する**。ルート 0 件（exit 1）と perf 0 件（exit 2）を件数で見せ、
    「測れなかった」と「0 だった」を区別するため——区別できない測定器は使えない。ppid の型の罠
    （#532 SU2: CIM の ParentProcessId は uint32 で返り、int の PID 集合との比較が一致しない）は
    実行時の出力では「子が居ない」と区別できないので、`lib/SnotraMemoryScope.Tests.ps1` が固定する。

.PARAMETER ProcessName
  ルートプロセス名（既定 snotra）。`Get-Process -Name` へ渡すのでワイルドカードも効くが、
  `snotra*` のように渡すとサイドカーもルートとして選ばれ、件数行の内訳が崩れる。
  完全な名前を渡すこと。変えるのはルートの名前だけで、サイドカー名は固定（`snotra-settings.exe`）。

.PARAMETER Label
  出力に付ける見出し（例 "egui-visible" / "egui-hidden"）。

.PARAMETER DelaySeconds
  サンプリング前の待機秒数。可視状態を測るときに使う（このスクリプトを起動した端末が
  フォーカスを奪うため）。**hide-on-blur を止める方が確実**なので、可視測定では
  config.toml の [general] auto_hide_on_focus_lost = false を推奨する。

.EXAMPLE
  npm run measure:memory
  pwsh -NoProfile -File scripts/measure-memory.ps1 -Label egui-hidden
#>
# 位置引数を拒む（#1296）。理由と検査は `lib/SnotraSmoke.Tests.ps1` の PositionalBinding の Describe
[CmdletBinding(PositionalBinding = $false)]
param(
    [string]$ProcessName = 'snotra',
    [string]$Label = '',
    [int]$DelaySeconds = 0
)

$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'lib/SnotraMemoryScope.psm1') -Force

if ($DelaySeconds -gt 0) {
    Write-Host "$DelaySeconds 秒後にサンプリングします..." -ForegroundColor Cyan
    Start-Sleep -Seconds $DelaySeconds
}

$roots = @(Get-Process -Name $ProcessName -ErrorAction SilentlyContinue |
    ForEach-Object { [uint32]$_.Id })
if ($roots.Count -eq 0) {
    Write-Host "プロセスが見つかりません: $ProcessName" -ForegroundColor Red
    exit 1
}

$scope = Select-SnotraMemoryScope -Processes @(Get-CimInstance Win32_Process) -RootIds $roots
$included = @{}
foreach ($id in $scope.Included) { $included[[uint32]$id] = $true }

$rows = @(Get-CimInstance Win32_PerfFormattedData_PerfProc_Process |
    Where-Object { $included.ContainsKey([uint32]$_.IDProcess) } |
    ForEach-Object {
        [pscustomobject]@{
            Name     = $_.Name
            ProcId   = [uint32]$_.IDProcess
            PrivWS   = [math]::Round($_.WorkingSetPrivate / 1MB, 1)
            PrivComm = [math]::Round($_.PrivateBytes / 1MB, 1)
        }
    })

$title = if ($Label) { "=== Snotra メモリ実測 [$Label] ===" } else { '=== Snotra メモリ実測 ===' }
Write-Host ''
Write-Host $title -ForegroundColor Cyan
Write-Host ("  ルート {0} 件 / 設定サイドカー {1} 件 / perf 取得 {2} 件" -f $roots.Count, @($scope.Sidecars).Count, $rows.Count)
# 除外の行は exit 2 の経路でも出す（件数は 0 件でも必ず出す）。
$excluded = @($scope.Excluded)
Write-Host ("  合算から除外した子孫 {0} 件（Snotra から起動したアプリ・サイドカーが開いたもの等）" -f $excluded.Count) -ForegroundColor Yellow
foreach ($e in $excluded) {
    Write-Host ("    {0} ({1})" -f $e.Name, $e.ProcessId) -ForegroundColor Yellow
}

if ($rows.Count -eq 0) {
    Write-Host '  perf カウンタが 0 件です。perf 未提供か、PID の取り違えの疑いがあり、' -ForegroundColor Red
    Write-Host '  「0 MiB だった」と読んではならない。ルートとサイドカーの件数と併せて手法を疑うこと。' -ForegroundColor Red
    exit 2
}

$rows | Sort-Object -Property PrivWS -Descending | Format-Table -AutoSize
$sumWs = [math]::Round(($rows | Measure-Object -Property PrivWS -Sum).Sum, 1)
$sumPc = [math]::Round(($rows | Measure-Object -Property PrivComm -Sum).Sum, 1)
Write-Host ("  合計 PrivWS  : {0,7} MiB  ← 本命軸（物理私有・flip 基準 3）" -f $sumWs) -ForegroundColor Green
Write-Host ("  合計 PrivComm: {0,7} MiB  （参考・commit）" -f $sumPc) -ForegroundColor DarkGray
Write-Host ''
