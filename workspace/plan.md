# plan — issue #1074（`RowsSnapshot::settled` を実体どおりの名へ改め、`settled` の二義性を消す）

調査: `workspace/research.md` / 敵対枠: `workspace/adversarial-1074.txt`
ブランチ: `chore/rename-rows-snapshot-settled`（base: main = `3e2ec13`）

## 目的

`settled` が `egui_shell` の中で「perf ヒューリスティック（打鍵が止まったか）」と「正しさの述語（最終クエリの結果が反映済みか）」の 2 つを指している。**perf 側を実体どおりの名へ改め、`settled` を正しさの意味だけに残す。** 挙動は 1 ビットも変えない。

**#1039 が入ると `settled` は 3 つになる**ため、その前に済ませる（ユーザー決定・2026-08-13「#1074 を先に片付ける」）。

## 受け入れ条件（issue の 3 つ）

1. `settled` という語が `egui_shell` の中で「最終クエリの結果が反映済みか」の意味だけを指すこと
2. icon worker ゲートの挙動が変わっていないこと（`cargo test -p snotra` が緑・`results_view.rs` の snapshot 差分ロジックが不変）
3. `is_unsettled` の doc から「`RowsSnapshot::settled` と同一視しないこと」の注意書きを**削れること**

## 決定（issue の未確定欄を潰した）

### D1. 新しい名は `input_idle`

意味は `!Debouncer::is_armed()`＝「debounce が予約を持っていない」。

| 候補 | 判定 |
|---|---|
| **`input_idle`**（採用） | 観測している事実をそのまま名乗る。英語として一般的で造語ではない |
| `typing_quiet` | `quiet` は技術用語として定着していない（造語寄り） |
| `input_settled` | `settled` を残すので二義性が消えない |
| `debounce_idle` | 機構を名前に漏らす。消費側（icon worker）は debounce を知る必要が無い |

**射程は doc に正確に書く**——`armed` は `Debouncer::cancel()`（Enter flush 経路・`launcher_controller.rs:1328`）でも下りるので、厳密には「打鍵が止まった」ではなく「**debounce が予約を持っていない**」である。敵対枠の ⚠️（名前が Enter flush 直後まで言い当てているか判定不能）はこの注記で受ける。

### D2. 受け入れ条件 3 は `search_dispatch.rs:79` の bullet を**丸ごと削除**する

あの bullet は「同じ語なので同一視しないこと」を散文で支えるために在る。名前が区別を担えば役目が終わる。**intra-doc link ごと消えるので追随も不要になる。**

### D3. `results_view.rs:36` の doc（「armed→settled の遷移だけで snapshot 差分が生まれ wake が 1 回走る」）は新しい名へ書き換える

記述の中身（遷移で wake が 1 回走るのは意図どおり）は事実のまま保つ。

## 変更ファイルと対象シンボル

| ファイル | 位置 | 変更 |
|---|---|---|
| `src-tauri/src/egui_shell/results_view.rs` | `:32-38` | フィールド宣言 + doc（`pub settled: bool` → `pub input_idle: bool`） |
| 同 | `:52` `:58` `:63` | `matches` の引数・分解束縛・比較 |
| 同 | `:165` `:666-669` | doc の言及・コメント・消費点 `if snapshot.settled {` |
| `src-tauri/src/egui_shell/view.rs` | `:1117-1133` | コメント・生産点の局所変数・struct literal のフィールド名 |
| `src-tauri/src/egui_shell/launcher_controller.rs` | `:194` | `is_search_armed` の doc の言及 |
| `src-tauri/src/egui_shell/search_dispatch.rs` | `:79` | bullet を削除（D2） |

**触らない（正しさの意味・受け入れ条件 1 に合致している）**:

- `search_dispatch.rs` の `struct Settled` と `SearchDispatch::accept` の返り値
- `launcher_controller.rs:877, 891-895` の `let Some(settled) = …`
- **trace イベント名 `egui_search:settled`** — 依存が実在する（`scripts/lib/SnotraTraceInvariants.psm1` の H7 と `$script:EventSearchSettled`、その Pester テスト、`scripts/smoke-egui.ps1:380`、`PERFORMANCE.md:469-470`）。**`AGENTS.md`「条件別チェック」の trace イベント名トリガーに触れないため、この改名では smoke の前提は動かない**
- `.superpowers/sdd/` と `docs/superpowers/plans|specs/` の言及（過去の作業の記録）

## 実装順序（1 フェーズ・`size:S`）

コンパイラが伝播を保証する側を先に倒し、機構が守らない側を名指しで潰す。

## 作業項目

