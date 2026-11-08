# #1404 独立導出（smoke-egui.ps1 を分けるか）

対象: ブランチ `chore/smoke-egui-split`・`scripts/smoke-egui.ps1`（958 行・作業ツリーで読んだ時点）。
`workspace/plan.md`・`research.md`・`adversarial-1404.txt` は読んでいない。

## 0. 構造の読み取り

| 行 | ブロック | 種別 |
|---|---|---|
| 1-39 | `param()`（ExePath/StartupWaitMs/ObserveTimeoutMs/HotkeyVks/ResultsQuery/PostMortemWaitMs/StartupObserveTimeoutMs） | 入口 |
| 41-57 | ファイル冒頭の説明コメント | 説明 |
| 59-74 | StrictMode・2 モジュール Import・ExePath 解決 | 段取り |
| 76-131 | profileDir・ResultsQuery 空 guard（フォールトインジェクションの注入口）・scan ダミー・seed | 段取り（シナリオ 1 固有） |
| 133-144 | `$VK_ESCAPE`/`$VK_BACK`・`Get-LetterVk` | 補助 |
| 146-164 | 既存プロセス停止・webview before・`Start-SnotraProcess` | 段取り |
| 166-238 | `Show-FailureEvidence`（シナリオ 1 catch・シナリオ 2 catch・末尾集計の 3 か所が共有） | 補助（スクリプト内関数） |
| 240-705 | シナリオ 1 本体（try）: hotkey 導出 → show → seed 健全性/first-run → results → パスクエリ → Ctrl+H → webview → Escape/hide/H1 → 押しっぱなしの Z → native-text | 検査の連鎖 |
| 706-716 | シナリオ 1 catch/finally | 段取り |
| 718-731 | シナリオ 1 プロセスの終了待ち（`Stop-SnotraProcessAndWait` と意図的に別） | 段取り |
| 733-913 | シナリオ 2（toast あり・高さ）: `Get-MainWindowDwmSize`・`Test-SnotraNoTraceEventInWindow`・round ループ | 別起動の検査 |
| 915-958 | env 到達性（*.bin）・集計・成功文言 | 集計 |

## 1. 判断: 現状維持＋スクリプト内の小さな整理（分けない・lib へも寄せない）

### 1a. 検査間で受け渡す状態（シナリオ 1 の中は 1 本の連鎖である）

| 状態 | 作る行 | 読む行（検査） |
|---|---|---|
| `$vks` / `$vksLabel` / `$hotkeySource` | 259-288 | show 298・305、押しっぱなしの Z 568/589/596・571、native-text 661・664 |
| `$failures`（ゲート） | 240 | 357・397・446・477・504・561・630・658・692・926（後段はすべて `$failures.Count -eq 0` で前段の成功に依存） |
| `$resultsChecked` | 241/356/358 | 397・489・504・561・658・951（末尾の到達不能 guard） |
| 入力欄の中身 "z" | 375（results） | 401（パスクエリが Backspace で消す前提） |
| `$pathTyped`（3 文字・seq 起点） | 423 | 446・448（Ctrl+H の起点 seq と 3→2 の期待値） |
| ウィンドウ可視/不可視 | show 298 → Escape 479 → Z の show 568 … Escape 632 → native-text show 661 … Escape 694 | 各ブロックの前提（「直前の Escape で隠れている」566） |
| 「show で入力欄が空に戻る」前提 | アプリ側 | 582（Z が 0→1）・622（`[0->1]`）・688（`0->1, 1->2, 2->3`）。Ctrl+H の後に "c:" が残ったまま hide しているので、この前提が無いと 3 ブロックとも期待値が変わる（⚠️ アプリ側の実装箇所は確かめていない。現に CI で緑であることからの推論） |
| `$errPath` / `$proc` / `$launchedAt` | 157・162・164 | 全ブロック・catch 709 |
| `$webviewBefore` | 155（起動前） | 472-475 |

### 1b. 順序の制約（ファイル内に明記されているもの）

- seed 健全性・first-run の判定は「show 観測の後・results の前」（308-317）
- H1 判定は「Escape/hide の後」かつ「追加の show/hide より前」（557-558: 既存の待ちは presence なので、前に show/hide を足すと既存の断言が新しい事象で満たされる）
- 押しっぱなしの Z と native-text は H1 の後で、増分（`-MinMatchCount`・seq 起点）で待つ
- シナリオ 2 はシナリオ 1 のプロセス終了待ち（727-731）の後
- `$generated`（env 到達性・919）はシナリオ 1 のプロファイルを見るので、シナリオ 1 が終わった後ならどこでもよい

