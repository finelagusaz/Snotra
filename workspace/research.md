# research — issue #1074（`RowsSnapshot::settled` を実体どおりの名へ改め、`settled` の二義性を消す）

対象: <https://github.com/finelagusaz/Snotra/issues/1074>（`type:refactor` / `size:S` / `rust`）
ブランチ: `chore/rename-rows-snapshot-settled`（base: main = `3e2ec13`）

## 0. このサイクルの前提（ユーザー決定・2026-08-13）

問い「#1039 と #1074 の順序をどうしましょうか」への回答は「**#1074 を先に片付ける**」。両 issue が推奨する順序であり、`settled` が 3 つ同居する期間を作らない。

**#1073 との射程の重なりは消えた**——同日 NOT_PLANNED でクローズしたため、`on_enter` を触る予定は無い。

## 1. issue の要約

`settled` が `egui_shell` の中で別の 2 つの意味を持つ。

| 場所 | 中身 | 性格 |
|---|---|---|
| `RowsSnapshot::settled` | `!armed` = 打鍵が止まっているか | **perf ヒューリスティック**（連打中は icon worker を積まない） |
| `search_dispatch::is_unsettled` | `armed \|\| pending_seq != 0` = 最終クエリの結果が未反映か | **正しさの述語** |

両者は否定の関係に無い（食い違うのは `armed == false ∧ pending != 0`＝#1038 が塞いだ欠陥状態そのもの）。区別を担っているのは `is_unsettled` の doc の散文だけ。**直すのは名前であって挙動ではない。**

## 2. 母集団（**issue 本文より 2 か所広い**）

すべて実読で確認（main = `3e2ec13`）。

### 2a. 改名する（perf ヒューリスティック＝ `!armed`）

| # | 位置 | 形 |
|---|---|---|
| 1 | `results_view.rs:32-38` | フィールド宣言 + doc（`pub settled: bool`） |
| 2 | `results_view.rs:52` | `RowsSnapshot::matches` の引数 |
| 3 | `results_view.rs:58` | 分解束縛（`settled: cur_settled`） |
| 4 | `results_view.rs:63` | 比較（`*cur_settled == settled`） |
| 5 | `results_view.rs:165` | doc の言及（「（settled 相当）」） |
| 6 | `results_view.rs:666-669` | コメント + 消費点（`if snapshot.settled {`） |
| 7 | `view.rs:1117-1133` | コメント + 生産点（`let settled = !self.controller.is_search_armed();`）+ 2 つの利用点 |
| 8 | **`launcher_controller.rs:194`** | `is_search_armed` の doc が「snapshot の `settled`・段 30」と名指す（**issue 本文に無い**） |
| 9 | **`search_dispatch.rs:79`** | `is_unsettled` の doc の intra-doc link `[`…RowsSnapshot::settled`]`（**issue 本文に無い**） |

**#9 は受け入れ条件 3 が「削れること」を求めている当の散文である。** 削除すれば追随は不要になる。**削除も改名もしなければ intra-doc link が切れ、`cargo doc --workspace --no-deps --document-private-items` が落ちる**（`.claude/rules/comments.md` のトリガー。**PostToolUse hook では沈黙し CI でのみ発火する**）。

### 2b. 改名しない（正しさの意味＝そのまま `settled` でよい）

受け入れ条件 1 が求めるのは「`settled` が『最終クエリの結果が反映済みか』の意味だけを指すこと」であり、以下はその意味に**合致している**。

| 位置 | 形 | 触らない理由 |
|---|---|---|
| `search_dispatch.rs` の `struct Settled` と `SearchDispatch::accept` の返り値 | 採り込みが成立したときの経過 | まさに「反映済み」の意味 |
| `launcher_controller.rs:877, 891-895` | `let Some(settled) = self.dispatch.accept(..)` | 同上 |
| **trace イベント名 `egui_search:settled`**（`launcher_controller.rs:889`） | 外部契約 | 同上。**加えて `PERFORMANCE.md:469` の計測手順が逐語で依存している**（「その `egui_search:settled` の直後に現れる最初の `egui_frame`」）。`AGENTS.md`「条件別チェック」の trace イベント名トリガーに当たる |

**実装者が過剰適用しないよう、この 2b を計画へ明記する。**

### 2c. 触らない（過去の作業の記録）

`.superpowers/sdd/`・`docs/superpowers/plans|specs/` の言及は当時の記録であり、遡って書き換えない。**live な `docs/architecture.md` と `PERFORMANCE.md` の `settled` は 2 系統とも正しさ側**（`is_unsettled` と trace イベント名）なので、この改名に追随する live doc は無い。

## 3. 名前の候補と選定

意味は「**打鍵が止まっているか**」（`!Debouncer::is_armed()`）。

| 候補 | 判定 |
|---|---|
| **`input_idle`** | **採用案。** 観測している事実（入力が途切れている）をそのまま名乗る。英語として一般的で造語ではない |
| `typing_quiet` | 意味は通るが `quiet` は英語の技術用語として定着していない（造語寄り） |
| `input_settled` | `settled` を残すので二義性が消えない |
| `debounce_idle` | 機構（debounce）を名前に漏らす。消費側（icon worker）は debounce を知る必要が無い |

**射程の注記**: `armed` は `Debouncer::cancel()` でも下りる（Enter の flush 経路）。ゆえに厳密には「打鍵が止まった」ではなく「**debounce が予約を持っていない**」である。`input_idle` はその近似であり、doc に正確な定義を残す。

