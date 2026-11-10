# 台帳: src-tauri/src/egui_shell/launcher_controller/hide_request.rs（基準 0265463c・候補 6 行・4 ブロック）

分類: 残す／縮める／書き直す／消す。行番号は基準 sha のもの。原文は候補行だけで、ブロックの全文は基準のファイルで読む。

| ID | 行 | 候補行（原文） | 分類 | 処置後の文（要旨） | 根拠（コードの位置・#NNN） | 範囲外メモ |
|---|---|---|---|---|---|---|
| B1 | 25-25 | /// クリア・codex #8）。view-local フラグだと hide 後 Focused(true) 非着信で永久 true 化し、 | | | | |
| B2 | 43-45 | /// 変わった**——当時は `read_visual` と `lang()` も engine lock を取っていたので「1 回増える ⏎ /// だけ」と評価できたが、両者が [`crate::egui_shell::read_config`] へ移ったあとは**ここが ⏎ /// 毎フレーム engine lock を取る唯一の箇所**になっていた。#1076 で読み口だけを移し、 | | | | |
| B3 | 60-60 | // Escape ラダー（folder 中は展開前状態へ復帰、top-level は hide 要求・#532 SU3 M2）。 | | | | |
| B4 | 86-86 | /// **旧・段 14（focus 復帰で猶予を捨てる）と旧・段 34（前フレームの focus を畳む）は | | | | |
