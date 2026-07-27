# config の背景色を実際に効かせる設計（#680 の 1 を含む）

- 日付: 2026-07-28
- 対象 issue: #680（`[visual]` 色設定の乖離のうち 1＝hex パーサ 2 本立て）/ #751 の「副次的な発見」（`panel_fill` が死んだ書き込みか）
- 対象 crate: `src-tauri/`（主）・`snotra-egui-runtime/`（`RuntimeFrame` と renderer）
- 前提: #532 SU7 完了後・#666 段 3（`view.rs` 分割）マージ後
- 契機: PR #791 のレビュー。同 PR は「色変更が実ウィンドウへ反映されない」を wake と repaint の配線として扱ったが、本 spec は**終端に消費者がいない**ことを欠陥とみなす

## 1. この spec が答えている問い

`config.toml` の `[visual].background_color` を変えても、main / results の背景は変わらない。

値の行き先は `view.rs` のテーマ適用ブロックにある `visuals.panel_fill` / `visuals.window_fill` への代入 2 本だけである。この 2 値を読む egui ウィジェット——`CentralPanel` / `SidePanel` / `TopBottomPanel` / `egui::Window` / popup / menu / `ComboBox`——は `src-tauri/src/` にも `snotra-egui-runtime/src/` にも**1 つも無い**（2026-07-28 grep 実測）。`view.rs` が使う `egui::Frame::new()` は fill を持たない構築子であり、`Frame::window()` とは違って `window_fill` を読まない。実際の背景は `snotra-egui-runtime/src/renderer.rs` の定数 `CLEAR_COLOR`（`0x0028_2828`）を `buffer.fill` が毎フレーム塗ったものである。

`config.rs` の `default_background_color()` が `#282828` で `CLEAR_COLOR` と一致しているため、**既定のままではこの乖離は観測できない**。非既定色を設定して初めて、「show の一瞬だけ設定色が見え、softbuffer が present した瞬間に `#282828` へ落ちる」という形で現れる。白フラッシュを消すために置いたネイティブ背景ブラシが、設定によっては白フラッシュを作る側に回る。

本 spec は、この値に消費者を与える。

## 2. 却下した案と、その根拠（否定の知識）

### 2.1 `EguiView` に pass 前フックと関連型 `PassData` を導入する案

当初案は、`run_ui` が root `Ui` を作る**前**に呼ばれるフック（`begin_pass`）を `EguiView` に足し、そこで clear color と style の両方を設定する形だった。`VisualSnapshot` を view の `self.` へ保持させないため、関連型 `PassData` で runtime に opaque に持ち回させる設計を含む。

**取り下げる。** pass 前フックが必要なのは #751 の 3 値（`extreme_bg_color` / `selection.bg_fill` / `weak_text_color`）だけであり、背景色には不要だからである。`render()` は `run_ui` → renderer の順に進むため、`update()` の中で決めた色は**同じフレームの `buffer.fill` に間に合う**（決定 1）。

したがって関連型の代価は、まるごと #751 のために払うことになる。そして #751 は issue 自身が「症状は**未観測**」「まず手順で症状を確定させること」と記しており、修正方針 3 案のうちどれを採るかも「この issue では決めない」と明記している。**未観測の症状のために trait へ型パラメータを足すのは過剰である。**

これは PR #791 の誤りの鏡像でもある。#791 は未観測の #751 へ対症療法（`request_repaint`）を撃った。当初案は同じ #751 へ構造変更を撃とうとした。対症療法より構造的な解を好む選好は正しいが、**直すべきかがまだ分かっていない対象に適用すれば、選好が正しくても過剰になる。**

### 2.2 view 側で全面を egui で塗る案

root `Ui` の先頭で `ui.painter().rect_filled(ui.max_rect(), 0.0, visual.background)` を撃てば、runtime を触らずに背景が効く。

**取り下げる。** `CLEAR_COLOR` という第 2 の SSOT が「egui が描かない領域の下地」として生き続け、両 view に同じ塗りを書くことになる。本 spec の目的は背景色の導出を 1 本にすることであり、この案は消費者を増やすだけで SSOT は 2 本のまま残る。

### 2.3 `CentralPanel` を導入して `panel_fill` を生かす案

egui の標準的な形へ寄せれば、死んだ書き込みが生きた書き込みになる。

