# 台帳: src-tauri/src/egui_shell/launcher_controller/frame_stages.rs（基準 0265463c・候補 9 行・5 ブロック）

分類: 残す／縮める／書き直す／消す。行番号は基準 sha のもの。原文は候補行だけで、ブロックの全文は基準のファイルで読む。

| ID | 行 | 候補行（原文） | 分類 | 処置後の文（要旨） | 根拠（コードの位置・#NNN） | 範囲外メモ |
|---|---|---|---|---|---|---|
| B1 | 36-36 | /// **返り値の破棄は `#[must_use]` が禁じる**（#934。それまでこの散文が唯一の歯止めだった）—— | 縮める | `（#934）` | 「それまで〜」は作業記録 | |
| B2 | 55-59 | // ResultsView::update() 側（実ゲート）に移設済み——main はもう読み書きしない。… Task 5 で … 移設済み … （Task 5 申し送り）。 ⏎ // SU5: in-flight 起動と一時通知は show を跨がない（… | 書き直す＋縮める | 「scroll gate（#632: …）の実ゲートは results ウィンドウの ResultsView::update() 側にあり、icon パイプライン（…）も results ウィンドウが持つ——main はどちらも読み書きしない。hide 中の常駐テクスチャは results 側の retain_visible が空 rows で自然に全クリアする。」・「in-flight 起動と一時通知は show を跨がない（… parity・#532）。…（stale Ok が再 show ウィンドウを hide で撃つ事故の backstop）」 | 移設の記録（Task 5・移設済み・もう）を現在形の所有へ。`results_view.rs` が `last_scrolled_selected`（scroll gate）・`icon_pending`・`retain_visible` を持ち、`view.rs` / `launcher_controller*` に `icon_missing` / `icon_pending` は無い（grep で確認）。`Task 5` は対応表で消す。`SU5` → #532。「並行性レビュー High」は `git log -S` 最古 70a34910（#647・#532）で、同じ括弧に #532 があるのでラベルだけ消した | |
| B3 | 80-80 | // #633: … 現クエリで再検索（runRefresh parity・SU6 spec 決定 3）。 | 縮める | `・#532）` | `git log -S 'SU6 spec 決定 3'` 最古 04612f60（#651・#532） | |
| B4 | 95-95 | // hotkey 登録失敗の pending 消費（SU6 spec 追補 2 + #652）。 | 縮める | `（#532・#652）` | `git log -S 'SU6 spec 追補 2'` 最古 d1bd98c9（#655・#532 Phase 2） | |
| B5 | 99-99 | //（WebView2 は hidden 中に期限切れ・改善方向の受容差異・SU6 spec 追補 2）。 | 縮める | `・#532）` | B4 と同じ | |

## 範囲外メモ

- `//!` と `poll_async` の「spec C 節 不変条件 2」は設計 spec を指すだけの参照
