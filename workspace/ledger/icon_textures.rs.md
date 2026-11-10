# 台帳: src-tauri/src/egui_shell/icon_textures.rs（基準 0265463c・候補 11 行・7 ブロック）

分類: 残す／縮める／書き直す／消す。行番号は基準 sha のもの。原文は候補行だけで、ブロックの全文は基準のファイルで読む。

| ID | 行 | 候補行（原文） | 分類 | 処置後の文（要旨） | 根拠（コードの位置・#NNN） | 範囲外メモ |
|---|---|---|---|---|---|---|
| B1 | 1-4 | //! egui メインウィンドウのアイコン・テクスチャ層（#532 SU4）。IconCache（PNG 永続層）とは別に、 ⏎ //! path→TextureHandle をセッション内で保持する。純粋核（PNG→ColorImage decode・可視集合 retain・ ⏎ //! 抽出要否述語）をここに置き、worker spawn / load_texture の driver は results_view.rs が持つ ⏎ //! （#646 PR2 で view.rs から移管——テクスチャは egui Context〔= ウィンドウの renderer〕従属のため）。 | 縮める＋書き直す | `（#532）`・「worker spawn / load_texture の driver は results_view.rs が持つ（テクスチャは egui Context〔= ウィンドウの renderer〕従属のため・#646）。」 | `SU4`・`PR2` は副ラベル。「view.rs から移管」は移設の記録。`results_view.rs` が `IconMsg`・`png_to_color_image`・`retain_visible`・`load_texture` を使う（grep で確認） |  |
| B2 | 10-11 | /// 構造的に無害（遅延到着 texture は現行行のキーでしか引かれない・SU4 決定 2）。 ⏎ /// driver（view.rs の worker spawn / load_texture）が消費する（#532 SU4 Task 5）。 | 縮める＋消す | 1 文目は「…引かれない・#532）」。2 文目「driver（view.rs の worker spawn / load_texture）が消費する（#532 SU4 Task 5）。」を消す | `SU4 決定 2` は #532 の副ラベル。2 文目は今は偽（driver は `results_view.rs`。B1 が現在形で持つ） | `mod.rs:61` も `Task 5`・`#646 PR2` を持つ（群 C） |
| B3 | 22-22 | /// （#532 SU4 Task 5）。 | 消す | 「driver（view.rs）が消費する（#532 SU4 Task 5）。」を消す | 今は偽（`png_to_color_image` を呼ぶのは `results_view.rs`）。呼び出し元の名指し自体が「書かないもの」 |  |
| B4 | 58-58 | /// 回数の演算のみで完結する（#532 SU4 Task 5）。 | 縮める | `（#532）` | `SU4 Task 5` は副ラベル |  |
| B5 | 67-69 | /// 可視集合に無い path の値を drop（メモリを可視集合に頭打ち・SU4 決定 A メモリ境界）。 ⏎ /// `needs_extraction` 同様に値型 `V` でジェネリック化（呼び出し側は `egui::TextureHandle`）。 ⏎ /// driver（view.rs）が消費する（#532 SU4 Task 5）。 | 縮める＋消す | 「（メモリを可視集合に頭打ち・#532）」。3 行目「driver（view.rs）が消費する（#532 SU4 Task 5）。」を消す | `SU4 決定 A メモリ境界` は副ラベル。3 行目は今は偽（`retain_visible` を使うのは `results_view.rs`） | `frame_stages.rs:58` が `Task 5 申し送り` を持つ（群 E） |
| B6 | 282-284 | /// [`wanted_icon_keys`] のエラー行テストは**緑のまま**ここだけが赤になった（2026-08-19 実測）。 ⏎ /// **この対照がこのテストを別に置く理由である**——今は両方が同時に赤くなるが、それは両者が ⏎ /// 同じ導出を通るようになった結果であって、片方で足りることの根拠ではない。 | 残す |  | 日付つきの変異の実測（2026-08-19）。このテストを別に置く理由（回帰検出器の対照）を運ぶ |  |
| B7 | 402-402 | /// **この検査が `view.rs` にも `results_view.rs` にも住めない理由**（移設してはならない）: | 残す |  | 「移設してはならない」は現在の制約（needle を被検査ファイルに置かない） |  |
| X1 | 410-411 | `///` 「#1133 の Phase 0a で、実際に `results_view.rs` へ置いた版が変異 7（ゲートの削除）では検出されなかった。」 | 縮める | 「#1133 で、実際に `results_view.rs` へ置いた版がゲートの削除の変異では検出されなかった。」 | `Phase 0a`・`変異 7` は #1133 の副ラベル。検査を被検査ファイルに置けない理由を支える過去の行為の 1 文は残す |  |
