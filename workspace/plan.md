# plan — #1403 SnotraSmoke.psm1 のコメントを、テストを動かすのに要る内容へ絞る

## 目的

調査（`workspace/research.md`）の結果、モジュールは分けない（ユーザー判断 2026-11-07: 「Smokeの文言だけ refine-ja 使ってシンプルにしようか。テストに必要な操作などに絞りたい」）。代わりに `scripts/lib/SnotraSmoke.psm1` のコメントを、呼び出し側がテストを組み・動かすのに要る内容（契約・前提・不変条件・その形でなければならない理由）へ絞り、文を平易にする。

## 受け入れ条件

1. `SnotraSmoke.psm1` のコメント以外は 1 トークンも変わらない（下の「不変条件」の検知器で測る）。throw / Write-Warning / Write-Host の文言も変えない（Tests が文言で照合している）
2. 各関数のコメントが `docs/comment-guidelines.md` に沿う: 経緯は `#NNN` に委ね、現在の形の理由と契約だけを残す。数・列挙・他所の現在形の写しを書かない
3. 外部から「正本」として指されている内容は残る（下の一覧）
4. `npm run test:powershell` と `npm run governance:check` が green

## 変更するファイル

- `scripts/lib/SnotraSmoke.psm1` — コメント（`#` 行・`<# #>` ブロック・param 内のコメント）だけ
- 外部の指し先がコメントの書き換えで宙に浮いたときだけ、その指し元の 1 行（下の一覧の範囲）

触らないもの: `SnotraSmoke.Tests.ps1`（承認時に範囲外と決めた）・関数の分割・`docs/adr/`（凍結）・`docs/superpowers/`（履歴）。

## 外部から正本として指されている内容（残す）

| 指し元 | 指す内容 |
|---|---|
| `scripts/{bench-startup,manual-smoke,measure-memory-stages,smoke-egui,smoke-startup}.ps1` の `-ExePath` の行 | `Resolve-SnotraCargoExecutable` の doc（既定の本体を導く理由・作法） |
| `scripts/manual-smoke.ps1:390` | `Get-SnotraRecordedExePath` の doc（絶対パスを記録へ載せない理由） |
| `scripts/smoke-egui.ps1:114`・`scripts/smoke-startup.ps1:56`・`scripts/visual-check-colors.ps1:100` | `New-SnotraVerificationProfile` の上のコメント（config.toml を置く共通の理由） |
| `-ExePath` を絶対形へ正規化する行の「（理由は resolver の doc）」（`bench-startup`・`manual-smoke`・`measure-memory-stages`・`smoke-egui`・`smoke-startup`・`visual-input-metrics`） | `Resolve-SnotraCargoExecutable` の doc の「解決後は絶対形へ正規化して出力へ載せる」 |
| `scripts/smoke-egui.ps1:723` | `Stop-SnotraProcessAndWait` と意図して別にした理由（片方を直すときはもう片方も見る） |
| `scripts/smoke-egui.ps1:862` | `Get-SnotraTraceEventCount` のコメント（件数を数える時点の罠） |
| `scripts/lib/SnotraSmoke.Tests.ps1:178`・`:190` | `Start-SnotraProcess` のコメント（コンソールウィンドウの機序・`-WindowStyle` と `-NoNewWindow` の排他） |
| `docs/build-commands.md`「スモーク運用メモ」・「エージェントが目視項目を自分で実施するとき」 | モジュールの役割の説明（文書側の記述。コメントとは独立） |
| psm1 内の相互参照（例: `Get-SnotraWindowCapture` → 「`Assert-SnotraSessionUnlocked` のコメント」） | 指し先の理由 |

psm1 の中で「正本はここである」と宣言している箇所（`New-SnotraVerificationProfile` の上・`Resolve-SnotraCargoExecutable` の doc・`Get-SnotraRecordedExePath` の doc など）は、宣言ごと残す。消すと、外部の 10 件を超える指名が宙に浮き、それを検出する機構は無い（3b 実測）。Rust 側（`hotkey.rs`・`env.rs` など）と `.claude/skills/implement/SKILL.md` にも psm1 への言及があるが、挙動への言及であってコメントの文面には依存していない。

## 実装の順序

### Phase 1 — 基準を取る

- [ ] 比較スクリプトを scratchpad に書く。`[System.Management.Automation.Language.Parser]::ParseFile` のトークンから Comment を除き、連続する NewLine を 1 つに畳み、Kind と Text を出力する。`#Requires` の行は別に取り出して比べる（Comment トークンなので、畳んだ列には現れない）
- [ ] 現行の psm1 から基準の列を `workspace/tokens-before.txt` へ書き出す
- [ ] 検知器を試す。scratchpad の写しで、(a) コメント行の削除・2 行の統合 → 差なし、(b) throw の文言 1 文字の変更・実コード 1 行の削除・`#Requires` の版の変更 → 差あり、を確かめる

### Phase 2 — 役割ごとに書き換える（refine-ja・技術文書モード）

