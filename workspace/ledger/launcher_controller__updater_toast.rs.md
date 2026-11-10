# 台帳: src-tauri/src/egui_shell/launcher_controller/updater_toast.rs（基準 0265463c・候補 5 行・4 ブロック）

分類: 残す／縮める／書き直す／消す。行番号は基準 sha のもの。原文は候補行だけで、ブロックの全文は基準のファイルで読む。

| ID | 行 | 候補行（原文） | 分類 | 処置後の文（要旨） | 根拠（コードの位置・#NNN） | 範囲外メモ |
|---|---|---|---|---|---|---|
| B1 | 1-1 | //! 更新 toast のボタン処置と install の spawn（#532 SU5・§20.4）。 | | | | |
| B2 | 24-26 | /// toast ボタンの処理（#532 SU5）。install は Update を原子取得して async へ（Task 8）。 ⏎ /// ⏎ /// **状態を変えたら `ctx.request_repaint()` する**（Task 10 実機スモークで発見・ | | | | |
| B3 | 30-30 | /// 無関係な入力（マウス移動等）が来るまで旧 toast が画面に残る（dismiss 後の stale 表示）。 | | | | |
| B4 | 61-61 | /// download → `on_before_exit`（=flush_persistent_state・Task 6 で builder に登録済み）→ | | | | |
