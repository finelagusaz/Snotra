# plan — #1026 起動計器の event と ok を同じ 1 か所から導く

## 目的と受け入れ条件

- **導出点は 1 つである**: イベント名（`startup:ready` / `startup:failed`）と `data.ok` を、`startup.rs` の 1 つの `match` が組で決める。`finish` から `if outcome.is_ok()` による名前の分岐が消える
- **`finish` は名前を知らない**: 純粋核が返した「名前 + payload」の組をそのまま `trace` へ渡すだけにする
- **8 通りすべてを単体テストで固定する**: `Ok(())` と `StartupFailure` の 7 variant のそれぞれについて、出る名前と `data.ok` / `data.reason` の対応を固定する。これが PR を止める最初の検知器になる（`research.md`「敵対的調査の採否」）
- **ハーネスとの契約は変えない**: イベント名の文字列、`ok` / `reason` のキーと値、イベント名が 2 つであること（ADR §1）は変えない

## 変更ファイルと対象シンボル

- `src-tauri/src/startup.rs`
  - 新設 `fn terminal(outcome: Result<(), StartupFailure>) -> Terminal`
    - `Terminal` は `{ event: &'static str, ok: bool }` の組
    - 名前と `ok` を決める唯一の `match` を持つ
  - 新設 `Timeline::terminal_line(&self, post_main_elapsed, outcome) -> (&'static str, serde_json::Value)`
    - `terminal(outcome).event` と `to_json` の payload を組で返す
  - `Timeline::to_json`: `ok` を `terminal(outcome).ok` から入れる（`outcome.is_ok()` の直読みをやめる）。`reason` は現行のまま
  - `finish`: `let (event, payload) = t.terminal_line(post_main, outcome)` を取って `trace(event, payload)` するだけにする。`HotkeyRegister` のマーク条件（`reached_the_arm`）は、名前とは別の概念なので現行のまま残す
  - doc の更新: `StartupFailure` の doc（「イベント名がこの意味を運ぶ」）と `finish` の doc に、導出点が `terminal` であることを書く
  - テスト
    - 新設 `every_outcome_pairs_event_and_ok_in_one_place`: 8 通りすべてについて `terminal_line` が返す名前と payload の `ok` / `reason` を、期待表（`Ok` → `startup:ready` / `true` / `null`、各 `Err` → `startup:failed` / `false` / その `reason()`）と照合する
    - 7 variant の列挙を `failure_reasons_are_stable_and_unique` と 1 つのテスト用定数で共有する（手書き列挙が 2 部にならないように）
- `scripts/lib/SnotraStartupContract.psm1`（コメントのみ。未確定欄の裁定で検査を残すと決まった）
  - `:55-62` の「別の場所の導出が食い違うことだけを捕まえる」を現行の事実へ書き直す
  - 書き直す中身: 導出は `terminal` の 1 つ。この検査が捕まえるのは、その 1 つの誤りが実バイナリの出力に現れた場合。実バイナリへの適用は `bench-startup.ps1` 経由で、PR を止めない。PR を止める検知器は Rust の単体テスト
- `scripts/bench-startup.ps1:162-167`（コメントのみ）: 上と整合させる

## 実装順序

1. テストを先に書く。`terminal` / `terminal_line` はシグネチャだけ置き、中身は `todo!()` にして Red を確かめる（`/implement` Step 2 の規律）
2. `terminal` と `terminal_line` を実装し、`to_json` の `ok` と `finish` を差し替えて Green にする
3. 変異を注入し、新テストが落ちることを確かめる（後述）
4. doc を更新し、ハーネスのコメントも更新する

## 不変条件と異常系

- **一度きり性（`FINISHED` の CAS）は変えない**。組は 1 回の `finish` 呼び出しの中で決まる
- **`HotkeyRegister` のマーク条件は名前と独立**。`outcome.is_ok() || reached_the_arm` は「arm まで到達したか」であって「成功したか」ではない。`terminal` へ畳まない（`symmetric-check` の観点: 名前の分岐とマークの分岐は別の対）
- `trace_enabled()` が偽のときの早期 return（`with_timeline` が `None`）は現行どおりで、名前の導出に到達しない
- 受容する残余（現行 doc の記述を維持）: 7 variant の列挙は型では守れず、variant を足したときの列挙漏れはテストでは落ちない（`failure_reasons_are_stable_and_unique` の doc）

## テスト方針と検証コマンド

- 変異注入（複製ではなく作業中のコードへ当て、確認後に元へ戻す。巻き戻しは内容ハッシュで照合する）
  - `terminal` の `Err` 腕の名前を `startup:ready` にする → 新テストが落ちる
  - `to_json` の `ok` を `terminal` 経由から `true` 固定へ変える → 新テストが落ちる
  - `terminal_line` が別の分岐で名前を作り直す → 新テストが落ちる
  - いずれもコンパイルエラーではなく assert で落ちることを出力で確かめる（`.claude/rules/safety-nets.md`）
