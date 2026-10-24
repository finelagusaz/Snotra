# 計画 — #1279 SearchState の結果を Arc で持ち、snapshot の発行で行を作り直さない

調査は `workspace/research.md`、敵対的調査の生出力は `workspace/adversarial-1279.txt`。

## 目的

行の所有者（`SearchState.results`・2 つの frame の `restore_results`）を `Arc<[SearchResult]>` にし、snapshot の発行は `Arc::clone` で済ませる。行が変わらないフレーム（↑↓・`input_idle` の反転）では行を作り直さない。挙動（何を描くか・何を選ぶか・クリックが何を起動するか）は変えない。

## 受け入れ条件

1. results の描画・選択・クリック照合の挙動が変わらない——既存の `cargo test -p snotra` が無改変で緑（型の付け替えに伴う機械的な書き換えを除く）、`smoke:egui` が緑
2. **発行** 1 回の確保が行数に比例しない——計数（`--features heap-trace` の `heap_trace::snapshot().allocs` を発行の前後で読む使い捨てプローブ）で A/B を示す。`put_rows` 側の Vec→Arc 変換（確保 1 回 + 行数比例の memcpy・打鍵あたり 2 回）は残り、この条件の対象外である（記録にそう書く）
3. ↑↓ で行を作り直さない——常設テスト（ptr 一致）+ 計時で A/B を示す
4. 計時・計数は同一セッションで A/B 交互に 3 回ずつ取り、`PERFORMANCE.md` へ出所つきで新節として着地させる

## 変更ファイルと対象シンボル

| ファイル | シンボル | 変更 |
|---|---|---|
| `src-tauri/src/egui_shell/search_state.rs` | `SearchState.results` | `Vec<SearchResult>` → `Arc<[SearchResult]>`（初期値 `Arc::default()`） |
| 同 | `FolderFrame.restore_results` / `ToolFrame.restore_results` | `Arc<[SearchResult]>` |
| 同 | `put_rows` | 引数を `Arc<[SearchResult]>` に |
| 同 | `set_results` / `accept_worker_rows` | シグネチャは `Vec` のまま、内部で `.into()`（要素を移す。controller の呼び出しは無変更） |
| 同 | 新設 `results_arc(&self) -> &Arc<[SearchResult]>` | 発行側が Arc を共有するための読み口。`results()` は `&[SearchResult]` のまま残す |
| 同 | `enter_folder` | `self.results.clone()` が Arc の clone になる（深い複製が消える副次効果） |
| 同 | `enter_tool` / `reset` | `std::mem::take` は空の Arc を残す・自前のツール行は `put_rows(rows.into(), 0)`・`reset` は `put_rows(Arc::default(), 0)`（`Vec::new().into()` は確保 1 回になるので使わない） |
| `src-tauri/src/egui_shell/results_view.rs` | `RowsSnapshot::matches` | `rows: &Arc<[SearchResult]>` を受ける。比較順を `generation` → `selected` → `input_idle` → `rows` にする（連言の評価順だけで意味は不変。**generation で行の比較を置き換えない**） |
| 同 | 新設 `ResultsShared::publish(&self, rows: &Arc<[SearchResult]>, selected, generation, input_idle) -> bool` | view.rs の lock → `matches` → 差し替え を移す。差し替えたら `true`（呼び出し側が `wake_results`） |
| `src-tauri/src/egui_shell/view.rs` | snapshot 発行（`show_results` 以降） | 非表示は `Arc::default()`（確保しない）、表示は `results_arc()` を渡して `publish`。`true` なら `wake_results`。**publish → クリック消費の順序（#699）は動かさない** |
| `PERFORMANCE.md` | 新節「採用: snapshot の発行で行を作り直さない（#1279）」 | 計数・計時の A/B |

`SPEC.md`: 更新不要（挙動不変・語彙層に `Vec` / `Arc` の記述なし）。`docs/architecture.md` L216「RowsSnapshot 発行（Arc<Mutex>）」は発行の機構の記述として今も正しい——変更不要。

