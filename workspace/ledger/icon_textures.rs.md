# 台帳: src-tauri/src/egui_shell/icon_textures.rs（基準 0265463c・候補 11 行・7 ブロック）

分類: 残す／縮める／書き直す／消す。行番号は基準 sha のもの。原文は候補行だけで、ブロックの全文は基準のファイルで読む。

| ID | 行 | 候補行（原文） | 分類 | 処置後の文（要旨） | 根拠（コードの位置・#NNN） | 範囲外メモ |
|---|---|---|---|---|---|---|
| B1 | 1-4 | //! egui メインウィンドウのアイコン・テクスチャ層（#532 SU4）。IconCache（PNG 永続層）とは別に、 ⏎ //! path→TextureHandle をセッション内で保持する。純粋核（PNG→ColorImage decode・可視集合 retain・ ⏎ //! 抽出要否述語）をここに置き、worker spawn / load_texture の driver は results_view.rs が持つ ⏎ //! （#646 PR2 で view.rs から移管——テクスチャは egui Context〔= ウィンドウの renderer〕従属のため）。 | | | | |
| B2 | 10-11 | /// 構造的に無害（遅延到着 texture は現行行のキーでしか引かれない・SU4 決定 2）。 ⏎ /// driver（view.rs の worker spawn / load_texture）が消費する（#532 SU4 Task 5）。 | | | | |
| B3 | 22-22 | /// （#532 SU4 Task 5）。 | | | | |
| B4 | 58-58 | /// 回数の演算のみで完結する（#532 SU4 Task 5）。 | | | | |
| B5 | 67-69 | /// 可視集合に無い path の値を drop（メモリを可視集合に頭打ち・SU4 決定 A メモリ境界）。 ⏎ /// `needs_extraction` 同様に値型 `V` でジェネリック化（呼び出し側は `egui::TextureHandle`）。 ⏎ /// driver（view.rs）が消費する（#532 SU4 Task 5）。 | | | | |
| B6 | 282-284 | /// [`wanted_icon_keys`] のエラー行テストは**緑のまま**ここだけが赤になった（2026-08-19 実測）。 ⏎ /// **この対照がこのテストを別に置く理由である**——今は両方が同時に赤くなるが、それは両者が ⏎ /// 同じ導出を通るようになった結果であって、片方で足りることの根拠ではない。 | | | | |
| B7 | 402-402 | /// **この検査が `view.rs` にも `results_view.rs` にも住めない理由**（移設してはならない）: | | | | |