→ シナリオ 1 の各検査は、前の検査が残した UI 状態・入力欄・seq を次の検査の入力にしている。検査ごとにファイルを分けると、これらがすべて引数か `$script:` 共有になり、順序の制約はファイルをまたいだ暗黙の契約になる。読みやすさの益より、受け渡しが見えなくなる害が大きい。

シナリオ 2（733-913）だけは別プロセス・別プロファイルで独立度が高い。それでも次を共有している: `Show-FailureEvidence`、`$failures`、`$scenario2Failed` → 末尾集計 943、`$ExePath`・3 つの予算パラメータ、シナリオ 1 の終了待ち（グローバル hotkey が同じ Alt+F12）。分けると「シナリオ 2 の失敗を集計が別ログで証拠出しする」配線（937-946）がファイルをまたぐ。

### 1c. 変更履歴（`git log -- scripts/smoke-egui.ps1`・29 件）

- 検査の追加はすべて「連鎖の末尾へ 1 hunk 追加＋成功文言（958 行）の更新＋隣接ブロックのコメント修正」の形で入っている
  - #1268（496f06be）: 末尾へ 39 行追加、パスクエリのコメント 379 行付近、成功文言
  - #1274（55e5c4a9）: 末尾へ 103 行追加、Ctrl+H の「検出しないもの」432 行付近、成功文言
  - #1289（921c0a97）: 末尾へ 59 行追加、Ctrl+H・Z ブロックのコメント複数、成功文言
  - #1004（925ef3ad）: results の直後へ 41 行
  - #904（0083620d）: シナリオ 2 を 172 行追加＋集計 10 行
- **隣接ブロックのコメントが互いを名指している**（445「H1 判定の後の押しっぱなしの Z のブロック（#1274）が検出する」・557-558・612「Ctrl+H と同じく」）。分けても、この相互参照はファイルをまたいで残る
- 1 ファイルであることが変更を難しくした記録（衝突・取り違え）は、hunk を見た範囲では見当たらない（⚠️ PR の議論までは読んでいない）
- 前例 `ADR-smoke-module-not-split` の判定軸（「変更が役割をまたいで難しくなった実例で益を示す」）をここに当てても、益の実例は無い

### 1d. lib へ寄せない理由

- `docs/build-commands.md:294`「各 smoke の合否条件と固有の TOML 節は、呼び出し側に残す」——検査そのものを lib へ移すとこの分担に反する
- lib の候補になりうるのはスクリプト内関数 4 本だけで、どれも他に呼び出し元が無い（grep: `Show-FailureEvidence`/`Get-LetterVk`/`Get-MainWindowDwmSize`/`Test-SnotraNoTraceEventInWindow` は `scripts/` 内で smoke-egui.ps1 以外に出現しない。出るのは `docs/superpowers/plans/` の凍結された計画だけ）。呼び出し元 1 つの関数を共有モジュールへ移す益は無い
- ⚠️ 例外候補: `Get-MainWindowDwmSize`（755-762）は `[SnotraSmokeInterop.Native]` を直に使い、型の `Add-Type` が先に済んでいることに暗黙に依存する（`ADR-smoke-module-not-split` が費用として挙げた依存そのもの）。現状は 823 の `Wait-SnotraWindow`（内部で `Initialize-SnotraNativeInterop`・`SnotraSmoke.psm1:926`）が 841 より先に走るので成立している。lib の `Get-SnotraWindowCapture`（`SnotraSmoke.psm1:1043`）も同じ DWM 読みを持つが、あちらは `GetWindowRect` へフォールバックする——高さ判定ではフォールバックしてはならない（744 行のコメント・`docs/build-commands.md:165`）ので、**同じ概念ではない**。寄せるなら「フォールバックしない DWM 読み」を lib に新設することになるが、1 呼び出し元のために lib の公開面を増やす益は薄い。#1404 の範囲外として触らないのが妥当（触るなら別 issue）

### 1e. スクリプト内の整理として選ぶもの（挙動を変えない）

境目を読めるようにする最小の手当て。順序もブロック構成も変えない。

