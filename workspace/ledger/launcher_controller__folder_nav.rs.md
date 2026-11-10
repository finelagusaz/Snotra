# 台帳: src-tauri/src/egui_shell/launcher_controller/folder_nav.rs（基準 0265463c・候補 6 行・6 ブロック）

分類: 残す／縮める／書き直す／消す。行番号は基準 sha のもの。原文は候補行だけで、ブロックの全文は基準のファイルで読む。

| ID | 行 | 候補行（原文） | 分類 | 処置後の文（要旨） | 根拠（コードの位置・#NNN） | 範囲外メモ |
|---|---|---|---|---|---|---|
| B1 | 1-1 | //! folder の突入・深掘り・折り返しと、列挙の別スレッドロード・その drain（#532 SU3 M2）。 | | | | |
| B2 | 25-25 | /// ナビゲーションスレッド → driver のメッセージ（#532 SU3 M2）。token（= folder_gen）で | | | | |
| B3 | 97-97 | /// 到着したナビ結果を採り込む（`poll_async` の段 12・#532 SU3 M2）。 | | | | |
| B4 | 133-133 | /// `nav_up` / `right` / `left` の bool で受け取る（#666 段 3）。↑↓ を `events` から | | | | |
| B5 | 138-138 | /// diff 以前から不記載のまま存在）: `move_selection` は `view.rs` の RowsSnapshot | | | | |
| B6 | 175-175 | // （SolidJS enterFolderExpansion と同一サイト・#532 SU3 M2 Finding #1）。 | | | | |
