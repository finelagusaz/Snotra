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

# about/settings are now separate processes (snotra-settings); only results window is pre-created.
$requiredLabels = @("results")
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

  $okByLabel = @{}
  foreach ($evt in $events) {
    if ($evt.event -eq "main:ensure_window:ok") {
      $label = [string]$evt.data.label
      if ($requiredLabels -contains $label) {
        $okByLabel[$label] = [int]$evt.data.elapsed_ms
      }
    }
  }

  $errorEvents = @($events | Where-Object { $_.event -like "*:error" })
  foreach ($label in $requiredLabels) {
    if (-not $okByLabel.ContainsKey($label)) {
      $failures += "run=$run missing main:ensure_window:ok label=$label"
    }
  }
  foreach ($errEvt in $errorEvents) {
    $failures += "run=$run error event=$($errEvt.event)"
  }

  $summaries += [pscustomobject]@{
    run = $run
    results_ms = if ($okByLabel.ContainsKey("results")) { $okByLabel["results"] } else { $null }
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
