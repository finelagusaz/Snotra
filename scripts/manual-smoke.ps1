param(
  # 検証するバイナリ。既定は debug ビルド（カテゴリ D は `cargo run -p snotra` 相当）。
  [string]$ExePath = "target/debug/snotra.exe",
  # 記録の出力先。既定は `$env:TEMP` 配下（リポジトリを汚さない。PR へ貼るのは -PostToPr）。
  [string]$OutFile = "",
  # 実施する項目番号（省略時は全 13 項目）。例: -Only 2,5,10
  [int[]]$Only = @(),
  # 記録を PR コメントとして投稿する（`gh pr comment`）。番号省略時は現ブランチの PR。
  [switch]$PostToPr,
  [int]$Pr = 0,
  # 変更が入ったバイナリかの確認に使う目印文字列（`docs/build-commands.md`「スモーク運用メモ」）。
  # 一致しなくても続行はする（**警告のみ**）——目印はビルド構成に依存するため。
  [string]$BinaryMarker = "launcher_controller",
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
# **trace には性質の違う 2 つが載る**（#757）。混ぜて読んではならない:
#
# 1. **presence（診断）** — 「そのイベントが出たか」。`egui_results:hide` は出るのに窓が残る、
#    という #671 PR A′ の回帰を presence を見る smoke は緑のまま通した。**合否ではない**。
#    目視が「駄目だった」ときに次の一手を選ぶための材料である
# 2. **不変条件（合否）** — H1 / H4 / H5。「起きてはならないことが起きていないか」を見るので
#    合否を名乗れる。判定は `scripts/lib/SnotraTraceInvariants.psm1`（Pester で実測済み）
#
# **項目の合否は目視と trace の両方が決める。** trace が緑でも目視が赤なら赤であり、逆も同じ。
# **SKIP は「判定できなかった」であって合格ではない**——理由が記録に必ず併記される。
#
# 使い方（ユーザーが自分の端末で実行する。Claude は実行できない——対話入力を要するため）:
#   cargo build -p snotra
#   npm run smoke:manual
#   npm run smoke:manual -- -Only 2,5,10          # 一部だけ再実施
#   npm run smoke:manual -- -PostToPr             # 終了後に PR へ記録を投稿

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# 対話入力が無い環境（エージェント・CI・パイプ）では**アプリを起動する前に**落ちる。
# ここが無いと `Read-Host` が EOF で空文字を返し続け、下の再入力ループが無限に回りながら
# 起動済みの snotra を残す（実測）。**判定を人間が入れられないなら、走らせる意味が無い。**
if ([Console]::IsInputRedirected) {
  throw "manual-smoke は対話入力を要する。stdin がリダイレクトされた環境（エージェント / CI / パイプ）では実行できない——人間の端末で直接実行すること。"
}

# trace の parse は `SnotraSmoke.psm1`（`Read-SnotraTraceEvents`）、不変条件の判定は
# `SnotraTraceInvariants.psm1` が持つ。**判定を純関数として外へ出してあるので、対話入力を
# 要する本スクリプトを走らせなくても Pester（`npm run test:powershell`）が検証できる。**
Import-Module (Join-Path $PSScriptRoot 'lib/SnotraSmoke.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'lib/SnotraTraceInvariants.psm1') -Force

# 表示・記録の列順。**判定器から引く**——ここに写しを置くと、判定を 1 つ足したときに
# モジュール側だけが直り、記録と exit code から新しい不変条件が黙って落ちる（`/symmetric-check`）。
$script:InvariantNames = Get-SnotraTraceInvariantNames

# --- 項目定義（この `$items` が常設 13 項目の SSOT である） ---
# **PR 本文の目視表とは別の母集団であって、写しではない**（`docs/adr/ADR-folder-location-display-surface.md`
# 「却下 6」）。ここに載るのは**どの変更でも壊れうる横断不変条件**（フォーカス奪取・hide の順序・
# クリック逆流の読み点・位置復元・フォント hot-reload・キャレット・ベースライン・通知期限）であり、
# 機能単位の受け入れ確認は PR 限りの目視表が持つ。**新機能のために先回りで足さない**——足す条件は
# 「その表示が実際に一度回帰したとき」である（#700 → 項目 11 がその経路）。
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
  @{ id = 7; title = "作業領域の下端で高さがクランプされる"
     inv = "I6（可視判定はクランプ前・set_size はクランプ後）"
     steps = @("main を画面下端の**手前**（作業領域の残りが数行ぶんある位置）へ動かす",
               "多数ヒットするクエリを打つ",
               "次に main を下端ぎりぎりまで下げて同じクエリを打つ")
     expect = @"
前半（残りが数行ある位置）: results の高さが作業領域の下端で切られ、**タスクバーの下へ潜らない**。入り切らない行はスクロールで辿れる
後半（下端ぎりぎり）: **1 行 + padding ぶんのはみ出しは仕様**（SPEC.md §8「作業領域の残りが 1 行に満たない場合でも 1 行分（+ padding）は表示し、そのぶんのはみ出しは受容する」・#675）。
  → **それを超えて大きく沈むなら FAIL**。そのとき main 窓自身が作業領域の下端を超えていないかも見る（超えていれば #738 の側）
"@
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
  # --- #666 段 3（view.rs の責務分割）で追加した 3 項目 ---
  # 既存の 3 と 10 が「クリック起動で古い行がちらつかない」（#699）と「reset-on-show と
  # results driver の順序」を既にカバーするため、重複させていない。
  @{ id = 11; title = "結果を ↑ で選び直した直後の打鍵が末尾に入る"
     inv = "↑↓ の消費（events.retain）は TextEdit の構築より前（#700）"
     steps = @("複数件ヒットするクエリ（例: `abc`）を打つ", "↑ を 1 回押して選択を動かす", "続けて 1 文字打つ（例: `x`）")
     expect = @"
打った文字が**キャレット位置（末尾）**に入る（`abc` → ↑ → `x` で `abcx`）。
**先頭に入ったら FAIL**（`xabc` になる）——#700 の再発である。分割で消費が TextEdit の後ろへ落ちると、
focus を保持したままの TextEdit が同じ ↑ を処理してキャレットをクエリ先頭へ飛ばす。
"@
     trace = @() }
  @{ id = 12; title = "Latin と CJK が混在する行のベースラインが揃う"
     inv = "フォント登録は index 0 への insert（push = 末尾ではない・#399 / #579）"
     steps = @("Latin と日本語が 1 行に混ざるクエリを打つ（例: `README` と `設定` が同居するパス）",
               "検索バーの入力欄と、results の行（名前・パスの 2 行とも）を見る",
               "config の `visual.font_family` を CJK をカバーしないフォント（例: `Segoe UI`）にして繰り返す")
     expect = @"
Latin の文字と日本語の文字の**下端が同じ高さに揃う**。片方だけ数 px 上下していたら FAIL。
softbuffer はカバレッジ AA を持たないため、2 フォント間の分数 px の差が整数 px へ丸められて
**目に見える段差**になる（glow / wgpu 期は sub-pixel AA が同じ差を隠していた）。
**この項目の検出器は目視だけである**——`font_definitions_*` テスト 4 件は index 0 への insert を
固定するが、実際のベースラインは測っていない。
"@
     trace = @() }
  @{ id = 13; title = "起動失敗の通知が数秒で自然に消える"
     inv = "通知の期限を張る唯一の主体は notice.remaining() ブロック（分割で drain_launch と別モジュールへ割れた）"
     steps = @("存在しないパス、または起動に失敗する項目を Enter で起動する（削除済みのショートカット等）",
               "検索バー直下に赤系の失敗通知が出ることを確認する",
               "**何も操作せずに**放置する")
     expect = @"
通知が**数秒で自然に消える**（マウスを動かしたり打鍵したりしなくても）。
消えずに残り、無関係な入力を与えて初めて消えるなら FAIL——`drain_launch` の 3 分岐は自前の
`request_repaint` を持たず、deadline を張っているのは `notice.remaining()` の 1 か所だけである。
分割でこの 2 つが同じフレームで呼ばれなくなると、この形で壊れる。
"@
     trace = @("egui_launch_done") }
)

# 有効な id は `$items` から導く（件数をリテラルで書くと項目の増減で嘘になる）。
$allIds = @($items | ForEach-Object { $_.id })

if ($Only.Count -gt 0) {
  $items = @($items | Where-Object { $Only -contains $_.id })
  if ($items.Count -eq 0) { throw "-Only に一致する項目がありません（$($allIds -join ', ') のいずれかを指定してください）" }
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

# trace ファイルの現在の状態。**不在時の形も「捨てた行」の数え方も `Read-SnotraTraceSnapshot`
# が持つ**（`smoke-egui.ps1` と同じ規則を共有する・`/dry-check`）。ここに写しを置かない。
function Get-TraceSnapshot {
  return (Read-SnotraTraceSnapshot -Path $traceFile)
}

# **マーカーは操作の「前」に打つ**（#757）。後に打つと直前の操作が前の区間へ紛れ込む。
# 事象が無ければ `Get-SnotraTraceMarker` が 0 を返すので、trace 不在の分岐は要らない。
function Get-CurrentTraceMarker {
  return (Get-SnotraTraceMarker -Events (Get-TraceSnapshot).Events)
}

function Get-TraceVerdict([hashtable]$snapshot, [array]$sectionList) {
  if (-not $snapshot.Available) { return $null }
  return (Test-SnotraTraceInvariants -Events $snapshot.Events -Sections $sectionList -DroppedLineCount $snapshot.Dropped)
}

function Show-Trace([string[]]$patterns, [array]$sectionList) {
  # **presence の一覧と不変条件の判定は同じ 1 回の読み取りを見る。** 別々に読むと、稼働中の
  # アプリが間に書き足した行で「並べた presence」と「判定した列」が食い違う。
  $snapshot = Get-TraceSnapshot
  if (-not $snapshot.Available) {
    # **不在と読み取り失敗を同じ文言にしない**（#872）。`Available=false` へ至る経路が
    # 2 本になったので、片方の説明だけを出すと**読めなかった実行を「まだ出ていません」と
    # 誤って説明する**——目視の判断材料が嘘になる。
    if ($snapshot.ReadError) {
      Write-Host "  （trace を読めませんでした: $($snapshot.ReadError)）" -ForegroundColor Red
    } else {
      Write-Host "  （trace なし: -NoLaunch で起動したか、まだ 1 行も出ていません）" -ForegroundColor DarkGray
    }
    return
  }
  if ($patterns.Count -eq 0) {
    Write-Host "  この項目に対応する presence の trace はありません（**目視が唯一の検出器**）。" -ForegroundColor DarkGray
  } else {
    foreach ($p in $patterns) {
      # 生行の**部分一致**である（`egui_launch` は `egui_launch_done` の行にも当たる）。
      # parse 済みイベントの `event` 一致へ寄せると件数が変わるので、ここは生行のまま。
      $hits = @($snapshot.Lines | Where-Object { $_ -match [regex]::Escape($p) })
      $tail = if ($hits.Count -gt 0) { $hits[-1] } else { "(観測なし)" }
      Write-Host ("  {0,-22} {1} 件  最後: {2}" -f $p, $hits.Count, $tail) -ForegroundColor DarkGray
    }
    Write-Host "  ※ 上の presence は診断であって合否ではない（#671 PR A′: hide の trace は出たのに窓は残った）" -ForegroundColor DarkGray
  }

  # 不変条件は presence と違い**合否を名乗れる**（#757）。ここまでの全区間で判定する。
  $verdict = Get-TraceVerdict $snapshot $sectionList
  if ($null -eq $verdict) { return }
  Write-Host "  不変条件（ここまでの全区間）: $(Format-SnotraTraceCountSummary -Result $verdict -Compact)" -ForegroundColor DarkGray
  foreach ($violation in $verdict.Violations) {
    Write-Host "    違反 $($violation.Invariant) 区間 $($violation.SectionId) / seq $($violation.Seq): $($violation.Message)" -ForegroundColor Red
  }
}

# --- 実施ループ ---
$results = @()
$sections = @()
$aborted = $false
foreach ($it in $items) {
  # **項目の読み上げより前に打つ。** ここから後に出る trace 行がこの項目のものである。
  $sections += @{ Id = $it.id; Title = $it.title; StartSeq = (Get-CurrentTraceMarker) }

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
      "t" { Show-Trace $it.trace $sections }
      "q" { $verdict = "ABORT"; $aborted = $true }
      default { Write-Host "p / f / s / t / q のいずれかを入力してください。" -ForegroundColor Yellow }
    }
  }
  if ($verdict -eq "ABORT") { break }

  $note = ""
  if ($verdict -ne "PASS") {
    $note = Read-Host "メモ（何が起きたか。FAIL / SKIP では必須ではないが、書かないと後から再現できません）"
    if ($verdict -eq "FAIL") { Show-Trace $it.trace $sections }
  }
  $results += @{ id = $it.id; title = $it.title; inv = $it.inv; verdict = $verdict; note = $note }
}

# --- trace の不変条件を判定する ---
# 全区間ぶんを 1 度だけ判定する（状態機械は trace 全体を 1 パスで舐め、違反を区間へ帰属させる）。
$finalSnapshot = Get-TraceSnapshot
$traceVerdict = Get-TraceVerdict $finalSnapshot $sections

function Get-SectionVerdictSummary([int]$itemId) {
  if ($null -eq $traceVerdict) { return "trace なし" }
  $row = Get-SnotraTraceSectionVerdict -Result $traceVerdict -SectionId $itemId
  if ($null -eq $row) { return "—" }
  return (($script:InvariantNames | ForEach-Object {
        if ($row[$_] -eq 'FAIL') { "**$_ FAIL**" } else { "$_ $($row[$_])" }
      }) -join ' / ')
}

# 目視と trace が食い違った項目。**どちらが赤でも赤である**——trace の緑は目視の赤を
# 打ち消さないし、目視の緑は trace の FAIL を打ち消さない。
$mismatches = @()
foreach ($r in $results) {
  if ($null -eq $traceVerdict) { continue }
  $row = Get-SnotraTraceSectionVerdict -Result $traceVerdict -SectionId $r.id
  if ($null -eq $row) { continue }
  $traceFailed = @(Get-SnotraTraceFailedInvariants -Verdicts $row)
  if ($traceFailed.Count -gt 0 -and $r.verdict -eq 'PASS') {
    $mismatches += "項目 $($r.id)「$($r.title)」— **目視 PASS だが trace は $($traceFailed -join ', ') が FAIL**。目視で見落とした回帰の可能性がある"
  } elseif ($traceFailed.Count -eq 0 -and $r.verdict -eq 'FAIL') {
    $mismatches += "項目 $($r.id)「$($r.title)」— 目視 FAIL だが trace は違反を検出せず。**この項目では目視が唯一の検出器である**"
  }
}

# --- 記録の書き出し ---
$done = @($results | Where-Object { $_.verdict -ne "ABORT" })
$pass = @($done | Where-Object { $_.verdict -eq "PASS" }).Count
$fail = @($done | Where-Object { $_.verdict -eq "FAIL" }).Count
$skip = @($done | Where-Object { $_.verdict -eq "SKIP" }).Count
$notRun = @($items | Where-Object { $id = $_.id; -not ($done | Where-Object { $_.id -eq $id }) })
# **赤とみなす状態の定義は `Get-SnotraTraceFailureCount` が単独で持つ**——違反・判定器の例外
# （code-review C2）・trace を読めなかったこと（#872）の 3 つ。ここで数え直すと、赤を 1 つ
# 足したときに exit code だけが追随しない。
$traceFail = Get-SnotraTraceFailureCount -Verdict $traceVerdict -ReadError $finalSnapshot.ReadError

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
$lines += "| 目視 | PASS $pass / FAIL $fail / SKIP $skip / 未実施 $($notRun.Count) |"
if ($null -ne $traceVerdict) {
  # **`Overall` だけを書かない**（code-review High-1）——1 区間でも PASS なら PASS を名乗るため、
  # 12 区間が SKIP でも「H4=PASS」に見える。実際に何件判定したかを併記する。
  $lines += "| trace 不変条件 | $(Format-SnotraTraceCountSummary -Result $traceVerdict) |"
  if ($traceVerdict.JudgeFailed) { $lines += "| 注意 | **判定器が例外で停止した**（SKIP は「調べられなかった」である） |" }
  # **判定器が 1 件も show を見ていないなら、検査が走らなかったのと同じである。** 目視が
  # 併走しているので赤にはしないが、記録が緑に見えないよう名指しする。
  if ($traceVerdict.Observed.ResultsShow -eq 0 -and $finalSnapshot.Events.Count -gt 0) {
    $lines += "| 注意 | **判定器は ``egui_results:show`` を 1 件も見ていない**（イベント名のドリフトの可能性——H4 / H5 は事実上検査されていない） |"
  }
  $lines += "| trace 行 | $($finalSnapshot.TraceLines) 行中 $($finalSnapshot.Events.Count) 行を parse / **捨てた行 $($finalSnapshot.Dropped)** |"
} elseif ($finalSnapshot.ReadError) {
  # **不在と読み取り失敗を同じ行にしない**（#872）。読めなかった実行を「1 行も出ていない」と
  # 記録へ書き残すと、後から読む人は trace が出ていないほうを疑い、**観測が落ちた事実が
  # 記録から消える**。exit code 側は `Get-SnotraTraceFailureCount` が赤にしている。
  $lines += "| trace 不変条件 | **読めなかった**（$($finalSnapshot.ReadError)）——判定していない。観測不能であって「問題なし」ではない |"
} else {
  $lines += "| trace 不変条件 | **判定していない**（trace なし: ``-NoLaunch`` で起動したか 1 行も出ていない） |"
}
$lines += ""
$lines += "| # | 項目 | 目視 | trace 不変条件 | 守る不変条件 | メモ |"
$lines += "|---|---|---|---|---|---|"
foreach ($r in $done) {
  $mark = switch ($r.verdict) { "PASS" { "PASS" } "FAIL" { "**FAIL**" } default { "SKIP" } }
  # 縦棒は列区切りゆえ全セルで潰す（1 セルだけ潰しても表が崩れる・code-review L3）。
  $note = if ($r.note) { $r.note.Replace("|", "\|") } else { "" }
  $lines += "| $($r.id) | $($r.title.Replace('|', '\|')) | $mark | $(Get-SectionVerdictSummary $r.id) | $($r.inv.Replace('|', '\|')) | $note |"
}
foreach ($r in $notRun) {
  $lines += "| $($r.id) | $($r.title.Replace('|', '\|')) | **未実施** | — | $($r.inv.Replace('|', '\|')) | |"
}
$lines += ""
if ($aborted) { $lines += "**途中で中断した**（未実施の項目は検証されていない——問題が無かったのではない）。"; $lines += "" }
$lines += "検証していない項目は「問題なし」ではない。SKIP / 未実施が残る状態でマージする場合は、その判断であることを明記すること。"

if ($mismatches.Count -gt 0) {
  $lines += ""
  $lines += "### 目視と trace の不一致"
  $lines += ""
  $lines += "**どちらが赤でも赤である。** trace の緑は目視の赤を打ち消さず、目視の緑は trace の FAIL を打ち消さない。"
  $lines += ""
  foreach ($m in $mismatches) { $lines += "- $m" }
}

if ($null -ne $traceVerdict) {
  $lines += ""
  $lines += "### trace 不変条件の判定"
  $lines += ""
  $lines += (Format-SnotraTraceVerdictTable -Result $traceVerdict)
}

$lines += ""
$lines += "**presence（イベントが出たか）は診断であって合否ではない**（#671 PR A′: ``egui_results:hide`` は出たのに窓は残った）。"
$lines += "合否を名乗れるのは H1 / H4 / H5 の不変条件（``scripts/lib/SnotraTraceInvariants.psm1``）と目視だけである。"

Set-Content -Path $OutFile -Value ($lines -join "`n") -Encoding UTF8

Write-Host ""
Write-Host "=== 目視: PASS $pass / FAIL $fail / SKIP $skip / 未実施 $($notRun.Count) ===" -ForegroundColor $(if ($fail -gt 0) { "Red" } elseif ($notRun.Count -gt 0 -or $skip -gt 0) { "Yellow" } else { "Green" })
if ($null -ne $traceVerdict) {
  $summary = Format-SnotraTraceCountSummary -Result $traceVerdict -Compact
  $anySkip = @($script:InvariantNames | Where-Object { $traceVerdict.Counts[$_].SKIP -gt 0 }).Count -gt 0
  Write-Host "=== trace 不変条件: $summary ===" -ForegroundColor $(if ($traceFail -gt 0) { "Red" } elseif ($anySkip) { "Yellow" } else { "Green" })
  if ($traceVerdict.JudgeFailed) { Write-Host "  判定器が例外で停止しました（SKIP は「調べられなかった」です）。" -ForegroundColor Red }
  if ($finalSnapshot.Dropped -gt 0) {
    Write-Host "  parse できなかった行が $($finalSnapshot.Dropped) 件あるため PASS を SKIP へ落としました。" -ForegroundColor Yellow
  }
} elseif ($finalSnapshot.ReadError) {
  Write-Host "=== trace 不変条件: 読めなかった（判定していない） ===" -ForegroundColor Red
  Write-Host "  $($finalSnapshot.ReadError)" -ForegroundColor Red
} else {
  Write-Host "=== trace 不変条件: 判定していない（trace なし） ===" -ForegroundColor Yellow
}
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

# **検出は exit code、出力は証拠**（#471）。目視と trace のどちらの FAIL でも落とす——
# 片方だけを exit code に乗せると、もう片方の赤が「記録に書いてあるだけ」になる。
if ($fail -gt 0 -or $traceFail -gt 0) { exit 1 }
