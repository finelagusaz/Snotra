param(
  # 検証するバイナリ。既定は debug ビルド（カテゴリ D は `cargo run -p snotra` 相当）。
  [string]$ExePath = "target/debug/snotra.exe",
  # 記録の出力先。既定は `$env:TEMP` 配下（リポジトリを汚さない。PR へ貼るのは -PostToPr）。
  [string]$OutFile = "",
  # 実施する項目番号（省略時は全 10 項目）。例: -Only 2,5,10
  [int[]]$Only = @(),
  # 記録を PR コメントとして投稿する（`gh pr comment`）。番号省略時は現ブランチの PR。
  [switch]$PostToPr,
  [int]$Pr = 0,
  # 変更が入ったバイナリかの確認に使う目印文字列（`docs/build-commands.md`「スモーク運用メモ」）。
  # 一致しなくても続行はする（**警告のみ**）——目印はビルド構成に依存するため。
  [string]$BinaryMarker = "window_coordinator",
  # アプリを起動しない（既に手で起動している場合）。trace の収集も行わない。
  [switch]$NoLaunch
)

# カテゴリ D（目視）の実施を支援し、**結果を成果物として残す**スクリプト。
#
# なぜ要るか: カテゴリ D は `docs/build-commands.md` が「PR 作成前に必須」と定める検証だが、
# 自動検出器を持たない不変条件（読み点の非対称・hide の順序・visual-only 変更の再描画）の
# **唯一の検出器**でありながら、実施の有無が会話にしか残らない。`AGENTS.md`「検証の作法」に
# 照らせば、届かない検証は実施の有無すら区別できない。ゆえに記録をファイルへ落とす。
#
# **trace は診断であって合否ではない**（#671 PR A′ の実測）: `egui_results:hide` は出るのに
# 窓が残る、という回帰を trace の presence を見る smoke は緑のまま通した。本スクリプトが
# trace を並べるのは、目視が「駄目だった」ときに次の一手を選ぶためであって、
# **合否を決めるのは常に目視である**。
#
# 使い方（ユーザーが自分の端末で実行する。Claude は実行できない——対話入力を要するため）:
#   cargo build -p snotra
#   npm run smoke:manual
#   npm run smoke:manual -- -Only 2,5,10          # 一部だけ再実施
#   npm run smoke:manual -- -PostToPr             # 終了後に PR へ記録を投稿

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# --- 項目定義（SSOT は PR 本文の目視表。ここはその実行可能な写しである） ---
# 写しである以上ドリフトしうる。**項目を増減したら PR 本文（と plan.md の目視表）も直す。**
$items = @(
  @{ id = 1; title = "show → 1 文字打鍵でフォーカスを奪わない"
     inv = "I7（results は raw 3 操作・tao の SW_SHOW は活性化する）"
     steps = @("ホットキーで検索窓を出す", "1 文字打つ（results 窓が main の直下に出る）", "続けて 2 文字目を打つ")
     expect = "2 文字目が入力欄に入る。フォーカスが results 窓へ移らない"
     trace = @("egui_show:done", "egui_results:show") }
  @{ id = 2; title = "hide で両窓が同時に消える"
     inv = "I3（main_visible=false は results.hide() の前）・I7"
     steps = @("results が出ている状態にする", "ホットキーで隠す")
     expect = "main と results が同時に消える。**results だけが最前面に残らない**"
     trace = @("egui_hide:done", "egui_results:hide") }
  @{ id = 3; title = "行クリック起動で古い行がちらつかない"
     inv = "I1（result_count はクリック逆流の消費**後**に読む）"
     steps = @("複数件ヒットするクエリを打つ", "行をマウスでクリックして起動する")
     expect = "起動の瞬間に**古い行が 1 フレーム残らない**。results はそのまま消える"
     trace = @("egui_launch", "egui_results:hide") }
  @{ id = 4; title = "main のドラッグに results が追従する"
     inv = "I13（position_results_below_main の第 2 消費者 = Moved リスナー）"
     steps = @("results を出した状態で、入力欄以外の余白をドラッグして窓を動かす")
     expect = "ドラッグ**中**も results が main の直下に追従する（離した後だけでなく）"
     trace = @() }
  @{ id = 5; title = "visual 設定の変更が results に反映される"
     inv = "I5（drive 末尾の無条件 wake_results・level-triggered）"
     steps = @("results を出したまま設定画面を開く", "背景色か font_size を変える", "設定を適用する")
     expect = "results が**キー入力なしで**新しい見た目に描き直される"
     trace = @() }
  @{ id = 6; title = "設定画面の間だけ最前面が外れる"
     inv = "I7（z-order は集約されていない: main は commands/window.rs・results は ResultsWindow）"
     steps = @("results を出した状態で設定画面を開く", "設定画面を閉じる")
     expect = "開いている間は両窓が設定画面の背後へ回り、閉じると両方とも最前面に戻る"
     trace = @() }
  @{ id = 7; title = "作業領域の下端でクランプされる"
     inv = "I6（可視判定はクランプ前・set_size はクランプ後）"
     steps = @("main を画面下端の近くまで動かす", "多数ヒットするクエリを打つ")
     expect = "results がタスクバーの下へ潜らない。入り切らない行は**スクロールで辿れる**"
     trace = @() }
  @{ id = 8; title = "hide → 再 show で位置が保たれる"
     inv = "save_placement_relative と position_on_target_monitor の両方が移設された確認"
     steps = @("main を既定と違う位置へ動かす", "隠す", "もう一度出す")
     expect = "同じ位置に戻る"
     trace = @("egui_show:done") }
  @{ id = 9; title = "カーソルのあるモニターに出る"
     inv = "位置復元側（position_on_target_monitor）の移設確認"
     steps = @("別モニターへマウスカーソルを移す（`follow_cursor_monitor = true` の場合）", "ホットキーを押す")
     expect = "カーソルのあるモニターに出る。単一モニター環境では **skip** してよい"
     trace = @("egui_show:done") }
  @{ id = 10; title = "hide 中の font_size 変更が再 show の 1 フレーム目から効く"
     inv = "I11（reset_size_guard は同一フレームの drive より前）"
     steps = @("検索窓を隠す", "隠したまま設定画面で font_size を変える", "ホットキーで出して 1 文字打つ")
     expect = "results が**最初のフレームから**新しい行高で出る（一瞬だけ旧行高で出て直る、が無い）"
     trace = @("egui_show:done", "egui_results:show") }
)

