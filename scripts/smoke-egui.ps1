param(
  [string]$ExePath = "target/release/snotra.exe",
  [int]$StartupWaitMs = 4000,
  [int]$ObserveTimeoutMs = 8000,
  [switch]$SeedConfig,
  # hotkey の仮想キーコード列（カンマ区切り・押下順・解放は逆順）。
  # **通常は指定不要**——既定では起動時の `hotkey:registered` trace から、アプリが実際に
  # 登録した VK 列を読む（対応表の SSOT は src-tauri/src/platform/hotkey.rs の injection_vks）。
  # 明示指定したときだけ trace より優先される（trace が出ない旧バイナリの検証など）。
  # pwsh -File / npm 経由で配列引数が壊れないよう文字列で受けて内部で分割する。
  [string]$HotkeyVks = "18,81"
  ,
  # results 窓の被覆に使う検索クエリ（1 文字想定）。既定 "" のとき:
  #   -SeedConfig で実際に seed できた場合のみ "z"（seed した zsnotrasmoke.exe に一致）を使う。
  #   seed しなかった場合（既存 config あり / -SeedConfig なし）は results 検査を skip する。
  # 既存 config を持つ開発機で検査したいときは、その索引に一致する文字を明示的に渡す。
  [string]$ResultsQuery = ""
)

# egui 経路の自動回帰 smoke（#532 SU7 PR1・spec: docs/superpowers/specs/2026-07-24-su7-flip-implementation-design.md 決定 3。
# results 窓の被覆は #671/#673 サイクル PR A で追加）。
# 起動 → keybd_event で hotkey 注入（既定は起動時の `hotkey:registered` trace から導出した VK 列。
# 明示 -HotkeyVks 指定時のみそちらを使う） → trace `egui_show:done` 観測 →
# （索引内容を制御できるとき）1 文字クエリを注入して `egui_results:show` 観測 → Escape 注入 →
# `egui_hide:done`（+ results 検査時は `egui_results:hide`）観測 → msedgewebview2 の
# グローバル増分 0 確認、で 1 シナリオ。
# - hotkey の修飾に Alt を含む場合、Alt 解放を含めて送る（Alt 押下中は ShowAfterAltRelease で
#   最大 350ms 遅延するため）。Alt を含まない hotkey（例 Ctrl+K）では無関係。
# - -SeedConfig（CI 用）: config.toml 不在時のみ最小の有効 TOML を seed し first-run 経路
#   （snotra-settings --first-run の spawn がフォーカスを奪う）を回避する。既存 config は決して上書きしない。
#   seed できたときは results 被覆用の索引対象も 1 件同梱する（-ResultsQuery 既定の導出元）。
# - WebView2/フロントエンドは #532 SU7 で撤去済みで、egui が唯一の UI 経路（env による経路選択は無い）。

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not (Test-Path $ExePath)) {
  throw "Executable not found: $ExePath"
}

$seededNow = $false
if ($SeedConfig) {
  $cfgDir = Join-Path $env:APPDATA "Snotra"
  $cfgPath = Join-Path $cfgDir "config.toml"
  if (-not (Test-Path $cfgPath)) {
    New-Item -ItemType Directory -Force -Path $cfgDir | Out-Null
    # results 窓の被覆用に、索引に必ず 1 件載るダミーを置く（中身は問わない——indexer は
    # 拡張子だけで判定する: snotra-core/src/indexer.rs の matches_extension）。
    # 名前は既存の索引と衝突しにくい接頭辞にし、-ResultsQuery 既定の "z" で引けるようにする。
    $scanDir = Join-Path $env:TEMP "snotra_smoke_scan"
    New-Item -ItemType Directory -Force -Path $scanDir | Out-Null
    $dummy = Join-Path $scanDir "zsnotrasmoke.exe"
    if (-not (Test-Path $dummy)) { New-Item -ItemType File -Path $dummy | Out-Null }
    $scanDirToml = $scanDir -replace '\\', '/'
    # 最小の有効 TOML。[hotkey]/[appearance]/[paths] は #[serde(default)] 無しの必須セクションで、
    # 空 TOML は parse 失敗し「破損復旧」経路（stderr 診断 + config.toml.bak 退避 + 復旧バルーン）を
    # 毎回踏んでしまう（PR #659 レビューで検出）。値は config.rs の既定と同一
    # （hotkey Alt+Q = 本スクリプト既定の -HotkeyVks 18,81 と一致）。
    # scan は上の 1 ファイルだけを対象にする（索引は 1 件・ビルドは即座に終わる）。
    # **`scan = []` と `[[paths.scan]]` を併記してはならない**——同一キーの再定義で TOML の
    # parse が落ち、config 破損復旧経路へ落ちる。`[paths]` を空ヘッダで置き、その下に
    # array-of-tables を続ける（`PathsConfig.scan` は #[serde(default)] ゆえ空でも可）。
    $seedToml = @"
[hotkey]
modifier = "Alt"
key = "Q"

[appearance]
window_width = 600

[paths]

[[paths.scan]]
path = "$scanDirToml"
extensions = [".exe"]
include_folders = false
"@
    Set-Content -Path $cfgPath -Value $seedToml -Encoding utf8
    $seededNow = $true
    Write-Host "Seeded minimal config: $cfgPath (scan: $scanDir)"
  } else {
    Write-Host "Config already exists, seed skipped: $cfgPath"
  }
}

