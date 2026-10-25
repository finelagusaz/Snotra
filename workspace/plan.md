# 計画: #1292 smoke:egui の既定 ExePath を debug の導出へ揃える

## 目的と受け入れ条件

- 目的: `smoke-egui.ps1` の既定 `-ExePath` を、`smoke-startup.ps1` と同じ形にする。具体的には `''` から `Resolve-SnotraCargoExecutable -Profile debug` で導き、起点は `$PSScriptRoot` とする。これでカテゴリ C の手順（`cargo build -p snotra` → `npm run smoke:egui`）が、直前に建てたバイナリを測るようになる
- 受け入れ条件
  1. カテゴリ C の手順をそのまま打つと、`smoke:egui` の出力の先頭に `本体: <その作業コピーの target>\debug\snotra.exe` が出て、その本体で検査が走る
  2. CI の smoke-egui job が測るバイナリは変わらない。`smoke.yml:116` は `-ExePath target/release/snotra.exe` を明示して渡しており、明示された値の意味は変えない
  3. 既定を直書きへ戻す退行を `npm run test:powershell` が赤にする。赤になる理由は既存のソース述語の `Should` であって、構文エラーではないこと
  4. `docs/build-commands.md` に「smoke:egui の既定は release」と読める記述が残らない

## 範囲（ユーザー判断 2026-10-25「最小＋follow-up issue」）

- この PR: `smoke-egui.ps1`、`SnotraSmoke.Tests.ps1` の一覧へ 1 本足すこと、`docs/build-commands.md`
- この PR でやらないこと（follow-up issue へ）
  - Pester の母集団を「resolver を呼ぶスクリプトの手書きの一覧」から、`[string]$ExePath =` の宣言を持つスクリプトから導く形へ変える件。セーフティネットの変更なので合意が要る
  - `manual-smoke.ps1:3` と `measure-memory-stages.ps1:44` の直書きを揃える件
- 触らないもの
  - `docs/adr/ADR-harness-executable-path-derivation.md`（凍結された歴史）
  - `.github/workflows/smoke.yml`（明示渡しなので変更は不要）
  - `SPEC.md`（製品の挙動は変わらない）

## 変更ファイルと対象シンボル

1. `scripts/smoke-egui.ps1`
   - `param()`: `[string]$ExePath = "target/release/snotra.exe",` を `[string]$ExePath = '',` にし、上に 1 行コメントを置く。文言は `smoke-startup.ps1:9` と同じ「空なら debug 本体を導く（#1179）。**理由の正本は `Resolve-SnotraCargoExecutable` の doc**。」とする
   - `Import-Module` の後に次を置く。順番は `smoke-startup.ps1:23-33` と同じにする
     - `$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path`
     - `if (-not $ExePath) { $ExePath = Resolve-SnotraCargoExecutable -RepositoryRoot $repoRoot -Profile debug }`
     - 存在の確認は `Test-Path -LiteralPath` に変え、`throw "Executable not found: $ExePath（先に cargo build -p snotra）"` とする
     - `$ExePath = (Resolve-Path -LiteralPath $ExePath).Path` で絶対形にし、`Write-Host "本体: $ExePath"` で出す
   - 既存の 63〜64 行目の確認は、上の確認で置き換える。二重に置かない
2. `scripts/lib/SnotraSmoke.Tests.ps1`
   - Describe「起動ハーネスの既定 ExePath はスクリプトの住むコピーから導かれる」の `-ForEach` へ `'smoke-egui.ps1'` を足す
   - 同じ Describe のヘッダコメントで、除外と対象を語る箇所を、事実に合わせて見直す
   - 572〜574 行目のコメント「`smoke-egui.ps1` が release ビルドで見ている」を、CI では release、と限定する形に直す（3b の B1）
2b. `scripts/lib/SnotraSmoke.psm1`
   - `Start-SnotraProcess` の上、367〜369 行目のコメント「release を使う `smoke-egui` が…一度も踏んでいない」を直す（3b の B1）。変更後はローカルの既定が debug になるので、CI の smoke-egui（release）に限った事実として書き直す。ローカルの debug では `WindowStyle Hidden` が同じ窓を隠す、という事実と矛盾しない形にする