## 散文の書き換え（撤去される挙動を描写している行）

母集団の導出コマンド（2026-10-25 に実行し、下の行を返すことを確認済み）:

```bash
grep -nE "^\s*(//|///).*(Vec|複製|作り直)" src-tauri/src/egui_shell/search_state.rs src-tauri/src/egui_shell/results_view.rs src-tauri/src/egui_shell/view.rs
```

当該行（ヒットのうち今回の変更で偽になるもの。他のヒットは別の文脈で無関係）:

- `results_view.rs` `RowsSnapshot::rows` doc「行を作り直すかは発行側が [`Self::matches`] で決める」
- `results_view.rs` `RowsSnapshot::matches` doc 全体（「行はスライスのまま突き合わせ」「**作り直しの単位は snapshot 全体である**——行が同じでも…行ごと作り直す」）——今回消える挙動そのもの
- `view.rs` L1139–1141「判定は行の列を作る前に行う…スライスのまま突き合わせさせる」・L1149「表示すべきでないフレームは空スライスを発行する」
- `search_state.rs` `put_rows` doc「take が空 Vec を残すので」・`enter_tool` のコメント「take が空 Vec を残すので」・`reset` のコメント「旧実装の `results.clear()` と違い…Vec ごと置き換える…」段落
- `search_state.rs` L99「`set_results(Vec::new())`」は `set_results` が `Vec` を受け続けるので**真のまま**（変更しない）

`grep` は識別子ではなく散文を拾うためのもの（AGENTS.md「機構・層・ファイル群を撤去する」）——実装後にもう一度走らせ、残ったヒットを「撤去を描写している／在る前提で書いている」へ振り分ける。

## 実装順序

1. `search_state.rs` の型の付け替え（`put_rows`・frame・`results_arc`）→ `cargo check -p snotra`
2. `results_view.rs` の `matches` / `publish` → `view.rs` の発行を `publish` へ
3. 常設テスト（下記）を先に書いて、A 相当の変異（publish の中で `Arc::from(&rows[..])` に戻す）で落ちることを確かめる（Red）→ 本実装で Green
4. 散文の書き換え
5. 使い捨てプローブで A/B 計測 → `PERFORMANCE.md` 着地 → プローブを撤去
6. 検証（カテゴリ A + C の `smoke:egui`）

## 不変条件と異常系

- **`put_rows` が行の差し替えの単一チョークポイントのまま**（#1039）——Arc 化で新しい書き込み口を作らない。`results_arc` は `&Arc` を返す読み口だけ
- **空 = 非表示**（`RowsSnapshot::rows` の doc）——非表示フレームは `Arc::default()` を発行。`Default for Arc<[T]>` は `align_of::<T>() <= 16` のとき static を共有して確保しない（std 1.99.1 `sync.rs` の `MAX_STATIC_INNER_SLICE_ALIGNMENT = 16`。`SearchResult` は `String`/`bool`/enum で align 8）
- **行の比較を generation で置き換えない**——plain_hidden の反転は generation を進めずに行を空にする。比較順の入れ替えは連言の評価順であって、行の比較は残る
- **publish → クリック消費の順序**（#699）は view.rs の同じ位置に残す
- `matches` の先頭の分解束縛（フィールド漏れの compile-fail）は残す
- **`matches` の行の比較は `Arc` 同士で書く**（`*cur_rows == *rows`。`**` で deref してスライス同士にすると ptr_eq の先行比較が効かず O(n) に戻る）。挙動が同じなので**テストでは捕まらない残余**——`matches` の doc に理由を書いて固定する（敵対的調査 B2・std の `ArcEqIdent` 特殊化は `[T]: Eq` のときだけ効く）
- 非表示フレームの `Arc::default()` は確保 0 だが static の参照カウントの原子操作が毎フレーム乗る——受容（`Option` で空を表すと「空 = 非表示」に 2 つ目の表現が生まれる）
- Arc 化で同じ行を複数の所有者（state・frame・snapshot）が指すが、`[SearchResult]` は不変（`Arc` は `&mut` を与えない）ので共有による書き換えの波及は無い