1. **ブロック見出しの書式を揃える**。現状は `# --- パスクエリ打鍵（#1004）---`（388）・`# --- Ctrl+英字…（#1268）---`（431）・`# --- focus 獲得時…（#927 / #1274）---`（544）・`# --- 打鍵を伴わない文字…（#1289）---`（648）・`# ---- シナリオ 2: …----`（733）があるが、hotkey/show（249）・seed 健全性（308）・results（353）・webview（471）・Escape/hide/H1（477）・env 到達性（915）・集計（926）は見出しが無い。全ブロックへ同じ書式の 1 行見出しを付け、シナリオ 1 の開始（240 の前）にも見出しを置く
2. **ファイル冒頭の説明（41-57）は帰結だけを書き、検査の一覧を写さない**。書くのは「ブロックは上から順に、前段が残した UI 状態・入力欄・seq を入力にする。H1 より後の待ちは増分で待つ。シナリオ 2 は別起動。検査の一覧は各ブロックの見出し」まで。見出し名を並べると、検査が数週ごとに末尾へ足される（§1c）たびに冒頭も直す必要があり、忘れても CI は緑のまま偽になる。1 の見出しを一覧の正本にする（§4-10・11 も同じ理由）。行番号・件数は書かない
3. 古くなった参照・前提を直す（§4）
4. ⚠️ 任意: `Get-MainWindowDwmSize`・`Test-SnotraNoTraceEventInWindow` は 755・781 とシナリオ 2 の途中で定義されている。`Show-FailureEvidence` と同じく「補助関数」として前方（`Show-FailureEvidence` の隣）へ集めると、段取り／補助／検査の境目が読みやすくなる。ただし移動は diff を大きくし、`git blame` を切るので、やるかは計画で決める（集めない場合も見出しで足りる）

分けない・寄せないことは ADR に残す価値がある（否定の知識）。⚠️ ADR を書くかは `AGENTS.md`「ドキュメント参照」の基準（否定の知識が生じた決定だけ）による——#1403 と同じく「分けない」なので、`ADR-smoke-module-not-split` と同型の `ADR-smoke-egui-not-split`（仮）が候補。

## 2. 変更するファイル・シンボル／変えてはならないもの

### 変更する

| ファイル | 対象 |
|---|---|
| `scripts/smoke-egui.ps1` | 見出しコメント（各ブロックの先頭）・冒頭説明（41-57）・399 の行番号参照・76/333/918 の `visual-check-colors.ps1:NN-NN` 参照・718-726 の終了待ちの理由・799 の誤字「この scriptの`catch`」 |
| `scripts/smoke-startup.ps1` | 47・164 の `visual-check-colors.ps1:NN-NN` 参照（同じパターンの古い行番号。§4） |
| `scripts/lib/SnotraSmoke.psm1` | 504-507 `Stop-SnotraProcessAndWait` の doc「後発は先発へ通知して即終了する（機序の正本は smoke-egui.ps1 のコメント）」——§4 の前提ずれを直すなら連動（⚠️ lib を触ると `npm run test:powershell` と smoke.yml の paths 発火対象になる） |
| `docs/build-commands.md` | 306「流れ」が show/hide/webview だけを書いており、results 以降の検査とシナリオ 2 を含まない（§4・⚠️ 直すかは「必要なことだけを書く」との兼ね合い。正本をスクリプト冒頭に置き、ここは指すだけにするのが書く約束に沿う） |
| （任意）`docs/adr/ADR-smoke-egui-not-split.md` | 分けない決定と費用（§1a-1c） |

### 変えてはならない（形を縛られている）