if ($Only.Count -gt 0) {
  $items = @($items | Where-Object { $Only -contains $_.id })
  if ($items.Count -eq 0) { throw "-Only に一致する項目がありません（1..10 を指定してください）" }
}

# --- 出所の記録（何を検証したのか後から辿れるようにする） ---
$branch = (git rev-parse --abbrev-ref HEAD 2>$null)
$sha = (git rev-parse HEAD 2>$null)
$dirty = (git status --porcelain 2>$null)
$startedAt = Get-Date

if ([string]::IsNullOrEmpty($OutFile)) {
  $shortSha = if ($sha) { $sha.Substring(0, 7) } else { "nogit" }
  $OutFile = Join-Path $env:TEMP "snotra-manual-smoke-$shortSha.md"
}

Write-Host ""
Write-Host "=== カテゴリ D 目視スモーク ===" -ForegroundColor Cyan
Write-Host "branch : $branch"
Write-Host "commit : $sha"
if ($dirty) { Write-Host "WARNING: working tree に未コミットの変更があります（記録の再現性が落ちます）" -ForegroundColor Yellow }
Write-Host "記録   : $OutFile"
Write-Host ""

# --- バイナリの出所確認（`docs/build-commands.md`「スモーク運用メモ」） ---
# cargo は hardlink し直すためタイムスタンプは当てにならない。変更固有の文字列で中身を見る。
$traceFile = $null
$proc = $null
if (-not $NoLaunch) {
  if (-not (Test-Path $ExePath)) {
    throw "$ExePath がありません。先に `cargo build -p snotra` を実行してください（-NoLaunch で自分で起動することもできます）"
  }
  if ($BinaryMarker) {
    $bytes = [IO.File]::ReadAllBytes((Resolve-Path $ExePath))
    $ascii = [Text.Encoding]::ASCII.GetString($bytes)
    if ($ascii.Contains($BinaryMarker)) {
      Write-Host "バイナリに目印 '$BinaryMarker' を確認しました。" -ForegroundColor Green
    } else {
      Write-Host "WARNING: バイナリに目印 '$BinaryMarker' が見つかりません。古いバイナリを見ている可能性があります（-BinaryMarker '' で無効化）。" -ForegroundColor Yellow
    }
  }

  $running = @(Get-Process -Name "snotra" -ErrorAction SilentlyContinue)
  if ($running.Count -gt 0) {
    Write-Host "既に snotra が $($running.Count) 個動いています。**このスクリプトは kill しません**——手で閉じてから続けてください。" -ForegroundColor Yellow
    $ans = Read-Host "そのまま続けますか？ [y/N]"
    if ($ans -ne "y") { exit 1 }
  }

  $traceFile = Join-Path $env:TEMP "snotra-manual-smoke-trace.log"
  $outLog = Join-Path $env:TEMP "snotra-manual-smoke-stdout.log"
  $savedTrace = $env:SNOTRA_TRACE
  $env:SNOTRA_TRACE = "1"
  try {
    $proc = Start-Process -FilePath $ExePath -PassThru -RedirectStandardError $traceFile -RedirectStandardOutput $outLog
  } finally {
    if ($null -eq $savedTrace) { Remove-Item Env:SNOTRA_TRACE -ErrorAction SilentlyContinue }
    else { $env:SNOTRA_TRACE = $savedTrace }
  }
  Write-Host "起動しました（pid=$($proc.Id)・trace=$traceFile）。ホットキーで検索窓を出せます。" -ForegroundColor Green
  Start-Sleep -Milliseconds 1500
}

