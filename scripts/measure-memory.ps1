<#
.SYNOPSIS
  Snotra のメモリ実測（#532 flip 基準 3）。PrivWS（物理私有 working set）を本命軸に
  プロセスツリー全体を合算する。

.DESCRIPTION
  採用ゲート（issue #532 comment 5011127487）と同じ手法を再現する:
  - 本命軸は Win32_PerfFormattedData_PerfProc_Process.WorkingSetPrivate（PrivWS）。
    WorkingSet64 は共有 Edge ページをツリー合算で二重計上するため**出力しない**。
  - 子孫プロセス（msedgewebview2 等）を BFS で辿って合算する。
  - **プロセス件数を必ず出力する**。BFS が沈黙して 0 件を返す罠（#532 SU2 実測: CIM の
    ParentProcessId は uint32 で返り、int の PID 集合との比較が一致しない）を出力から
    検知できるようにするため——「測れなかった」と「0 だった」を区別できない測定器は使えない。

.PARAMETER ProcessName
  ルートプロセス名（既定 snotra）。

.PARAMETER Label
  出力に付ける見出し（例 "egui-visible" / "webview2-hidden"）。

.PARAMETER DelaySeconds
  サンプリング前の待機秒数。可視状態を測るときに使う（このスクリプトを起動した端末が
  フォーカスを奪うため）。**hide-on-blur を止める方が確実**なので、可視測定では
  config.toml の [general] auto_hide_on_focus_lost = false を推奨する。

.EXAMPLE
  npm run measure:memory
  pwsh -NoProfile -File scripts/measure-memory.ps1 -Label egui-hidden
#>
param(
    [string]$ProcessName = 'snotra',
    [string]$Label = '',
    [int]$DelaySeconds = 0
)

$ErrorActionPreference = 'Stop'

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

# (ppid -> 子 pid) の索引を一度だけ作る。**型は uint32 に揃える**（上記 SU2 の罠）。
$byPpid = @{}
foreach ($p in Get-CimInstance Win32_Process) {
    $parent = [uint32]$p.ParentProcessId
    if (-not $byPpid.ContainsKey($parent)) {
        $byPpid[$parent] = New-Object System.Collections.ArrayList
    }
    [void]$byPpid[$parent].Add([uint32]$p.ProcessId)
}

# ルートから子孫を BFS（$pid は PowerShell の自動変数なので別名を使う）。
$tree = @{}
$queue = New-Object System.Collections.Queue
foreach ($r in $roots) { [void]$queue.Enqueue($r) }
while ($queue.Count -gt 0) {
    $cur = [uint32]$queue.Dequeue()
    if ($tree.ContainsKey($cur)) { continue }
    $tree[$cur] = $true
    if ($byPpid.ContainsKey($cur)) {
        foreach ($c in $byPpid[$cur]) { [void]$queue.Enqueue($c) }
    }
}

$rows = @(Get-CimInstance Win32_PerfFormattedData_PerfProc_Process |
    Where-Object { $tree.ContainsKey([uint32]$_.IDProcess) } |
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
Write-Host ("  ルート {0} 件 / ツリー {1} 件 / perf 取得 {2} 件" -f $roots.Count, $tree.Count, $rows.Count)

if ($rows.Count -eq 0) {
    Write-Host '  perf カウンタが 0 件です。BFS 不成立か perf 未提供の疑いがあり、' -ForegroundColor Red
    Write-Host '  「0 MiB だった」と読んではならない。ツリー件数と併せて手法を疑うこと。' -ForegroundColor Red
    exit 2
}

$rows | Sort-Object -Property PrivWS -Descending | Format-Table -AutoSize
$sumWs = [math]::Round(($rows | Measure-Object -Property PrivWS -Sum).Sum, 1)
$sumPc = [math]::Round(($rows | Measure-Object -Property PrivComm -Sum).Sum, 1)
Write-Host ("  合計 PrivWS  : {0,7} MiB  ← 本命軸（物理私有・flip 基準 3）" -f $sumWs) -ForegroundColor Green
Write-Host ("  合計 PrivComm: {0,7} MiB  （参考・commit）" -f $sumPc) -ForegroundColor DarkGray
Write-Host ''
