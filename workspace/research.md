# 調査 — #1279 SearchState の結果を Arc で持ち、snapshot の発行で行を作り直さない

## issue の要約

#1278 で `RowsSnapshot.rows` は `Arc<[SearchResult]>` になり、results ウィンドウの受け取り（`clone`）は行を複製しなくなった。しかし行の所有者 `SearchState.results` は `Vec<SearchResult>` のままで、発行側（`view.rs` の snapshot 発行）は差分があるたびに `rows.into()`（`From<&[T]> for Arc<[T]>`＝要素ごと clone）で全行を深く複製する。`RowsSnapshot::matches` は snapshot 全体で比べるので、行が同じでも選択の移動・`input_idle` の反転で行ごと作り直す。

求める形: `SearchState.results` / `FolderFrame.restore_results` / `ToolFrame.restore_results` を `Arc<[SearchResult]>` で持ち、発行は `Arc::clone`、`matches` は `&Arc` で比べる（std の ptr_eq 先行比較で無変化フレームが O(1)）。**行の比較を generation で置き換えない**（plain_hidden 反転フレームは generation を進めずに空行を発行する）。発行側で `guard.rows` を使い回す浅い直しは採らない。

受け入れ: 描画・選択・クリック照合の挙動が不変（既存検査 + `smoke:egui`）。発行 1 回の確保が行数に比例しないこと・↑↓ で行を作り直さないことを計数か計時で示す（同一セッションで A/B 交互に 3 回ずつ）。

## 関連ファイル・シンボル（grep で実在確認済み）

- `src-tauri/src/egui_shell/search_state.rs`
  - `SearchState.results: Vec<SearchResult>`（L169・初期化 L221 `Vec::new()`）
  - `results() -> &[SearchResult]`（L248）——読み手は controller（`activation.rs` 101/350/510/624・`folder_nav.rs` 161/200）・`view.rs`（1137/1145/1311）・テスト多数。スライスで足りる
  - `put_rows(rows: Vec<SearchResult>, next_selected)`（L272・行差し替えの単一チョークポイント #1039）
  - `set_results(Vec)`（L284）・`accept_worker_rows(seq, Vec, now)`（L572）——外部からの入口はこの 2 つだけ
  - `enter_folder`（L349 `restore_results: self.results.clone()` ＝現状は深い複製）
  - `enter_tool`（L441 `std::mem::take(&mut self.results)`）
  - `on_escape`（L505/L513 `put_rows(t|f.restore_results, ..)`）
  - `reset`（L543 `put_rows(Vec::new(), 0)`）
  - `move_selection`（L316・行を触らない）
  - `FolderFrame.restore_results`（L82）・`ToolFrame.restore_results`（L114）——どちらも `pub` フィールドだが外部から読む箇所は無い（grep: `restore_results` は search_state.rs だけ）
- `src-tauri/src/egui_shell/results_view.rs`
  - `RowsSnapshot`（L21・`rows: Arc<[SearchResult]>`）・`RowsSnapshot::matches(&self, rows: &[SearchResult], ..)`（L54・先頭の分解束縛でフィールド漏れを compile-fail に）
  - 回帰テスト `snapshot_clone_shares_rows_instead_of_copying_them`（受け取り側の ptr_eq）
  - テスト用 `rows_for_test(n)`・計時プローブ `kittest_row_draw_cost_probe`（`#[ignore]`）
- `src-tauri/src/egui_shell/view.rs` L1137–1170: `show_results`・`rows: &[SearchResult]`（非表示は `&[]`）・`guard.matches(..)`・`RowsSnapshot { rows: rows.into(), .. }`・`wake_results`
- 呼び出し側（`launcher_controller/search_flow.rs`・`activation.rs`）は `set_results(Vec)` / `accept_worker_rows(.., Vec, ..)` だけを呼ぶ
- `PERFORMANCE.md` L2774「採用: results の snapshot の受け取りで行を複製しない」——今回の記録の置き場の先例

## 再利用できる既存パターン

- #1278 の型だけで効く直し + ptr_eq の回帰テスト（`snapshot_clone_shares_rows_instead_of_copying_them`）
- 計時は使い捨てのプローブ・A/B 交互 3 回・`PERFORMANCE.md` へ出所つき記録（#1278 の先例）
- `put_rows` の単一チョークポイント——`Vec → Arc` の変換を `put_rows` の入口（あるいは `set_results` / `accept_worker_rows`）に閉じれば、controller 側の呼び出しは 1 行も変えずに済む

## 技術的制約（std 1.99.1 のソースで確認）

- `PartialEq for Arc<T>` は `T: ?Sized + Eq`（`MarkerEq`）のとき `ptr::eq(..) || **self == **other`（`alloc/src/sync.rs` L3588）。`SearchResult` は `PartialEq, Eq` derive 済み（`snotra-core/src/ui_types.rs` L11）、`[T]: Eq` も成立 → 同一 Arc 同士の比較は O(1)、別 Arc なら従来どおり中身比較（意味不変）
- `Default for Arc<[T]>`（1.80 安定化）は static の空 inner を共有し**確保しない**（L3866）——非表示フレームの空行・`std::mem::take` の残置に使える。toolchain は `rust-toolchain.toml` で 1.99 固定
- `From<Vec<T>> for Arc<[T]>` は要素を移す（`String` を複製しない）。ただし Arc の inner（refcount 2 語 + 要素）は新規確保 + 要素の memcpy が 1 回走る（行数比例の memcpy・`String` 確保は 0）。差し替え 1 回あたり 1 度だけで、発行のたびではない
- `RowsSnapshot` の `#[derive(PartialEq)]` は既に Arc 比較なので変更不要
- 表示ゲートの意味（`rows.is_empty()` が非可視を表す・plain_hidden は generation を進めない）は維持する——非表示は `Arc::default()` を発行

