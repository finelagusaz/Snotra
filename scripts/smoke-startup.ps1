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

  $errorEvents = @($events | Where-Object { $_.event -like "*:error" })
  foreach ($errEvt in $errorEvents) {
    $failures += "run=$run error event=$($errEvt.event)"
  }

  $summaries += [pscustomobject]@{
    run = $run
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