- `scripts/smoke-egui.ps1` 1-2 行 `[CmdletBinding(PositionalBinding = $false)]` と `param()` に `Position=` を足さないこと——`SnotraSmoke.Tests.ps1:392-418`（母集団は `package.json` の `smoke:egui` から導く・`SnotraSmoke.Tests.ps1:56-63`）
- `param()` 内の `[string]$ExePath = ''` を 1 行で（`SnotraSmoke.Tests.ps1:311-318`）、`$repoRoot =` を `$PSScriptRoot` から 1 行・1 か所で（同 :320-322・本体 66 行）、`Resolve-SnotraCargoExecutable -RepositoryRoot $repoRoot`（同 :326-335・本体 68 行）。いずれもコメント行を除いた行で判定するので、コメントの整理では壊れない
- 上の Describe は `scripts/` **直下**の `.ps1` 全部を母集団にする（`SnotraSmoke.Tests.ps1:50-54`）。分割する案を採るなら、直下に新しい `.ps1` を置くと `$ExePath` を語った時点で補集合の It（:338-350）に掛かる——**分けない理由の 1 つ**（分ける場合は直下以外へ置く必要がある）
- `.github/workflows/smoke.yml:108` の呼び方 `pwsh -NoProfile -File scripts/smoke-egui.ps1 -ExePath target/release/snotra.exe`、paths の `scripts/smoke-egui.ps1`（:24）と `scripts/lib/**`（:26）——分けて `scripts/` 直下・`lib/` 以外へファイルを置くと **paths に載らず、そのファイルだけを変えた PR で smoke が走らない**（#686 と同族の形）。これも分けない理由
- `package.json:17` の `smoke:egui`
- パラメータ名と既定値（`-ResultsQuery ''` の guard が起動前に throw すること 94-102・`docs/build-commands.md:315` のフォールトインジェクション手順。`-StartupWaitMs 0 -ObserveTimeoutMs 1`・同 :325）
- trace イベント名・失敗文言・成功文言（958）——`AGENTS.md:63` のトリガーと smoke の前提。成功文言を変えると CI ログの読み手の照合先が変わる
- 検査の順序（§1b）
- `src-tauri/CLAUDE.md`「trace の presence 検査は状態の検査ではない（#671 PR A′）」への正準形参照（766）——governance:check の G-heading-refs が照合する
- `snotra-egui-runtime/CLAUDE.md:41`「`scripts/smoke-egui.ps1` の #1274 の節の手順 2」・`docs/adr/ADR-keyboard-input-owned-by-subclass.md:17`「#1274 の節」——散文で節と手順番号を名指している。手順番号（566 の「1.」…629 の「9.」）と #1274 の見出しを変えない（変えるなら参照元も直す。ADR は凍結なので直さない＝変えない）
- `scripts/measure-memory-stages.ps1:19`「理由はあちらの注入箇所のコメント」・`scripts/lib/SnotraSmoke.psm1:506` の散文参照——注入箇所（249-258）と終了待ち（718-726）のコメントを消さない

## 3. 挙動を変えないことの確かめ方

コメントだけの変更なら、**コメントと改行を除いたトークン列が main と一致する**ことで機械的に確かめられる。次のスクリプトを一時ファイル（例 `compare-tokens.ps1`・リポジトリの外かスクラッチ）へ書き、リポジトリルートで実行する（本文の写し。セッションのスクラッチにも同じものを置いた: `C:\Users\Eoh\AppData\Local\Temp\claude\68217c6d6914cc71\17e0dd85-2ce0-4bcf-bf10-0a3bec024ac4\scratchpad\compare-tokens.ps1`）:

```powershell
param([string]$Rev = 'HEAD', [string]$Path = 'scripts/smoke-egui.ps1', [string]$WorkFile = '')
# コメントと改行を除いたトークン列を、git の版と作業ファイルで比べる（挙動を変えないことの検査）。
$ErrorActionPreference = 'Stop'
function Get-CodeTokens([string]$Text) {
  $tokens = $null; $errors = $null
  [void][System.Management.Automation.Language.Parser]::ParseInput($Text, [ref]$tokens, [ref]$errors)
  if ($errors.Count) { throw "parse error: $($errors[0].Message)" }
  @($tokens | Where-Object { $_.Kind -notin 'Comment', 'NewLine', 'LineContinuation', 'EndOfInput' } | ForEach-Object { "$($_.Kind)`t$($_.Text)" })
}
$old = (git show "${Rev}:$Path") -join "`n"
if (-not $WorkFile) { $WorkFile = $Path }
$new = Get-Content -LiteralPath $WorkFile -Raw
$a = Get-CodeTokens $old; $b = Get-CodeTokens $new
$diff = @(Compare-Object $a $b -SyncWindow 0)
"tokens: old=$($a.Count) new=$($b.Count) diff=$($diff.Count)"
if ($diff.Count) { $diff | Select-Object -First 10 | Format-Table -AutoSize | Out-String | Write-Host; exit 1 }
```

```
pwsh -NoProfile -File <置き場>/compare-tokens.ps1 -Rev main -Path scripts/smoke-egui.ps1
pwsh -NoProfile -File <置き場>/compare-tokens.ps1 -Rev main -Path scripts/smoke-startup.ps1
```

実測（2026-11-08・HEAD 692e8871 の作業ツリー）:
- 無変更: `tokens: old=3072 new=3072 diff=0`・exit 0
- 変異 1（コメント行を 1 行足す）: diff=0・exit 0（通るべきものが通る）
- 変異 2（`Start-Sleep -Milliseconds 1500` → 1400）: diff=2・exit 1
- 変異 3（here-string の文言を変える）: diff=2・exit 1

この検査の外に残るもの: 関数定義の移動（§1e の 4）はトークン順が変わるので**赤になる**——移動を採るなら、この検査は使えず、AST の関数単位比較か実機 smoke に頼ることになる（移動を勧めない理由の 1 つ）。逆に、here-string 内の文言の変更（失敗メッセージ）は赤になるので、メッセージの誤字を直す場合は差分を目で確かめて通す。

あわせて実行するもの（`docs/build-commands.md` のカテゴリから該当分）:

1. `npm run test:powershell`——`SnotraSmoke.Tests.ps1` の ExePath 導出と PositionalBinding の Describe が smoke-egui.ps1 / smoke-startup.ps1 の形を縛る（lib を触る場合は必須・`docs/build-commands.md:215`）
2. `npm run governance:check`——コメント中の見出し参照（766 の `src-tauri/CLAUDE.md`「…」）と新しい ADR の参照を照合する
3. `npm run smoke:egui`（ローカル・先に `cargo build -p snotra`）。コメントだけでもパース可否は実行でしか確かめられない部分がある（here-string 境界など）。トークン比較が緑なら等価だが、1 回は通す
4. フォールトインジェクション: `pwsh -NoProfile -File scripts/smoke-egui.ps1 -ResultsQuery '' -ExePath <任意の既存ファイル>` が起動前に throw すること（`docs/build-commands.md:315`）
5. CI: `scripts/smoke-egui.ps1` は `smoke.yml` の paths に載るので、PR で smoke-egui job が自動で走る（job が実際に走ったことを PR のチェック欄で確かめる）
6. コメントの組み直しで主張を落としていないか: `.claude/rules/governance-docs.md` の集合差の手順を当てる。例
   `git show main:scripts/smoke-egui.ps1 | grep -oE '#[0-9]+|`[^`]+`|ADR-[a-z0-9-]+' | sort -u > old.txt` と作業ファイルで同じものを作り `comm -3 old.txt new.txt`（実行済み: main と作業ツリーで各 123 件・差 0。集合から `#1274` を 1 件抜いた変異で `#1274` が 1 行出ることを確かめた。注意: `` `[^`]+` `` は行をまたぐ組を拾わず、コード中の `` ` `` 行継続と組になる誤抽出もありうるので、差が出たら目で読む）

## 4. 同じ変更で直すべきもの（smoke-egui.ps1 の外を含む）

