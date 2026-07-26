# plan: issue #714 — 世代交代フレームの scroll_to_me をアニメーションなしにする（案 B′）

前提は `workspace/research.md` と `workspace/plan-review-2b.md`（独立導出）。type:fix / size:S。挙動変更は「世代交代フレームのスクロール遷移が瞬時になる」のみ。

**plan-review（2026-07-26）で初版の案 (A)（`vertical_scroll_offset(0.0)` 直置き）は反証され、(B′) へ改訂した**（下の「否定の知識」）。

## 採る案: (B′) 世代変化フレームだけ per-call でアニメーションを無効化

`Response::scroll_to_me_animation(None, egui::style::ScrollAnimation::none())`（egui 0.35 `response.rs:822-827` で `scroll_to_me` がこの API の糖衣であることを確認済み）を、世代交代フレームの選択行だけに使う。

- 正しい目標は「先頭」ではなく「**選択行が瞬時に見えていること**」——再検索（selected=0）では瞬時に先頭、`on_escape` の folder/tool 復帰（selected≠0 復元・`search_state.rs:334,343`）では瞬時に復元位置
- ↑↓ の選択移動（世代不変）は従来どおりアニメーション付き `scroll_to_me`（issue 要件）

## 否定の知識（plan-review で反証された初版案）

- **(A) `vertical_scroll_offset(0.0)` 直置ち**: ① `on_escape` 復帰と index 再構築後の再検索は selected≠0 のまま世代を進める——先頭 0 固定は選択行を見失わせる ② バックグラウンド reindex 完了の再検索は**内容同一でも**世代を加算（`set_results` は比較せず無条件 `+= 1`）——閲覧中に無操作で先頭スナップする新規の破れを作る ③ builder offset は in-flight の `offset_target` を消さない（egui 実装確認・独立導出）
- **`ScrollArea::animated(false)`**: stale な `offset_target` を更新も破棄もしない（独立導出が却下）
- **設計 spec §7（`docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md`・PR #741）の機構記述は (A) のまま**であり訂正が要る。契約⑤の意図（不連続遷移にアニメーション経路を使わない）は (B′) が満たす。**受け皿: 本実装の実機確認後、PR #741 ブランチへ spec §7 の訂正コミットを足す**（本セッション内で実施）

## 変更ファイル一覧

| ファイル | 変更 |
|---|---|
| `src-tauri/src/egui_shell/results_view.rs` | ① `RowScroll { None, Animated, Instant }` enum と純粋な指示関数 `scroll_directive(selected, do_scroll, generation_changed) -> RowScroll` を追加（TDD: Red→Green） ② `draw_result_row` の `scroll: bool` 引数を `RowScroll` へ ③ update() で世代検知の真偽を `generation_changed` に保持し `scroll_directive` を呼ぶ ④ なぜ per-call 無効化か・(A) を却下した理由の要約コメント |
| `src-tauri/src/egui_shell/search_state.rs` | `rows_generation` の doc に「選択移動（↑↓）で進めてはならない——進めると #714 の Instant 経路に入りアニメーション要件が壊れる」を 1〜2 行追記（既存不変条件の耐久線。既存テスト `rows_generation_is_stable_on_enter_folder` が挙動を固定済み） |

## 実装

```rust
/// 行スクロールの指示（#714）。世代交代（結果集合の総入れ替え）は位置を持ち越さず
/// 瞬時に選択行へ、選択移動（世代不変）は従来のアニメーションで寄せる。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RowScroll { None, Animated, Instant }

fn scroll_directive(selected: bool, do_scroll: bool, generation_changed: bool) -> RowScroll {
    if !(selected && do_scroll) { RowScroll::None }
    else if generation_changed { RowScroll::Instant }
    else { RowScroll::Animated }
}

// draw_result_row 内（現行 :277 の scroll_to_me を置換）:
match scroll {
    RowScroll::None => {}
    RowScroll::Animated => response.scroll_to_me(None),
    RowScroll::Instant => {
        response.scroll_to_me_animation(None, egui::style::ScrollAnimation::none())
    }
}
```

## 不変条件