- カテゴリ A: `docs/build-commands.md` の fmt / clippy / test / doc
- カテゴリ C: `smoke:startup`（`finish` に触れるため）
- カテゴリ E: ハーネスのコメントを変える場合は `npm run test:powershell`
- カテゴリ F: `npm run governance:check`

## `SPEC.md`・関連文書の更新要否

- `SPEC.md`: 不要（起動計器の記述は無い。`git grep` 済み）
- `docs/adr/ADR-startup-instrument-contract-shape.md`: 不要（凍結された歴史。§1 の結論は変わらない）
- `PERFORMANCE.md:2714`: 不要（イベント名の列挙だけで、導出点には触れていない）

## 作業項目

### フェーズ 1 — 純粋核とテスト

- [x] テスト用定数（7 variant の列挙）を置き、`failure_reasons_are_stable_and_unique` をそれへ寄せる
- [x] `every_outcome_pairs_event_and_ok_in_one_place` を書き、`terminal` / `terminal_line` を `todo!()` で置いて Red を確かめる
- [x] `terminal` / `terminal_line` を実装し、`to_json` の `ok` を差し替えて Green にする
- [x] `finish` を `terminal_line` の組を渡すだけにする
- [ ] 変異 3 種を注入し、assert で落ちることと巻き戻しを確かめる（主エージェントは同じ木へ注入しない——`/implement` 3b。検証の委譲先が実施する）

### フェーズ 2 — 文書

- [x] `StartupFailure` と `finish` の doc を導出点に合わせて更新する
- [x] `SnotraStartupContract.psm1:55-62` と `bench-startup.ps1:162-167` のコメントを、導出点が 1 つになった現行の事実へ更新する（判定・Pester は変えない）
- [x] 変更で偽になる散文が他に無いか、`to_json（ok）` / `別の場所の導出` の語で `git grep` する

## 未確定（実装前に潰す）

- [x] **ハーネスの event / ok 整合検査（`SnotraStartupContract.psm1:144-157`）を残すか外すか** — **裁定: 残し、説明の散文だけ直す**（2026-09-23・人間レビューの回答「1 推奨案での修正OK」）。判定と Pester は変えず、`scripts/**` はコメントの変更だけになる（issue の 3 つ目の項目。`scripts/**` はセーフティネットの範囲なので、ルート `CLAUDE.md` 最重要ルール 2 により人間の合意で決めた）
  - 理由 1: 導出が 1 つになると、この検査は Rust 単体テストと重なる。ただし実機で出た 1 行を観測できるのはこの検査だけである
  - 理由 2: 費用は 1 ブロックと Pester 3 ケースで、`bench-startup.ps1` は既に観測として走っている
  - 却下した代替: 外す（検査ブロック・Pester の 3 ケース `Tests.ps1:145-165`・`bench-startup.ps1:162-167` のコメントを撤去する）。実機の 1 行を観測する唯一の手段を失うため採らない

## 人間レビュー

- [x] 承認済み — 2026-09-23 / 問い: "ハーネスの event / ok 整合検査を残すか外すか（issue の 3 つ目の項目）。…推奨は「残し、説明の散文だけ直す」でございます。／計画の承認。`workspace/plan.md` へ注釈を書き込んでいただくか、このままでよろしければ承認のお言葉をくださいまし。" / 回答: "1 推奨案での修正OK 2 承認"

## セルフレビュー

- リスク: 通常
  - `/plan-review`「リスク判定」の高リスク条件に当たらない
  - 永続形式・状態遷移・並行処理は変えない（`FINISHED` の CAS には触れない）
  - `scripts/**` はコメントの変更だけで、判定は変えない（裁定で確定。`/plan-review` の追加実行は不要）
- plan-review: 未実施（通常リスク）
- エージェント数: 1（3b の敵対的調査のみ）
- 要対処: 2 件を反映
  - ハーネスが PR を止めない事実を、目的とテストの位置づけに反映した
  - `HotkeyRegister` のマーク条件を `terminal` へ畳まないことを不変条件に明記した
- 未検証
  - `/dry-check`: 新関数の手書き重複は、`outcome.is_ok()` の出現 3 箇所（`:462` / `:535` / `:542`）を読んで判定した。`:535` は別概念（arm 到達）
  - LSP の findReferences は実装時に `finish` / `to_json` の呼び出し元で行う（`finish` の呼び出し 4 点は grep で列挙済み。シグネチャは変えない）
