# 台帳: src-tauri/src/egui_shell/launcher_controller.rs（基準 0265463c・候補 11 行・8 ブロック）

分類: 残す／縮める／書き直す／消す。行番号は基準 sha のもの。原文は候補行だけで、ブロックの全文は基準のファイルで読む。

| ID | 行 | 候補行（原文） | 分類 | 処置後の文（要旨） | 根拠（コードの位置・#NNN） | 範囲外メモ |
|---|---|---|---|---|---|---|
| B1 | 1-1 | //! 検索セッション層（show を跨ぐ状態・結果・選択・起動・履歴・期限）の所有者（#666 段 3）。 | | | | |
| B2 | 11-11 | //!   `Destroyed` を越えて worker の停止・join を止める・#671 PR D） | | | | |
| B3 | 69-70 | /// blur 猶予の状態機械（#745）。**hide を跨いだ持ち越しは `consume_reset_pending` の ⏎ /// `reset()` が塞ぐ**——旧 2 フィールド（`was_focused` / `unfocus_at`）はこの型へ畳んだ。 | | | | |
| B4 | 76-79 | // emit dedup は共有 EguiShellState.hide_pending（show がクリア・codex #8）。view-local には持たない。 ⏎ folder_tx: Sender<FolderMsg>, ⏎ folder_rx: Receiver<FolderMsg>, ⏎ /// ナビゲーションでロードした (ctx, 全ソート済み) キャッシュ。打鍵フィルタの源（#532 SU3 M2）。 | | | | |
| B5 | 90-90 | /// #633: index build 完了世代の last-seen（AppState.index_generation と比較・SU6 spec 決定 3）。 | | | | |
| B6 | 98-100 | /// Plain 検索の要求を worker へ送る送信端（#1004 PR 2）。 ⏎ search_tx: Sender<crate::egui_shell::SearchRequest>, ⏎ /// worker からの結果受信端。`drain_search` が毎フレーム吸い出す（#1004 PR 2）。 | | | | |
| B7 | 182-182 | /// 独立実装を持っていた重複の解消（レビュー是正 3）。 | | | | |
| B8 | 194-194 | /// 既存ヘルパー群と同型。SU6 の hot-reload 拡張時もこの読み口のまま動く）。 | | | | |