| # | 場所 | ずれ | 根拠 |
|---|---|---|---|
| 1 | `scripts/smoke-egui.ps1:399` | 「既存の再注入前と同じ Backspace 作法・356〜362 行」→ 実際は 369-374。#1004（925ef3ad）で入った時点で既に 358-362 付近を指しており、その後の追加でずれた | `git blame -L 397,401`・`git show 925ef3ad:scripts/smoke-egui.ps1` で Backspace は 359-361 行。行番号ではなく「results の再注入（`$resultsShown` のループ）と同じ作法」と名指す |
| 2 | `scripts/smoke-egui.ps1:76` | `visual-check-colors.ps1:54-58 と同じ理由` → 54-58 は今 `[CmdletBinding]`/`param()`。理由は 64-66（`$shotDir` と検証用プロファイルのコメント） | `sed -n 50,66p scripts/visual-check-colors.ps1` |
| 3 | `scripts/smoke-egui.ps1:333` | `visual-check-colors.ps1:143-159 の Test-SeedHealth` → `Test-SeedHealth` は 154 行から | `grep -n Test-SeedHealth`（154・329） |
| 4 | `scripts/smoke-egui.ps1:918` | `visual-check-colors.ps1:292-304 と同型` → `*.bin` の検査は 309-316 付近 | `sed -n 288,316p` |
| 5 | `scripts/smoke-startup.ps1:47`・`:164` | 同じ `visual-check-colors.ps1:54-58`・`:292-304` の古い行番号（**同じパターンの別経路**） | `grep -rn "\.ps1:[0-9]" scripts/` |
| 6 | `scripts/smoke-egui.ps1:718-726` | 終了待ちの理由「先発が生きたまま後発を起動すると single-instance で後発が即終了」は #1306（aa4d2311）後は**シナリオ 1→2 には当たらない**（確認済み）: 単一インスタンスの identifier はプロファイルの鍵で決まり（`src-tauri/src/main.rs:246-252`）、鍵は正規化した絶対パスの FNV-1a ハッシュ（`snotra-core/src/config/location.rs:83-95`・`profile_identity` :114-117）なので、`target/smoke-egui/profile` と `.../profile-toast` は別の mutex になる。待ちが今も要る理由は、両プロファイルとも既定ホットキー Alt+F12（`SnotraSmoke.psm1:225-229`）で、先発が登録を持ったままだと後発の `RegisterHotKey` が失敗すること（⚠️ こちらは推定・未実測）。直すなら理由の文だけを直し、待ちそのものは残す | `git show aa4d2311 -- scripts/smoke-egui.ps1` はこの節を触っていない |
| 7 | `scripts/lib/SnotraSmoke.psm1:504-507` | `Stop-SnotraProcessAndWait` の doc が「機序の正本は smoke-egui.ps1 のコメント」と 6 の旧い機序を指す。同じプロファイルを再起動する呼び出し元（smoke-startup のループ等）には今も当たるので偽ではない。6 を直すなら、正本の所在がずれないように確かめる（⚠️ lib を触ると Pester 必須） | |
| 8 | `scripts/smoke-egui.ps1:806-808` | シナリオ 2 は `hotkey:registered` の `ok` を見ない（シナリオ 1 は 273-275 で見る）。6 の推定どおりなら、登録失敗は「show が観測できない」という別の文言で落ちる。⚠️ 挙動の変更になるので #1404（構造の整理）の範囲外。別 issue 候補として報告するだけ | |
| 9 | `scripts/smoke-egui.ps1:799` | 誤字「この scriptの`catch`」 | |
| 10 | `docs/build-commands.md:306`「流れ」 | show → hide → webview だけを書き、results（311 行で別項）・パスクエリ・Ctrl+H・押しっぱなしの Z・native-text・シナリオ 2（toast・高さ）を含まない。⚠️ 直すなら列挙を増やさず、「検査の並びはスクリプト冒頭が正本」と指す形にする（数え上げは古くなる・`AGENTS.md`「検証の作法（全タスク共通）」） | |
| 11 | `scripts/smoke-egui.ps1:41-57` 冒頭説明 | 「…で 1 シナリオ」と書くが、Ctrl+H 以降とシナリオ 2 を含まない。§1e の 2 で置き換える | |

ずれていないことを確かめたもの:
- `CONTRIBUTING.md:92`・`docs/build-commands.md:307` の「SSOT は `injection_vks`」と本体 10 行の「`PreparedHotkey`」は同じもの（`src-tauri/src/platform/hotkey.rs:19-22`・`injection_vks` は `PreparedHotkey` のフィールド）
- `snotra-egui-runtime/CLAUDE.md:41`「#1274 の節の手順 2」は 573 行の「2.」（Z を押しっぱなしにして文字が入るのを待つ＝`WM_CHAR` が来なければ赤）に今も着地する
- `docs/superpowers/plans/*` 中の古い行番号（例 `2026-07-25-pr-a-…md:74`「1-10 行」）は凍結された計画で、直さない

## 5. 確信の持てない所見（⚠️ まとめ）

- ⚠️ 「show で入力欄が空になる」前提をアプリ側で確かめていない（§1a）。整理では触らないが、冒頭説明に「後段は前段の状態を入力にする」と書くなら、この前提も書くか迷う
- §4-6 の「single-instance の転送はシナリオ 1→2 に当たらない」は location.rs で確認済み。⚠️ 待ちが今も要る理由（ホットキーの競合）は推定。確かめるなら、待ちを外した複製で 2 本目の `hotkey:registered` の `ok` が false になるかを見る（ライブのスクリプトは変えない）
- ⚠️ 1 ファイルが変更を難しくした実例が無い、という判断は hunk と commit 件名の範囲で、PR の議論・レビュー指摘は読んでいない
- ⚠️ 関数の前方移動（§1e の 4）はトークン比較が使えなくなる。採らないことを勧める
- ⚠️ 新 ADR を書くか（`ADR-smoke-module-not-split` の帰結に足すか、別に立てるか）は判断が割れる。#1403 の ADR は psm1 の話で、smoke-egui.ps1 の判断は否定の知識として別物なので、別に立てる側に寄る
- ⚠️ §4-10 の `docs/build-commands.md` を直すと、文書の写しを減らす向きにはなるが、#1404 の「前提を壊さない」の範囲を超えるかは呼び出し側の判断