## テスト方針

常設（`#[cfg(test)]`）:

- `results_view.rs`: `publish_shares_rows_when_only_selection_changes`——同じ Arc を selected 0 → 1 で 2 回 `publish`、2 回目が `true` を返し、snapshot の行が渡した Arc と `ptr::eq`（A 相当の変異で落ちる）
- 同: `publish_skips_when_nothing_changed`——同じ引数で 2 回目は `false`（wake しない。無変化判定の意味の保存）、加えて**中身が同じで別の Arc** でも `false`（中身比較が残っていることの固定——generation 置き換えの退行で落ちる）
- 同: 非表示（`Arc::default()`）の publish で行が空になり `true`
- `search_state.rs`: `move_selection` の前後で `results_arc()` が同じ Arc を指す・`enter_folder` → `on_escape` で復帰した行が突入前と同じ Arc を指す（深い複製をしない）

使い捨て（コミットしない）:

- 合成 200 / 1,000 行で「↑↓ 相当（selected だけ変えた）発行 1 回」の (i) 確保回数を `cargo test -p snotra --release --features heap-trace` で、(ii) 時間を feature 無しの `--release` で、A（main `55e5c4a`）/ B（このブランチ）交互に各 3 回取る。**計数と計時を同じ実行で取らない**（`heap_trace.rs` の `//!`）。heap-trace の計数はプロセス全体で共有されるので、計数はテスト名で絞り `-- --ignored --nocapture --test-threads=1` で走らせ、`snapshot()` は `publish`（A は `matches` + 構築）の直前と直後に同じスレッドで、間に何も挟まずに読む。A 側（`55e5c4a`）にも `heap_trace.rs` と `heap-trace` feature が在ることを確認済み（main の HEAD である）。A と B は API が違うので、プローブ本体は版ごとに発行の実コード（A: `matches` + `rows.into()`、B: `publish`）を呼ぶ 2 形にする。合成行の分布は実運用点から採った値ではないと記録に書く。issue 本文の推定額（35–38 µs）は出所が無いので比較に使わない

