# 調査 — issue #1404 smoke-egui.ps1 の検査と段取りの境目を整理する

測定点: main 692e8871（2026-11-08）・ブランチ `chore/smoke-egui-split`。

## issue の要約

- ペイン: `scripts/smoke-egui.ps1` が 958 行（2026-11-08 も同じ）。検査が並んでおり、分けられるのか肥大しているだけなのか判断できていない
- あるべき姿: 検査ごとの責務と共通の段取り（起動・注入・trace の待ち）の境目が読める。分ける／`scripts/lib/` へ寄せる／現状維持は中身を読んでから決める。CI（`smoke.yml`）と `docs/build-commands.md` のスモークの前提を壊さない
- 関係: #1403（`SnotraSmoke.psm1` の分割）は「分けない」で閉じた（`ADR-smoke-module-not-split`・PR #1406）

### issue の前提の訂正

- issue が検査として挙げる「Alt 解放」は検査ではない。`Send-SnotraKeyChord` が押下順の逆に解放するという注入の段取りであり、それを観測・断言するブロックは無い（`smoke-egui.ps1` の hotkey 注入ブロックのコメントが述べるだけ）

## ファイルの構成（実測・958 行）

全体: コード 535 行・コメント 365 行・空行 58 行。

| 区画 | 行 | 総 | コード | コメント | 種別 |
|---|---|---|---|---|---|
| param | 1-39 | 39 | 13 | 26 | 入口 |
| 前段（本体解決・空クエリ guard・seed・既存停止・起動） | 40-165 | 126 | 56 | 57 | 段取り |
| `Show-FailureEvidence` | 166-239 | 74 | 56 | 15 | 段取り（証拠出力） |
| hotkey 導出 → show | 240-307 | 68 | 46 | 20 | 検査 1 |
| seed 健全性 / first-run 不発 | 308-352 | 45 | 13 | 29 | 検査 2 |
| results show | 353-387 | 35 | 24 | 10 | 検査 3 |
| パスクエリ `c:\`（#1004） | 388-430 | 43 | 23 | 17 | 検査 4 |
| Ctrl+H（#1268） | 431-470 | 40 | 20 | 19 | 検査 5 |
| webview 増分 + Escape hide + results hide + 不変条件（H1 ほか） | 471-543 | 73 | 37 | 30 | 検査 6 |
| 押しっぱなしの Z（#927/#1274） | 544-647 | 104 | 67 | 30 | 検査 7 |
| 打鍵を伴わない文字（#1289） | 648-705 | 58 | 46 | 10 | 検査 8 |
| シナリオ 1 の catch/finally + 終了待ち | 706-731 | 26 | 14 | 11 | 段取り |
| シナリオ 2（toast ありの 2 回目 show の高さ・#755/#801/#878） | 732-914 | 183 | 96 | 73 | 検査 9（別プロセス） |
| 後段（env 到達の `*.bin` 確認・集計・成功文言） | 915-958 | 44 | 24 | 18 | 検査 10 + 段取り |

- `# --- 名前（#NNN）---` の見出し行があるのは検査 4・5・7・8 だけで、検査 1〜3・6・9 には無い（シナリオ 2 だけ `# ---- シナリオ 2: …----`）
- スクリプト内の関数は 4 本: `Get-LetterVk`・`Show-FailureEvidence`・`Get-MainWindowDwmSize`・`Test-SnotraNoTraceEventInWindow`

## 検査をまたいで受け渡す状態と順序の制約

シナリオ 1 の検査 1〜8 は 1 プロセスを共有し、次の状態を前の検査から受け取る。

- `$failures`（全検査が「0 件なら進む」ゲートに使う）
- `$resultsChecked`（検査 3 が立て、4・6・7・8 と成功判定が読む）
- `$vks` / `$vksLabel` / `$hotkeySource`（検査 1 が決め、7・8 が再注入に使う）
- `$pathTyped`（検査 4 が作り、検査 5 が起点 seq に使う。検査 4 が走らない経路でも `$null` で定義してある・#1273）
- `$errPath` / `$proc` / `$launchedAt` / `$webviewBefore` / `$ObserveTimeoutMs` / `$VK_ESCAPE` / `$VK_BACK`
- 入力欄の中身（検査 3 の `z` を検査 4 が Backspace で消す。検査 5 は検査 4 の 3 文字が前提）と、ウィンドウの可視性（検査 6 が隠し、7・8 が出し直す）

コメントに書かれた順序の制約:

- 検査 2 は「`egui_show:done` 観測の後」かつ「results 検査より前」（seed 健全性ブロックのコメント）
- 検査 7 は検査 6（不変条件の判定）より後。既存の待ちは presence なので、前に show/hide を足すと既存の断言が新しい事象で満たされる（押しっぱなしの Z ブロックのコメント）
- シナリオ 2 はシナリオ 1 のプロセス終了を待ってから起動する（single-instance 転送・#755/#801 是正 B）
- 後段の `*.bin` 確認はシナリオ 1 のプロファイル（`$profileDir`）を見る

シナリオ 2 も、検査の結果には依存しないが、状態は共有する: `$failures`・`$ExePath`・`$ObserveTimeoutMs`・`$StartupObserveTimeoutMs`・`$PostMortemWaitMs`・`Show-FailureEvidence` を使い、`$scenario2Failed`・`$toastErrPath`・`$toastLaunchedAt` を後段の集計ブロックへ渡す。

帰結: 検査 1〜8 を別スクリプトへ分けると、各スクリプトで起動・hotkey 待ち（CI のコールドスタートで最大 25s 予算）をやり直すか、プロセスと上の状態を受け渡す仕組みが要る。プロセスを分けて切り出せるのはシナリオ 2 だけで、それも上の状態と集計の受け渡しが要る。

## 繰り返される段取り（実測）

