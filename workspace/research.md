# research: issue #714 — 再検索時、結果窓が前のスクロール位置から先頭までアニメーションする

前提資料: `docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md`（契約⑤「不連続遷移にアニメーション経路を使わない」の view 側実装・§7 が対処案を確定済み・実施順序 §8 の 2 番）。※同 spec は PR #741（未マージ）で追加——本ブランチからは見えないが GitHub 上で参照可能。

## issue の要約

スクロールを伴う位置まで選択を移動した後に別キーワードで再検索すると、結果窓が旧スクロール位置から先頭まで 0.1〜0.3 秒かけて視覚的にスクロールする。新しい検索結果は先頭選択なので**瞬時に先頭が見えているべき**。「選択行が変わったので寄せる」経路（アニメーションが自然）と「結果集合の総入れ替え」（位置を持ち越さないのが正しい）が同じ `scroll_to_me` 経路に流れていることが原因。**↑↓ の選択移動のアニメーションは維持する**（issue 明記）。

## 関連コード（2026-07-26 の main で実在確認済み）

| 場所 | 現状 |
|---|---|
| `src-tauri/src/egui_shell/results_view.rs:482-485` | 世代検知: `snapshot.generation != self.last_generation` で scroll gate（`last_scrolled_selected`）を強制リセット（#632 reviewer Important 3 の後継・Fix 3）。**修正の挿入点はこの分岐の真偽をローカルに保持して `ScrollArea` 構築へ渡す形** |
| `src-tauri/src/egui_shell/results_view.rs:487-505` | `do_scroll` 算出 → `egui::ScrollArea::vertical().show(...)` → 行描画。`sel && do_scroll` が `draw_result_row` へ |
| `src-tauri/src/egui_shell/results_view.rs:277` | `response.scroll_to_me(None)`——アニメーション経路（egui 0.35 `style.rs` の `ScrollAnimation { points_per_second: 1000.0, duration: 0.1..=0.3 }`） |
| `src-tauri/src/egui_shell/results_view.rs:420-427` | 空 rows の早期 return。`last_scrolled_selected` はリセットするが **`last_generation` はリセットしない**——hidden 中に世代が進むと、再表示の最初の可視フレームが「世代変化フレーム」になる（本修正にとって望ましい: 新リストは先頭表示で始まる） |
| `src-tauri/src/egui_shell/view.rs`（`reset_selection`） | 毎打鍵 `selected=0`（SolidJS parity・M1 gap 是正のコメント）。よって再検索直後の世代変化フレームでは通常 selected=0 |
| egui 0.35 `scroll_area.rs:495,508` | `ScrollArea::scroll_offset(Vec2)` / `vertical_scroll_offset(f32)`——**そのフレームの offset を直接指定する API が実在**（vendored ソースで確認） |

## 既存パターン

- 世代検知の分岐（:482）が既にあり、修正はその真偽を `ScrollArea` 構築（:489）まで運ぶだけ。新しい状態フィールドは不要
- `scroll_to_me` は残す: 世代変化フレームで offset を 0 に直置きした後、selected=0 なら scroll_to_me は不可視→可視の移動を要さず無発火（アニメーションなし）。仮に selected≠0 の世代交代（現行フローでは通常起きない）が来ても、**先頭からの**寄せになり「前のリストの位置持ち越し」は起きない——issue の症状だけが消える最小修正

## 技術的制約

- `results_view.rs` は実窓 Context 依存で純粋核ではない（ユニットテスト対象外・検証は目視 + trace）
- **体感（瞬時 vs アニメーション）の自動アサートは困難だが、副次信号は取れる**: スクロールアニメーションは 0.1〜0.3 秒の連続描画バーストを生む（issue「副次的な効果」）。`SNOTRA_EGUI_REPAINT_TRACE` で再検索直後のフレームバーストの有無を before/after 比較できる
- PR #741（#697）は未マージだが、触るファイルが重ならない（#741 は results_view.rs に触れていない）
- 設計書 §8 の「#714 修正 + `measurement.md` プロトコルで再測定」のうち**基線再測定は #737 サイクル冒頭へ送る**——`PERFORMANCE.md`「warm frame は日をまたいで比較しない（同日・同条件で両方を測る）」に照らし、基線と上限適用後を #737 の同一セッションで測るほうが比較が成立する（受け皿: #737 の受け入れ条件 1 が「同一プロトコルの実測」を要求しており、その実測に基線取得が内包される）

## 未解決の疑問

なし（対処案は設計書 §7 が確定済み・API 実在確認済み・要求は一意）
