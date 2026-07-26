# research: #654 egui 経路の視覚 parity 残余（入力欄 text_color + updater 失敗詳細）

調査日: 2026-07-26 / ブランチ: `fix/654-input-text-color-updater-detail` / HEAD: `f938a0b`

## issue の要約

egui 経路に残る 2 件の視覚差を「実装する / 受容する」判断とともに解消する。

- **項目 1**: updater の install 失敗トーストが generic 文言固定で、失敗理由が `SNOTRA_TRACE` にしか残らない
- **項目 2**: 検索入力欄の**テキスト本体**が `.text_color()` 未指定で egui 既定色にフォールバックする

**2026-07-26 のユーザー判断: 両方実装する。**

## 前提の訂正（issue 本文と実測の食い違い）

issue は 2 件とも「WebView2 parity の縮小差」「flip（SU7）前に判断」という枠組みで書かれているが、**SU7 は完了済みで WebView2 経路は存在しない**（`src-tauri/CLAUDE.md` 冒頭）。比較対象が消えたため、両項目は parity ではなく**それ自体の価値**で判断する必要がある。以下は実測で覆った 2 点。

### 訂正 A: 項目 2 は「カスタムテーマで乖離しうる」ではなく**既定設定で既に間違っている**

issue は「ダークテーマ既定では egui 既定色（≈`#E0E0E0`）と config `text_color` 既定が近く、目視で差が出にくい」とする。実測は違う:

| 経路 | 値 |
|---|---|
| `TextEdit` の色決定（egui 0.35 `widgets/text_edit/builder.rs:463-466`） | `self.text_color` → `visuals.override_text_color` → `visuals.widgets.inactive.text_color()` |
| `WidgetVisuals::text_color()`（`style.rs:1318`） | `self.fg_stroke.color` |
| Dark の `inactive.fg_stroke`（`style.rs:1687`） | `Color32::from_gray(180)` = **`#B4B4B4`** |
| config 既定 `visual.text_color`（`snotra-core/src/config.rs:1586`） | **`#E0E0E0`** = gray(224) |

テーマ解決も Dark で確定する: `snotra-egui-runtime/src/input.rs` は `RawInput.system_theme` を**一度も埋めない**（grep で 0 件）→ egui `Options::theme()`（`memory/mod.rs:350-356`）が `theme_preference = System` → `system_theme = None` → `fallback_theme = Dark`（`memory/mod.rs:319`）へ落ちる。`view.rs` の `set_visuals` は `panel_fill` / `window_fill` / `extreme_bg_color` / `selection.bg_fill` の 4 つだけを上書きし、`override_text_color` も `widgets.*` も触らない（`view.rs:1210-1215`）。

**ゆえに既定設定のまま、入力欄の文字だけが gray(180) で描かれ、結果行の表示名（`#E0E0E0`）より暗い。** カスタムテーマ限定の理論上の差ではなく、既定で見える不整合である。

### 訂正 B: 項目 1 の詳細はトーストにほぼ収まる（「1 行だから無理」ではない）

トーストは 1 行（高さ = `bar_height`・既定 43px）で、メッセージはボタン群の左端でクリップされる（`view.rs:1617-1629`）。ここから「詳細を足しても見えない」と推測しかけたが、headless egui の galley 実測（一時プローブ・撤去済み）で否定された。

`InstallFailed` では `show_install: false`（`notify.rs:186-190`）ゆえ、描かれるボタンは `[閉じる]` 1 個だけで可用幅が広い。

| 対象 | 実測幅 |
|---|---|
| `閉じる` ボタン（`button_size` = 12.006px） | galley 36.0px → 枠込み 52.0px |
| `Dismiss` ボタン | galley 39.8px → 枠込み 55.8px |
| 可用メッセージ幅（`window_width` = 600・`閉じる` のみ） | **約 532px** |
| `更新に失敗しました`（`status_size` = 13.05px） | 117.4px |
| `更新に失敗しました: io error: アクセスが拒否されました。 (os error 5)` | 409.3px → **全部収まる** |
| `更新に失敗しました: Network Error: error sending request for url (https://example.com/releases/latest.json)` | 617.0px → **超過**（末尾が切れる） |

可用幅の導出: `cursor_x` は `rect.right() - 8.0` から始まり、ボタン 1 個で `-(52.0 + 8.0)`。`text_clip` の右端は `cursor_x + 8.0`、テキスト開始は `rect.left() + 8.0`。600 − 8 − 60 ≒ 532px。

**結論**: 詳細のために使える幅は約 415px（`status_size` で Latin 60 文字強）。短い失敗理由は全文表示でき、長い URL 付きエラーは末尾が欠ける。**現状のクリップは省略記号を出さない**ため、超過時は文字の途中でぶつ切りになる → 末尾省略（`TextWrapping::truncate_at_width`）へ変えるのが妥当。

## 関連コード

