param(
  [int]$Iterations = 5,
  [int]$WaitMs = 1800,
  # 最初の trace 1 行が出るまでの予算（#690 follow-up）。これを過ぎたら諦めて観測窓へ進み、
  # 結果として trace 0 件になれば失敗する。**$WaitMs とは役割が別**——こちらは起動を待つ
  # 時間、$WaitMs は最初の trace 以降にイベントを集める窓。実測分散（0.6s〜8s超）に対する
  # 余裕として 12s を既定にする。
  [int]$FirstTraceTimeoutMs = 12000,
  # 空なら debug 本体を導く（#1179）。**理由の正本は `Resolve-SnotraCargoExecutable` の doc**。
  [string]$ExePath = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
Import-Module (Join-Path $PSScriptRoot 'lib/SnotraSmoke.psm1') -Force

if ($Iterations -lt 1) {
  throw "Iterations must be >= 1"
}
if ($WaitMs -lt 200) {
  throw "WaitMs must be >= 200"
}
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $ExePath) {
  $ExePath = Resolve-SnotraCargoExecutable -RepositoryRoot $repoRoot -Profile debug
}
if (-not (Test-Path -LiteralPath $ExePath)) {
  # **復旧手順を文言に持たせる**（#1179）。既定が自分のコピーを指すようになったぶん、worktree では
  # 「本体が無い」が起きやすくなった——以前は隣のコピーを拾えていたのだから。
  throw "Executable not found: $ExePath（先に cargo build -p snotra）"
}
$ExePath = (Resolve-Path -LiteralPath $ExePath).Path   # 出力へ載せる形（理由は上の doc）
Write-Host "本体: $ExePath"

# Two-window architecture (main + results, #646 PR2): startup smoke only exercises
# main show/hide timing; the results window is driven by main's update() and has
# no independent lifecycle to verify here.
$summaries = @()
$failures = @()

# 検証用プロファイル（#804）。**実ユーザーの %APPDATA%\Snotra は読みも書きもしない**——
# 5 起動が実 config を作る/汚すことが #804 の元凶だった。target/ の下に置くのは
# visual-check-colors.ps1:54-58 と同じ理由（cargo clean が掃くので新しい後始末機構を足さない。
# CARGO_TARGET_DIR 環境で掃除対象から外れるのは受容する残余
# ——ADR-config-dir-env-seam-rejected-alternatives.md §4）。
$profileDir = Join-Path $PSScriptRoot '..\target\smoke-startup\profile'

# **ループの前に 1 回だけ seed し、5 起動で共有する**（#804・不変条件 5）。毎回作り直すと 5 回
# すべてが first-run になり、**いま CI が測っているもの（first-run でない起動）とは別のものを
# 測り始める**。カバレッジの変更は本 issue の目的ではない。
#
# **索引 0 件の最小 TOML**。config.toml を置く共通の理由は New-SnotraVerificationProfile の
# 上のコメントが正本で、startup 固有の帰結は「first-run になると探索パスのシードが撒かれ、
# 索引 0 件でなくなる」ことである。[general] は省略する——既定値がそのまま望ましい。
# **掃除と共通セクションの骨格は共有モジュールが所有する**（#843）。startup 固有の意味論は
# PathEntries を渡さない「索引 0 件」と、ループ前に 1 回だけ作って 5 起動で共有することである。
$profile = New-SnotraVerificationProfile -ProfileDir $profileDir
$profileFull = $profile.FullPath
Write-Host "検証用プロファイル: $profileFull（実ユーザーの config には触れません）"

