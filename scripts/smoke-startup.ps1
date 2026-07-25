param(
  [int]$Iterations = 5,
  [int]$WaitMs = 1800,
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
  $summaries += [pscustomobject]@{
    run = $run
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
