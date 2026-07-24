# #646: egui UI の 2 ウィンドウ化 + メトリクス font 連動 設計

日付: 2026-07-24 / 対象: #646「egui 版のUIを整える」 / 先行: #532 SU7(flip 3 部作完了・WebView2 完全撤去済み・PR #662)
brainstorm で issue 本文の「3 ウィンドウ構成」を **2 窓構成に確定**した(決定 3)。

## 背景と言語化

- SU6.5 の外観目視(G2)で「固定行高 30px / バー高 52px は font_size=24 前提のチューニングであり、既定の 15 では間延びする」が設計課題として挙がった(#646 コメント)。SU6.5 #643 は parity 制約(WebView2 も固定値)ゆえバー高を据え置いたが、**SU7 で WebView2 が消滅し parity 制約は存在しない**。固定高を font 連動へ改める自由がある
- issue 本文の 3 窓案(トースト / メイン / 検索結果)のうち、トーストは issue 自身が「実態は 1 ウィンドウでもいいのか」と未決だった。brainstorm でトーストはメイン同一窓と決定し、2 窓構成になった
- ユーザーが窓分離で得たいものは 3 点: **バーと結果の間の透明ギャップ**(Raycast/PowerToys Run 風)・**角丸と影の輪郭表現**・**伸縮挙動の分離**(バーは常に同じ形・同じ位置)

## スコープ

**含む**: メトリクス(行高・バー高・toast 高)の font_size 連動化 + 結果行の 2 行表示化(名前 + パス・決定 9)(PR1)/ 結果リストの独立窓化・透明ギャップ・DWM 角丸と影・実件数フィット(PR2)/ 新 config キー 3 つ(`window_gap` / `row_padding` / `bar_padding`)。

**含まない**: トーストの別窓化(メイン同一窓を維持)/ 新キーの設定 UI(`snotra-settings`)露出(config.toml 直編集で調整・露出は後続 issue)/ 角丸半径の config 化(DWM 管理ゆえ指定不可)/ テーマ・配色の変更。

## 決定事項

### 決定 1: PR は 2 段(PR1 = メトリクス連動、PR2 = 窓分離)

PR1 は 1 窓のまま固定値を連動式へ置き換える。目に見える改善(既定 font 15 の間延び解消)を先に届け、窓分離のリスクを PR2 に隔離する。PR2 は PR1 の `Metrics` を前提に窓を分ける。

### 決定 2: `Metrics` 純粋核 + config 3 キー(live-read)

`layout.rs`(純粋核・ユニットテスト対象)に新設:

```rust
pub struct Metrics {
    pub bar_height: f64,    // font_size + bar_padding
    pub row_height: f64,    // max(font_size + path_size + row_padding + 4.0, 24.0)  2 行表示(決定 9)
    pub toast_height: f64,  // bar_height と同値
}
```

- **加算式の意味**: `bar_padding` は現行バー高を「font_size=24 でチューニングされた結果」と読み直したもの——既定 28 のとき font 24 で 52 となり**現行とピクセル一致**。`row_height` は 2 行表示(決定 9)の積算: name 行(`font_size`)+ path 行(`path_size = max(font_size × 0.78, 9)`・`RowTheme` と同係数)+ 行間 4 + `row_padding`。既定(font 15・row_padding 6)で約 37px。下限 24 はアイコン 16px + 余白の安全床
- **config キー**(`[visual]`・すべて `#[serde(default = ...)]` で旧 config は既定値のまま読める=移行不要):
  - `window_gap: u32 = 4` — メイン窓と結果窓の隙間 px(PR2 で使用。モックアップ確認で 8 → 4 に決定)
  - `row_padding: u32 = 6`
  - `bar_padding: u32 = 28`
- 3 キーとも `font_size` と同じ**毎フレーム live-read**(`row_theme()` と同じ読み取りで `Metrics` を導出。新しい保持状態は作らない)。config 保存 → `config-applied` wake → 即反映
- `view.rs` の固定値 3 箇所(`draw_result_row` の `row_h = 30.0`・toast 行の `52.0`・`compute_window_height` 引数)を `Metrics` 経由へ置換し、直書きを消滅させる

### 決定 3: トーストはメイン同一窓(2 窓構成)

窓は "main"(バー + toast)と "results"(結果リスト)の 2 つ。toast の見た目の境界は描画(背景色差・区切り)で作る。窓数を増やさないことでフォーカス判定・位置同期の複雑さを持ち込まない。

### 決定 4: 結果窓は「フォーカスを取らない従属窓」+ DWM 角丸

- "results" は setup で生成(**窓生成は setup 限定**の不変条件を維持・`src-tauri/CLAUDE.md`)し、`WS_EX_NOACTIVATE` 拡張スタイルでクリックしてもアクティベーションを辞退する。キーボードフォーカスは常にメイン窓の入力欄にあり、**`blur_should_hide` の純粋核は無改修**(フォーカスを持ちうる窓が従来どおり "main" だけ)
- 表示も非活性(show がフォーカスを奪わないこと。tauri `show()` が内部で活性化する場合は Win32 `ShowWindow(SW_SHOWNOACTIVATE)` へ落とす——実装時に実測で確定)
- 角丸と影は DWM `DwmSetWindowAttribute(DWMWA_WINDOW_CORNER_PREFERENCE, Round)` に委ねる(Windows 11)。softbuffer は AA を持たず自前角丸は品質が出ない——ネイティブ機構優先。メイン窓にも同じ角丸を適用して 2 窓の輪郭言語を揃える
- 却下案: (B) 結果窓もフォーカス可能 + 合成フォーカス hide 判定——窓間フォーカス移動の過渡(両方 false)をデバウンスで吸う必要があり誤 hide の温床。現在の結果窓操作はクリックとホイールのみでフォーカス不要。(C) 1 窓 + レイヤードウィンドウ透過——softbuffer はアルファ合成前提でなく、ギャップのクリック透過も得られない

### 決定 5: 状態は「main が所有・results は写しを描く」一方向フロー

- `SearchWindowView`(main)が検索状態の所有者のまま。毎フレーム、描画用スナップショット(結果行・選択位置・view_kind・indexing 表示状態)を `Arc<Mutex<RowsSnapshot>>` へ発行する
- 新設 `ResultsView`("results" の `EguiView`)はスナップショットを読んで描くだけ。行クリックは index を共有スロットへ積み、main の `egui::Context` を `request_repaint()` で起こして **main 側が起動処理**する(起動ロジックを一箇所に保つ)。既存 `ToastAction` の遅延 dispatch と同型
- 選択移動(↑↓)は main が受け、スナップショット更新後に results の ctx を wake。**「paint 後に状態を変えたら repaint」規範(`src-tauri/CLAUDE.md` egui_shell 節)を窓間に拡張**した形——どちらかの窓の状態に触れたら、その窓の ctx を起こす
- `icon_textures` はテクスチャが egui Context(= 窓の renderer)従属のため `ResultsView` へ移管。main はアイコンを描かない
- indexing 中の案内 overlay・instant/folder 行・`scroll_to_me` は描画ごと results 窓へ移動(挙動不変)

### 決定 6: 位置とライフサイクルは main 起点の従属

- results の位置 = `main.outer_position + main 高さ + window_gap`。results view が毎フレーム突き合わせ、ずれたときだけ `set_position`(`last_set_height` と同型のデルタガード)。toast の出没でメインが伸びれば自然に追従する
- 可視性: `show_results`(結果があり、かつ plain 非表示条件に当たらない)が true なら表示・false なら hide。メイン窓の高さは `bar_height (+ toast_height)` のみで、**結果による伸縮はしなくなる**
- hide は現行 `hide_egui_main` 合流点で**両窓を隠す**(working set trim も同じ合流点のまま)。show は main のみ能動表示し、results は次フレームの `show_results` 判定に従う(reset-on-show でクエリ空 = 結果なし = 非表示)

### 決定 7: 結果窓の高さは実件数フィット(仕様変更・SPEC 同期)

現行 `compute_window_height` は `max_results` 固定で、結果が少ないと窓内に空白が残る。分離後は `min(実件数, max_results) × row_height + padding` で**カードが中身にフィット**する。窓を分けた以上「空白 = 窓の余り」は許されないための押し出し。`HeightParams` に実件数が入り、layout.rs のテストを更新する。SPEC の動的高さ節(§4.5 系)を同期する(AGENTS.md ステップ 0: 文書化された挙動の変更 = 仕様変更)。

### 決定 8: 検証戦略

- **ユニット**: `Metrics` の式・下限・config 反映(layout.rs)/ 実件数フィットの高さ算出 / スナップショット述語
- **skill**: `/plan-review`(計画後)・`/state-check`(results 可視性という新ガード条件・reset-on-show との相互作用)・`/race-check`(スナップショット共有・クリック逆流の await/フレーム間競合)・`/persistence-check`(config 3 キーの後方互換)
- **smoke**: `scripts/smoke-egui.ps1` の前提(trace イベント名・hotkey)が壊れないか確認。実機 GUI smoke で 2 窓の従属(位置・ギャップ・フォーカス非奪取・toast 出没時の追従)と DWM 角丸を目視
- **PR1 の後方互換の要**: バーと toast は font_size=24 のスクリーンショット比較でピクセル一致(`bar_height` の式が現行を再現する証拠)。結果行は 2 行化(決定 9)で**意図的に変わる**ため一致比較の対象外——目視で新レイアウト(上段名前・下段パス・全幅)を確認する

### 決定 9: 結果行は 2 行表示(名前 + パス)を既定にする

モックアップの 2 行トグルを目視して決定(2026-07-24)。

- 上段 = 名前(`font_size`・`text_color`)、下段 = パス(`path_size`・`hint_text_color`・**左寄せ**)。アイコン 16px は 2 行ぶんの左に縦中央
- パスが行の全幅を使えるため、中間省略(`truncate_middle`)は幅超過時のフォールバックに退く。#632 で入れた「name 60% 制限 + path 右寄せ + 実測幅による重なり回避」の座標計算は**廃止**——2 行化で name と path が幅を取り合わなくなり、描画コードは単純になる
- 1 行モードは設けない(config キー化しない・YAGNI。要望が出たら後続 issue)
- 行高は決定 2 の式(`font_size + path_size + row_padding + 4`)。実装は PR1(`draw_result_row` と `Metrics` は同じ変更単位)

## リスク・実装時に実測で確定する点

1. tauri `Window::builder` に `WS_EX_NOACTIVATE` を直接指定する API が無い場合、生成後に `SetWindowLongPtrW(GWL_EXSTYLE)` で付与する(windows クレート v0.62 での API 形状・feature フラグ `Win32_Graphics_Dwm` / `Win32_UI_WindowsAndMessaging` を実装前に確認——`src-tauri/CLAUDE.md` の規律)
2. tauri `show()` が活性化を伴う場合の非活性表示(決定 4 の SW_SHOWNOACTIVATE フォールバック)
3. DWM 角丸は Windows 11 専用(`DWMWA_WINDOW_CORNER_PREFERENCE` は build 22000+)。Windows 10 では角のまま=装飾なしで受容(機能影響なし)
4. 2 窓とも同一イベントループスレッドで `update()` が走る(runtime は窓ごと状態を `HashMap` 管理・`runtime.rs`)。スナップショットの `Mutex` は同一スレッド内の順次アクセスが基本で競合は薄いが、`/race-check` で worker スレッド(icon/folder load)からの wake 経路を検証する

## SPEC 同期対象

- §4.5(動的ウィンドウ高さ): 実件数フィット + 2 窓構成へ
- §11(視覚): 固定 30px/52px の記載箇所を計画時に grep で数え上げ、連動式(`font_size + padding`)へ同期・`window_gap` / `row_padding` / `bar_padding` キーの追加
- §20.3(updater toast): toast 高が `bar_height` 連動になる旨(位置・挙動は不変)
- 結果行レイアウトの記載箇所(name/path の 1 行併記・右寄せを記す §): 2 行表示へ(該当 § は計画時に grep で確定)