for ($run = 1; $run -le $Iterations; $run++) {
  Resolve-SnotraExistingProcess -Policy Stop

  $errPath = Join-Path $env:TEMP ("snotra_smoke_startup_{0}.err" -f $run)
  $outPath = Join-Path $env:TEMP ("snotra_smoke_startup_{0}.out" -f $run)
  if (Test-Path $errPath) { Remove-Item $errPath -Force }
  if (Test-Path $outPath) { Remove-Item $outPath -Force }

  # 2 つの env は共有モジュールが Start-Process の成功・例外の両経路で元へ戻す。
  $proc = Start-SnotraProcess -ConfigDir $profileFull -Trace -FilePath $ExePath `
    -StandardErrorPath $errPath -StandardOutputPath $outPath

  # **最初の trace が出るまで待ってから、観測窓 $WaitMs を開く**（#690 follow-up）。
  #
  # 旧実装は固定 $WaitMs（1,800ms）だけ待って打ち切っていた。しかし同一 runner・同一
  # バイナリで最初の trace までが **0.6s / 5.2s / 8s超** と大きくばらつくことを実測して
  # おり、遅い側に振れた起動は**丸ごと無音**のまま観測を終えていた（5 回中 3 回が
  # trace 0 件・それでも不在だけを見る検査は自明に成立するので緑だった）。
  #
  # 固定待機を一律に伸ばすと 5 起動 × 予算が常時かかる。最初の 1 行を待つ形なら、
  # 速い起動は速いまま・遅い起動だけ待つ。$WaitMs は**最初の trace 以降**の観測窓として
  # 残す——ここを縮めると後続の first-run イベントを取りこぼし、検査が痩せる。
  # **待つのは「最初の行」ではなく「最初の `[trace]` 行」である**（#786）。行の存在だけを
  # 見ると、アプリが最初に stderr へ出す `[index-load] ...`（非 trace の診断行）で抜けてしまい、
  # 観測窓が**まだ 1 件も trace が出ていない時点**で開く——#690 が塞いだはずの穴が同じ
  # 関数の中で開いたままだった。**キャッシュが温まっているほど `[index-load]` が早く出るので、
  # 開発機ほど早く失敗する**（CI と手元で挙動が分かれる）。
  #
  # 判定の形は `Wait-SnotraTraceCondition` が単独で持つ（#872）——期限跨ぎの取りこぼしと
  # 読み取り失敗の沈黙もそこで塞がっている。ここに写しを置かない。
  $swFirst = [System.Diagnostics.Stopwatch]::StartNew()
  $firstEvent = Wait-SnotraTraceCondition -Path $errPath -Predicate { $true } `
    -TimeoutMs $FirstTraceTimeoutMs -PollMs 100 -AbortIfExited $proc -Description '最初の trace'
  $firstTraceMs = if ($null -eq $firstEvent) { $null } else { [int]$swFirst.Elapsed.TotalMilliseconds }

  Start-Sleep -Milliseconds $WaitMs
  if (-not $proc.HasExited) {
    Stop-Process -Id $proc.Id -Force
  }
  # **この 120ms は待ちではなく、書き終えを待つ猶予である。** プロセスの終了待ちは
  # ループ先頭の `Resolve-SnotraExistingProcess -Policy Stop` が持つ（#872 で
  # `Stop-SnotraProcessAndWait` を通るようになった）ので、次の起動が single-instance で
  # 沈黙することはない。ここが守るのは直後の `Read-SnotraTraceEvents` が読む stderr である。
  Start-Sleep -Milliseconds 120

  $events = @(Read-SnotraTraceEvents -Path $errPath)

  # **seed が読めたことを肯定的に確かめる**（#804・不変条件 4）。seed が parse に失敗すると本体は
  # 既定 config で起動し、既定の scan パスで索引を作る——**この smoke の他の判定（trace ≥ 1・
  # first-run 不発・`*.bin` の ∃）はすべて通り、緑のまま**である。ゆえにここが唯一の
  # 受け皿になる。`[config] ` 付きの eprintln は読み込み失敗の全 arm に在る（成功時に出るのは
  # duplicate instant command と invalid hotkey fallback の 2 系統だけで、
  # この seed は instant_commands 無し・妥当な Alt+Q ゆえどちらも踏まない）。
  # smoke-egui.ps1 と visual-check-colors.ps1 の Test-SeedHealth が同型の判定を持つ（#843）。
  if (Test-Path $errPath) {
    foreach ($d in @(Select-String -Path $errPath -SimpleMatch '[config] ')) {
      $failures += "run=$run seed した config が読めていません: $($d.Line)"
    }
  }

  # **trace が 0 件ならイベント不在の検査は自明に成立する**——空振りの合格である。
  # #690 の調査で、冷えた CI runner の初回起動が 20 秒間 trace を 1 行も出さない状態を
  # 実測した。その状態でも本 smoke は緑を返していた（アサーションが不在の検査だけゆえ）。
  # SNOTRA_TRACE=1 の起動は最低でも `hotkey:registered` を出すため、0 件は異常である。
  if ($events.Count -eq 0) {
    $failures += "run=$run trace が 0 件（アプリが 1 行も出していない）。起動経路を観測できていない"
  }

  # trace の失敗名には統一分類がなく、best-effort の失敗も含まれる。実在イベントに一致しない
  # 汎用パターンを置いて「起動エラー不在」を主張せず、この smoke が所有する契約を肯定的に検査する（#845）。

  # **first-run 経路を踏んでいないことを肯定的に検査する**（#804・不変条件 6）。seed を起動より
  # 前に置いているので踏まないはずだが、イベント名の汎用的な失敗分類には頼らない
  # ——実際のイベント名は :not_found / :spawned / :already_running / :exited で
  # （commands/window.rs:53,74,87,123）どれも :error で終わらない。
  # CI（Smoke workflow）は snotra だけをビルドするので :not_found に留まり **false green のまま通る**が、
  # release.yml は snotra-settings.exe を同じ target/release/ へ置くため、**そこでだけ設定 GUI が
  # 実際に spawn されて 5 起動ぶん残る**（Get-Process snotra は完全一致ゆえ snotra-settings を
  # kill しない）。この肯定的検査はその最悪ケースを想定して置いている。
  $firstRunEvents = @($events | Where-Object { $_.event -like "cmd:launch_settings_process:*" })
  foreach ($frEvt in $firstRunEvents) {
    $failures += "run=$run first-run 経路を踏んだ event=$($frEvt.event)（seed が起動より前に置かれていない）"
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
  }
}

# **env が効いたことの肯定的証拠**（#804・不変条件 1）。効いていなければ本体は実 config を読み、
# 実プロファイルへ書くので、ここには seed した config.toml しか残らない。
# **ループ後に 1 回だけ検査する**——各回で見ると、index.bin が書かれる前に Stop-Process -Force
# された回が false red になる（visual-check-colors.ps1:292-304 の実測は単発起動のものである）。
$generated = @(Get-ChildItem -Path $profileDir -Filter '*.bin' -ErrorAction SilentlyContinue)
if ($generated.Count -eq 0) {
  $failures += "SNOTRA_CONFIG_DIR が効いていない: 検証用プロファイルに *.bin が 1 件も生成されていません ($profileFull)"
} else {
  Write-Host "プロファイルへの書き込みを確認: $($generated.Name -join ', ')（SNOTRA_CONFIG_DIR は効いています）"
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
