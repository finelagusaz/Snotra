param(
  [string]$ExePath = "target/release/snotra.exe",
  [int]$StartupWaitMs = 4000,
  [int]$ObserveTimeoutMs = 8000,
  [switch]$SeedConfig,
  # hotkey の仮想キーコード列（カンマ区切り・押下順・解放は逆順）。既定は Alt(18)+Q(81) = config.rs の既定 hotkey。
  # hotkey は config.toml 依存のため、実 config を持つ実機ではその値を渡す（例: Ctrl+K は "17,75"）。
  # pwsh -File / npm 経由で配列引数が壊れないよう文字列で受けて内部で分割する。
  [string]$HotkeyVks = "18,81"
)

# egui 経路の自動回帰 smoke（#532 SU7 PR1・spec: docs/superpowers/specs/2026-07-24-su7-flip-implementation-design.md 決定 3）。
# 起動 → keybd_event で Alt+Q（既定 hotkey）注入 → trace `egui_show:done` 観測 → Escape 注入 →
# `egui_hide:done` 観測 → msedgewebview2 のグローバル増分 0 確認、で 1 シナリオ。
# - hotkey は Alt 解放を含めて送る（Alt 押下中は ShowAfterAltRelease で最大 350ms 遅延するため）。
# - -SeedConfig（CI 用）: config.toml 不在時のみ最小の有効 TOML を seed し first-run 経路
#   （snotra-settings --first-run の spawn がフォーカスを奪う）を回避する。既存 config は決して上書きしない。
# - egui/WebView2 の経路選択は呼び出し側の env（SNOTRA_EGUI_MAIN）に従う。flip（PR2）後は env なしが既定。

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not (Test-Path $ExePath)) {
  throw "Executable not found: $ExePath"
}

if ($SeedConfig) {
  $cfgDir = Join-Path $env:APPDATA "Snotra"
  $cfgPath = Join-Path $cfgDir "config.toml"
  if (-not (Test-Path $cfgPath)) {
    New-Item -ItemType Directory -Force -Path $cfgDir | Out-Null
    # 最小の有効 TOML。[hotkey]/[appearance]/[paths] は #[serde(default)] 無しの必須セクションで、
    # 空 TOML は parse 失敗し「破損復旧」経路（stderr 診断 + config.toml.bak 退避 + 復旧バルーン）を
    # 毎回踏んでしまう（PR #659 レビューで検出）。値は config.rs の既定と同一
    # （hotkey Alt+Q = 本スクリプト既定の -HotkeyVks 18,81 と一致）。scan 空 = インデックス対象なし
    # （smoke は show/hide のみで索引不要・CI のスキャンを省く）。
    $seedToml = @'
[hotkey]
modifier = "Alt"
key = "Q"

[appearance]
window_width = 600

[paths]
scan = []
'@
    Set-Content -Path $cfgPath -Value $seedToml -Encoding utf8
    Write-Host "Seeded minimal config: $cfgPath"
  } else {
    Write-Host "Config already exists, seed skipped: $cfgPath"
  }
}

Add-Type -Namespace SmokeInput -Name Native -MemberDefinition @'
[DllImport("user32.dll")]
public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, UIntPtr dwExtraInfo);
'@

$KEYEVENTF_KEYUP = 0x2
$VK_ESCAPE = 0x1B

function Send-Key {
  param([byte]$Vk, [switch]$Up)
  $flags = if ($Up) { $KEYEVENTF_KEYUP } else { 0 }
  [SmokeInput.Native]::keybd_event($Vk, 0, $flags, [UIntPtr]::Zero)
}

function Wait-TraceEvent {
  param([string]$Path, [string]$EventName, [int]$TimeoutMs)
  $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
  $pattern = '"event":"' + $EventName + '"'
  while ([DateTime]::UtcNow -lt $deadline) {
    try {
      if ((Test-Path $Path) -and (Select-String -Path $Path -Pattern $pattern -SimpleMatch -Quiet)) {
        return $true
      }
    } catch {
      # 書き込み中のファイル読取り競合は無視して再試行
    }
    Start-Sleep -Milliseconds 200
  }
  return $false
}