検証コマンド（`docs/build-commands.md` カテゴリ A + C の `smoke:egui`）:

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra
cargo doc --workspace --no-deps --document-private-items
cargo build -p snotra && npm run smoke:egui
npm run governance:check   # PERFORMANCE.md・コメントの見出し参照を触るため（カテゴリ F）
```

変異注入（`/implement`「3b. 委譲へ渡すもの」の委譲先が行う）: (1) publish で `Arc::from(&rows[..])` に戻す → ptr テストが落ちる、(2) `matches` の行の比較を `true` に置き換える（分解束縛の `cur_rows` は `let _ = cur_rows;` で使い続け、**コンパイルを通す形にする**——未使用束縛で `-D warnings` が落ちるのはコンパイラの検知であってテストの検知ではない・`.claude/rules/safety-nets.md`）→ 非表示（空）の publish が `true` を返すテストが落ちる、(3) `enter_folder` で `Arc::from(self.results.to_vec())` → frame の ptr テストが落ちる。

新設関数（`publish`・`results_arc`）は LSP findReferences で呼び出し元を確認し、`/dry-check` を通す。

## 作業項目

### Phase 1 — 型の付け替え

- [x] `SearchState.results` / `FolderFrame.restore_results` / `ToolFrame.restore_results` を `Arc<[SearchResult]>` に
- [x] `put_rows` の引数を Arc に、`set_results` / `accept_worker_rows` は内部で `.into()`
- [x] `results_arc()` を新設
- [x] `cargo check -p snotra` が緑

### Phase 2 — 発行

- [x] `RowsSnapshot::matches` を `&Arc` で受け、比較順を入れ替える
- [x] `ResultsShared::publish` を旧挙動の本体（`Arc::from(&rows[..])`）で先に書き、常設テストを書いて ptr テストが落ちるのを確認（Red）
- [x] `publish` の本体を `Arc::clone` に直して Green にし、view.rs の発行を `publish` へ置き換える（非表示は `Arc::default()`）
- [x] 常設テストが緑（Red: ptr テストが assert 文言で落ちることを確認 → Green: `cargo test -p snotra` 318 passed）
- [x] 新設の `publish`・`results_arc` の呼び出し元を LSP findReferences で確認し、`/dry-check` を通す（本番の呼び出し元は各 1・view.rs の発行。手書き重複 0）

### Phase 3 — 散文と記録

- [x] 「散文の書き換え」の行を直し、導出 grep を再実行して残りを振り分ける
- [x] 使い捨てプローブで A/B 各 3 回の計数・計時を取る（2026-10-25・HAL9000。発行 1 回の確保は A = 2n+1 回／B = 0 回で 3 回とも同値、200 行の選択移動は A 12.7–13.4 µs／B 0.018–0.020 µs）
- [x] `PERFORMANCE.md` へ新節を出所つきで書き、プローブを撤去する（「採用: snapshot の発行で行を作り直さない（#1279）」・worktree と target を撤去済み）

### Phase 4 — 検証

- [ ] 検証コマンドをすべて実行して緑

## 未確定（実装前に潰す）

- [x] 敵対的調査（`workspace/adversarial-1279.txt`）の所見を反映する — research.md「敵対的調査の反映」に採否を残した。計画への反映: 計数器を heap-trace へ（B1）・`matches` の比較の書き方を不変条件へ（B2）・(a) の切り出しを確定（B3/W4）・比較順の並べ替えを維持し issue の推定額を受け入れから外す（B4）・`reset` は `Arc::default()`（K1）・受け入れ 2 を「発行」に限定（W2）

## 人間レビュー

- [x] 承認済み — 2026-10-25 / 問い: "このまま承認なさるか、`workspace/plan.md` に注釈をお書き添えくださるか、どちらかでお知らせくださいませ。" / 回答: "OK"

## セルフレビュー

- リスク: 通常（行差し替えの単一チョークポイントの裏で型を付け替えるだけ。永続化・新しい状態・新しいスレッドは無い）
- plan-review: 未実施（通常リスク）
- エージェント数: 1（3b の敵対的調査）
- 照合: (1) issue の要件——3 つの所有者の Arc 化・`matches` の `&Arc` 化・受け入れ 2 項目——がすべて作業項目に対応 (2) 境界: 非表示（空 Arc）・同一 Arc・中身が同じ別 Arc・selected だけの変化・folder/tool の往復——それぞれにテストがある (3) 新しい状態・リソースは無い（Arc の解放は所有者の drop） (4) より単純な代替（発行側で `guard.rows` を使い回す）は issue が却下済み (5) 不変条件の検知: ptr テスト・「中身が同じ別 Arc で publish しない」テスト・既存の挙動テスト。検知できない残余は B2 の 1 件で、doc で固定する
- `/race-check`: 該当（ウィンドウをまたぐ `ResultsShared.snapshot`）。型は #1278 で既に `Arc`、publish の順序（#699）は不変で、lock 中の仕事が深い複製から参照カウントの加算へ減る向きだけ——新しい競合の窓は無い
- `/state-check`・`/symmetric-check`・`/persistence-check`: 非該当（モード・ガード・対称ペア・永続形式に触れない）
- `/dry-check`: 新設の `publish`・`results_arc` について実装時に実行（計画の作業項目に含む）
- 要対処: 敵対的調査の 7 件を反映（上記）
- 未検証: 計測値（実装後に取る）

## 計画外の作業（実装中に判明）

- [x] `egui_shell/mod.rs` の `RowsSnapshot` re-export を撤去——消費者が view.rs の発行だけで、`publish` へ移ったため未使用になり `-D warnings` で落ちた
