param(
  [string]$ExePath = "target/release/snotra.exe",
  [int]$StartupWaitMs = 4000,
  [int]$ObserveTimeoutMs = 8000,
  # hotkey の仮想キーコード列（カンマ区切り・押下順・解放は逆順）。
  # **通常は指定不要**——既定では起動時の `hotkey:registered` trace から、アプリが実際に
  # 登録した VK 列を読む（対応表の SSOT は src-tauri/src/platform/hotkey.rs の PreparedHotkey）。
  # 明示指定したときだけ trace より優先される（trace が出ない旧バイナリの検証など）。
  # pwsh -File / npm 経由で配列引数が壊れないよう文字列で受けて内部で分割する。
  [string]$HotkeyVks = "18,81"
  ,
  # results 窓の検証に使う検索クエリ（1 文字想定）。既定 "z" は seed が置くダミー
  # zsnotrasmoke.exe に一致する（#804: 検証用プロファイルを**常に** seed するので、
  # 既定が空振りすることはない）。
  # **空文字を明示的に渡すと下の guard でアプリを起こさずに落ちる**——これが
  # フォールトインジェクションの注入口である（旧 -RequireResults の後継・docs/build-commands.md）。
  [string]$ResultsQuery = "z"
  ,
  # 失敗して trace が 0 行だったときだけ、追加でこの時間まで「最初の 1 行」を待つ（#690 follow-up）。
  # **観測窓を広げる前に遅延を測るため**の予算であって、合否には影響しない（失敗は失敗のまま）。
  # 0 を渡すと事後観測を行わない。
  [int]$PostMortemWaitMs = 30000
  ,
  # `hotkey:registered`（起動後**最初**の観測）専用の予算（#690 follow-up）。
  # ここだけ cold start を含む。CI で「起動後 12,000ms 経っても trace 0 行」を 3 回実測した
  # （プロセスは生存＝クラッシュではない）一方、成功時は起動から 0.6s で出ている。
  # **この二極の原因は未解明**であり、この予算は原因究明までの緩和にすぎない。
  #
  # **壁時計から起動レイテンシを推定してはならない**（一度誤った）: seed の print から
  # hotkey 観測までの壁時計には、下の `Add-Type`（実行時 C# コンパイル・冷えた runner で
  # 7〜25s 変動）を含む**起動前**の時間が乗る。起動起点の計測（$launchedAt）だけが
  # アプリの遅延を表す。
  # 以降の観測（show/hide/results）はアプリが温まった後ゆえ `ObserveTimeoutMs` のまま。
  # 広げた分の盲目化は、成功時のレイテンシ表示（下）が補う。
  [int]$StartupObserveTimeoutMs = 25000
)

# egui 経路の自動回帰 smoke（#532 SU7 PR1・spec: docs/superpowers/specs/2026-07-24-su7-flip-implementation-design.md 決定 3。
# results 窓の検証は #671/#673 サイクル PR A で追加）。
# 起動 → keybd_event で hotkey 注入（既定は起動時の `hotkey:registered` trace から導出した VK 列。
# 明示 -HotkeyVks 指定時のみそちらを使う） → trace `egui_show:done` 観測 →
# （索引内容を制御できるとき）1 文字クエリを注入して `egui_results:show` 観測 → Escape 注入 →
# `egui_hide:done`（+ results 検査時は `egui_results:hide`）観測 → msedgewebview2 の
# グローバル増分 0 確認、で 1 シナリオ。
# - hotkey の修飾に Alt を含む場合、Alt 解放を含めて送る（Alt 押下中は ShowAfterAltRelease で
#   最大 350ms 遅延するため）。Alt を含まない hotkey（例 Ctrl+K）では無関係。
# - 検証用プロファイル（#804）: SNOTRA_CONFIG_DIR で target/smoke-egui/profile を指し、そこへ
#   最小の有効 TOML を**常に** seed する。**実ユーザーの %APPDATA%\Snotra は読みも書きもしない**
#   ——退避も復元も持たないことが、異常終了しても実 config が壊れない構造的な保証である。
#   seed は results 検証用の索引対象を 1 件だけ含む（-ResultsQuery 既定 "z" の導出元）。
#   seed を起動より前に置くことで first-run 経路（snotra-settings --first-run の spawn が
#   フォーカスを奪う）も踏まない——下の 3 判定（seed 健全性 / first-run 不発 / env 到達性）で
#   それぞれを肯定的に検査する。
# - WebView2/フロントエンドは #532 SU7 で撤去済みで、egui が唯一の UI 経路（env による経路選択は無い）。

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
Import-Module (Join-Path $PSScriptRoot 'lib/SnotraSmoke.psm1') -Force
# H1（hide 後に results が取り残されない）の判定はここに書かない——`manual-smoke.ps1` と
# **同じ導出**を共有する（#757）。
Import-Module (Join-Path $PSScriptRoot 'lib/SnotraTraceInvariants.psm1') -Force

if (-not (Test-Path $ExePath)) {
  throw "Executable not found: $ExePath"
}

# 検証用プロファイル（#804）。**target/ の下に置く**（visual-check-colors.ps1:54-58 と同じ理由
# ——cargo clean が掃くので新しい後始末機構を足さない。CARGO_TARGET_DIR を設定している環境では
# cargo の掃除対象から外れるが、それは受容する残余である
# ——ADR-config-dir-env-seam-rejected-alternatives.md §4）。
# パスの計算に副作用は無いので guard より前に置いてよい（下の throw が指し先を示せる）。
$profileDir = Join-Path $PSScriptRoot '..\target\smoke-egui\profile'