| ファイル | 位置（見出し・シンボル） | 役割 |
|---|---|---|
| `src-tauri/src/egui_shell/view.rs` | `EguiView::update` 内の `TextEdit::singleline`（1464 行付近） | 検索入力欄。`.interactive()` / `.font()` / `.hint_text(...color(path_color))` はあるが `.text_color()` が無い（**項目 2 の修正点**） |
| 同 | updater toast 描画（`toast_row` を消費するブロック・1570-1630 行付近） | `ToastKind` → 文言 → クリップ描画。`line1` の生成と `with_clip_rect(...).text(...)`（**項目 1 の描画点**） |
| 同 | `spawn_install`（1026 行付近） | `download_and_install` の `Err(e)` で `trace_main("egui_update_install_failed", {error})` を出したうえで `phase = InstallFailed` を代入（**項目 1 の詳細の入手点**。`e.to_string()` は既にここにある） |
| 同 | `draw_toast_button`（1065 行付近） | 右詰めボタン。`cursor_x` を進めるので、これが可用幅の右端を決める |
| `src-tauri/src/egui_shell/notify.rs` | `UpdaterPhase`（88-103 行）・`ToastKind`（116-121 行）・`UpdaterUi::toast`（171-193 行） | `InstallFailed` / `ToastKind::Failed` は #648(C) で payload を落とした unit variant（**項目 1 で message を復活させる**） |
| `src-tauri/src/egui_shell/strings.rs` | `update_failed`（103-108 行） | 引数なしの generic 文言（**項目 1 で detail 併記へ**） |
| 同 | `launch_failed` / `launch_timeout`（41-54 行） | **既存パターン**: `(l: Language, detail: &str) -> String` で detail は**呼び出し側が整形済み**（`" ({m})"` or `""`） |
| `src-tauri/src/egui_shell/visual.rs` | `RowTheme`（46-47 行）・導出（103-113 行） | `name_color` ← config `text_color`、`path_color` ← config `hint_text_color`。**項目 2 が使うのは `name_color`** |
| `src-tauri/src/egui_shell/results_view.rs` | 行描画（272 行付近） | `TextWrapping::truncate_at_width(avail)` + `layout_job` の**既存の末尾省略パターン**（項目 1 の描画で流用） |
| `SPEC.md` §11「見た目の規範」 | 559 行 | 「**色は config `[visual]` から取る。** 描画コードに色リテラルを書かない」（項目 2 の根拠） |
| `SPEC.md` §11「as-built」 | 571 行 | 「検索入力欄は `font_size` に追従し hint は `hint_text_color` で描かれる」（項目 2 で入力テキスト色を追記する箇所） |

### 列挙の事実確認

上記のファイル・関数はすべて grep で実在と行位置を確認済み。加えて:

- **`TextEdit::text_color` は egui 0.35 に実在する**（`widgets/text_edit/builder.rs:250` — `pub fn text_color(mut self, text_color: Color32) -> Self`）。issue 本文の記述を鵜呑みにせず API を確認した
- **`TextWrapping::truncate_at_width` は epaint 0.35 に実在する**（`text/text_layout_types.rs:701` — `max_rows: 1, break_anywhere: true`。`overflow_character` は Default の `Some('…')` を継承）

## 同一パターン全コードパス検索（AGENTS.md「バグ発見時」）

項目 2 の根本パターン＝「描画テキストに色を明示せず egui 既定へ落ちる箇所」を `view.rs` / `results_view.rs` 全体で列挙した。

| 描画点 | 色 | 判定 |
|---|---|---|
| `view.rs:1088` `draw_toast_button` の galley | `theme.name_color` / `theme.path_color`（enabled で分岐） | OK |
| `view.rs:1472` 入力欄 `hint_text` | `bar_theme.path_color` | OK（#643） |
| `view.rs:1559` status 行 | `visual.hint` | OK |
| `view.rs:1623` toast メッセージ | `theme.name_color` | OK |
| `results_view.rs:276 / 301 / 312 / 313` 行の name / path | `theme.name_color` / `theme.path_color` | OK |
| **`view.rs:1464` 入力欄の TextEdit 本体** | **未指定 → egui 既定 gray(180)** | **NG（唯一の該当）** |

**色未指定はこの 1 箇所のみ**。ゆえに項目 2 は一箇所の修正で「§11 テーマ消費の完全化」を達成でき、全称表現（「すべての描画テキストが config 由来になる」）を嘘なく書ける。

## 既存パターン（再利用できるもの）

- **detail 併記の文言**: `launch_failed(l, detail)` / `launch_timeout(l, detail)` が確立済み。`update_failed` も同型へ揃える
- **末尾省略**: `results_view.rs` の `TextWrapping::truncate_at_width(avail)` + `painter().layout_job()`
- **1 フレーム 1 lock のテーマ読み**: `visual.row`（`RowTheme`）は既にトーストと入力欄の両方で使われており、新たな読みは増えない

## 技術的制約

- **Win32 依存なし**。両項目とも egui 描画層に閉じる（`SendInput` 等の非同期 API は関与しない）
- **`spawn_install` は async タスク**（`tauri::async_runtime::spawn`）で、`Err(e)` の枝は worker スレッドから `UpdaterUiState` の Mutex を取り `phase` を代入し `wake_main` する。**`message` を足しても並行境界の形は変わらない**（同じ lock 内で同じ 1 代入。フィールドが増えるだけ）
- **hidden 中は `update()` が走らない**（`src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」）。`InstallFailed` は既に `wake_main` を伴っており、message 追加でこの契約は変わらない
- **`UpdaterPhase` に derive は無い**（`notify.rs:88`）ため String フィールド追加でトレイト境界の問題は生じない。`ToastKind` は `#[derive(Debug, PartialEq)]` を持つので `Failed { message: String }` でも問題ない（`Available { version: String }` が先例）
- **可用幅は `window_width`（config・既定 600）依存**。ユーザーが窓幅を狭めれば詳細の可視量は減る——末尾省略にすることで「切れている」ことが読者に伝わる形にする

## 未解決の疑問

- **`update_failed` の detail 引数の契約をどちらに寄せるか**（呼び出し側整形 `": {msg}"` / 関数側でセパレータ生成）。`launch_failed` は前者。同名引数で契約が違うと罠になるため**前者へ揃える**方針とするが、plan で確定させる
- **トーストの `[閉じる]` ラベルは言語で幅が違う**（`閉じる` 52.0px / `Dismiss` 55.8px）。可用幅の差は 3.8px で、末尾省略にすれば挙動差は「省略位置が数 px 動く」だけ。追加の対処は不要と判断した