3. `docs/build-commands.md`
   - 210 行目: `smoke:egui` の行の「既定 ExePath = target/release」を、debug を導くことが分かる形に直す
   - 229〜234 行目の段落: 対象に `smoke:egui` を加え、「この 2 つでは」のような数で言う書き方をやめる。名指すか、正本（resolver の doc）を指す
   - 279 行目の注は、そのまま真であることを読んで確かめる

## 実装順序

1. 先に `SnotraSmoke.Tests.ps1` の一覧へ `'smoke-egui.ps1'` を足し、`npm run test:powershell` が現状の smoke-egui で赤になることを確かめる（Red）。赤の理由が述語 (1) の `Should`（既定が空リテラルでない）であることを出力で読む
2. `smoke-egui.ps1` を直して Green にする
3. `docs/build-commands.md` を直す
4. 検証（下記）

## 不変条件と異常系

- 明示された `-ExePath` の意味は変えない。相対パスは cwd 相対のまま扱う。CI と `-ResultsQuery ''` のフォールトインジェクション手順（`docs/build-commands.md` のスモーク運用メモ）は、どちらも明示で渡すので影響しない
- **フォールトインジェクションの手順と、新しい導出の順序が干渉しないこと。** `-ResultsQuery ''` の guard は「アプリを起こさずに落ちる」ことが性質であり、`-ExePath` には既存のファイルを明示で渡す。既定の枝の導出（`cargo metadata` の呼び出し）が guard より前に走っても、明示した場合は走らない。既定のまま `-ResultsQuery ''` を打つ場合は、cargo metadata を 1 回呼んでから落ちるが、アプリは起こさない
- debug が無いときは、復旧の手順つきで `throw` する（fail-closed）
- debug が古いまま残っていると、それを測る。これはカテゴリ C の手順でビルドを先に打つことで防ぐ。測った本体のパスが出力に出るので見える
- `cargo metadata` が失敗したときは、resolver が `throw` する（既存の挙動）

## テスト方針と検証コマンド

- 退行の検知器: 既存のソース述語（3 つ）に `smoke-egui.ps1` を載せる。新しい assertion は足さない
- 変異注入（`/implement` の委譲先が行う）: 複製の上で `smoke-egui.ps1` の既定を `"target/release/snotra.exe"` へ戻し、Pester が赤になることを確かめる。赤の理由が `Should` であることを出力で読む。`$repoRoot` の導出を cwd 起点（`(Get-Location).Path`）へ変える変異も同じように確かめる
- 実機の確認: `cargo build -p snotra` → `npm run smoke:egui` を既定のまま打ち、出力の `本体:` が `C:\workspace\Snotra\target\debug\snotra.exe` を指し、検査が緑になること
- カテゴリの検証（`docs/build-commands.md`）
  - E/C: `npm test`、`npm run test:powershell`
  - F: `npm run governance:check`
- CI が変わらないことの根拠: `smoke.yml:116` の明示渡しを読み直す。PR の Smoke workflow は、`scripts/smoke-egui.ps1` が paths に入るので自動で起動する。その緑を PR 本文のチェックリストで確かめる

## SPEC.md・関連文書の更新

- `SPEC.md`: 不要（開発用スクリプトの既定値であり、製品の挙動ではない）
- `docs/build-commands.md`: 要（上記）
- ADR: 新設しない。却下した案 (b) の理由はこの計画と PR 本文に書く。否定の知識として残す重さは無い

## 作業項目

### Phase 1 — 検知器を先に赤にする

- [ ] `SnotraSmoke.Tests.ps1` の `-ForEach` へ `'smoke-egui.ps1'` を足し、ヘッダコメントの対象と除外の記述を事実に合わせる
- [ ] `npm run test:powershell` が smoke-egui の既定の述語で赤になることを、出力で確かめる

