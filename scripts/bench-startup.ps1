param(
  [int]$Iterations = 5,
  [int]$WaitMs = 3000,
  [string]$ExePath = "C:\workspace\Snotra\target\release\snotra.exe"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($Iterations -lt 1) { throw "Iterations must be >= 1" }
if ($WaitMs -lt 500) { throw "WaitMs must be >= 500" }
if (-not (Test-Path $ExePath)) { throw "Executable not found: $ExePath" }

$savedTraceEnv = $env:SNOTRA_TRACE

function Restore-TraceEnv {
  param([string]$Saved)
  if ($null -eq $Saved) {
    Remove-Item Env:SNOTRA_TRACE -ErrorAction SilentlyContinue
  } else {
    $env:SNOTRA_TRACE = $Saved
  }
}

# --- Memory helper: sum WorkingSet64 for a process tree ---
function Get-ProcessTreeMemoryMB {
  param([int]$RootPid)
  $total = 0
  $procs = Get-Process -ErrorAction SilentlyContinue |
    Where-Object { $_.Id -eq $RootPid }
  if ($procs) {
    $total += $procs.WorkingSet64
  }
  # WebView2 spawns child processes whose parent is the root or another child.
  # On Windows, Get-CimInstance gives us ParentProcessId.
  try {
    $children = Get-CimInstance Win32_Process -Filter "ParentProcessId=$RootPid" -ErrorAction SilentlyContinue
    foreach ($child in $children) {
      $cp = Get-Process -Id $child.ProcessId -ErrorAction SilentlyContinue
      if ($cp) { $total += $cp.WorkingSet64 }
      # grandchildren (WebView2 renderer)
      $grandchildren = Get-CimInstance Win32_Process -Filter "ParentProcessId=$($child.ProcessId)" -ErrorAction SilentlyContinue
      foreach ($gc in $grandchildren) {
        $gcp = Get-Process -Id $gc.ProcessId -ErrorAction SilentlyContinue
        if ($gcp) { $total += $gcp.WorkingSet64 }
      }
    }
  } catch {}
  return [math]::Round($total / 1MB, 1)
}

function Get-ProcessTreeCount {
  param([int]$RootPid)
  $count = 0
  $procs = Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.Id -eq $RootPid }
  if ($procs) { $count++ }
  try {
    $children = Get-CimInstance Win32_Process -Filter "ParentProcessId=$RootPid" -ErrorAction SilentlyContinue
    foreach ($child in $children) {
      $count++
      $grandchildren = Get-CimInstance Win32_Process -Filter "ParentProcessId=$($child.ProcessId)" -ErrorAction SilentlyContinue
      $count += @($grandchildren).Count
    }
  } catch {}
  return $count
}

Write-Host "=== Snotra Startup Benchmark ===" -ForegroundColor Cyan
Write-Host "Exe:        $ExePath"
Write-Host "Iterations: $Iterations"
Write-Host "Wait:       ${WaitMs}ms"
Write-Host ""

$summaries = @()

for ($run = 1; $run -le $Iterations; $run++) {
  Write-Host "Run $run/$Iterations ... " -NoNewline

  # Kill any leftover snotra processes
  Get-Process snotra -ErrorAction SilentlyContinue | Stop-Process -Force
  Start-Sleep -Milliseconds 300

  $errPath = Join-Path $env:TEMP ("snotra_bench_{0}.err" -f $run)
  $outPath = Join-Path $env:TEMP ("snotra_bench_{0}.out" -f $run)
  if (Test-Path $errPath) { Remove-Item $errPath -Force }
  if (Test-Path $outPath) { Remove-Item $outPath -Force }

  $env:SNOTRA_TRACE = "1"
  $proc = Start-Process -FilePath $ExePath -PassThru `
    -RedirectStandardError $errPath -RedirectStandardOutput $outPath

  # Wait for startup to stabilize
  Start-Sleep -Milliseconds $WaitMs

  # Measure memory (process tree)
  $memMB = 0.0
  $procCount = 0
  if (-not $proc.HasExited) {
    $memMB = Get-ProcessTreeMemoryMB -RootPid $proc.Id
    $procCount = Get-ProcessTreeCount -RootPid $proc.Id
  }

  # Kill
  if (-not $proc.HasExited) {
    Stop-Process -Id $proc.Id -Force
  }
  Restore-TraceEnv -Saved $savedTraceEnv
  Start-Sleep -Milliseconds 200

  $row = [pscustomobject]@{
    run           = $run
    memory_MB     = $memMB
    processes     = $procCount
  }
  $summaries += $row
  Write-Host ("mem={0}MB procs={1}" -f $memMB, $procCount)
}

Write-Host ""
Write-Host "=== Results ===" -ForegroundColor Cyan
$summaries | Format-Table -AutoSize

# Compute averages
$avgMem = [math]::Round(($summaries | Measure-Object -Property memory_MB -Average).Average, 1)
$avgProcs = [math]::Round(($summaries | Measure-Object -Property processes -Average).Average, 1)

Write-Host "=== Averages ===" -ForegroundColor Green
Write-Host ("  Memory:       {0} MB" -f $avgMem)
Write-Host ("  Processes:    {0}" -f $avgProcs)
Write-Host ""

# Output machine-readable JSON summary
$jsonSummary = @{
  exe        = $ExePath
  iterations = $Iterations
  wait_ms    = $WaitMs
  averages   = @{
    memory_MB   = $avgMem
    processes   = $avgProcs
  }
  runs = @($summaries | ForEach-Object {
    @{
      run         = $_.run
      memory_MB   = $_.memory_MB
      processes   = $_.processes
    }
  })
} | ConvertTo-Json -Depth 3

$jsonPath = Join-Path $env:TEMP "snotra_bench_result.json"
$jsonSummary | Out-File -FilePath $jsonPath -Encoding utf8
Write-Host "JSON saved to: $jsonPath"