# results 検査に使うクエリを決める。空文字なら検査を skip する。
if ([string]::IsNullOrEmpty($ResultsQuery) -and $seededNow) {
  $ResultsQuery = "z"
}

Add-Type -Namespace SmokeInput -Name Native -MemberDefinition @'
[DllImport("user32.dll")]
public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, UIntPtr dwExtraInfo);
'@

$KEYEVENTF_KEYUP = 0x2
$VK_ESCAPE = 0x1B
$VK_BACK = 0x08

# 1 文字クエリの VK は英字のみを想定（VK_A..VK_Z = 0x41..0x5A = ASCII 大文字と同値）。
function Get-LetterVk {
  param([string]$Ch)
  $u = $Ch.ToUpperInvariant()
  if ($u.Length -ne 1 -or $u[0] -lt 'A' -or $u[0] -gt 'Z') {
    throw "ResultsQuery must be a single A-Z letter, got: '$Ch'"
  }
  return [byte][int][char]$u[0]
}

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

function Get-TraceEventData {
  param([string]$Path, [string]$EventName, [int]$TimeoutMs)
  # trace の行形式: `[trace] {"seq":N,"ts_ms":M,"event":"...","data":{...}}`（src-tauri/src/trace.rs）
  $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
  $pattern = '"event":"' + $EventName + '"'
  while ([DateTime]::UtcNow -lt $deadline) {
    try {
      if (Test-Path $Path) {
        $line = Select-String -Path $Path -Pattern $pattern -SimpleMatch |
                Select-Object -Last 1
        if ($null -ne $line) {
          $json = $line.Line -replace '^\[trace\]\s*', ''
          return ($json | ConvertFrom-Json).data
        }
      }
    } catch {
      # 書き込み中のファイル読取り競合は無視して再試行
    }
    Start-Sleep -Milliseconds 200
  }
  return $null
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
$resultsChecked = $false
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
  # 押すキーは**アプリが実際に登録した値**から採る（scripts が config を読み解いて
  # VK へ変換すると、hotkey.rs の parse_modifier / parse_vk / 修飾ビット→VK の 3 表が
  # PowerShell 側に写り、ドリフトする）。-HotkeyVks が明示指定されたときだけそちらを使う。
  # $hotkeySource / $vksLabel は「実際に注入する VK 列」を失敗メッセージへも一致させるための
  # 単一の出所（#671 サイクル PR A レビュー指摘 2）。$HotkeyVks（既定値・未指定でも常に非空）を
  # そのままメッセージへ出すと、trace 由来の VK で失敗したときに実際と異なる値を報告してしまう。
  if ($PSBoundParameters.ContainsKey('HotkeyVks')) {
    $hotkeySource = "explicit"
    $vks = @($HotkeyVks -split ',' | ForEach-Object { [byte]([int]$_.Trim()) })
    Write-Host "Hotkey VKs (explicit): $($vks -join ',')"
  } else {
    $hotkeySource = "from trace"
    $hk = Get-TraceEventData -Path $errPath -EventName "hotkey:registered" -TimeoutMs $ObserveTimeoutMs
    if ($null -eq $hk) {
      throw "hotkey:registered trace not observed within ${ObserveTimeoutMs}ms — cannot determine which keys to inject"
    }
    if (-not $hk.ok) {
      throw "hotkey registration failed in the app (modifier=$($hk.modifier) key=$($hk.key)) — smoke cannot proceed"
    }
    $vks = @($hk.vks | ForEach-Object { [byte][int]$_ })
    Write-Host "Hotkey VKs (from trace): $($vks -join ',') (modifier=$($hk.modifier) key=$($hk.key))"
  }
  $vksLabel = ($vks -join ',')
  if ($vks.Count -lt 1) {
    if ($hotkeySource -eq "explicit") {
      throw "HotkeyVks must contain at least one VK code"
    } else {
      throw "hotkey:registered trace reported an empty VK list — cannot determine which keys to inject"
    }
  }
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
    $failures += "egui_show:done not observed within ${ObserveTimeoutMs}ms x2 after hotkey ($vksLabel, $hotkeySource)"
  }

  # results 窓の被覆（#671/#673 サイクル PR A）。索引内容を制御できるときだけ実行する。
  $resultsChecked = $false
  if ($failures.Count -eq 0 -and -not [string]::IsNullOrEmpty($ResultsQuery)) {
    $resultsChecked = $true
    $queryVk = Get-LetterVk $ResultsQuery
    # 索引構築中は plain 検索が抑止される（SPEC §4.7）。起動直後の負荷で 1 回目の打鍵が
    # 抑止側に落ちることがあるため、hotkey 注入と同じく一度だけ再注入する。入力欄は
    # 打鍵間でクリアされないため、2 回目の前に Backspace を 1 回送って先行入力を消してから
    # 再注入する（送らないと 2 回目は実質 "zz" になり、単一文字マッチ前提の索引とは
    # 原理的に一致しえない）。打鍵が落ちていたケースでは Backspace も同様に落ちるだけで
    # 2 回目の単独入力は無害、抑止で 1 回目が吸収されたケースでは Backspace が残存文字を
    # 消して単一文字クエリを再成立させる。
    $resultsShown = $false
    foreach ($attempt in 1..2) {
      if ($attempt -gt 1) {
        Send-Key $VK_BACK
        Start-Sleep -Milliseconds 50
        Send-Key $VK_BACK -Up
        Start-Sleep -Milliseconds 50
      }
      Send-Key $queryVk
      Start-Sleep -Milliseconds 50
      Send-Key $queryVk -Up
      if (Wait-TraceEvent -Path $errPath -EventName "egui_results:show" -TimeoutMs $ObserveTimeoutMs) {
        $resultsShown = $true
        break
      }
    }
    if (-not $resultsShown) {
      $failures += "egui_results:show not observed within ${ObserveTimeoutMs}ms x2 after typing '$ResultsQuery'"
    }
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

    # main の hide は hide_egui_main が results も同時に隠す（#646 PR2 決定 6）。
    # show 側を検査したときだけ対で検査する（対称ペア・/symmetric-check）。
    if ($resultsChecked -and
        -not (Wait-TraceEvent -Path $errPath -EventName "egui_results:hide" -TimeoutMs $ObserveTimeoutMs)) {
      $failures += "egui_results:hide not observed within ${ObserveTimeoutMs}ms after Escape"
    }

    # #671 PR A′: hide 後に results だけが最前面に取り残されないこと（orphan 検出）。
    # **presence 検査ではこの事故を素通りする**——orphan でも egui_results:hide は出るため。
    # orphan は必ず「hide 以降の余分な egui_results:show」として現れる（main が hidden でも
    # repaint 要求は飛ぶ: config-applied / indexing-* / updater 完了の wake_main）。
    # 静定を待ってから、最後の egui_hide:done より後ろの行だけを見る。
    if ($resultsChecked -and $failures.Count -eq 0) {
      Start-Sleep -Milliseconds 1500
      $lines = @(Get-Content -Path $errPath)
      $hideIdx = -1
      for ($i = $lines.Count - 1; $i -ge 0; $i--) {
        if ($lines[$i] -match '"event":"egui_hide:done"') { $hideIdx = $i; break }
      }
      # $hideIdx が最終行なら「後ろ」は空。PowerShell の範囲演算子は降順にも回るため、
      # 空区間を範囲式で書くと逆走して誤検出する——先に件数で弾く。
      if ($hideIdx -ge 0 -and $hideIdx -lt ($lines.Count - 1)) {
        $after = $lines[($hideIdx + 1)..($lines.Count - 1)]
        $orphan = @($after | Where-Object { $_ -match '"event":"egui_results:show"' })
        if ($orphan.Count -gt 0) {
          $failures += "results window re-shown after egui_hide:done ($($orphan.Count) x egui_results:show); main is hidden but results is left on top"
        }
      }
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
if ($resultsChecked) {
  Write-Host "egui smoke passed (show/hide + results show/hide observed, webview delta 0)." -ForegroundColor Green
} else {
  Write-Host "egui smoke passed (show/hide observed, webview delta 0)." -ForegroundColor Green
  Write-Host "NOTE: results window coverage was SKIPPED (no controlled index). Pass -SeedConfig on a machine without %APPDATA%/Snotra/config.toml, or pass -ResultsQuery <letter> matching your index." -ForegroundColor Yellow
}