**取り下げる。** `CentralPanel` は余白とレイアウトを持ち込むため、現在の raw painter ベースのレイアウトが動く。侵襲の大きさに対して得るものが「egui らしさ」に留まる。なお `CLEAR_COLOR` は依然として `CentralPanel` の外側の下地として残るため、SSOT も 1 本にならない。

### 2.4 ネイティブブラシを `config-applied` の listener から更新する案

hidden 中の config 変更を即座にブラシへ反映できる。

**取り下げる。** `app.listen` のコールバックは emit 元スレッドで同期実行される（`src-tauri/CLAUDE.md`「Win32 メッセージ配送の注意」・tauri 2.11.4 実測）。`config-applied` の emit 元は config 監視スレッドであり、そこから tao の `set_background_color`（内部で `InvalidateRect` + `UpdateWindow`）を呼ぶと、窓のスレッドへの同期メッセージ送信になる。決定 3 の「show 直前に無条件で設定する」なら、この越境そのものが要らない。

## 3. 決定

### 決定 1: clear color をフレームごとに view が決める

`snotra-egui-runtime` に次を足す。

- `RuntimeFrame` に `clear_color: Option<egui::Color32>` フィールドと `pub fn set_clear_color(&mut self, color: egui::Color32)`
- `EguiWindow::render()` が `run_ui` の**後**に `frame.clear_color` を読み、`EguiRenderer::paint` へ引数として渡す
- `EguiRenderer::paint` 内の `buffer.fill` が受け取った色を使う。`None` のときは現行の `CLEAR_COLOR` へ落ちる

**paint だけが `run_ui` 抜きで走る経路は無い**（2026-07-28 実測）。`render()` は `run_ui` → `handle_platform_output` → `paint` → `apply_frame_commands` の一本道で、paint 失敗時の再試行は `request_repaint_after(delay)` により**次フレームの `render()` 全体**をやり直す（不変条件⑤）。ゆえに `clear_color` をフレームローカルの `RuntimeFrame` に置いても、再試行フレームで失われることはない。

`src-tauri` 側は、両 view の `update()` が `frame.set_clear_color(visual.background)` を 1 行呼ぶだけである（`SearchWindowView` と `ResultsView`）。

**遅れは原理的に発生しない。** `render()` の順序が `run_ui`（view が色を決める）→ paint（背景を塗る）だからである。#751 が抱えている「root `Ui` が pass 冒頭で `Arc<Style>` を掴むため、`ctx.set_visuals` はその pass に届かない」という制約は、**style を経由しないこの経路には無い**。

`Color32` → softbuffer の `u32` は `0x00RRGGBB`（alpha は捨てる）。

**`None` フォールバックを残す理由**: 起動直後、view が 1 枚目のフレームを描く前にも renderer は呼ばれうる。また `set_clear_color` の呼び忘れは背景が既定色に留まる形で現れ、視覚検証（§5）で捕捉できる。呼び忘れを型で禁じる形（`EguiView` に `fn clear_color()` を要求する）は採らない——view が `read_visual` を 2 回呼ぶことになり、#673 決定 4 が閉じた窓（フレーム内で新旧が混ざる）を開け直すためである。

### 決定 2: `panel_fill` / `window_fill` への代入を撤去する

消費者ゼロを確認したうえで削除する（§1）。`visual.rs` の `//!` および `VisualSnapshot::background` のフィールド doc から、`panel_fill` を指す記述を決定 1 の経路へ書き換える。

### 決定 3: ネイティブ背景ブラシは show 直前に無条件で設定する

エッジ検出をやめ、`window_coordinator::show_egui_main`（main）と `ResultsWindow::show`（results）が show の直前に毎回 `set_background_color` を撃つ。

これにより次の 3 つが撤去される。

- `VisualApplied::background_hex`
- `VisualSnapshot::background_hex_changed`
- `SearchWindowView::applied_background_hex`

**エッジ検出がこの値に不利な理由**: ブラシの消費者は「show してから最初の softbuffer present までの一瞬」であり、変化の検出者は `update()` である。hidden 中は `update()` が走らない（#697 実測）ため、config 変更後の初回 show では旧色が一瞬見える。両者が別のタイミングに住んでいる以上、**変化した瞬間に居合わせる必要がない形へ倒す**のが正しい。show は頻繁な操作ではなく、同値の再設定は安価である。

