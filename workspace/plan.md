# 計画 — issue #1404 smoke-egui.ps1 の検査と段取りの境目を整理する

調査: `workspace/research.md`。

## 結論（分ける／寄せる／現状維持）

**ファイルは分けない。`scripts/lib/` へも寄せない。スクリプトの中で境目を読めるようにする。**

- 検査を別スクリプトに分けない: シナリオ 1 の検査は 1 プロセスと状態（`$failures`・`$resultsChecked`・`$vks`・`$pathTyped`・入力欄の中身・ウィンドウの可視性）を順に受け渡す。分けると起動と hotkey 待ち（CI で最大 25s の予算）をやり直すか、受け渡しの仕組みが要る
- シナリオ 2 だけを別ファイルにもしない: 単独で切り出せるのはここだけだが、`Show-FailureEvidence`・観測予算の引数・`$failures` の集計・終了待ちを共有している。入口が 2 本になると `smoke.yml` のステップ・`package.json`・#1293 / #1296 の検査の母集団・文書の名指しが増える。費用に対し、変更履歴に益の実例が無い（research「変更履歴から見た…」）
- `SnotraSmoke.psm1` へ寄せない: 寄せる候補の段取りは smoke-egui.ps1 だけが使う。寄せると export・Pester・`Mock -ModuleName` の費用が出る（`ADR-smoke-module-not-split`）
- 中で行うこと: 繰り返される段取りをスクリプト内関数にする（前例は同ファイルの `Test-SnotraNoTraceEventInWindow`）。検査ごとの見出し行をそろえる。冒頭に構成（シナリオ・受け渡す状態・順序の制約）を書く。ずれた行番号参照を直す
- 却下したもの（成功文言の自動組み立て）: 検査を足すたびに末尾の成功文言を 1 行直す編集は、履歴で 3 回起きた。走った検査の名前を集める変数で組み立てれば消えるが、受け渡す状態が 1 つ増える。1 行の編集の費用より大きいと判断した

## 目的と受け入れ条件

- AC1: smoke-egui.ps1 の各検査ブロックの先頭に、同じ形の見出し行がある。シナリオの境目にも見出し行がある。検査の判定ロジック（待つイベント・断言・失敗文言・`$failures` のゲート）は変えない
- AC2: research の「繰り返される段取り」の表のうち、1 キー打鍵・増分の待ち・入力変化の読み取りが、スクリプト内関数の呼び出しになっている
- AC3: 冒頭のコメントに、シナリオの構成・検査をまたいで受け渡す状態・順序の制約が書いてある。検査の一覧は写さない（正本は見出し行）
- AC4: smoke-egui.ps1 と smoke-startup.ps1 のコメントに行番号での参照が残っていない（`grep -nE '\.(ps1|rs):[0-9]|[0-9]+〜[0-9]+ 行|[0-9]+-[0-9]+ 行' scripts/smoke-egui.ps1 scripts/smoke-startup.ps1` が 0 件）
- AC5: 変更前後で、実機の通し実行が緑で、出力の検査結果の行（`Hotkey VKs`・`toast ありの show #1/#2` の DWM と payload height・`プロファイルへの書き込みを確認`・`egui smoke passed`）が一致する（DWM の値は DPI 由来なので同じ機械で比べる）
- AC6: 文書化されたフォールトインジェクション 2 つが変更前と同じ性質で赤になる。`-ResultsQuery ''` は起動前の同じ throw 文言、`-StartupWaitMs 0 -ObserveTimeoutMs 1` は赤と「失敗時の証拠」ブロックの出力（後者の赤の理由は main でも試行ごとにぶれるので、理由の一致は求めない・実装時に実測）
- AC7: `docs/build-commands.md` の smoke-egui の「流れ」が、検査を列挙せずスクリプトの見出し行を正本として指す
- AC8: 「分けない」決定とその却下理由が ADR に残っている

## 変更するファイルと対象シンボル

