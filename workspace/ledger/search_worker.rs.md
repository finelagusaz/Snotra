# 台帳: src-tauri/src/egui_shell/search_worker.rs（基準 0265463c・候補 1 行・1 ブロック）

分類: 残す／縮める／書き直す／消す。行番号は基準 sha のもの。原文は候補行だけで、ブロックの全文は基準のファイルで読む。

| ID | 行 | 候補行（原文） | 分類 | 処置後の文（要旨） | 根拠（コードの位置・#NNN） | 範囲外メモ |
|---|---|---|---|---|---|---|
| B1 | 5-5 | //! **`egui::Context` を持たない**——長寿命 worker が Context clone を握ると `RepaintScheduler` の Arc がウィンドウの `Destroyed` を越えて生き、停止を妨げる（#671 PR D）。起床は `wake_main` を使う。 | | | | |
