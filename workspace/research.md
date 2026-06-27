# research — issue #399 設定UIの見出しに Semibold ウェイトを適用する

## issue の要約

PR #398 で `snotra-settings` のタイポグラフィを Fluent タイプランプ（見出し18 / 本文14 / 副文12）に**サイズのみ**準拠させた。本 issue では Fluent 推奨の「**見出しは Semibold**」を実現する。セクション見出し（`style::section_heading`）・モーダル題（`style::modal_header`）を Semibold で描画し、本文・副文は Regular のまま。Semibold フォント不在環境では panic せず Regular へ graceful degrade する。

## 関連コード

- `snotra-settings/src/font.rs` — フォント登録。`configure_fonts(ctx)` が日本語フォント候補（`YuGothM.ttc` 他）を `"jp_font"` として Proportional/Monospace の fallback に積む（Regular 相当・Yu Gothic Medium）。`list_system_fonts()` は Visual タブ用（本件では変更不要）。
- `snotra-settings/src/style.rs` — `apply_type_ramp(ctx)` が `ctx.all_styles_mut` で `TextStyle::{Heading, Body, Button, Small}` の **`size` のみ**を書き換える。`FontFamily` は egui 既定を温存（＝全 Regular）。`section_heading`/`modal_header` はともに `ui.heading()` を呼ぶ。
- `snotra-settings/src/app.rs` `run()`（600-605行）— 初期化順は `configure_fonts` → `apply_win11_theme` → `apply_type_ramp`。
- `snotra-settings/SETTINGS-DESIGN.md` — タイポグラフィ節（21行）に「ウェイトは size のみ／Semibold 化は follow-up（#399）」と注記済み。
- `snotra-settings/CLAUDE.md` — `style.rs` / `apply_type_ramp` の説明（タイポグラフィの SSOT）、`font.rs` の説明。

### `ui.heading()` / `TextStyle::Heading` 使用箇所（grep 済み・影響範囲確定）

クレート内で `ui.heading()` を呼ぶのは **`style.rs:114`（section_heading）と `style.rs:135`（modal_header）の2箇所のみ**。他に `ui.heading()` / `TextStyle::Heading` の参照なし（app.rs のサイドバー・About は painter 直描画 or `ui.label`、visual の theme_card は `TextStyle::Body`）。
→ `apply_type_ramp` で `TextStyle::Heading.family` を切り替えれば section_heading + modal_header の両方が Semibold 化し、**巻き込みゼロ**。issue の対象とちょうど一致する。

## 既存パターン

- **size ランプの SSOT レバー**: `apply_type_ramp` が `ctx.all_styles_mut` で `TextStyle` を一括書き換えする方式は #398 で確立済み。Semibold も同じレバーに「`Heading.family` の切り替え」を1行足すだけで全見出しに波及する（`section_heading`/`modal_header` を個別に触らない）。
- **フォント候補ループ + graceful fallback**: `configure_fonts` の `jp_font_candidates` は「存在する最初の候補を採用、無ければ警告」パターン。Semibold 登録も同じ構造（候補 → 読めたものだけ登録 → 1つも無ければ family 未登録）で書ける。

## 技術的制約

- **egui のウェイトは別 FontFamily が必須**: `TextStyle` は `FontId{ size, family }`。Semibold は size では表現できず、Semibold フェイスを別 `FontFamily::Name("semibold")` に登録し、`Heading.family` をそこへ向ける。
- **`FontData.index`（ttc フェイス選択）**: context7 で eframe 0.34 API 確認済み。`FontData::from_owned` は `index: 0` で構築されるため、構築後に `data.index = N` で ttc コレクション内のフェイスを選べる。`FontDefinitions.font_data: BTreeMap<String, Arc<FontData>>`、`.families: BTreeMap<FontFamily, Vec<String>>`（Vec はグリフ解決の優先順 fallback リスト）。
- **採用フォント（実地調査済み）**:
  - Latin: `C:\Windows\Fonts\seguisb.ttf`（Segoe UI Semibold・face 0）— 存在確認済み。
  - 日本語: `C:\Windows\Fonts\YuGothB.ttc` — `name` テーブル列挙で **face 0=游ゴシック Bold / face 1=Yu Gothic UI Bold / face 2=Yu Gothic UI Semibold** と判明。**face 2（Yu Gothic UI Semibold）**が真の Semibold。
- **FontTweak**: 日本語 Semibold（Yu Gothic UI Semibold）は既存 `jp_font` と同じ `y_offset_factor: 0.3` で Latin ベースラインに整列。Latin（Segoe UI Semibold）は tweak 不要（既定 Latin と同じ Latin フォントのため）。
- **グリフ網羅（退行防止）**: Semibold ファミリは `[Semibold フェイス…] ++ Proportional の fallback チェーン`で構成し、Semibold フェイスに無いグリフは Regular fallback で必ず描画（欠字ゼロ）。
- **フォント読込は起動時1回**（既存方針）。毎フレーム禁止＝`configure_fonts` 内で完結。
- **ユニットテスト非対象**: snotra-settings は egui UI でモック困難につきユニットテストを書かない方針（`snotra-settings/CLAUDE.md`）。検証は clippy + 視覚スモーク（acceptance は目視）。
- **SPEC.md**: snotra-settings のタイポグラフィは SPEC の IPC/状態遷移スコープ外。#398（size ランプ）も SPEC 非更新。本件も SPEC 更新なし。

## 未解決の疑問

- なし。フォントファイル存在・ttc フェイス index・影響範囲（grep）・egui API（context7）すべて実地確認済み。最終確認は視覚スモークで Semibold 描画と整列を目視する。
