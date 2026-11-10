# 台帳: src-tauri/src/egui_shell/launcher_controller/frame_stages.rs（基準 0265463c・候補 9 行・5 ブロック）

分類: 残す／縮める／書き直す／消す。行番号は基準 sha のもの。原文は候補行だけで、ブロックの全文は基準のファイルで読む。

| ID | 行 | 候補行（原文） | 分類 | 処置後の文（要旨） | 根拠（コードの位置・#NNN） | 範囲外メモ |
|---|---|---|---|---|---|---|
| B1 | 36-36 | /// **返り値の破棄は `#[must_use]` が禁じる**（#934。それまでこの散文が唯一の歯止めだった）—— | | | | |
| B2 | 55-59 | // ResultsView::update() 側（実ゲート）に移設済み——main はもう読み書きしない。 ⏎ // icon パイプライン（icon_textures/icon_missing/icon_pending）も Task 5 で ⏎ // results ウィンドウへ移設済み——main はもう保持しない。hide 中の常駐テクスチャは ⏎ // results 側の retain_visible が空 rows で自然に全クリアする（Task 5 申し送り）。 ⏎ // SU5: in-flight 起動と一時通知は show を跨がない（resetForShow の | | | | |
| B3 | 80-80 | // #633: index build 完了の世代検知 → 現クエリで再検索（runRefresh parity・SU6 spec 決定 3）。 | | | | |
| B4 | 95-95 | // hotkey 登録失敗の pending 消費（SU6 spec 追補 2 + #652）。reset_pending 消費より後 | | | | |
| B5 | 99-99 | //（WebView2 は hidden 中に期限切れ・改善方向の受容差異・SU6 spec 追補 2）。 | | | | |