1 役割ずつ、refine-ja で書き換え案を作ってから当てる。各役割の後にトークン列を比べ、差が無いことを確かめ、`npm run governance:check` を回す。governance:check は `.psm1` のコメントを読み、コードスパンが 2 行にまたがると赤になる（3b 実測）。書き換えで折返しが変わると踏みうる。`<# #>` の閉じを巻き込む書き換えも、トークン列の差で止まる。

- [ ] Win32 相互運用（`Initialize-SnotraNativeInterop`〜`Get-SnotraWindowDpi`）
- [ ] プロファイル・env・起動（`New-SnotraVerificationProfile`〜`Start-SnotraProcess`）
- [ ] 本体の解決・プロセス（`Resolve-SnotraCargoExecutable`〜`Resolve-SnotraExistingProcess`）
- [ ] trace（`Read-SnotraTraceEvents`〜`Get-SnotraTraceEventCount`）
- [ ] 注入・ウィンドウ・捕捉（`Send-SnotraKey`〜`Get-SnotraWindowCapture`）

書き換えの基準（`docs/comment-guidelines.md`「第一原則」「短く保つ」「歴史メモの様式」から）:

- 残す: 呼び出し側の契約（引数の意味・戻り値・失敗時の挙動）、壊すと検査が嘘になる理由、実測で決まった制約（条件つき）、外部から指されている理由
- `#NNN` へ委ねる: 経緯の再現（「以前は〜だった」「是正 C」など）、CI run 番号などの観測の詳細
- 消す: コードの逐語訳、他のコードの現在の状態の写し（数・列挙）
- 太字は、読み飛ばすと事故になる 1 文に限る

### Phase 3 — 検証

- [ ] トークン列の差が無い（`workspace/tokens-after.txt` と比べる）
- [ ] 外部の指し元を 1 件ずつ開き、指す内容が書き換え後のコメントに残っていることを確かめる
- [ ] `npm run governance:check`
- [ ] `npm run test:powershell`

## 不変条件と異常系

- **コメント以外は変えない。** 検知器は、NewLine を畳んだトークン列の比較と、`#Requires` の行の比較である（Phase 1）
- `<# #>` ブロックの `.SYNOPSIS` などの見出し語は残す（Get-Help の利用は 0 件だが、形をそろえておく）
- コメントの中の参照切れ（存在しないパス・ADR）は、governance:check が psm1 を見ないので検出されない（3b 実測）。指し先を足す・変えるときは、そのパスを開いて確かめる

## テスト方針と検証コマンド

挙動は変えないので、新しいテストは足さない。`docs/build-commands.md`「変更後の検証チェックリスト」の E（PowerShell）と F（governance）を回す。変異注入（実装時の委譲レビュー）は、トークン比較の検知器に「文言を 1 文字変えた変異」が検知されることを確かめる。

## SPEC.md・関連文書の更新

- `SPEC.md`: 不要（挙動を変えない）
- `docs/build-commands.md`: 不要（モジュールの役割の説明は変わらない）
- issue #1403: 分けない結論と、調査で見つけた 2 点（`smoke-egui.ps1` の Native 型への暗黙の依存・`Read-SnotraTraceSnapshot` のテストが `SnotraTraceInvariants.Tests.ps1` に居る置き場のずれ）を PR 本文に記録する。直すかどうかは別に決める

## 未確定（実装前に潰す）

- [x] 範囲に `SnotraSmoke.Tests.ps1` のコメントを含めるか — 含めない（承認時の回答「承認・psm1 だけ (推奨)」）
- [x] 外部の指し先の一覧が完全か — 3b で漏れが 3 種見つかった（`smoke-startup.ps1:56`・`visual-check-colors.ps1:100`・resolver の doc を指す 6 行・`smoke-egui.ps1:723`）。主が grep で実在を確かめ、表へ足した
- [x] トークン比較が「コメントだけの変更」の検知器として十分か — 3b で 2 方向とも崩れた。`#Requires` の変更を見逃す（Comment トークンゆえ）。コメント行の削除で NewLine の数が変わり、誤って赤になる。NewLine を畳み、`#Requires` を別に比べる形へ直した（Phase 1）

## セルフレビュー

- リスク: 通常（コメントだけ。挙動は変えず、検知器で測る）
- plan-review: 未実施（通常リスク）
- エージェント数: 1（3b の敵対的調査）
- 要対処: 3b の 4 件を反映（指し先の漏れ・governance:check が psm1 を読む・検知器の 2 方向の穴・件数）
- 未検証: なし
- 散文の全件列挙: 未実施（通常リスク）

## 人間レビュー

- [x] 承認済み — 2026-11-07 / 問い: "#1403 の計画（workspace/plan.md）です。SnotraSmoke.psm1 は分けず、コメントだけを refine-ja で書き換えます。残すのは、呼び出し側の契約・壊すと検査が嘘になる理由・外から正本として指されている内容です。経緯は #NNN に委ねます。コードとメッセージが 1 トークンも変わらないことを、トークン列の比較で確かめます。この計画で進めてよろしいですか？また、範囲に SnotraSmoke.Tests.ps1（965 行）のコメントも含めますか？" / 回答: "承認・psm1 だけ (推奨)"
