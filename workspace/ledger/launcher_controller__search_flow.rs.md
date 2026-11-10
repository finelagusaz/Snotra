# 台帳: src-tauri/src/egui_shell/launcher_controller/search_flow.rs（基準 0265463c・候補 8 行・6 ブロック）

分類: 残す／縮める／書き直す／消す。行番号は基準 sha のもの。原文は候補行だけで、ブロックの全文は基準のファイルで読む。

| ID | 行 | 候補行（原文） | 分類 | 処置後の文（要旨） | 根拠（コードの位置・#NNN） | 範囲外メモ |
|---|---|---|---|---|---|---|
| B1 | 1-1 | //! 検索の発行（打鍵 → debounce → worker への要求）と、その結果の採り込み（#1004 PR 2）。 | | | | |
| B2 | 40-40 | /// **この読みは #1076 で `engine.lock()` から [`crate::egui_shell::read_config`] へ移した。** | | | | |
| B3 | 53-53 | /// view_kind 先の同期 dispatch（#532 SU3 M2）。folder は cache/error を同期フィルタ、 | | | | |
| B4 | 57-57 | /// （/code-review #637 finding 9。当時は engine lock の回数だった・#1076 で読み口が移った）。 | | | | |
| B5 | 117-118 | // ここは config の読みだけを担う（#1124 で `commands/instant.rs` から移した—— ⏎ // かつて IPC コマンドと共有していた相手は #532 SU7 で消滅していた）。 | | | | |
| B6 | 204-207 | // "view" = Folder ビューへ遷移していた（#1039 のガード）。ガードは Tool も ⏎ //          弾くが、そちらは `enter_tool` が in-flight を失効させるため ⏎ //          production では到達しない。 ⏎ // "seq"  = 追い越された、または同期で差し替えて失効していた。 | | | | |