1. `scripts/smoke-egui.ps1`
   - 新設のスクリプト内関数（名前に `Snotra` 接頭辞を付けない。モジュールの export と区別するため）
     - `Send-KeyTap -VirtualKey <byte> [-HoldMs <int> = 50]`: down → `HoldMs` → up。後ろの sleep は持たない（呼び出し側の既存の sleep をそのまま残す）
     - `Invoke-AndWaitNextEvent -Path <string> -EventName <string> -Action <scriptblock>`: `Get-SnotraTraceEventCount` で数え、`& $Action`、`Wait-SnotraTraceEvent -MinMatchCount (件数 + 1) -TimeoutMs $ObserveTimeoutMs` の結果（イベントか `$null`）を返す。throw するか `$failures` へ積むかは呼び出し側に残す（箇所ごとに違うため）
     - `Get-InputChangesSince -Path <string> -AfterSeq <long>`: `Read-SnotraTraceSnapshot` から `egui_input:changed` かつ `seq > AfterSeq` を `@()` で返す
     - `Format-InputChanges -Events <array>`: `"$before->$after"` を `', '` で連結した文字列を返す
   - 置き換える箇所
     - 1 キー打鍵: Backspace ×2・クエリ字・`c`・`\`・Escape（検査 6・押しっぱなしの Z の終わり・打鍵を伴わない文字の終わり・シナリオ 2）・X・押しっぱなしの Z の 2 回目（`-HoldMs 150`）
     - 増分の待ち: 押しっぱなしの Z の手順 1・3・4・9、打鍵を伴わない文字の show と終わりの hide、シナリオ 2 の show と hide
     - 入力変化の読み取り: Ctrl+H・押しっぱなしの Z・打鍵を伴わない文字の 3 か所（断言の式は各ブロックに残す）
   - 見出し行: `# === シナリオ 1: 既定プロファイル ===` / `# === シナリオ 2: toast あり ===` と、検査ごとに `# --- <検査名>（#NNN）---`。序数は付けない（`.claude/rules/governance-docs.md`）
   - 冒頭コメント（既存の 41-57 行の説明を置き換える）: 2 シナリオの構成、`$failures` が 0 件のときだけ次へ進むゲート、受け渡す状態、research「検査をまたいで受け渡す状態と順序の制約」の 4 制約
   - 行番号参照 5 件を記号名へ: 76 行 → `visual-check-colors.ps1` の `$profileDir` を置く箇所、333 行 → `visual-check-colors.ps1` の `Test-SeedHealth`、346 行 → `commands/window.rs` の `cmd:launch_settings_process:*` の 4 イベント（行番号を外し、イベント名だけを残す）、399 行 → 「results 検査の再注入前の Backspace と同じ作法」、918 行 → `visual-check-colors.ps1` の `*.bin` の確認（着地先は実装時に grep で確かめる）
2. `scripts/smoke-startup.ps1`: 同じパターンの行番号参照 2 件（47 行の `visual-check-colors.ps1:54-58`、164 行の `visual-check-colors.ps1:292-304`）を記号名へ。コメントだけで、コードは触らない
3. `docs/build-commands.md`: smoke-egui の「流れ」の項を、見出し行を正本として指す 1 文へ直す
4. `docs/adr/ADR-smoke-egui-not-split.md`（新規）: 文脈（#1404・測定点）、決定、却下した代替案（検査ごとのスクリプト化・シナリオ 2 の別ファイル化・`SnotraSmoke.psm1` への移動・成功文言の自動組み立て）と理由、帰結（分割を再検討するときに数え直す費用）

触らない: `.github/workflows/smoke.yml`・`package.json`・`scripts/lib/*`・`AGENTS.md`・上記以外のスクリプト。`scripts/` 直下に `.ps1` を足さない（`smoke.yml` の `paths` に載らないため）。smoke-egui.ps1 の `param()`・`$ExePath` の宣言・`$repoRoot =` の行・`Resolve-SnotraCargoExecutable` の呼び出し行は触らない（#1293 / #1296 の Describe が形を縛る）。

research の「検査 1〜10」の通し番号はコード・ADR・PR 本文へ写さない。そこでは検査名で書く（序数の引用になるため）。

## 挙動の差（意図して入れる 1 点）

