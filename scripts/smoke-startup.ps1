param(
  [int]$Iterations = 5,
  [int]$WaitMs = 1800,
  # 最初の trace 1 行が出るまでの予算（#690 follow-up）。これを過ぎたら諦めて観測窓へ進み、
  # 結果として trace 0 件になれば失敗する。**$WaitMs とは役割が別**——こちらは起動を待つ
  # 時間、$WaitMs は最初の trace 以降にイベントを集める窓。実測分散（0.6s〜8s超）に対する
  # 余裕として 12s を既定にする。
  [int]$FirstTraceTimeoutMs = 12000,
  [string]$ExePath = "C:\workspace\Snotra\target\debug\snotra.exe"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($Iterations -lt 1) {
  throw "Iterations must be >= 1"
}
if ($WaitMs -lt 200) {
  throw "WaitMs must be >= 200"
}
if (-not (Test-Path $ExePath)) {
  throw "Executable not found: $ExePath"
}

# Two-window architecture (main + results, #646 PR2): startup smoke only exercises
# main show/hide timing; the results window is driven by main's update() and has
# no independent lifecycle to verify here.
$summaries = @()
$failures = @()
$savedTraceEnv = $env:SNOTRA_TRACE

function Restore-TraceEnv {
  param([string]$Saved)
  if ($null -eq $Saved) {
    Remove-Item Env:SNOTRA_TRACE -ErrorAction SilentlyContinue
  } else {
    $env:SNOTRA_TRACE = $Saved
  }
}

for ($run = 1; $run -le $Iterations; $run++) {
  Get-Process snotra -ErrorAction SilentlyContinue | Stop-Process -Force

  $errPath = Join-Path $env:TEMP ("snotra_smoke_startup_{0}.err" -f $run)
  $outPath = Join-Path $env:TEMP ("snotra_smoke_startup_{0}.out" -f $run)
  if (Test-Path $errPath) { Remove-Item $errPath -Force }
  if (Test-Path $outPath) { Remove-Item $outPath -Force }

  $env:SNOTRA_TRACE = "1"
  $proc = Start-Process -FilePath $ExePath -PassThru -RedirectStandardError $errPath -RedirectStandardOutput $outPath

  # **最初の trace が出るまで待ってから、観測窓 $WaitMs を開く**（#690 follow-up）。
  #
  # 旧実装は固定 $WaitMs（1,800ms）だけ待って打ち切っていた。しかし同一 runner・同一
  # バイナリで最初の trace までが **0.6s / 5.2s / 8s超** と大きくばらつくことを実測して
  # おり、遅い側に振れた起動は**丸ごと無音**のまま観測を終えていた（5 回中 3 回が
  # trace 0 件・それでも「*:error 不在」は自明に成立するので緑だった）。
  #
  # 固定待機を一律に伸ばすと 5 起動 × 予算が常時かかる。最初の 1 行を待つ形なら、
  # 速い起動は速いまま・遅い起動だけ待つ。$WaitMs は**最初の trace 以降**の観測窓として
  # 残す——ここを縮めると後続イベント（*:error）の取りこぼしが増え、検査が痩せる。
  $firstTraceMs = $null
  $swFirst = [System.Diagnostics.Stopwatch]::StartNew()
  while ($swFirst.Elapsed.TotalMilliseconds -lt $FirstTraceTimeoutMs) {
    if ((Test-Path $errPath) -and @(Get-Content $errPath -ErrorAction SilentlyContinue).Count -gt 0) {
      $firstTraceMs = [int]$swFirst.Elapsed.TotalMilliseconds
      break
    }
    if ($proc.HasExited) { break }
    Start-Sleep -Milliseconds 100
  }

  Start-Sleep -Milliseconds $WaitMs
  if (-not $proc.HasExited) {
    Stop-Process -Id $proc.Id -Force
  }
  Restore-TraceEnv -Saved $savedTraceEnv
  Start-Sleep -Milliseconds 120

  $events = @()
  if (Test-Path $errPath) {
    foreach ($line in Get-Content $errPath) {
      if ($line -match '^\[trace\] (.+)$') {
        try {
          $events += ($Matches[1] | ConvertFrom-Json)
        } catch {
        }
      }
    }
  }

  # **trace が 0 件なら「*:error が無い」は自明に成立する**——空振りの合格である。
  # #690 の調査で、冷えた CI runner の初回起動が 20 秒間 trace を 1 行も出さない状態を
  # 実測した。その状態でも本 smoke は緑を返していた（アサーションが不在の検査だけゆえ）。
  # SNOTRA_TRACE=1 の起動は最低でも `hotkey:registered` を出すため、0 件は異常である。
  if ($events.Count -eq 0) {
    $failures += "run=$run trace が 0 件（アプリが 1 行も出していない）。この状態では「*:error 不在」は何も証明しない"
  }

  $errorEvents = @($events | Where-Object { $_.event -like "*:error" })
  foreach ($errEvt in $errorEvents) {
    $failures += "run=$run error event=$($errEvt.event)"
  }

  # event_count は成功時にも出す（**検査が実際に何かを見た**ことを読み手に示すため。
  # 沈黙を合格と読ませないための肯定的報告）。
  # first_trace_ms は成功時にも出す。**起動レイテンシの分散はまだ原因未解明**であり、
  # 予算内に収まっていても数字が残れば、悪化の傾向を人が読める（予算に触れて初めて
  # 気づく状態にしない）。null は「予算内に 1 行も出なかった」。
  $summaries += [pscustomobject]@{
    run = $run
    first_trace_ms = if ($null -eq $firstTraceMs) { "n/a" } else { $firstTraceMs }
    event_count = $events.Count
    error_count = $errorEvents.Count
  }
}

$summaries | Format-Table -AutoSize

if ($failures.Count -gt 0) {
  Write-Host ""
  Write-Host "Startup smoke failed:" -ForegroundColor Red
  foreach ($f in $failures) {
    Write-Host " - $f" -ForegroundColor Red
  }
  exit 1
}

Write-Host ""
Write-Host "Startup smoke passed ($Iterations runs)." -ForegroundColor Green