# 既存インスタンスは single-instance 転送で smoke を汚すため停止（smoke-startup.ps1 と同じ前提）
Get-Process snotra -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 300

$webviewBefore = @(Get-Process msedgewebview2 -ErrorAction SilentlyContinue).Count

$errPath = Join-Path $env:TEMP "snotra_smoke_egui.err"
$outPath = Join-Path $env:TEMP "snotra_smoke_egui.out"
Remove-Item $errPath, $outPath -Force -ErrorAction SilentlyContinue

$savedTraceEnv = $env:SNOTRA_TRACE
$env:SNOTRA_TRACE = "1"
$proc = Start-Process -FilePath $ExePath -PassThru -RedirectStandardError $errPath -RedirectStandardOutput $outPath
if ($null -eq $savedTraceEnv) {
  Remove-Item Env:SNOTRA_TRACE -ErrorAction SilentlyContinue
} else {
  $env:SNOTRA_TRACE = $savedTraceEnv
}

$failures = @()
try {
  # 起動完了（hotkey 登録含む）待ち。既定は hidden 起動（show_on_startup=false）。
  Start-Sleep -Milliseconds $StartupWaitMs
  if ($proc.HasExited) {
    throw "Process exited during startup (exit code $($proc.ExitCode))"
  }

  # hotkey 注入（押下順 → 逆順で解放。Alt を含む場合、Alt up が最後に来ることで
  # ShowAfterAltRelease〔Alt 押下中は show を最大 350ms 繰り延べる〕が解決する）。
  # CI runner は起動直後の負荷で初回注入を取りこぼすことがある（PR #662 で flake 実測・
  # 再走で合格）ため、観測できなければ一度だけ再注入する。
  $vks = @($HotkeyVks -split ',' | ForEach-Object { [byte]([int]$_.Trim()) })
  if ($vks.Count -lt 1) { throw "HotkeyVks must contain at least one VK code" }
  $shown = $false
  foreach ($attempt in 1..2) {
    foreach ($vk in $vks) {
      Send-Key $vk
      Start-Sleep -Milliseconds 50
    }
    [array]::Reverse($vks)
    foreach ($vk in $vks) {
      Send-Key $vk -Up
      Start-Sleep -Milliseconds 50
    }
    [array]::Reverse($vks)  # 再試行に備えて押下順へ戻す
    if (Wait-TraceEvent -Path $errPath -EventName "egui_show:done" -TimeoutMs $ObserveTimeoutMs) {
      $shown = $true
      break
    }
  }
  if (-not $shown) {
    $failures += "egui_show:done not observed within ${ObserveTimeoutMs}ms x2 after hotkey ($HotkeyVks)"
  }

  # 表示中に WebView2 プロセスが増えていないこと（グローバル before/after・SU2 G4 と同じ測り方）
  $webviewAfter = @(Get-Process msedgewebview2 -ErrorAction SilentlyContinue).Count
  if ($webviewAfter -gt $webviewBefore) {
    $failures += "msedgewebview2 count increased: $webviewBefore -> $webviewAfter"
  }

  if ($failures.Count -eq 0) {
    # Escape 注入（表示中の egui 窓がフォーカスを持つ前提）
    Send-Key $VK_ESCAPE
    Start-Sleep -Milliseconds 50
    Send-Key $VK_ESCAPE -Up

    if (-not (Wait-TraceEvent -Path $errPath -EventName "egui_hide:done" -TimeoutMs $ObserveTimeoutMs)) {
      $failures += "egui_hide:done not observed within ${ObserveTimeoutMs}ms after Escape"
    }
  }
} finally {
  if (-not $proc.HasExited) {
    Stop-Process -Id $proc.Id -Force
  }
}

if ($failures.Count -gt 0) {
  Write-Host ""
  Write-Host "egui smoke failed:" -ForegroundColor Red
  foreach ($f in $failures) {
    Write-Host " - $f" -ForegroundColor Red
  }
  if (Test-Path $errPath) {
    Write-Host ""
    Write-Host "--- trace tail ---"
    Get-Content $errPath -Tail 40
  }
  exit 1
}

Write-Host ""
Write-Host "egui smoke passed (show/hide observed, webview delta 0)." -ForegroundColor Green
