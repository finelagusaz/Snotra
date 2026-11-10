# 台帳: src-tauri/src/egui_shell/launcher_controller.rs（基準 0265463c・候補 11 行・8 ブロック）

分類: 残す／縮める／書き直す／消す。行番号は基準 sha のもの。原文は候補行だけで、ブロックの全文は基準のファイルで読む。

| ID | 行 | 候補行（原文） | 分類 | 処置後の文（要旨） | 根拠（コードの位置・#NNN） | 範囲外メモ |
|---|---|---|---|---|---|---|
| B1 | 1-1 | //! 検索セッション層（show を跨ぐ状態・結果・選択・起動・履歴・期限）の所有者（#666 段 3）。 | 縮める | `（#666）` | 「段 3」は #666 の作業単位の副ラベル | |
| B2 | 11-11 | //!   `Destroyed` を越えて worker の停止・join を止める・#671 PR D） | 縮める | `・#671）` | | |
| B3 | 69-70 | /// blur 猶予の状態機械（#745）。**hide を跨いだ持ち越しは `consume_reset_pending` の ⏎ /// `reset()` が塞ぐ**——旧 2 フィールド（`was_focused` / `unfocus_at`）はこの型へ畳んだ。 | 消す（後半のみ） | 前半の現在形の約束だけ残す | 旧フィールド名と畳んだ経緯は `ADR-blur-grace-single-field-state-machine` が持つ | |
| B4 | 76-79 | // emit dedup は共有 EguiShellState.hide_pending（show がクリア・codex #8）。… /// …打鍵フィルタの源（#532 SU3 M2）。 | 縮める | `codex #8` → `#532`・`#532 SU3 M2` → `#532` | 対応表（codex #n → #532） | |
| B5 | 90-90 | /// #633: index build 完了世代の last-seen（AppState.index_generation と比較・SU6 spec 決定 3）。 | 縮める | `・#532）` | `git log -S 'SU6 spec 決定 3'` の最古 → 04612f60（#651・#532 Phase 2）。対応表で SU → #532 | |
| B6 | 98-100 | /// Plain 検索の要求を worker へ送る送信端（#1004 PR 2）。… `drain_search` が毎フレーム吸い出す（#1004 PR 2）。 | 縮める | `（#1004）` ×2 | | |
| B7 | 182-182 | /// 独立実装を持っていた重複の解消（レビュー是正 3）。 | 書き直す | 「show 経路と同じ 1 つの実装を通る（#904）」 | `git log -S 'レビュー是正 3'` → 0083620d（#904）。`window_coordinator.rs` の show 経路（`read_indexing(app)` の呼び出し）も同じ関数を通る（grep で確認） | 同 doc の「state.rs:14 で確認済み」は行番号での参照（範囲外） |
| B8 | 194-194 | /// 既存ヘルパー群と同型。SU6 の hot-reload 拡張時もこの読み口のまま動く）。 | 書き直す | 「hot-reload した値もこの読み口のまま読む・#532」 | 未来形（SU6 の拡張時）を現在形へ。都度 `read_config` で読むことは同 fn の本体。`git log -S` 最古 → 70a34910（#647・#532 Phase 2） | |
| X1 | 76 | // query フィールドは SearchState.query へ移譲（削除）。 | 消す | | 移設の記録で、今の制約を運ばない | |
| X2 | 133 | /// 分割で 2 か所がファイルをまたいだため、値を綴る点を 1 つに戻した。 | 書き直す | 「2 か所は別のファイルにあるので、値を綴る点をここ 1 つに保つ（#1200）」 | `new_search_debounce` の呼び出しは `launcher_controller.rs:new` と `frame_stages.rs`（reset-on-show）の 2 か所（grep で確認）。`git log -S` → 6344d8b8（#1200） | |