## 未解決の疑問

- `put_rows` の引数型: `Arc<[SearchResult]>` にして `set_results` / `accept_worker_rows` が `.into()` するか、`put_rows` を `impl Into<Arc<..>>` にするか（前者が簡素。restore 経路は Arc をそのまま渡せる）
- 「↑↓ で行を作り直さない」の常設の検知器を置く席: 発行は `view.rs` の update にインラインで、単体テストから直接叩けない。(a) 発行の判定 + 構築を `ResultsShared` か `RowsSnapshot` の関数へ切り出してテストする、(b) `SearchState` 側で「`move_selection` の前後で `rows()` が同じ Arc を指す」だけを固定し、発行は型（`Arc::clone` 以外に `&Arc` から行を得る書き方は `to_vec().into()` 等の明示的な複製しかない）に任せる
  - → **(a) に決定**（下の B3・W4）。(b) の括弧内の主張は偽だった

## 敵対的調査の反映（`workspace/adversarial-1279.txt`・sonnet 1 体）

| 所見 | 採否 | 理由・反映先 |
|---|---|---|
| B1 `kittest_row_draw_cost_probe` は発行も確保も測らない | 採る | 計数は既存の `heap-trace` feature（`heap_trace::snapshot().allocs`）を発行の前後で読む使い捨てプローブ、計時は feature 無しの別ビルドで発行単体を測る（`heap_trace.rs` の `//!` が「フレーム時間の計測と同じ実行で使わない」と明記——自分で読んで確認）。自前の計数アロケータは書かない |
| B2 `matches` を `**cur == **rows`（deref）で書くと ptr_eq が効かず O(n) に戻り、テストは緑のまま | 採る | 計画の不変条件に「`Arc` 同士で比べる（`**` で deref しない）」を明記。挙動が同じなので ptr テストは区別できない——テストで捕まらない残余として記録 |
| B3 本メモ「`to_vec().into()` 等の明示的な複製しかない」は偽（`Arc::from(state.results())` が型を通る） | 採る | 未解決の疑問 (b)「型に任せる」を却下し (a)（`ResultsShared::publish` へ切り出して ptr テスト）に決定 |
| B4 単発打鍵の発行 4 回・行の変化 1 回は正しい（連打中は 0 回）。trailing の結果着は別 Arc で中身比較 ~1.15 µs が走る。issue の 35–38 µs は 4 × 11.5–12.6 と合わず出所が無い | 採る | 比較順の並べ替え（generation 先行）を計画に入れる。issue の推定額は受け入れに使わない。`put_rows` の Vec→Arc 変換（確保 1 回 + memcpy）は打鍵あたり 2 回残る |
| B5 切り出す関数が `wake_results` まで含むと単体テストできない | 採る | `publish` は判定と差し替えだけ、wake は呼び出し側（計画どおり） |
| K1 書き込みは `put_rows` だけ。`.into()` が要るのは `set_results`・`accept_worker_rows` に加え `enter_tool` の自前の行と `reset` | 採る | `reset` は `Arc::default()`（`Vec::new().into()` は確保 1 回・別ポインタ）、`enter_tool` は `rows.into()` |
| K2 読み手は `&[T]` で足りる・`restore_results` は search_state.rs 内だけ | 壊れず | — |
| K3 ↑↓・debounce・plain_hidden は `put_rows` を通らず世代を進めない | 壊れず | — |
| K4 std の ptr_eq 先行・`Arc::default()` 確保 0・`From<Vec>` は String を複製しない | 壊れず | 実測でも一致 |
| K5 実運用 config は `result_limit = 200`。空クエリは `recent_limit = 8` 行 | 壊れず（注意を採る） | プローブの行は合成で 200 / 1,000 行を作る（実運用の打鍵では 1〜2 文字のクエリで 200 件）。合成行の分布は実運用点から採った値ではないと記録に書く |
| W1 無変化フレームの比較額は合成データで 1.15 µs | 保留 | 額を主張に使わない |
| W2 `put_rows` 側の変換費用は未分離 | 採る | 受け入れの文言を「**発行** 1 回の確保」に限定し、`put_rows` 側は行数比例の memcpy が残ると明記 |
| W3 非表示フレームの `Arc::default()` は static の参照カウントの原子操作が毎フレーム乗る | 受容 | 確保 0・原子操作数回。`Option` で空を表すと「空 = 非表示」に 2 つ目の表現が生まれ、型の単一性を崩すので採らない |
| W4 (b) のテストだけでは発行側を保証しない | 採る | (a) を主、(b) を補助として両方置く |
| W5 lock の保持時間は短くなる向きだけ | 採る | `/race-check` の対象（ウィンドウをまたぐ共有状態）だが、型は #1278 で既に `Arc` で、変わるのは保持中の仕事の量だけ——セルフレビューに記録 |