**副次的な効果**: results への tao API 呼び出しが `ResultsWindow` の内側に入る。`Manager` から results の生ハンドルを引く書き方（`get_window("results")`）は `src-tauri/CLAUDE.md`「Win32 / Tauri 注意事項」が「依然コンパイルが通り黙って no-op する」と警告していた面であり、呼ぶ動機そのものが消える。

なお tao 0.35.3 の `set_background_color` は `window_state` への代入と `InvalidateRect` / `UpdateWindow` だけで、`apply_diff` を通らない（実測）。ゆえに同 CLAUDE.md の「results の 3 操作は raw へ寄せる」の判定基準（フラグ差分が生じるか）には**当たらない**——この API は tao 経由のままでよい。

### 決定 4: 背景色の hex パーサを 1 本にする

ブラシ色を `VisualSnapshot::background`（`egui::Color32`）から `tauri::window::Color` へ変換して得る。これにより `config_watcher::parse_hex_color` の製品コード上の消費者が 0 になるため、関数ごと撤去し、既存のテスト群は変換関数側の等価なテストへ移す。

- 受理形が `Color32::from_hex` に揃い、`#RGB` / `#RGBA` も通る（**#680 の 1 が閉じる**）
- alpha は両経路とも捨てる。根拠は softbuffer 側で、buffer が `0x00RRGGBB` である以上**定常の背景に alpha を表現する先が無い**。ブラシ側（tao の `Color` は alpha 成分を持つ）で単独に alpha を効かせると定常と食い違うため、一致させる目的で捨てる。`#RRGGBBAA` を書いた場合 alpha は無視される（撤去する `parse_hex_color` が alpha 255 固定だったのと同じ実効挙動）
- `egui_shell::create` が受け取る `background_color_hex: &str` も同じ変換を通す（窓生成時の初期ブラシ）

## 4. 触らないもの

- **wake の配線**。背景色は決定 1 で clear color 経由になり、results が新色を描くには results のフレームが 1 枚走ればよい。可視中は `drive_results_window` 末尾の level-triggered wake が毎フレーム供給し、hidden 中は描かない。`2026-07-25-egui-window-ownership-and-event-delivery-design.md` 決定 5 の理由（results は config 系イベントを一切 listen しない）は維持され、同決定は無変更でよい
- **#751 の 3 値**（`extreme_bg_color` / `selection.bg_fill` / `weak_text_color`）。issue 自身が定めた順序——実機で症状を確定させてから方針を選ぶ——に従い、本 spec では扱わない。決定 1 は style を経由しないため #751 の機構とは独立であり、先に入れても後の選択肢を狭めない
- **#680 の 2**（`window_gap` 既定値の手書きコピー）。背景色と関係しない

## 5. 検証

- **単体**: `Color32` → softbuffer `u32` と `Color32` → `tauri::window::Color` の変換。alpha 落とし・`#RGB` 展開・parse 失敗時の既定フォールバックを含む。`parse_hex_color` の既存テストが担っていた命題（`#RRGGBB` の受理・不正形の拒否）は移設先で維持する
- **視覚**（`docs/build-commands.md` カテゴリ D・`cargo run -p snotra`）: `background_color` に**非既定色**（例 `#4A2B5C`）を設定し、次の 3 点を目視する
  1. 定常の背景が設定色である
  2. show の一瞬も同色である（ブラシと clear color が一致しフラッシュしない）
  3. results の背景も同色である

  **既定 `#282828` では `CLEAR_COLOR` と一致して差が出ないため、既定色での確認は本 spec の検証にならない。**
- **回帰**: 既定色のままで従来と同じ見え方であること
- **`#RGB` 経路**: `background_color = "#FFF"` で、ブラシと定常が**ともに白**であること（#680 の 1 が閉じたことの確認）

## 6. 副次的に閉じるもの

- **#680 の 1**（hex パーサ 2 本立て）— 決定 4
- **#751 の「副次的な発見」**（`panel_fill` が死んだ書き込みか）— 確認して撤去したため、issue から落とせる
- **PR #791 のレビュー指摘 C3**（results へ `get_window` で生ハンドルを引く）— 決定 3 の副次的な効果

## 7. 残余

- #751 の 3 値は未解決のまま残る（§4）。本 spec の実装後も、色だけの config 変更では入力欄まわりが次フレームまで旧色に留まりうる
- 決定 1 の `None` フォールバックは、`set_clear_color` の呼び忘れを静かに旧挙動へ落とす。検知手段は §5 の視覚検証だけである（受容する残余）
