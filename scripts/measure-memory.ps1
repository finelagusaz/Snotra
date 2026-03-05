$procs = Get-Process | Where-Object { $_.ProcessName -match 'snotra|Snotra|msedgewebview2' -and $_.MainWindowTitle -ne '' -or $_.ProcessName -match 'snotra|Snotra' }

# Group by process name
$groups = Get-Process | Where-Object { $_.ProcessName -match 'snotra|Snotra' } | Group-Object ProcessName

Write-Host "`n=== Snotra Memory Usage ===" -ForegroundColor Cyan

$totalWS = 0
$totalPB = 0

foreach ($g in $groups) {
    foreach ($p in $g.Group) {
        $ws = [math]::Round($p.WorkingSet64 / 1MB, 1)
        $pb = [math]::Round($p.PrivateMemorySize64 / 1MB, 1)
        $totalWS += $ws
        $totalPB += $pb
        Write-Host ("  {0,-30} PID:{1,-8} WS: {2,7} MB  Private: {3,7} MB" -f $p.ProcessName, $p.Id, $ws, $pb)
    }
}

# Also check WebView2 processes spawned by Snotra
$snotraPids = (Get-Process | Where-Object { $_.ProcessName -match 'snotra|Snotra' }).Id
$webviews = Get-Process | Where-Object { $_.ProcessName -eq 'msedgewebview2' }
$wvCount = 0
$wvWS = 0
$wvPB = 0

foreach ($wv in $webviews) {
    try {
        $parent = (Get-CimInstance Win32_Process -Filter "ProcessId = $($wv.Id)").ParentProcessId
        if ($snotraPids -contains $parent) {
            $ws = [math]::Round($wv.WorkingSet64 / 1MB, 1)
            $pb = [math]::Round($wv.PrivateMemorySize64 / 1MB, 1)
            $wvCount++
            $wvWS += $ws
            $wvPB += $pb
            Write-Host ("  {0,-30} PID:{1,-8} WS: {2,7} MB  Private: {3,7} MB" -f "msedgewebview2 (child)", $wv.Id, $ws, $pb) -ForegroundColor DarkGray
        }
    } catch {}
}

Write-Host ""
Write-Host ("  Snotra total:    WS: {0,7} MB  Private: {1,7} MB" -f $totalWS, $totalPB) -ForegroundColor Green
if ($wvCount -gt 0) {
    Write-Host ("  WebView2 ({0}):   WS: {1,7} MB  Private: {2,7} MB" -f $wvCount, $wvWS, $wvPB) -ForegroundColor Yellow
    Write-Host ("  Grand total:     WS: {0,7} MB  Private: {1,7} MB" -f ($totalWS + $wvWS), ($totalPB + $wvPB)) -ForegroundColor White
}
Write-Host ""