function Show-Trace([string[]]$patterns) {
  if (-not $traceFile -or -not (Test-Path $traceFile)) {
    Write-Host "  （trace なし: -NoLaunch で起動したか、まだ 1 行も出ていません）" -ForegroundColor DarkGray
    return
  }
  $all = @(Get-Content -Path $traceFile -ErrorAction SilentlyContinue)
  if ($patterns.Count -eq 0) {
    Write-Host "  この項目に対応する trace はありません（**目視が唯一の検出器**）。" -ForegroundColor DarkGray
    return
  }
  foreach ($p in $patterns) {
    $hits = @($all | Where-Object { $_ -match [regex]::Escape($p) })
    $tail = if ($hits.Count -gt 0) { $hits[-1] } else { "(観測なし)" }
    Write-Host ("  {0,-22} {1} 件  最後: {2}" -f $p, $hits.Count, $tail) -ForegroundColor DarkGray
  }
  Write-Host "  ※ trace は診断であって合否ではない（#671 PR A′: hide の trace は出たのに窓は残った）" -ForegroundColor DarkGray
}

# --- 実施ループ ---
$results = @()
$aborted = $false
foreach ($it in $items) {
  Write-Host ""
  Write-Host ("--- [{0}] {1}" -f $it.id, $it.title) -ForegroundColor Cyan
  Write-Host ("守る不変条件: {0}" -f $it.inv) -ForegroundColor DarkCyan
  $n = 1
  foreach ($s in $it.steps) { Write-Host ("  {0}. {1}" -f $n, $s); $n++ }
  Write-Host ("期待: {0}" -f $it.expect) -ForegroundColor White

  $verdict = $null
  while (-not $verdict) {
    $ans = Read-Host "判定 [p]ass / [f]ail / [s]kip / [t]race を見る / [q]uit"
    switch ($ans.ToLower()) {
      "p" { $verdict = "PASS" }
      "f" { $verdict = "FAIL" }
      "s" { $verdict = "SKIP" }
      "t" { Show-Trace $it.trace }
      "q" { $verdict = "ABORT"; $aborted = $true }
      default { Write-Host "p / f / s / t / q のいずれかを入力してください。" -ForegroundColor Yellow }
    }
  }
  if ($verdict -eq "ABORT") { break }

  $note = ""
  if ($verdict -ne "PASS") {
    $note = Read-Host "メモ（何が起きたか。FAIL / SKIP では必須ではないが、書かないと後から再現できません）"
    if ($verdict -eq "FAIL") { Show-Trace $it.trace }
  }
  $results += @{ id = $it.id; title = $it.title; inv = $it.inv; verdict = $verdict; note = $note }
}

