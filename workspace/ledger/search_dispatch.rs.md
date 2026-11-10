# 台帳: src-tauri/src/egui_shell/search_dispatch.rs（基準 0265463c・候補 4 行・3 ブロック）

分類: 残す／縮める／書き直す／消す。行番号は基準 sha のもの。原文は候補行だけで、ブロックの全文は基準のファイルで読む。

| ID | 行 | 候補行（原文） | 分類 | 処置後の文（要旨） | 根拠（コードの位置・#NNN） | 範囲外メモ |
|---|---|---|---|---|---|---|
| B1 | 57-59 | /// （`docs/superpowers/specs/2026-08-10-search-worker-design.md`）が置いた**規約**だったが、 ⏎ /// #1039 で [`crate::egui_shell::search_state::SearchState`] の**行の差し替えと同じ private ⏎ /// メソッドの内側**へ入れたことで機構になった——`results` は private で、そこへ代入する経路が | | | | |
| B2 | 71-71 | /// 現在 in-flight の seq（無ければ 0）。判定器が「失効した結果を採っていないか」を読む材料である。 | | | | |
| B3 | 141-141 | // クエリを空にした → 行を差し替える側（#1039 以降は `SearchState::put_rows`）が撃つ | | | | |