### Phase 2 — smoke-egui を導出の形へ揃える

- [ ] `smoke-egui.ps1` の `param()` の既定を `''` にし、コメントを置く
- [ ] `Import-Module` の後へ `$repoRoot` の導出・resolver の呼び出し・`-LiteralPath` での存在の確認・絶対形への正規化・`本体:` の出力を置き、旧い確認を消す
- [ ] `npm run test:powershell` が緑になる

### Phase 3 — 文書

- [ ] `docs/build-commands.md` の 210 行目と 229〜234 行目の段落を直し、279 行目の注が真のままであることを読んで確かめる
- [ ] `SnotraSmoke.Tests.ps1:572-574` と `SnotraSmoke.psm1:367-369` のコメントを、CI では release、と限定する形に直す
- [ ] `npm run governance:check` が緑になる

### Phase 4 — 検証

- [ ] `cargo build -p snotra` → 既定のまま `npm run smoke:egui` を 3 回連続で回す。毎回 `本体:` の行が `target\debug\snotra.exe` を指し、緑であることを確かめる
- [ ] `npm test` が緑になる
- [ ] follow-up issue の本文を書く（母集団を導出する検知器と、残り 2 本の直書き）。起票はユーザーの確認を経る

## 未確定（実装前に潰す）

- [x] 3b の敵対的調査の所見を反映する — B1（コメント 2 か所）は変更ファイルに加えた。B3 は research の書き方を弱めた。B2 は follow-up issue へ回す。W1 は実機の確認を 3 回にした（`workspace/research.md` の 3b の表）
- [x] debug の既定で smoke:egui が安定するか（W1） — 測り方: Phase 4 で 3 回連続で回す。PR #1291 は `-ExePath target/debug/snotra.exe` で緑だった。PR #1291 の base は #1290（`55e5c4a`、#1274 の Z を押しっぱなしにするブロック）で、head もそれを含む（`git merge-base --is-ancestor` で実測）。ただし、検証を走らせた時点のブランチが #1290 を含んでいたかまでは、記録から決められない。判断: 既定を debug にする根拠として足りる。Phase 4 の 3 回で 1 回でも赤なら、実装を止めてユーザーへ戻す（その場合は案 (b) との比較からやり直す）

## 人間レビュー

- [x] 承認済み — 2026-10-25 / 問い: "計画書へ注釈を書き込んでくださるか、このまま承認してくださいますか。" / 回答: "OK"

## セルフレビュー

- リスク: 通常（開発用スクリプトの既定値と、既存の検知器の一覧へ 1 本足すだけ。永続化・並行性・状態遷移・網羅性の要件は無い。検知器の母集団を変える件は範囲の外へ出した）
- plan-review: 未実施（通常リスク）
- エージェント数: 1（3b）
- 照合 5 点
  1. issue の受け入れ条件 2 つは、受け入れ条件 1・2 と Phase 2・4 が受け持つ
  2. 境界条件: release だけが在る・debug だけが在る・両方とも無い・明示の相対パス・`-ResultsQuery ''` の注入。この 5 つを「不変条件と異常系」で扱った
  3. 新しい状態やリソースは無い。導出は `cargo metadata` の 1 回の呼び出しで、失敗すれば resolver が throw する
  4. より単純な既存パターン: `smoke-startup.ps1` の形をそのまま写す。それ自体が既存のパターンである
  5. 壊してはならない不変条件（CI が release を測ること・既定が直書きへ戻らないこと）には、`smoke.yml:116` の明示渡しと Pester の述語がある
- 条件別チェック: セーフティネット（Pester の一覧）の変更は、既存の検知器の母集団へ 1 本足すだけである。足す前に「壊れたとき緑のまま推移するか」を問うと、推移する（#1179 と同じ型）。ゆえに置く。変異注入は `/implement` の委譲先が行う
- 要対処: 3b の B1（2 か所）と W1 を反映した
- 未検証: debug で 3 回連続して安定するかは実装時に測る