# results の検証が skip されるなら、ここで落とす（#686 → #804 で**無条件の要求へ格上げ**）。
#
# **skip へ至る経路のうち沈黙するのはこの 1 本だけである**——他は必ず exit≠0 で鳴る:
# 実行ファイル不在 / `hotkey:registered` 未観測 / `egui_show:done` 未観測 / `egui_results:show`
# 未観測 / `ResultsQuery` が A-Z 単字でない（`Get-LetterVk` が throw）。ゆえにこの 1 箇所で
# 沈黙経路を塞げる。**アプリ起動前に判定が確定している**ので、この guard はプロセスを
# 起こさずに落ちる（フォールトインジェクションが実機を触らずに済む）。
#
# **旧 -RequireResults の opt-in を撤去したのは、その前提が偽になったからである**——「ローカルでは
# 索引を制御できないのが普通」を根拠に既定 skip にしていたが、検証用プロファイルを常に seed する
# 以上、索引は常に制御下にある。**これは検出器の削除ではなく格上げである**（ローカルでも赤くなる）。
if ([string]::IsNullOrEmpty($ResultsQuery)) {
  throw @"
results window coverage would be SKIPPED: -ResultsQuery が空である。
  profile: $profileDir
検証用プロファイルは常に seed されるので、既定（"z"）のままなら必ず results 検査が走る。
空文字を明示的に渡したのなら、それは**フォールトインジェクションの注入口**であり、
この throw が期待される結果である（docs/build-commands.md「スモーク運用メモ」）。
"@
}

# results 窓の検証用に、索引に必ず 1 件載るダミーを置く（中身は問わない——indexer は
# 拡張子だけで判定する: snotra-core/src/indexer.rs の matches_extension）。
# 名前は既存の索引と衝突しにくい接頭辞にし、-ResultsQuery 既定の "z" で引けるようにする。
$scanDir = Join-Path $env:TEMP "snotra_smoke_scan"
New-Item -ItemType Directory -Force -Path $scanDir | Out-Null
$dummy = Join-Path $scanDir "zsnotrasmoke.exe"
if (-not (Test-Path $dummy)) { New-Item -ItemType File -Path $dummy | Out-Null }
$scanDirToml = $scanDir -replace '\\', '/'
# 最小の有効 TOML。必須セクションの骨格は共有モジュールが持ち、results 固有の scan だけを
# PathEntries として渡す（#843）。3 本の seed が同型ではないことは維持する。
# [hotkey]/[appearance]/[paths] は #[serde(default)] 無しの必須セクションで、
# 空 TOML は parse 失敗し「破損復旧」経路（stderr 診断 + config.toml.bak 退避 + 復旧バルーン）を
# 毎回踏んでしまう（PR #659 レビューで検出）。値は config.rs の既定と同一
# （hotkey Alt+Q = 本スクリプト既定の -HotkeyVks 18,81 と一致）。
# scan は上の 1 ファイルだけを対象にする（索引は 1 件・ビルドは即座に終わる）。
# **`scan = []` と `[[paths.scan]]` を併記してはならない**——同一キーの再定義で TOML の
# parse が落ち、config 破損復旧経路へ落ちる。`[paths]` を空ヘッダで置き、その下に
# array-of-tables を続ける（`PathsConfig.scan` は #[serde(default)] ゆえ空でも可）。
$pathEntries = @"
[[paths.scan]]
path = "$scanDirToml"
extensions = [".exe"]
include_folders = false
"@
$profile = New-SnotraVerificationProfile -ProfileDir $profileDir -PathEntries $pathEntries
$cfgPath = $profile.ConfigPath
$profileFull = $profile.FullPath
Write-Host "Seeded verification profile: $cfgPath (scan: $scanDir)"

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

# 既存インスタンスは single-instance 転送で smoke を汚すため停止（smoke-startup.ps1 と同じ前提）
Resolve-SnotraExistingProcess -Policy Stop
Start-Sleep -Milliseconds 300

$webviewBefore = @(Get-Process msedgewebview2 -ErrorAction SilentlyContinue).Count

$errPath = Join-Path $env:TEMP "snotra_smoke_egui.err"
$outPath = Join-Path $env:TEMP "snotra_smoke_egui.out"
Remove-Item $errPath, $outPath -Force -ErrorAction SilentlyContinue