## 4. 再利用できる既存パターン

- **`matches` の分解束縛が漏れを compile-fail にする**（`results_view.rs:46` の doc が明記）。改名は型が伝播を保証する
- struct literal のフィールド名も同様（`view.rs:1133`）
- ゆえに**改名の網羅性はコンパイラが担保する**——grep の網羅性に頼る部分は doc・コメント・intra-doc link だけである

## 5. 技術的制約

- **挙動は 1 ビットも変えない。** `matches` の比較順序も変えない
- **intra-doc link の検査は CI でのみ発火する**（hook は沈黙）。`cargo doc` を手で 1 回走らせる（`.claude/rules/comments.md`）
- `docs/comment-guidelines.md`「訳語」の判定は**日本語の訳語**に対する規約であり、英語識別子の選定はその射程外。ただし (B) 造語の観点は英語にも当たるものとして上の表で当てた

## 6. 未解決の疑問

1. 語を `input_idle` で確定してよいか（上の判定で足りるか）
2. 受け入れ条件 3 の「散文が削れること」は、`search_dispatch.rs:79` の bullet を**丸ごと削除**でよいか（`RowsSnapshot::settled` への intra-doc link ごと消える）
3. `results_view.rs:36` の doc にある「armed→settled の遷移だけで snapshot 差分が生まれ wake が 1 回走る」の記述の追随（#1074 未確定欄が名指している）

## 7. 敵対的調査（Step 3b）の結果

1 体（general-purpose / sonnet）。全文は `workspace/adversarial-1074.txt`。

### 壊せた項目（1 件・**採用**）

**命題 5「改名の網羅性はコンパイラが担保する」は強すぎた。** 機構が守るのは**識別子の実在**だけである——分解束縛・struct literal・intra-doc link 記法 `[`…`]`。§2a のうち **4 件は平文のバッククォート言及**で、書き換え忘れても `cargo doc` も `governance:check` も検知しない。

| 平文の言及（機構が守らない） | 形 |
|---|---|
| `launcher_controller.rs:194` | `snapshot の \`settled\`・段 30` |
| `results_view.rs:36` | `armed→settled の遷移だけで…`（バッククォートすら無い） |
| `results_view.rs:165` | `（settled 相当）` |
| `results_view.rs:666` | `snapshot.settled は旧 view.rs の…` |

**機序も自分で裁定した**（採るのは所見であって説明ではない）: `Cargo.toml:22` の `broken_intra_doc_links = "deny"` が効くのは `[`…`]` のリンク記法だけであり、上の 4 件はいずれもその形をしていない（実読）。`.rs` の doc コメントが G-stale-identifiers の母集団外であることは `docs/adr/ADR-stale-identifier-detector-scope.md` が持つ。

→ **計画では、この 4 件を作業項目として名指しで列挙する。** 「コンパイラが守るから grep でよい」ではなく、守られない側を数え上げて潰す。

### 壊せなかった項目（4 件）

- **命題 1（母集団 9 件で全部）**: `.rs` 全体と他 3 crate まで複数手法で数え直したが 10 件目は出なかった
- **命題 2（§2b の 3 系統は改名しない）**: `accept()` → `set_results()` → trace の直列を実読し、意味が「反映済み」と一致することを確認
- **命題 3（intra-doc link を放置すると `cargo doc` が落ちる）**: **実際に改名して実験し、`error: unresolved link to …RowsSnapshot::settled` を実測**。実験後の復元は `git status --porcelain` / `git diff --stat` の両方で確認済み（呼び出し側でも追跡ファイル差分ゼロを再確認した）
- **命題 4（`input_idle` 最良・射程注記の正しさ）**: `Debouncer::cancel()` が Enter flush 経路（`launcher_controller.rs:1328`）から armed を下ろすことを実測。より良い代替案は出なかった

### ⚠️ 確信が持てない所見（2 件）

1. `input_idle` が Enter flush 直後の状態まで言い当てているかは語感の問題で判定不能 → **§3 の射程注記で doc に正確な定義を残すことで受ける**
2. trace 名を「外部契約」と呼ぶ強さは誇張気味かもしれない → **依存の所在を事実で書くことにした**（下記）

### 道具について判明した事実（**呼び出し側でも実測**）

- **`egui_search:settled` への依存は実在する**: `scripts/lib/SnotraTraceInvariants.psm1`（H7 の判定・`$script:EventSearchSettled = 'egui_search:settled'`）、その Pester テスト、`scripts/smoke-egui.ps1:380`。**`.github/workflows/` は trace 名に依存しない**
- `.claude/hooks/post-edit.mjs` の `selectChecks` に `cargo doc` の分岐は無く、`ci.yml` に該当ステップが在る——`.claude/rules/comments.md` の「CI でのみ発火し PostToolUse hook は沈黙する」は正確
- `Cargo.toml:22` の `broken_intra_doc_links = "deny"` により、**手元の `cargo doc` も CI と同じ厳格さで動く**

### 注記: LSP 診断の残像

敵対枠の実験（改名 → 復元）の直後、LSP が「`RowsSnapshot` に `settled` は無い / 利用可能なフィールドは `input_idle`」という**復元前の診断**を返した。`git status` と実ファイルの grep で接地して否定した（`input_idle` は 0 件・`pub settled: bool` は健在）。**委譲と並行して診断を読むときの既知の罠である。**