# --- 記録の書き出し ---
$done = @($results | Where-Object { $_.verdict -ne "ABORT" })
$pass = @($done | Where-Object { $_.verdict -eq "PASS" }).Count
$fail = @($done | Where-Object { $_.verdict -eq "FAIL" }).Count
$skip = @($done | Where-Object { $_.verdict -eq "SKIP" }).Count
$notRun = @($items | Where-Object { $id = $_.id; -not ($done | Where-Object { $_.id -eq $id }) })

$lines = @()
$lines += "## カテゴリ D 目視スモークの記録"
$lines += ""
$lines += "| | |"
$lines += "|---|---|"
$lines += "| 実施 | $($startedAt.ToString('yyyy-MM-dd HH:mm')) |"
$lines += "| branch | ``$branch`` |"
$lines += "| commit | ``$sha`` |"
$lines += "| バイナリ | ``$ExePath`` |"
if ($dirty) { $lines += "| 注意 | **working tree に未コミットの変更あり** |" }
$lines += "| 結果 | PASS $pass / FAIL $fail / SKIP $skip / 未実施 $($notRun.Count) |"
$lines += ""
$lines += "| # | 項目 | 判定 | 守る不変条件 | メモ |"
$lines += "|---|---|---|---|---|"
foreach ($r in $done) {
  $mark = switch ($r.verdict) { "PASS" { "PASS" } "FAIL" { "**FAIL**" } default { "SKIP" } }
  $note = if ($r.note) { $r.note.Replace("|", "\|") } else { "" }
  $lines += "| $($r.id) | $($r.title) | $mark | $($r.inv) | $note |"
}
foreach ($r in $notRun) {
  $lines += "| $($r.id) | $($r.title) | **未実施** | $($r.inv) | |"
}
$lines += ""
if ($aborted) { $lines += "**途中で中断した**（未実施の項目は検証されていない——問題が無かったのではない）。"; $lines += "" }
$lines += "検証していない項目は「問題なし」ではない。SKIP / 未実施が残る状態でマージする場合は、その判断であることを明記すること。"
$lines += ""
$lines += "trace は診断であって合否ではない（#671 PR A′: ``egui_results:hide`` は出たのに窓は残った）。合否はすべて目視による。"

Set-Content -Path $OutFile -Value ($lines -join "`n") -Encoding UTF8

Write-Host ""
Write-Host "=== 結果: PASS $pass / FAIL $fail / SKIP $skip / 未実施 $($notRun.Count) ===" -ForegroundColor $(if ($fail -gt 0) { "Red" } elseif ($notRun.Count -gt 0 -or $skip -gt 0) { "Yellow" } else { "Green" })
Write-Host "記録: $OutFile"
if ($traceFile) { Write-Host "trace: $traceFile" }

if ($proc -and -not $proc.HasExited) {
  Write-Host ""
  Write-Host "起動した snotra (pid=$($proc.Id)) はそのままです。閉じる場合は手で終了してください。" -ForegroundColor DarkGray
}

if ($PostToPr) {
  $target = if ($Pr -gt 0) { "$Pr" } else { "" }
  Write-Host ""
  Write-Host "PR へ投稿します..." -ForegroundColor Cyan
  if ($target) { gh pr comment $target --body-file $OutFile }
  else { gh pr comment --body-file $OutFile }
}

if ($fail -gt 0) { exit 1 }