# env は共有モジュールが Start-Process の成功・例外の両経路で元へ戻す（#843）。
$proc = Start-SnotraProcess -ConfigDir $profileFull -Trace -FilePath $ExePath `
  -StandardErrorPath $errPath -StandardOutputPath $outPath
$launchedAt = Get-Date

# 失敗時の証拠出力（#690 の follow-up）。
#
# **失敗チャネルは 2 本ある**——`throw`（前提が崩れて続行不能）と `$failures` への
# 蓄積（検査項目の不合格）。証拠の出力は後者の中にしか無かったため、`throw` は
# `finally`（プロセス kill だけ）を通って**何の手掛かりも残さずに**スクリプトを終えていた。
# 実際 CI で `hotkey:registered` 未観測が出たとき、ログには seed から例外までの 18 秒の
# 空白しか無く、「アプリが遅れた」のか「起動せず死んだ」のかを**区別できなかった**。
# ゆえに両チャネルをこの 1 関数へ合流させる。
#
# プロセスの生死を先に出すのが要点である: trace 0 行のとき「観測できなかった」と
# 「イベントが出なかった」は別物で、前者ならプロセス状態が切り分ける。
function Show-FailureEvidence {
  param(
    [string]$Path,
    [System.Diagnostics.Process]$Proc,
    [string]$Context,
    [datetime]$LaunchedAt,
    [int]$PostMortemWaitMs = 0
  )
  Write-Host ""
  Write-Host "--- 失敗時の証拠（$Context）---" -ForegroundColor Yellow
  if ($PSBoundParameters.ContainsKey('LaunchedAt')) {
    Write-Host ("起動からの経過: {0:N0} ms" -f ((Get-Date) - $LaunchedAt).TotalMilliseconds)
  }
  $alive = $false
  if ($null -ne $Proc) {
    if ($Proc.HasExited) {
      Write-Host ("プロセス: 既に終了 (exit code $($Proc.ExitCode)) — 起動途中で落ちた疑い") -ForegroundColor Red
    } else {
      $alive = $true
      Write-Host "プロセス: 生存中 — 起動はしている（クラッシュではなく未到達/遅延）"
    }
  } else {
    Write-Host "プロセス: 本スクリプトが既に終了させた後（生死は判定材料にならない）"
  }
  if (-not (Test-Path $Path)) {
    Write-Host "trace ファイルが存在しない: $Path" -ForegroundColor Red
    return
  }
  $all = @(Get-Content -Path $Path -ErrorAction SilentlyContinue)
  Write-Host ("trace 行数: {0}" -f $all.Count)

  # **窓を広げる前に、まず遅延を測る。**「なぜか通った」で終わらせないため。
  # 0 行かつプロセス生存のときだけ、追加で待って最初の 1 行が出るかを見る。
  # 出れば「遅延」（何 ms かが分かる＝観測窓を決める根拠になる）、出なければ
  # 「未到達/ハング」で、両者は対処が違う。失敗時だけ走るので通常時間には効かない。
  if ($all.Count -eq 0 -and $alive -and $PostMortemWaitMs -gt 0) {
    Write-Host ("0 行。事後観測に入る（最大 {0:N0} ms・最初の 1 行が出るかを測る）..." -f $PostMortemWaitMs)
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    while ($sw.Elapsed.TotalMilliseconds -lt $PostMortemWaitMs) {
      Start-Sleep -Milliseconds 250
      $all = @(Get-Content -Path $Path -ErrorAction SilentlyContinue)
      if ($all.Count -gt 0) { break }
      if ($Proc.HasExited) {
        Write-Host ("事後観測中にプロセスが終了 (exit code $($Proc.ExitCode))") -ForegroundColor Red
        break
      }
    }
    if ($all.Count -gt 0) {
      $total = ((Get-Date) - $LaunchedAt).TotalMilliseconds
      Write-Host ("**遅延**: 最初の trace は起動から約 {0:N0} ms 後に出た（ハングではない）。観測窓の根拠にできる。" -f $total) -ForegroundColor Yellow
    } else {
      Write-Host ("**未到達**: 追加 {0:N0} ms 待っても 1 行も出ない。窓を広げても解決しない。" -f $PostMortemWaitMs) -ForegroundColor Red
    }
  }

  if ($all.Count -eq 0) {
    Write-Host "**0 行** — アプリが 1 行も出していない。起動前に落ちたか SNOTRA_TRACE が効いていない。" -ForegroundColor Red
    return
  }
  Write-Host "--- trace tail ---"
  $all | Select-Object -Last 40
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
  # VK へ変換すると、core の意味 parser と platform の Win32 変換が PowerShell 側にも写り、
  # ドリフトする）。-HotkeyVks が明示指定されたときだけそちらを使う。
  # $hotkeySource / $vksLabel は「実際に注入する VK 列」を失敗メッセージへも一致させるための
  # 単一の出所（#671 サイクル PR A レビュー指摘 2）。$HotkeyVks（既定値・未指定でも常に非空）を
  # そのままメッセージへ出すと、trace 由来の VK で失敗したときに実際と異なる値を報告してしまう。
  if ($PSBoundParameters.ContainsKey('HotkeyVks')) {
    $hotkeySource = "explicit"
    $vks = @($HotkeyVks -split ',' | ForEach-Object { [byte]([int]$_.Trim()) })
    Write-Host "Hotkey VKs (explicit): $($vks -join ',')"
  } else {
    $hotkeySource = "from trace"
    # **この 1 件だけ別予算を使う**（$StartupObserveTimeoutMs）。cold start を含むのはここだけで、
    # 以降の観測（show/hide/results）はアプリが温まった後だから ObserveTimeoutMs のままでよい。
    # 一律に広げると、本来速いはずの検査の失敗検出まで鈍る。
    $hkEvent = Wait-SnotraTraceEvent -Path $errPath -EventName "hotkey:registered" -TimeoutMs $StartupObserveTimeoutMs
    if ($null -eq $hkEvent) {
      throw "hotkey:registered trace not observed within ${StartupObserveTimeoutMs}ms — cannot determine which keys to inject"
    }
    $hk = $hkEvent.data
    if (-not $hk.ok) {
      throw "hotkey registration failed in the app (modifier=$($hk.modifier) key=$($hk.key)) — smoke cannot proceed"
    }
    $vks = @($hk.vks | ForEach-Object { [byte][int]$_ })
    # **成功時にもレイテンシを出す**（#690 follow-up）。予算を広げただけだと、アプリの起動が
    # 遅くなっても予算内に収まる限り緑のままで**気づけない**。数字を毎回出せば、退行は
    # 予算に触れる前に人が読める（`event_count` を成功時に出すのと同じ考え方）。
    $startupMs = ((Get-Date) - $launchedAt).TotalMilliseconds
    Write-Host ("Hotkey VKs (from trace): $($vks -join ',') (modifier=$($hk.modifier) key=$($hk.key))")
    # **固定待機を併記する**——この値は「アプリが何 ms で準備できたか」ではなく
    # 「観測できたのが何 ms 後か」であり、下限が StartupWaitMs で頭打ちになっている。
    # 併記しないと 4,052ms を「起動に 4 秒かかった」と読まれる（実際の trace は 118ms）。
    Write-Host ("起動→hotkey:registered 観測: {0:N0} ms（うち固定待機 {1:N0} ms・予算 {2:N0} ms）" -f `
      $startupMs, $StartupWaitMs, $StartupObserveTimeoutMs)
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
    Send-SnotraKeyChord -VirtualKeys $vks
    if ($null -ne (Wait-SnotraTraceEvent -Path $errPath -EventName "egui_show:done" -TimeoutMs $ObserveTimeoutMs)) {
      $shown = $true
      break
    }
  }
  if (-not $shown) {
    $failures += "egui_show:done not observed within ${ObserveTimeoutMs}ms x2 after hotkey ($vksLabel, $hotkeySource)"
  }

  # **seed 健全性と first-run の判定はここで行う**（#804）。
  #
  # **`Start-Sleep $StartupWaitMs` の直後では早すぎる**（実測）: `-RedirectStandardError` は
  # プロセス起動時に**空のファイルを作る**ので `Test-Path` は真になり、本体がまだ 1 行も
  # 書いていない段階では `Select-String` が 0 件を返して**沈黙のまま合格する**。CI では
  # 「起動後 12,000ms 経っても trace 0 行」を 3 回実測しており、固定待機 4,000ms はその保証に
  # ならない。上の hotkey 観測（予算 $StartupObserveTimeoutMs）と `egui_show:done` 観測を
  # 通過した時点なら本体は必ず出力しているので、**ここが最も早い健全な観測点である**。
  # **results 検査より前**であることは保つ——不変条件 4 が要求するのはその順序であって、
  # 位置そのものではない。
  #
  # **ログが空なら赤にする**——「観測できなかった」を合格と読ませない。
  if (@(Get-Content $errPath -ErrorAction SilentlyContinue).Count -eq 0) {
    throw "本体が stderr へ 1 行も出していません（$errPath）。seed が読めたかを確かめられません。"
  }

  # **seed が読めたことを肯定的に確かめる**（不変条件 4）。seed が parse に失敗すると既定 config で
  # 起動し、索引が空になって `egui_results:show` が出ない——**症状は「results 検査の失敗」と同じだが
  # 原因が違う**。ここで先に落とすことで赤の理由が正確になる。
  # **config.toml.bak の不在を根拠にしない**——退避は best-effort で、fs::rename が失敗すれば parse
  # 失敗でも .bak は現れない（config.rs の backup_invalid）。`[config] ` 付きの eprintln は読み込み
  # 失敗の全 arm に在るので、これが健全な観測点である。**ただし「成功時には出ない」は無条件には
  # 真でない**——duplicate instant command と invalid hotkey fallback は parse 成功後に出る。
  # この seed は instant_commands 無し・妥当な Alt+Q ゆえどちらも踏まないが、
  # **seed を変えるときはこの前提を確かめること**。
  # visual-check-colors.ps1:143-159 の Test-SeedHealth と同型だが、共有ヘルパーにはしない（#843）。
  $configDiag = @(Select-String -Path $errPath -SimpleMatch '[config] ')
  if ($configDiag.Count -gt 0) {
    throw ("seed した config が読めていません（results 検査の失敗ではありません）:`n" +
      (($configDiag | ForEach-Object { "  " + $_.Line }) -join "`n"))
  }

  # **first-run 経路を踏んでいないことを肯定的に確かめる**（不変条件 6）。使い捨てプロファイルを
  # 使う以上、config.toml が無ければ `setup_first_run` が `launch_settings_process(--first-run)` を
  # 呼び、設定 GUI がフォーカスを奪う。
  # **上の `[config] ` 検査ではこれを捕まえられない**——config.toml が「存在しない」分岐では読み込み
  # 失敗の eprintln が出ないからである（だから別の検査が要る）。
  # **失敗を表すイベント名の汎用パターンにも頼れない**——実際のイベント名は :not_found / :spawned /
  # :already_running / :exited で（commands/window.rs:53,74,87,123）どれも :error で終わらない。
  $firstRunEvents = @(Select-String -Path $errPath -SimpleMatch 'cmd:launch_settings_process:')
  if ($firstRunEvents.Count -gt 0) {
    throw ("first-run 経路を踏みました（seed が起動より前に置かれていない）:`n" +
      (($firstRunEvents | ForEach-Object { "  " + $_.Line }) -join "`n"))
  }

  # results 窓の検証（#671/#673 サイクル PR A）。**#804 以降は常に走る**——検証用プロファイルを
  # 常に seed するので `$ResultsQuery` は起動前の guard で非空が保証されている。下の条件に
  # `IsNullOrEmpty` が残っているのは防御であって、skip の経路が生きているという意味ではない。
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
        Send-SnotraKey -VirtualKey $VK_BACK
        Start-Sleep -Milliseconds 50
        Send-SnotraKey -VirtualKey $VK_BACK -Up
        Start-Sleep -Milliseconds 50
      }
      Send-SnotraKey -VirtualKey $queryVk
      Start-Sleep -Milliseconds 50
      Send-SnotraKey -VirtualKey $queryVk -Up
      if ($null -ne (Wait-SnotraTraceEvent -Path $errPath -EventName "egui_results:show" -TimeoutMs $ObserveTimeoutMs)) {
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
    Send-SnotraKey -VirtualKey $VK_ESCAPE
    Start-Sleep -Milliseconds 50
    Send-SnotraKey -VirtualKey $VK_ESCAPE -Up

    if ($null -eq (Wait-SnotraTraceEvent -Path $errPath -EventName "egui_hide:done" -TimeoutMs $ObserveTimeoutMs)) {
      $failures += "egui_hide:done not observed within ${ObserveTimeoutMs}ms after Escape"
    }

    # main の hide は hide_egui_main が results も同時に隠す（#646 PR2 決定 6）。
    # show 側を検査したときだけ対で検査する（対称ペア・/symmetric-check）。
    if ($resultsChecked -and
        $null -eq (Wait-SnotraTraceEvent -Path $errPath -EventName "egui_results:hide" -TimeoutMs $ObserveTimeoutMs)) {
      $failures += "egui_results:hide not observed within ${ObserveTimeoutMs}ms after Escape"
    }

    # #671 PR A′: hide 後に results だけが最前面に取り残されないこと（H1・orphan 検出）。
    # **presence 検査ではこの事故を素通りする**——orphan でも egui_results:hide は出るため。
    # orphan は必ず「hide 以降の余分な egui_results:show」として現れる（main が hidden でも
    # repaint 要求は飛ぶ: config-applied / indexing-* / updater 完了の wake_main）。
    #
    # **判定の正本は `lib/SnotraTraceInvariants.psm1` の H1 である**（#757）。以前はここに
    # 生行の正規表現で書かれており、`manual-smoke.ps1` に同じ不変条件を足すと導出が 2 か所に
    # なるので移した。オーケストレーション（静定待ち・ゲート・失敗文言）はここに残る。
    # smoke の実行は最後まで hidden で終わるため hide 窓は閉じない——**違反は窓が閉じて
    # いなくても FAIL、無違反は SKIP** という H1 の非対称がこの経路を成立させている。
    if ($resultsChecked -and $failures.Count -eq 0) {
      Start-Sleep -Milliseconds 1500
      # 「捨てた行」の数え方は `Read-SnotraTraceSnapshot` が持つ（`manual-smoke.ps1` と同じ
      # 規則を共有する・`/dry-check`）——非 trace の診断行を数えると毎回 degrade する。
      $snapshot = Read-SnotraTraceSnapshot -Path $errPath
      $dropped = $snapshot.Dropped
      $invariants = Test-SnotraTraceInvariants -Events $snapshot.Events -Sections @() -DroppedLineCount $dropped

      # **H1 だけを読むと「判定できなかった」が「正常」と同じ値になる**（code-review C1）。
      # smoke は最後まで hidden で終わるため H1 の正常値は SKIP であり、trace 不在・判定器の
      # 例外・イベント名のドリフトがすべて緑と見分けられなかった。3 つとも読み、さらに
      # **判定器が実際に走った肯定的な証拠**を要求する。
      if ($invariants.JudgeFailed) {
        $reasons = @($invariants.Unjudgeable | ForEach-Object { $_.Reason })
        $failures += "trace invariant judge aborted: $($reasons -join '; ')"
      }
      # 不変条件ごとに特殊分岐を置かない——H1 だけ別の文言にしていた理由は歴史的な英文の
      # 保存だったが、その文字列を参照する箇所はリポジトリ内に無い。判定の詳細は判定器が持つ。
      foreach ($invariant in (Get-SnotraTraceInvariantNames)) {
        if ($invariants.Overall[$invariant] -ne 'FAIL') { continue }
        $detail = @($invariants.Violations | Where-Object { $_.Invariant -eq $invariant })
        $failures += "trace invariant $invariant violated ($($detail.Count) x): $(($detail | ForEach-Object { $_.Message }) -join '; ')"
      }
      # **肯定的な証拠**: ここへ来る時点で `Wait-SnotraTraceEvent` が show / results:show / hide を
      # 観測済みなので、判定器が 1 件も `egui_results:show` を見ていないなら判定器側の欠陥である
      # （trace を読めていない・イベント名がドリフトした）。**沈黙を合格と読ませない**。
      # 数えるのは判定器（`Observed`）である——ここで生イベントから数え直すと、イベント名の
      # 写しがこちらへ増え、名前がドリフトしたとき当の assertion が黙る。
      if ($invariants.Observed.ResultsShow -eq 0) {
        $failures += "trace invariant judge saw 0 x egui_results:show although Wait-SnotraTraceEvent observed it; the detector did not actually run"
      }
      # **捨てた行を failure にはしない**（#757 D6/D7）。旧判定は生行を見ていたので今日は無害に
      # 済んでおり、ここで赤にすると不変条件と無関係な理由で CI が落ちる新しい経路を作る。
      # 証拠としてだけ残す（H1 の PASS はモジュール側で SKIP へ落ちている）。
      if ($dropped -gt 0) {
        Write-Host "  [#757] parse できなかった trace 行が $dropped 件ある（H1 の PASS は SKIP へ落ちている）" -ForegroundColor Yellow
      }
    }
  }
} catch {
  # **`finally` より前に走る**ので、ここではまだプロセスが生きている＝生死を証拠にできる。
  # 出したら握り潰さずに再送出する（exit code は従来どおり非 0 のまま）。
  Show-FailureEvidence -Path $errPath -Proc $proc -Context "throw: $($_.Exception.Message)" `
    -LaunchedAt $launchedAt -PostMortemWaitMs $PostMortemWaitMs
  throw
} finally {
  if (-not $proc.HasExited) {
    Stop-Process -Id $proc.Id -Force
  }
}

# **シナリオ 2 の起動前に、シナリオ 1 のプロセスの終了を待つ**（#755/#801 是正 B）。
# `Stop-Process -Force` は終了を待たない。`tauri_plugin_single_instance` が登録されているため、
# 先発がまだ生きたまま後発を起動すると、後発は先発へ通知して即終了し——trace が 1 行も
# 書かれないまま `hotkey:registered` の待ちが予算を使い切ってから throw する（間欠的な赤）。
$scenario1ExitWaitMs = 5000
if (-not $proc.WaitForExit($scenario1ExitWaitMs)) {
  throw ("シナリオ 1 のプロセス（pid=$($proc.Id)）が ${scenario1ExitWaitMs}ms 以内に終了しませんでした。" +
    "single-instance 転送によりシナリオ 2 が沈黙する恐れがあるため中断します。")
}

# ---- シナリオ 2: toast ありで 2 回目の show の高さが保たれるか（#755 / #801）----
#
# **既定プロファイル（toast なし）の高さ断言は両 issue を 1 件も捕まえない**——toast も status も
# 無い状態では main は常にバー高であり、そこは一度も壊れていない。捕まえるには「toast あり」かつ
# 「2 回目の show」が要る: #755 は 2 回目で 43px へ固着し、#801 は 1 フレームの伸びとして出る。
# **両者は 1 回の show では排他である**（2026-08-04 実測・#878 のコメント）。
#
# `SNOTRA_EGUI_FAKE_UPDATE` は起動時に読まれるためシナリオ 1 に相乗りできず、起動が 1 回増える。
#
# 高さは **DWM の実表示矩形**（`DWMWA_EXTENDED_FRAME_BOUNDS` = 属性 9）で読む。`GetWindowRect` は
# 不可視のリサイズ枠を含み、2 行の高さが 1 行の 2 倍にならない（実測: 118 / 64 に対し DWM は 110 / 56）。
$toastProfileDir = Join-Path $PSScriptRoot '..\target\smoke-egui\profile-toast'
# **font_size / bar_padding を明示 seed する**——既定値が変わってもこの検査の期待値が黙って動かない。
$toastProfile = New-SnotraVerificationProfile -ProfileDir $toastProfileDir -ShowIcons $false `
  -AdditionalSections "[visual]`r`nfont_size = 15`r`nbar_padding = 28"
$expectedBarLogical = 15 + 28   # layout::Metrics::from_config: bar_height = font_size + bar_padding
# **bar + toast/status の 2 行分**（是正 E の期待値。toast 行は bar と同じメトリクスで積まれる
# 前提——この前提が崩れたら両方の期待値を一緒に見直すこと）。
$expectedShowHeightLogical = $expectedBarLogical * 2
$toastErrPath = Join-Path $toastProfileDir 'stderr.log'
Remove-Item -LiteralPath $toastErrPath -Force -ErrorAction SilentlyContinue

function Get-MainWindowDwmSize {
  param([IntPtr]$Handle)
  $r = New-Object SnotraSmokeInterop.Native+RECT
  if ([SnotraSmokeInterop.Native]::DwmGetWindowAttribute($Handle, 9, [ref]$r, 16) -ne 0) {
    throw 'DwmGetWindowAttribute(EXTENDED_FRAME_BOUNDS) に失敗しました。'
  }
  [pscustomobject]@{ W = $r.Right - $r.Left; H = $r.Bottom - $r.Top }
}

# **可視区間に `egui_main:height_mismatch` が現れないことを判定する（是正 D）。** #801 は
# 「行が変わっていないのに高さが変わった」ときにだけこの trace が出る——表示直後の
# 1 フレーム（約 16ms）の中で起きるため、`Wait-SnotraWindow`（PollMs 200）や 100ms 間隔の
# サンプリングでは過渡をほぼ観測できない（旧 `min -ne max` 断言が原理的に無力だった理由）。
# **判定は presence（区間内に 1 件でも出たか）で行う。** `seq` は全 trace 事象を貫く単一の
# AtomicU64（`src-tauri/src/trace.rs`）なので、区間の境界は show/hide の `seq` で切れる
# （`SnotraTraceInvariants.psm1` の `Get-SnotraTraceMarker` と同じ考え方）。`-BeforeSeq` を
# 省くと「ここまでに読めた分すべて」が上限になる（最終 round・次の hide が無い場合）。
function Test-SnotraNoHeightMismatch {
  param(
    [Parameter(Mandatory)] [string]$Path,
    [Parameter(Mandatory)] [long]$AfterSeq,
    [long]$BeforeSeq
  )
  $events = (Read-SnotraTraceSnapshot -Path $Path).Events
  return @($events | Where-Object {
    $_.event -eq 'egui_main:height_mismatch' -and [long]$_.seq -gt $AfterSeq -and
    (-not $PSBoundParameters.ContainsKey('BeforeSeq') -or [long]$_.seq -lt $BeforeSeq)
  })
}

# **証拠出力へ渡す起点は自前で取る**——`$launchedAt` はシナリオ 1 の起動時刻であり、
# ここで使うと「起動からの経過」が数十秒ずれて手掛かりを誤らせる。
$toastLaunchedAt = Get-Date
# **高さ断言の不合格は throw ではなく `$failures` を通る**（下の round ループ）ため、
# この scriptの`catch`（$toastErrPath を渡す）を経由せず、集計失敗ブロック（末尾）まで
# 静かに素通りする。集計失敗ブロックが証拠を出すログをシナリオ 2 のものへ切り替えられるよう、
# ここで発生源を憶えておく（code-review 指摘）。
$scenario2Failed = $false
$toastProc = Start-SnotraProcess -ConfigDir $toastProfile.FullPath -FilePath $ExePath -Trace `
  -StandardErrorPath $toastErrPath -ExtraVariables @{ SNOTRA_EGUI_FAKE_UPDATE = '1' }
try {
  $hk2 = Wait-SnotraTraceEvent -Path $toastErrPath -EventName 'hotkey:registered' -TimeoutMs $StartupObserveTimeoutMs
  if ($null -eq $hk2) { throw 'シナリオ 2: hotkey:registered を観測できませんでした' }
  $vks2 = @($hk2.data.vks | ForEach-Object { [byte][int]$_ })

  foreach ($round in 1..2) {
    # **presence ではなく increment で待つ**（是正 A）。round 2 の待ちが round 1 の行へ
    # 即座に一致して戻ると、round 1 の hide が効かなかった場合でも round 2 は可視のままの
    # 窓をトグルし、`Wait-SnotraWindow` は `IsWindowVisible` しか見ないので通ってしまい、
    # #755/#801 が現れる局面を一度も観測しないまま緑になる。打鍵の前に件数を数え、
    # `件数 + 1` を待つことで自分が引き起こした新しい 1 件だけを待つ。
    $showBaseline = Get-SnotraTraceEventCount -Path $toastErrPath -EventName 'egui_show:done'
    Send-SnotraKeyChord -VirtualKeys $vks2
    $showEvent = Wait-SnotraTraceEvent -Path $toastErrPath -EventName 'egui_show:done' `
      -TimeoutMs $ObserveTimeoutMs -MinMatchCount ($showBaseline + 1)
    if ($null -eq $showEvent) {
      throw "シナリオ 2: $round 回目の egui_show:done を観測できませんでした"
    }
    $hwnd2 = Wait-SnotraWindow -Title 'Snotra' -Process $toastProc -TimeoutMs 5000

    # **`egui_show:done` の payload の height を断言する**（是正 E）。DWM 実測（下）だけでは、
    # サンプリング区間中 `AppState.indexing` が真なら toast が 1 つも無くても bar+status の
    # 2 行で同じ高さになり得るため、単独では「toast が実際に出たか」の証拠にならない。
    # この値は DWM のようにスクリーン矩形を後から読むのではなく、本体がその show で
    # 適用した高さそのものとして報告する値であり、DPI 換算なしの論理 px である。
    if ($showEvent.data.height -ne $expectedShowHeightLogical) {
      $failures += ("toast ありの show #{0}: egui_show:done の height が bar+toast 2 行分（{1} 論理 px）ではありません（実測 {2}・#755）" -f `
        $round, $expectedShowHeightLogical, $showEvent.data.height)
      $scenario2Failed = $true
    }

    # **高さの実測（DWM 矩形）は 1 点で残す**——「`set_size` が OS へ実際に効いたか」を見る
    # 別の観点であり、捨てない。#801 の 1 フレームの伸びの検出は上の payload 断言と下の
    # `egui_main:height_mismatch` 不在断言が担うため、旧 1 秒サンプリング + `min -ne max` は
    # 撤去した（#801 の過渡・約 16ms を 100ms 間隔のポーリングでは原理的に観測できなかった）。
    Start-Sleep -Milliseconds 300
    $size2 = Get-MainWindowDwmSize -Handle $hwnd2

    # 論理 px への換算は **config が幅を固定していることを較正点にする**（DPI API を別に読まない）。
    # seed の window_width は 600（New-SnotraVerificationProfile の既定）。
    $scale = $size2.W / 600.0
    $barPx = $expectedBarLogical * $scale

    # **片側の断言にする**——DWM 矩形には環境依存の数 px のずれが乗る（実測 +2px）。
    # 判別したいのは「バー 1 行（43 論理 px）」と「バー + toast（86 論理 px）」で、
    # 差は 43 論理 px ある。1.5 倍を閾値に置けば、ずれに影響されず両者を分けられる。
    if ($size2.H -lt 1.5 * $barPx) {
      $failures += ("toast ありの show #{0}: 窓が toast 行を含む高さになっていない（DWM 高さ {1}px / バー 1 行 ≒ {2:N0}px・#755）" -f $round, $size2.H, $barPx)
      $scenario2Failed = $true
    }
    Write-Host ("toast ありの show #{0}: DWM {1}x{2}・payload height {3}（バー 1 行 ≒ {4:N0}px）" -f `
      $round, $size2.W, $size2.H, $showEvent.data.height, $barPx)

    $hideEvent = $null
    if ($round -lt 2) {
      # マーカーは打鍵の**前**に打つ——後に打つと、打鍵が引き起こした 1 件が自分自身の
      # ベースラインへ紛れ込み、`件数 + 1` が実際には「もう 1 件」を要求してしまう
      # （`Get-SnotraTraceEventCount` のコメントと同じ罠）。
      $hideBaseline = Get-SnotraTraceEventCount -Path $toastErrPath -EventName 'egui_hide:done'
      Send-SnotraKey -VirtualKey 27
      Send-SnotraKey -VirtualKey 27 -Up
      $hideEvent = Wait-SnotraTraceEvent -Path $toastErrPath -EventName 'egui_hide:done' `
        -TimeoutMs $ObserveTimeoutMs -MinMatchCount ($hideBaseline + 1)
      if ($null -eq $hideEvent) {
        throw "シナリオ 2: $round 回目の egui_hide:done を観測できませんでした"
      }
      Start-Sleep -Milliseconds 400
    } else {
      # 最終 round には次の hide が無いため、区間の終端を「ここまでの観測」にする。
      # #801 の不一致は表示直後の 1 フレームで起きるので、show 観測から DWM 実測までの
      # 経過で大半は足りるが、遅延到着分も拾えるよう軽く待ってから下の判定へ入る。
      Start-Sleep -Milliseconds 300
    }

    # 区間は「この round の egui_show:done の後、次の egui_hide:done（最終 round は
    # ここまでの観測）まで」（是正 D）。
    $mismatches = if ($null -ne $hideEvent) {
      Test-SnotraNoHeightMismatch -Path $toastErrPath -AfterSeq ([long]$showEvent.seq) -BeforeSeq ([long]$hideEvent.seq)
    } else {
      Test-SnotraNoHeightMismatch -Path $toastErrPath -AfterSeq ([long]$showEvent.seq)
    }
    if ($mismatches.Count -gt 0) {
      $failures += ("toast ありの show #{0}: 可視区間中に egui_main:height_mismatch が {1} 件出た（show_h={2} / frame_h={3}・#801）" -f `
        $round, $mismatches.Count, $mismatches[0].data.show_h, $mismatches[0].data.frame_h)
      $scenario2Failed = $true
    }
  }
} catch {
  Show-FailureEvidence -Path $toastErrPath -Proc $toastProc -Context "シナリオ 2 throw: $($_.Exception.Message)" `
    -LaunchedAt $toastLaunchedAt -PostMortemWaitMs $PostMortemWaitMs
  throw
} finally {
  if (-not $toastProc.HasExited) { Stop-Process -Id $toastProc.Id -Force }
}

# **env が効いたことの肯定的証拠**（#804・不変条件 1）。効いていなければ本体は実 config を読み、
# 実プロファイルへ書くので、ここには seed した config.toml しか残らない。「env が届いていない」と
# 「検査対象が出なかった」を切り分けるのはこの 1 行である
# （visual-check-colors.ps1:292-304 と同型。実測で出るのは index.bin で、索引 0 件でも書かれる）。
$generated = @(Get-ChildItem -Path $profileDir -Filter '*.bin' -ErrorAction SilentlyContinue)
if ($generated.Count -eq 0) {
  $failures += "SNOTRA_CONFIG_DIR が効いていない: 検証用プロファイルに *.bin が 1 件も生成されていません ($profileFull)"
} else {
  Write-Host "プロファイルへの書き込みを確認: $($generated.Name -join ', ')（SNOTRA_CONFIG_DIR は効いています）"
}

if ($failures.Count -gt 0) {
  Write-Host ""
  Write-Host "egui smoke failed:" -ForegroundColor Red
  foreach ($f in $failures) {
    Write-Host " - $f" -ForegroundColor Red
  }
  # 証拠の出力は throw 経路と同じ関数へ寄せる（片方だけ計装される状態に戻さないため）。
  # ここへ来る時点で finally が既にプロセスを終了させているので、生死は判定材料にならない
  # ——$null を渡してその旨を明示する（誤った手掛かりを出さない）。
  # ここへ来る時点でプロセスは終了済みゆえ事後観測はしない（生存が前提の測定である）。
  #
  # **シナリオ 2 の失敗も含むなら、その trace ログでも証拠を出す**（code-review 指摘）。
  # シナリオ 2 の高さ断言は throw ではなく $failures を通るため、シナリオ 2 自身の catch
  # （$toastErrPath を渡す）を経由せずここまで来る。ここで $errPath だけを見ると、
  # シナリオ 2 の失敗（本検出器が主眼とする経路）に対して無関係なシナリオ 1 のログを
  # 指してしまう。$scenario2Failed が立っているときは両方の証拠を出す
  # （シナリオ 1 側の既存出力は変えない）。
  if ($scenario2Failed) {
    Show-FailureEvidence -Path $toastErrPath -Proc $null -Context "検査項目の不合格（シナリオ 2: toast あり）" -LaunchedAt $toastLaunchedAt
  }
  Show-FailureEvidence -Path $errPath -Proc $null -Context "検査項目の不合格" -LaunchedAt $launchedAt
  exit 1
}

Write-Host ""
if (-not $resultsChecked) {
  # **到達不能な経路である**（#804）——`$ResultsQuery` は起動前の guard で非空が保証され、
  # 失敗 0 件でここまで来たなら results 検査は走っている。それでも到達したら「検査が走らない
  # まま緑」であり、それは #686 が塞いだ当の沈黙経路なので、黄色い NOTE ではなく赤で落とす。
  Write-Host "egui smoke failed: results 検査が走らないまま合格しかけました（到達不能なはずの経路）。" -ForegroundColor Red
  exit 1
}
Write-Host "egui smoke passed (show/hide + results show/hide observed, webview delta 0)." -ForegroundColor Green