- シナリオ 2 の Escape は今 down と up の間に sleep が無い（`27` を直書き）。`Send-KeyTap` へ寄せると 50ms が入る。シナリオ 1 の Escape と同じ間隔になるだけで、待つイベントと断言は変わらない。AC5 の通し実行で確かめる

## 実装の順序

1. 変更前の基準を取る（debug 本体のビルド・通し実行・フォールトインジェクション 2 つ。出力を `workspace/` に保存）
2. 見出し行・冒頭コメント・行番号参照・古い理由と誤字（コメントだけ。トークン列の差 0 で確かめて区切る）
3. スクリプト内関数を足し、箇所を置き換える（挙動に触る段を後ろに分け、差分を読みやすくする）
4. 変更後の通し実行とフォールトインジェクション、出力の突き合わせ
5. `docs/build-commands.md` と ADR

## 不変条件と異常系

- 各検査のゲート（`$failures.Count -eq 0`・`$resultsChecked`・`$null -ne $pathTyped`）の条件と位置は変えない
- 押しっぱなしの Z の `$zHeld` を立てる位置（down の注入より前）と `finally` の up は変えない。手順 2・5 の Z の down は単独の `Send-SnotraKey` のまま残す（打鍵ではなく押しっぱなし）
- 増分の待ちでは、件数を `& $Action` の**前**に数える（後に数えると引き起こした 1 件が基準に紛れる・シナリオ 2 のコメント）。関数の中で順序を固定する
- 関数の戻り値を受ける側は `@()` で包む（関数境界で配列が展開される・シナリオ 2 のコメント）
- 失敗文言は一字も変えない（CI のログを読む人と、過去の issue の文言検索のため）
- 検査 6 の Escape の待ちは presence のまま（増分へそろえない）
- 押しっぱなしの Z のブロックの番号付き手順（`# 1.`〜`# 9.`）の番号と、手順 2 の位置は変えない（`snotra-egui-runtime/CLAUDE.md` が「#1274 の節の手順 2」と名指す）
- hotkey 注入ブロックのコメント（trace から VK を採る理由）と、シナリオ 1 の終了待ちのコメントは消さない（`measure-memory-stages.ps1` が「あちらの注入箇所のコメント」、`SnotraSmoke.psm1` の `Stop-SnotraProcessAndWait` の doc が「機序の正本は `scripts/smoke-egui.ps1` のコメント」と散文で指す）

## テスト方針と検証コマンド

- 基準と変更後: `cargo build -p snotra` → `pwsh -NoProfile -File scripts/smoke-egui.ps1`（画面ロックなし。このリポジトリのビルド成果物の snotra を止める）。出力を保存し、AC5 の行を突き合わせる
- フォールトインジェクション: `pwsh -NoProfile -File scripts/smoke-egui.ps1 -ResultsQuery '' -ExePath <既存ファイル>` と `-StartupWaitMs 0 -ObserveTimeoutMs 1`。赤の理由（throw の文言・証拠ブロック）が変更前と同じであること
- コメントだけの段（Phase 2）の確かめ: コメントと空白を除いたトークン列を main と比べ、差 0 であること。スクリプトは独立導出の結果 `workspace/plan-review-1404-derive.md`「3. 挙動を変えないことの確かめ方」にある（委譲先が無変更で差 0・待ち時間の変更で赤になることを実測済み）。あわせて `#NNN`・バッククォート語・ADR 名の集合差（`.claude/rules/governance-docs.md` の `comm` の手順）で、落とした識別子が意図した行番号参照だけであることを確かめる
- `pwsh -NoProfile -File scripts/run-pester.ps1`（既定 ExePath と PositionalBinding の Describe を含む）
- `npm run governance:check`
- 変異注入（委譲先が行う）: `Send-KeyTap` の up を消す → 通し実行が赤になる／`Invoke-AndWaitNextEvent` で件数を `& $Action` の後に数える → 押しっぱなしの Z かシナリオ 2 の待ちが切れて赤になる／`Get-InputChangesSince` の `seq` 比較を `-ge` にする → Ctrl+H の断言が赤になる。赤の理由が狙った断言であることを出力で確かめる
- CI: `smoke.yml` がこのファイルの変更で自動起動する（PR 本文のチェックリストで確かめる）