- [ ] T1. `results_view.rs` のフィールドと `matches`（引数・分解束縛・比較）を改名する
- [ ] T2. `view.rs` の生産点（局所変数・struct literal）を追随させる
- [ ] T3. **機構が守らない平文の言及 4 件**を書き換える（下の一覧を 1 件ずつ潰す）
  - [ ] `launcher_controller.rs:194`（`snapshot の \`settled\`・段 30`）
  - [ ] `results_view.rs:36`（`armed→settled の遷移…`・D3）
  - [ ] `results_view.rs:165`（`（settled 相当）`）
  - [ ] `results_view.rs:666`（`snapshot.settled は旧 view.rs の…`）
- [ ] T4. `search_dispatch.rs:79` の bullet を削除する（D2・受け入れ条件 3）
- [ ] T5. `cargo doc --workspace --no-deps --document-private-items` を手で走らせる（**PostToolUse hook は沈黙する**・`.claude/rules/comments.md`）
- [ ] T6. 改名後に `settled` を全文検索し、残った出現がすべて「正しさの意味」であることを 1 件ずつ確かめる（受け入れ条件 1 の検算）

## 不変条件と異常系

- **挙動は 1 ビットも変えない。** `matches` の比較順序も変えない
- **`matches` の分解束縛はフィールドの漏れを compile-fail にする**（同関数の doc が明記）——この性質を壊さない
- **`egui_search:settled` を巻き込まない**（巻き込むと H7 の判定と `PERFORMANCE.md` の計測手順が同時に壊れる）
- 異常系は無い（型の名前だけが変わる）

## テスト方針と検証コマンド

`docs/build-commands.md` カテゴリ A（`.rs` 変更）。PostToolUse hook が fmt / clippy / test を自動実行する（**沈黙 = 合格**）。

加えて **`cargo doc` を手で 1 回**（hook の射程外・T5）。

**新しいテストは書かない。** この変更は挙動を変えないので、受け入れ条件 2 の検知は既存の `cargo test -p snotra` が担う。**改名漏れの検知はコンパイラ（識別子）と T3/T6（散文）が担う。**

## SPEC・関連文書の更新要否

**不要。** live な `docs/architecture.md` と `PERFORMANCE.md` の `settled` は 2 系統とも正しさ側（`is_unsettled` と trace イベント名）であり、この改名に追随する live doc は無い（実 grep で確認）。`.md` を触らないため `npm run governance:check` のトリガーにも当たらない。

## 未確定（実装前に潰す）

**空。** issue が挙げた 3 件（語の選定 / #1039 との順序 / `results_view.rs:36` の doc 追随）は D1・冒頭のユーザー決定・D3 で解けた。

## セルフレビュー

- リスク: 通常（挙動不変の改名・母集団は 9 件で有界・伝播の大半をコンパイラが保証）
- plan-review: 未実施（通常リスク）/ 自己レビューのみ
- エージェント数: 1（Step 3b の敵対枠）
- 要対処: **1 件反映**——命題 5「網羅性はコンパイラが担保する」が強すぎた。機構が守らない平文 4 件を作業項目 T3 へ**名指しで列挙**した（「grep でよい」で済ませない）。⚠️ 2 件も反映（名前の射程を doc に書く D1 / trace 依存を「外部契約」と呼ばず所在を事実で書く）
- 未検証: なし

### 5a の自己照合

1. **issue の全要件に作業項目が対応する** → 受け入れ条件 1 = T1〜T3 + T6、2 = 既存テスト、3 = T4
2. **境界条件を列挙し、各条件に検証がある** → 「改名する／しない」の境界が全体。改名しない 3 系統を計画に명記し、T6 が検算する
3. **新しい状態・リソース・プロセスに正常/失敗/破棄経路がある** → 該当なし（新しい状態を作らない）
4. **より単純な既存パターンで置き換えられないか** → 改名そのものが最小手段。`is_search_armed` を負論理へ変える案は生産点が増えるので採らない
5. **壊してはならない不変条件に検知手段がある** → `matches` の分解束縛（compile-fail）・`cargo doc`（intra-doc link）・`cargo test -p snotra`（ゲート挙動）。**平文の言及だけは機構が無く、T3 の名指し列挙が唯一の手段である**（受容する残余ではなく、有界な列挙で潰す）

**`/dry-check` は非該当と判断した。** `AGENTS.md`「条件別チェック」の「関数・型を新規定義／改名／導入」行が挙げているが、あの skill が探すのは「同等ロジックの手書き重複」であり、この変更は**新しいロジックを 0 行足さない**。同じ行が挙げる「改名は下流の compile-fail を移行漏れ検出器に」の側は T1〜T2 で実行する。

## 人間レビュー

- [x] 承認済み — 2026-08-13 / 問い: "お待ちしているのは次のいずれかでございます。1. `workspace/plan.md` への注釈 2. 明示的な承認 / とくにご意見が分かれそうな 2 点——**名前を `input_idle` にしたこと**と、**`egui_search:settled` を触らないこと**——について、違うお考えがあればその一言だけでも結構です。" / 回答: "承認"

注釈は無し。ゆえに D1（`input_idle`）と「`egui_search:settled` を触らない」はそのまま実装へ渡す。