| 段取り | 出現 | 箇所 |
|---|---|---|
| 1 キーの打鍵（down → 間隔 → up） | 11 | 間隔 50ms が 9（Backspace ×2・クエリ字・`c`・`\`・Escape ×3（検査 6・7・8）・X）、間隔 150ms が 1（検査 7 の Z の 2 回目）、間隔なしが 1（シナリオ 2 の Escape・`27` を直書き） |
| hotkey で show して増分で待つ（`Get-SnotraTraceEventCount` → chord → `-MinMatchCount (+1)`） | 4 | 検査 7 の手順 1・4、検査 8、シナリオ 2 |
| hotkey で hide して増分で待つ | 1 | 検査 7 の手順 3 |
| Escape で hide して増分で待つ | 3 | 検査 7 の手順 9、検査 8、シナリオ 2（round 1） |
| seq 以降の `egui_input:changed` を読み、`before->after` に整形する | 3 | 検査 5・7・8（断言の形はそれぞれ違う） |

- 検査 6 の Escape は presence で待つ（シナリオ 1 で最初の hide なので増分である必要が無い）。増分の待ちへそろえるかは挙動の変更になる
- 共通モジュールに「1 キーの打鍵」に当たる関数は無い（`Send-SnotraKey` は down か up の片方・`Send-SnotraKeyChord` は複数キーの chord）。1 キーの chord（`Send-SnotraKeyChord -VirtualKeys @($vk)`）は down → 50ms → up → 50ms で、末尾に 50ms が増える

## 変更履歴から見た「分けていないことの費用」（`ADR-smoke-module-not-split` の帰結が求める益の根拠）

2026-08-01 以降の smoke-egui.ps1 への機能追加・変更コミットのハンクを区画へ対応させた。

| コミット | 触った区画 |
|---|---|
| 925ef3ad #1004 | 検査 4 を追加（自区画だけ） |
| 85220ca1 #878 | シナリオ 2 内だけ（関数 + round ループ） |
| 496f06be #1268 | 検査 5 を追加 + 検査 4 に `$pathTyped = $null` を前置 + 成功文言 |
| 55e5c4a9 #1274 | 検査 7 を追加 + 検査 5 の「検出しないもの」コメント + 成功文言 |
| 921c0a97 #1289 | 検査 8 を追加 + 検査 5・7 のコメントと失敗文言（同じ機構の説明の書き換え）+ 成功文言 |
| efc1191f #1292 / 0a81a3f5 #1366 / aa4d2311 #1306 | 入口・前段だけ |
| 0083620d #904 | シナリオ 2 を新設 + 後段の集計ブロック（`$scenario2Failed` の証拠出力）。終了待ちも同時に足した |
| 7112e8b3 #879・dd338215 #939 | それぞれ 2 区画にまたがる（3b の所見・数行規模） |
| 0047e3c5 #824 | seed 健全性ブロックのコメント |
| docs の一括置換 4 件（#1307・#1373・#1375・#1376） | 全区画の語の置換（機械的） |

- 区画をまたいだ変更は、(a) 成功文言の 1 行（3 件）、(b) 隣の検査の「検出しないもの」コメント（2 件）、(c) 受け渡す状態の前置（1 件）、(d) 同じ機構の説明の書き換え（1 件）、(e) シナリオを足すときの後段の集計ブロックへの配線（#904）。どれも数行で、1 ファイルであることが変更を難しくした記録は見当たらない（3b はコミットメッセージと issue でも反証を探し、見つからなかった）
- (b)(d) は検査が同じ機構（subclass の文字の扱い）を見ていることから来る意味の結合で、ファイルを分けても消えない。(a) は検査を足すたびに末尾の固定文言を書き換える形

## 関連するファイル・モジュール・関数

- `scripts/smoke-egui.ps1`（対象）
- `scripts/lib/SnotraSmoke.psm1`: `Send-SnotraKey`・`Send-SnotraKeyChord`・`Wait-SnotraTraceEvent`・`Wait-SnotraTraceCondition`・`Get-SnotraTraceEventCount`・`Read-SnotraTraceSnapshot`・`Start-SnotraProcess`・`New-SnotraVerificationProfile`・`Resolve-SnotraExistingProcess`・`Wait-SnotraWindow`・`Send-SnotraUnicodeText`・`Send-SnotraPostedChar`。`Initialize-SnotraNativeInterop` は export されていない
- `scripts/lib/SnotraTraceInvariants.psm1`: `Test-SnotraTraceInvariants`・`Get-SnotraTraceInvariantNames`
- `scripts/lib/SnotraSmoke.Tests.ps1`: 既定 ExePath の Describe（母集団は `scripts/` 直下の `.ps1` で `$ExePath` を語るもの・#1293）と PositionalBinding の Describe（母集団は `package.json` の scripts・#1296）。smoke-egui.ps1 の本体（検査のロジック）を縛る Pester は無い
- `.github/workflows/smoke.yml`: `paths` に `scripts/smoke-egui.ps1` と `scripts/lib/**`、ステップ `pwsh -NoProfile -File scripts/smoke-egui.ps1 -ExePath target/release/snotra.exe`
- `package.json` の `smoke:egui`、`AGENTS.md` の条件別チェックの行、`docs/build-commands.md` の smoke の項（305-325 行付近）
- `docs/adr/ADR-smoke-module-not-split.md`、`docs/adr/ADR-config-dir-env-seam-rejected-alternatives.md` §3（3 つの seed は同型ではない）

## 再利用できる既存のパターン

- 検査 4〜8 の `# --- 名前（#NNN）---` 見出し行（境目の目印として既に使われている形）
- `Test-SnotraNoTraceEventInWindow`: シナリオ 2 で 2 つのイベント名に同じ区間の切り方を使うため、名前を引数に出したスクリプト内関数（「書き写さないため」と理由が書いてある）。段取りをスクリプト内関数へ寄せる前例
- `ADR-smoke-module-not-split`: 「分けない」決定を否定の知識として残した前例

## 技術的な制約

- ファイル名と位置は固定: `smoke.yml` の `paths` とステップ、`package.json`、`AGENTS.md`、`docs/build-commands.md` が名指す
- 直下に `.ps1` を増やし、それが `$ExePath` を語ると、#1293 の補集合の It が許可リストへの追加を求める（語らなければ影響しない）。`package.json` の入口にすれば PositionalBinding の母集団に入る
- `smoke.yml` の `paths` は `scripts/smoke-egui.ps1` と `scripts/lib/**` だけを名指す。直下に補助の `.ps1` を足すと、それだけを変える PR では smoke が起動しない
- #1293 の Describe は smoke-egui.ps1 のソースの形も縛る: `[string]$ExePath =` の行が 1 つ、`$repoRoot =` の行が 1 つで `$PSScriptRoot` を含む、`Resolve-SnotraCargoExecutable` の呼び出しが `-RepositoryRoot $repoRoot` を渡す。#1296 は `param()` の `PositionalBinding = $false` を縛る。この行々は触らない
- 関数を `SnotraSmoke.psm1` へ移すと export・Pester・`Mock -ModuleName` の費用が出る（`ADR-smoke-module-not-split` の却下理由）。スクリプト内関数のまま整理するなら、その費用は出ない
- `Get-MainWindowDwmSize` は `[SnotraSmokeInterop.Native]` を直接使い、先行するモジュール関数の `Add-Type` に暗黙に依存する（同 ADR が費用として名指す）。`Get-SnotraWindowCapture` の DWM 読みとは失敗時の意味が違う（こちらは throw、あちらは `GetWindowRect` へ落ちる）
- 検査のロジックを自動で縛るテストが無いので、挙動を変えないことの確認は実機の通し実行（debug 本体・画面ロックなし・このリポジトリのビルド成果物の snotra を止める）と、文書化された 2 つのフォールトインジェクション（`-ResultsQuery ''`・`-StartupWaitMs 0 -ObserveTimeoutMs 1`）、および CI の `smoke.yml`（このファイルの変更で自動起動）で行う
- このファイルは `.claude/rules/safety-nets.md` の `paths` に入る（読んだときに配送された）。ルート `CLAUDE.md` 最重要ルール 2 により、PR 本文の冒頭に「マージは合意してから」と書く

## コメントの序数参照（行番号）のずれ（`.claude/rules/governance-docs.md` が禁じる形）

- 76 行: `visual-check-colors.ps1:54-58` → 現在そこは `param()`。指したい内容（プロファイルを `target/` 下へ置く理由）は別の行
- 333 行: `visual-check-colors.ps1:143-159 の Test-SeedHealth` → 関数は 154 行から
- 399 行: 「356〜362 行」（自ファイルの Backspace の作法）→ 現在は 369-374 行
- 918 行: `visual-check-colors.ps1:292-304` → 現在は results の捕捉の処理で、`*.bin` の確認ではない
- 346 行: `commands/window.rs:53,74,87,123` → 現在は 52・76・89・127（3b の所見・grep で確認）
- 同じパターン: `smoke-startup.ps1` の 47 行（`visual-check-colors.ps1:54-58`）と 164 行（`visual-check-colors.ps1:292-304`）。同じ着地先のずれ
- 規範の文言: `governance-docs.md` が禁じるのは「序数・ファイル名の連番・検査 ID・その他の引用される識別子」で、行番号は名指しされていない。行番号はその「引用される識別子」に当たり、実際にずれているので直す

## `docs/build-commands.md` の記述のずれ

- smoke-egui の「流れ」（306 行付近）は show → Escape → hide → webview しか書いておらず、検査 4・5・7・8・シナリオ 2 が載っていない。列挙を足すと、検査を足すたびに写しが古くなる

## 敵対的調査（3b）の所見と採否

出力: `workspace/adversarial-1404.txt`。壊せた 7・壊せなかった 6・⚠️ 5。

| 所見 | 採否 | 理由・反映 |
|---|---|---|
| B1 行番号参照が 5 件目（window.rs）と smoke-startup に 2 件 | 採る | grep で着地先を確認。上の節と計画へ反映 |
| B2 958 行（959 は split の要素数） | 採る | `wc -l` で 958 を確認。表を直した |
| B3 打鍵は 11 ペア | 採る | 間隔の違いごとに表へ分けた |
| B4 シナリオ 2 も状態を共有する | 採る | 「受け渡す状態」の節へ追記 |
| B5 変更履歴の表が 16 件中 8 件 | 採る | 4 件と docs 置換を追記。#904 を `git show` で確認（シナリオ 2 + 後段）。結論は変わらない |
| B6 `smoke.yml` の paths の帰結 | 採る | 制約へ追記。計画は直下に `.ps1` を足さない |
| B7 #1293 はソースの形も縛る・補集合は `$ExePath` を語る場合だけ | 採る | 制約へ追記 |
| N1〜N5 | 壊せなかった | 区画の行数・段取りの件数・状態の連鎖・各ファイルの記述は維持 |
| N6 見出し行の記述の矛盾 | 採る | 「検査 4・5・7・8 だけ」へ直した（W2 も解消） |
| W1 Alt 解放は `egui_show:done` の観測で間接に検査されている | 一部採る | Alt を含む hotkey で show が出ること自体は検査 1 が見る。ただし Alt 解放を対象にした断言は無いので、「独立した検査ではない」は維持 |
| W3 Z の 150ms を 1 キー打鍵に数えるか | 採る | 間隔ごとに分けて数えた |
| W4 `target/smoke-egui/` を名指す箇所 | 不要 | 計画はディレクトリ名を変えない |
| W5 「検査 9・10」の通し番号を PR 本文や ADR へ写すと序数の引用になる | 採る | 番号は research の作業上の呼び名に留め、コード・ADR・PR 本文では検査名で書く（計画へ明記） |

## 未解決の疑問

- （計画で解消）打鍵の間隔は `Send-KeyTap -HoldMs` で残し、末尾の sleep は呼び出し側に残す。シナリオ 2 の Escape だけ 50ms へそろえ、検査 6 は presence のまま（plan「挙動の差」）
- （計画で解消）成功文言の自動組み立ては却下（plan「結論」）