## SPEC.md・関連文書の更新

- `SPEC.md`: 不要（製品の挙動は変えない）
- `docs/build-commands.md`: 要（上記 2）
- `docs/adr/`: 新規 1 本（上記 3）
- PR 本文の冒頭に「マージは合意してから」（smoke-egui.ps1 は `.claude/rules/safety-nets.md` の対象・ルート `CLAUDE.md` 最重要ルール 2）

## 作業項目

### Phase 1 — 基準
- [x] debug 本体をビルドし、変更前の通し実行とフォールトインジェクション 2 つの出力を `workspace/` に保存する

### Phase 2 — 境目（コメントだけ・コードのトークンは変えない）
- [x] シナリオと検査の見出し行をそろえる
- [x] 冒頭コメントを構成・受け渡す状態・順序の制約へ書き換える
- [x] smoke-egui.ps1 の行番号参照 5 件と smoke-startup.ps1 の 3 件（実装中に 139 行の `commands/window.rs` を追加で発見）を記号名へ直し、AC4 の grep が 0 件であることを確かめる
- [x] シナリオ 1 の終了待ちのコメントへ、2 シナリオのプロファイルは別なので単一インスタンスの mutex は分かれていること（#1306・`isolated_profile_key`）と、残る共有資源がホットキー（両プロファイルとも Alt+F12）であることを書き足す。単一インスタンスの機序の説明は残す
- [x] シナリオ 2 のコメントの誤字「この scriptの`catch`」を直す
- [x] トークン列の差 0 と、識別子の集合差を確かめる — smoke-egui 3072/3072・smoke-startup 458/458 で差 0。集合差で落ちたのは置き換えた冒頭の流れの説明にあった `egui_hide:done` `egui_results:hide` だけ（コードには残る）。基準: 通し実行 緑・`-ResultsQuery ''` は起動前 throw・`-StartupWaitMs 0 -ObserveTimeoutMs 1` はシナリオ 2 の throw で赤（`workspace/baseline-*.txt`）

### Phase 3 — 段取りの関数化
- [x] `Send-KeyTap`・`Invoke-AndWaitNextEvent`・`Get-InputChangesSince`・`Format-InputChanges` を足す
- [x] 1 キー打鍵・増分の待ち・入力変化の読み取りの各箇所を置き換える — 計画との差: 押しっぱなしの Z の 2 回目の down/up は `$zHeld` と組なので `Send-KeyTap` にせず残した（コメントで理由を示す）

### Phase 4 — 検証
- [x] 変更後の通し実行・フォールトインジェクション 2 つを実行し、AC5・AC6 を突き合わせる — 通し実行 緑・AC5 の行は基準と一致（VK 18,123・DWM 602x88・payload 86 ×2・index.bin, window.bin・成功文言）。空クエリは同じ throw 文言（行番号だけ 95→100）。1ms 予算は変更後 3 回とも held-key の不合格、main の版は 2 回で throw と held-key に割れた（もともと競争）
- [x] run-pester と governance:check を実行する — test:powershell 168 passed・governance:check 17 件 passed

### Phase 5 — 文書
- [x] `docs/build-commands.md` の「流れ」を見出し行の参照へ直す
- [x] `docs/adr/ADR-smoke-egui-not-split.md` を書く

## 未確定（実装前に潰す）

- [x] 敵対的調査（3b）の所見を research と計画へ反映する — 壊せた 7 件をすべて採った（採否は research「敵対的調査（3b）の所見と採否」）。計画への反映: 行番号参照を 5 件 + smoke-startup の 2 件へ、直下に `.ps1` を足さない、#1293/#1296 が縛る行を触らない、通し番号を写さない

## 人間レビュー

- [x] 承認済み — 2026-11-08 / 問い: "#1404 の計画（workspace/plan.md）を承認しますか？ 結論は「smoke-egui.ps1 は分けない・lib へも寄せない・スクリプトの中で境目を読めるようにする」です。独立レビューと判断が割れたのは 1 点だけです。繰り返される段取り（1 キー打鍵 11 か所・増分の待ち 8 か所・入力変化の読み取り 3 か所）を、スクリプト内の関数 4 本にまとめるかどうかです。まとめると、シナリオ 2 の Escape に 50ms の間隔が入ります。挙動に触れるのはこの 1 点だけです。" / 回答: "関数化も含めて承認 (Recommended)"