1. **↑↓ ナビのアニメーションは不変**: 世代不変フレームは `Animated`。`rows_generation` を進める箇所は `search_state.rs` の 5 箇所のみで選択移動は含まれない（偵察が全列挙・private フィールドで迂回代入なし）
2. **#632 の scroll gate 機構（`last_scrolled_selected` / `last_generation` の更新規則）は不変**
3. **`RowScroll` の分岐は網羅 match**——bool 引数の追加で黙って既定に落ちる形を作らない
4. **受容する残余**（独立導出が文書化・コメントに要約）: (a) ↑↓ 押下直後 ~0.1s 以内に再検索が完了すると旧アニメーションの残り時間が食い込みうる（目標位置は正しい） (b) 内容同一の reindex 再検索では「アニメーション付きで選択行へ戻る」が「瞬時に戻る」に変わる（目標は同じ・頻度は稀）

## テスト方針

- **TDD（純粋核）**: `scroll_directive` に `#[cfg(test)]` テストを先に書く（Red: スタブで 1 件落とす → Green）。ケース: 非選択→None / 選択+gate 閉→None / 選択+gate 開+世代不変→Animated / 選択+gate 開+世代交代→Instant
- post-edit hook: clippy + `cargo test -p snotra`（沈黙=合格）
- **実機検証（カテゴリ D・受け入れ条件）**: `cargo run -p snotra` で (1) 検索 → ↓ で スクロール位置まで移動 → 別キーワード再検索 → **瞬時に先頭** (2) ↑↓ のアニメーション維持 (3) folder 展開 → Escape 復帰 → 瞬時に復元位置
- 副次信号: `SNOTRA_EGUI_REPAINT_TRACE` で再検索直後の 0.1〜0.3 秒バーストが消えることを確認

## SPEC.md 更新要否

不要（スクロールアニメーションの有無は未文書化・独立導出も同判定）。spec §7 の訂正は上記「否定の知識」の受け皿で行う。

## 基線再測定（設計 spec §8 の 2 番後半）

**#737 サイクル冒頭へ送る**。`PERFORMANCE.md`「warm frame は日をまたいで比較しない」——基線と上限適用後を #737 の同一セッションで測る。受け皿: #737 受け入れ条件 1 の実測に内包。

## コミット構成

1. `chore: workspace 調査・計画 (issue #714)`
2. `fix(egui): 世代交代フレームの scroll_to_me をアニメーションなしにし、位置の持ち越しを断つ (#714)`

## セルフレビュー

### 5a. plan-review の反映

- 要対処 1（reindex の無条件世代加算で無操作先頭スナップ）→ (A) を却下し (B′) へ改訂（目標が「選択行」になり、破れは「瞬時 vs アニメーション」の差に縮む——残余 (b) として受容・明記）
- 軽微 2（escape 復帰の selected≠0）→ (B′) が構造的に正しく扱う
- 軽微 3（ホイール操作との同一フレーム競合）→ 要対処 1 の具現例であり (B′) で同様に縮退
- 独立導出との差分: 案の選択（A→B′）・`draw_result_row` の enum 化・`rows_generation` doc 追記・純粋核テストをすべて採用。spec §7 訂正の受け皿を新設。**一致**: 変更は results_view.rs 中心の 1 経路に閉じる・SPEC/CLAUDE.md 追随不要・世代カウンタ 4 概念のうち rows のみが対象

### 5b. plan-review が扱わない 3 観点

1. **境界条件**: rows=1 件（スクロール不要・全指示で無害）/ 世代交代と ↑↓ が同一フレーム（世代検知が勝ち Instant——「新リストの選択行へ瞬時」で正しい）/ hidden 中の複数回世代進行（再表示の初可視フレームが Instant——望ましい）/ in-flight アニメーション中の Instant（egui は scroll target を置換・残余 (a)）
2. **シンプル化**: 新規状態フィールドなし。enum + 純関数 1 つが最小形（bool 2 つを裸で渡す形は呼び出し点で意味が読めず、3 値の網羅 match が「黙って既定に落ちる」を防ぐ）
3. **破壊不変条件 + 検知手段**: 「↑↓ のアニメーション維持」が壊れたら即アウト → `scroll_directive` の Animated ケースをユニットテストが固定 + 実機 (2) で目視。「毎フレーム瞬時スクロール」事故（gate 破れ）→ 世代検知の更新規則不変 + 実機 (1)(2)