## plan-review 結果

- リスク: 高（CI が走らせるスクリプト＝`.claude/rules/safety-nets.md` の対象を変える）
- レビュー方式: 独立導出 1 体（Step 2b・結果は `workspace/plan-review-1404-derive.md`）
- エージェント数: 1

### 要対処（反映済み）
- 導出 ∖ plan: シナリオ 1 の終了待ちの理由が #1306 以後は当たらない — 計画へ追加（Phase 2）— `snotra-core/src/config/location.rs` の `isolated_profile_key_from` が絶対パスの FNV-1a を鍵にし、`src-tauri/src/main.rs` がそれで identifier（＝mutex 名）を分けることを読んで確認。待ちは残し、理由の文だけ直す
- 導出 ∖ plan: 散文で指されているコメントと手順番号（`snotra-egui-runtime/CLAUDE.md` の「手順 2」・`measure-memory-stages.ps1`・`SnotraSmoke.psm1`）を壊さない — 不変条件へ追加
- 導出 ∖ plan: コメントだけの変更をトークン列の比較で確かめる手段 — テスト方針へ追加し、Phase をコメントだけの段と関数化の段に分けた
- 導出 ∖ plan: 誤字 1 件 — Phase 2 へ追加

### 軽微
- plan ∖ 導出: 段取りのスクリプト内関数化（`Send-KeyTap` ほか 4 本）は導出に無い。導出は見出し・冒頭・参照の整理だけで足りるとした。計画は、issue の「共通の段取りの境目が読める」には段取りを名前の付いた呼び出しにするのが直接効くと判断して残す。挙動に触るのはこの段だけなので、Phase を分けて通し実行で確かめる。**人間の判断に委ねる点として承認の問いに含める**
- 判断の不一致: ADR を新設するか既存へ寄せるか — 導出も新設寄り。ADR は凍結された記録なので既存の `ADR-smoke-module-not-split` には足さず、新設する

### 未検証
- 2 プロファイルを同時に起動したときにホットキー登録が実際に失敗すること（終了待ちの残る理由）は実測していない。Win32 の `RegisterHotKey` は同じ組み合わせをシステムで 1 つにしか許さないという仕様からの導き。コメントでは仕様として書き、実測値とは書かない

### 範囲外（別 issue の候補）
- シナリオ 2 は `hotkey:registered` の `ok` を見ていない（シナリオ 1 は見る）。直すと挙動が変わるので本 issue では扱わない

### 判断
- 実装着手: 可（人間の承認待ち）

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体
- エージェント数: 1（ほかに 3b の 1 体）
- 要対処: 4 件・すべて計画へ反映（上記）
- 未検証: ホットキー登録の競合の実測（上記）
- 散文の全件列挙: 語 4 / 総件数 114 行 / ★未掲載 41（計画へ足した 0。生きた文書・スクリプトの該当行は、ファイル名と挙動を変えないので偽にならないことを 1 件ずつ確認。残りは凍結された ADR と `docs/superpowers/`）
- 試行（#1300・高リスクのみ・独立レビュー 1 回ごとに 1 行）: 方式 Step 2b / 設計変更 1（Phase をコメントだけの段と関数化の段へ分けた） / 主の見落とし 3（終了待ちの古い理由・散文で指されるコメント/手順番号・誤字）
- [x] レビュー round 1（sha feaf85f6）: verify:tree C 1 回目 red（smoke:egui のパスクエリ・blur による hide の一過性）→ 単独再実行 green。委譲: Critical/High 0・Medium 2（既存構造のため範囲外 1／AC3 どおり維持 1）・Low 3 を修正。変異 3/3 赤・対照 緑
- [x] レビュー round 2（sha 85b51300）: verify:tree C green。委譲: Low 3 件の解消を確認・新しい Critical/High 0・変異 2 を新しい形へ当て直して赤・巻き戻し照合一致
