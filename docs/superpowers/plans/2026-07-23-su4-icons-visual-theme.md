# SU4（アイコン + 視覚 pass + §11 テーマ消費）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** egui メインウィンドウの結果行に実アイコンを描き（IPC なし・Rust テクスチャ層）、#632 の legibility/scroll を是正し、色・フォント・font_family を config テーマ値から描く。

**Architecture:** 既存 `IconCache`/`icons.bin`（PNG 永続層）は不変のまま、`SearchWindowView` に egui テクスチャ層を新設する。settled/trailing で worker スレッドがアイコンを抽出 → `ColorImage` を channel 送信し、`update()` で `load_texture` する（folder 展開の per-nav thread + channel + `request_repaint` 構造を踏襲）。テーマ色/サイズ/font_family は view 側で egui visuals + fontdb 解決により honor し、`snotra-egui-runtime`（SU1）は触らない。

**Tech Stack:** Rust / egui 0.35 / softbuffer / Tauri v2 / fontdb 0.23 / png 0.18 / Win32（`SHGetFileInfoW`）

## Global Constraints

- **SU1 runtime（`snotra-egui-runtime`）は触らない** — テーマ背景は view の egui visuals + 窓生成 `.background_color` で honor する。`renderer.rs` の `CLEAR_COLOR` は過渡専用ゆえ不変。
- **既存 `IconCache`/`icons.bin` と WebView2 経路の `get_icons_batch` は不変** — egui 用は別 helper を足す（zero-copy コマンドを destabilize しない・「既存コマンドの一斉移行はしない」規約）。
- **worker は folder の per-nav `std::thread::spawn` パターンに限定** — supersede/single-flight は復活させない。アイコンの staleness は path キー付けで構造的に無害（folder 式 token を載せない）。
- **#579 単一フォント不変条件は 2 枝へ進化** — font_family 解決時は user primary + jp_font fallback、解決失敗時のみ jp_font 単一。テストは「どの命題を証明していたか」を追跡して書き換える。
- **font_family 任意フォントのベースライン残余は受容** — SPEC §11 に parity-gap として明記。既定 Segoe UI は Probe 2 で clean 実測。
- **Windows / release は `panic="abort"`** — バッファ境界・font parse は事前検証（icon PNG は自前エンコードゆえ RGBA8 固定）。
- **API はバージョン依存**: egui 0.35（`Color32::from_hex -> Result` / `FontData.index: u32` / `ColorImage::new([w,h], Vec<Color32>)`）・fontdb 0.23（`Query`/`with_face_data`）を各タスクのビルドで一次確認する。
- **コミットはブランチ `feat/532-su4-icons-visual`**（main 直コミット禁止）。
- **検証**: `*.rs` 編集で PostToolUse hook が clippy + crate test を自動実行（沈黙 = 合格）。視覚欠陥（truncation・fallback emoji・font drift）は PR 前に実機視覚スモークで確認（自動テスト外）。

---

## File Structure

- `src-tauri/Cargo.toml` — `fontdb = "0.23"` 追加（Modify）
- `src-tauri/src/egui_shell/view.rs` — font 登録の honor 分岐・テーマ visuals 適用・`draw_result_row` の色/サイズ/truncate/scroll/icon slot・worker spawn・texture drain（Modify）
- `src-tauri/src/egui_shell/icon_textures.rs` — **新設**: egui テクスチャ層の純粋核（`IconMsg`・`png_to_color_image`・`retain_visible`・`needs_extraction`）+ ユニットテスト（Create）
- `src-tauri/src/egui_shell/mod.rs` — 窓生成 `.background_color` を config から / `icon_textures` モジュール宣言（Modify）
- `src-tauri/src/commands/icon.rs` — `load_icon_pngs`（worker 用・owned PNG 返し）+ `ensure_icon_cache_loaded_if_enabled` を `pub(crate)` 化（Modify）
- `src-tauri/CLAUDE.md` — モジュール索引に `icon_textures.rs` を追加（Modify）
- `snotra-egui-mvp/CLAUDE.md` + `SPEC.md` — #579 不変条件の進化 / §11 parity-gap 注記（Modify）

---

## Task 1: font_family honor（fontdb 解決 + user primary/jp fallback + #579 進化）

**Files:**
- Modify: `src-tauri/Cargo.toml`（`[dependencies]` に `fontdb = "0.23"`）
- Modify: `src-tauri/src/egui_shell/view.rs:25-68`（`japanese_font_definitions` / `configure_japanese_font` 周辺 + テスト）
- Modify: `SPEC.md`（§11 に parity-gap 注記）
- Modify: `snotra-egui-mvp/CLAUDE.md`（jp_font 先頭不変条件の進化を反映）

**Interfaces:**
- Produces:
  - `fn resolve_font_family(name: &str) -> Option<(Vec<u8>, u32)>` — fontdb でファミリ名→(バイト列, face index)。見つからなければ None。
  - `fn font_definitions(jp_bytes: &'static [u8], user: Option<(Vec<u8>, u32)>) -> egui::FontDefinitions` — user Some なら user primary + jp_font fallback、None なら jp_font 単一。
  - `configure_japanese_font` は内部で `resolve_font_family(config.visual.font_family)` を呼び上記へ渡す（config 読みは `app_handle` 経由・setup では未取得なら既定 "Segoe UI"）。

- [ ] **Step 1: fontdb を依存に追加**

`src-tauri/Cargo.toml` の `[dependencies]` に追記（`png = "0.18"` の近く）:

```toml
fontdb = "0.23"
```

Run: `cargo fetch -p snotra 2>&1 | tail -3`
Expected: fontdb 0.23.x が解決される（エラーなし）

- [ ] **Step 2: 失敗するテストを書く（font_definitions の 2 枝）**

`view.rs` の `#[cfg(test)] mod tests` の既存テスト `jp_font_is_registered_at_index_zero_for_both_families` を、進化後の不変条件を証明する形へ**書き換える**（元テストの命題「jp_font 単一・index 0」は「解決失敗時（user=None）」の枝で保存し、「解決成功時（user=Some）」の枝を新たに固定する）:

```rust
#[test]
fn font_definitions_fallback_is_jp_single_stack() {
    // user=None（font_family 解決失敗）: jp_font 単一・両ファミリ index 0（#579 の元不変条件）。
    let dummy: &'static [u8] = &[0u8; 4];
    let fonts = font_definitions(dummy, None);
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let list = fonts.families.get(&family).expect("family present");
        assert_eq!(list.first().map(String::as_str), Some("jp_font"),
            "解決失敗時は jp_font 単一・先頭（#579 再発防止）");
    }
}

#[test]
fn font_definitions_honor_puts_user_first_jp_fallback() {
    // user=Some（honor）: user_font 先頭・jp_font は fallback（index 1）＝WebView2 CSS スタック parity。
    let dummy: &'static [u8] = &[0u8; 4];
    let user = vec![0u8; 4];
    let fonts = font_definitions(dummy, Some((user, 0)));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let list = fonts.families.get(&family).expect("family present");
        assert_eq!(list.first().map(String::as_str), Some("user_font"),
            "honor 時は user_font 先頭（font_family 優先）");
        assert_eq!(list.get(1).map(String::as_str), Some("jp_font"),
            "honor 時も jp_font は fallback として残す（CJK 被覆）");
    }
}
```

- [ ] **Step 3: テストが落ちるのを確認**

Run: `cargo test -p snotra font_definitions 2>&1 | tail -20`
Expected: FAIL（`font_definitions` 未定義 + 旧 `japanese_font_definitions` 名の不一致でコンパイルエラー）

- [ ] **Step 4: font_definitions を実装**

`view.rs` の既存 `japanese_font_definitions` を `font_definitions` へ改名し、user 枝を足す。既存の `configure_japanese_font` からの呼び出しも Step 5 で更新する:

```rust
fn font_definitions(
    jp_bytes: &'static [u8],
    user: Option<(Vec<u8>, u32)>,
) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let mut jp = egui::FontData::from_static(jp_bytes);
    jp.tweak = egui::FontTweak {
        scale: 1.0,
        y_offset_factor: 0.3,
        y_offset: 0.0,
        ..Default::default()
    };
    fonts.font_data.insert("jp_font".to_owned(), jp.into());
    match user {
        Some((bytes, face_index)) => {
            let mut uf = egui::FontData::from_owned(bytes);
            uf.index = face_index; // TTC face 指定（settings font.rs:138 と同型）
            fonts.font_data.insert("user_font".to_owned(), uf.into());
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                // user_font 先頭（font_family 優先）+ jp_font fallback（CJK 被覆）= CSS スタック parity。
                let list = fonts.families.entry(family).or_default();
                list.insert(0, "jp_font".to_owned());
                list.insert(0, "user_font".to_owned());
            }
        }
        None => {
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                // 解決失敗時は jp_font 単一・先頭（#579: push=末尾だとベースラインずれ再発）。
                fonts.families.entry(family).or_default().insert(0, "jp_font".to_owned());
            }
        }
    }
    fonts
}
```

- [ ] **Step 5: resolve_font_family と configure_japanese_font の結線**

`view.rs` に fontdb 解決を追加し、`configure_japanese_font` が config の font_family を honor するよう更新する。config は `configure_japanese_font` が `&egui::Context` しか持たないため、呼び出し側（`setup`）から font_family 文字列を渡す形へ広げる:

```rust
/// config font_family をシステムから解決して (バイト列, face index) を返す。
/// 見つからなければ None（呼び出し側が jp_font 単一へフォールバック）。Database は
/// 解決後に drop（非常駐・列挙コストはフォント設定時の一度きり）。
fn resolve_font_family(name: &str) -> Option<(Vec<u8>, u32)> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let query = fontdb::Query {
        families: &[fontdb::Family::Name(name)],
        weight: fontdb::Weight::NORMAL,
        stretch: fontdb::Stretch::Normal,
        style: fontdb::Style::Normal,
    };
    let id = db.query(&query)?;
    db.with_face_data(id, |data, face_index| (data.to_vec(), face_index))
}

fn configure_japanese_font(context: &egui::Context, font_family: &str) {
    let candidates = [
        "C:/Windows/Fonts/YuGothM.ttc",
        "C:/Windows/Fonts/yugothic.ttf",
        "C:/Windows/Fonts/msgothic.ttc",
        "C:/Windows/Fonts/meiryo.ttc",
    ];
    if JP_FONT_BYTES.get().is_none() {
        for path in candidates {
            if let Ok(bytes) = std::fs::read(path) {
                let _ = JP_FONT_BYTES.set(bytes.into_boxed_slice());
                break;
            }
        }
    }
    if let Some(bytes) = JP_FONT_BYTES.get() {
        let static_bytes: &'static [u8] =
            unsafe { std::mem::transmute::<&[u8], &'static [u8]>(bytes) };
        let user = resolve_font_family(font_family);
        context.set_fonts(font_definitions(static_bytes, user));
    }
}
```

`impl EguiView for SearchWindowView` の `setup` を、config から font_family を読んで渡すよう更新する（config 読みは `AppState` 経由・既定 "Segoe UI"）:

```rust
fn setup(&mut self, context: &egui::Context) {
    let font_family = self
        .app_handle
        .try_state::<crate::AppState>()
        .map(|s| s.engine.lock().unwrap().config().visual.font_family.clone())
        .unwrap_or_else(|| "Segoe UI".to_string());
    configure_japanese_font(context, &font_family);
}
```

- [ ] **Step 6: テストが通るのを確認**

Run: `cargo test -p snotra font_definitions 2>&1 | tail -20`
Expected: PASS（2 テスト）

- [ ] **Step 7: SPEC §11 と snotra-egui-mvp/CLAUDE.md を同期**

`SPEC.md` の §11「ビジュアル」末尾に parity-gap 注記を足す:

```markdown
- egui 経路（softbuffer）の font_family は fontdb 解決で「ユーザーフォント優先 + Yu Gothic フォールバック」（WebView2 CSS スタック parity）。既定 Segoe UI は混在行のベースライン整列を実測確認済み。ただし egui はフォント単位の粗い縦位置補正しか持たないため、非 MS フォント選択時は混在行でベースラインがずれうる（視覚スモークでのみ顕在化する受容残余・#532 SU4）
```

`snotra-egui-mvp/CLAUDE.md` の「jp_font を先頭に単一化」不変条件の記述に、「font_family 解決時は user primary + jp_font fallback へ進化（#532 SU4・解決失敗時のみ単一スタック）」を追記する（grep で該当箇所を特定・見出し名で辿る）。

- [ ] **Step 8: コミット**

```bash
git add src-tauri/Cargo.toml src-tauri/src/egui_shell/view.rs SPEC.md snotra-egui-mvp/CLAUDE.md
git commit -F <tmpfile>
```

コミットメッセージ（tmpfile 経由・HEREDOC 不使用）:
```
feat(egui): font_family honor（fontdb 解決・user primary + jp fallback）（#532 SU4）

#579 単一フォント不変条件を 2 枝へ進化: 解決時 user 先頭 + jp_font fallback
（CSS スタック parity）、解決失敗時のみ jp_font 単一。任意フォント残余は SPEC §11 注記。

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
```

---

## Task 2: §11 テーマ色・font_size・窓背景を config から

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`（`draw_result_row` の色/サイズ・update の visuals 適用・`RowTheme` 導入）
- Modify: `src-tauri/src/egui_shell/mod.rs:55`（窓生成 `.background_color` を config から）

**Interfaces:**
- Produces:
  - `struct RowTheme { name_color: egui::Color32, path_color: egui::Color32, selection: egui::Color32, name_size: f32, path_size: f32 }`
  - `fn hex_color(s: &str, fallback: egui::Color32) -> egui::Color32` — hex 文字列→Color32（失敗時 fallback）。
  - `fn row_theme(&self) -> RowTheme` — 実行中 config から `RowTheme` を組む（`text_color`/`hint_text_color`/`selected_row_color`/`font_size`）。
- Consumes（Task 1）: なし（独立）。

- [ ] **Step 1: hex_color の失敗するテストを書く**

`view.rs` の `#[cfg(test)] mod tests` に追加:

```rust
#[test]
fn hex_color_parses_and_falls_back() {
    use super::hex_color;
    assert_eq!(hex_color("#E0E0E0", egui::Color32::BLACK),
        egui::Color32::from_rgb(0xE0, 0xE0, 0xE0));
    // 不正文字列は fallback（release panic=abort ゆえ unwrap しない）。
    assert_eq!(hex_color("not-a-color", egui::Color32::RED), egui::Color32::RED);
}
```

- [ ] **Step 2: テストが落ちるのを確認**

Run: `cargo test -p snotra hex_color 2>&1 | tail -15`
Expected: FAIL（`hex_color` 未定義）

- [ ] **Step 3: hex_color と RowTheme を実装**

`view.rs`（`draw_result_row` の近く）に追加:

```rust
/// `#RRGGBB` 文字列を Color32 へ。失敗時は fallback（release は panic=abort ゆえ unwrap しない）。
fn hex_color(s: &str, fallback: egui::Color32) -> egui::Color32 {
    egui::Color32::from_hex(s).unwrap_or(fallback)
}

/// 1 結果行の描画テーマ（config テーマ値から都度導出・#576 と同設計でキャッシュしない）。
struct RowTheme {
    name_color: egui::Color32,
    path_color: egui::Color32,
    selection: egui::Color32,
    name_size: f32,
    path_size: f32,
}
```

`impl SearchWindowView` に `row_theme` を追加:

```rust
fn row_theme(&self) -> RowTheme {
    let (text, hint, sel, size) = self
        .app_handle
        .try_state::<crate::AppState>()
        .map(|s| {
            let engine = s.engine.lock().unwrap();
            let v = &engine.config().visual;
            (v.text_color.clone(), v.hint_text_color.clone(),
             v.selected_row_color.clone(), v.font_size)
        })
        .unwrap_or_else(|| ("#E0E0E0".into(), "#808080".into(), "#333333".into(), 15));
    RowTheme {
        name_color: hex_color(&text, egui::Color32::from_rgb(0xE0, 0xE0, 0xE0)),
        path_color: hex_color(&hint, egui::Color32::from_rgb(0x80, 0x80, 0x80)),
        selection: hex_color(&sel, egui::Color32::from_rgb(0x33, 0x33, 0x33)),
        name_size: size as f32,
        path_size: (size as f32 * 0.78).max(9.0), // WebView2 の name>path 比を踏襲
    }
}
```

- [ ] **Step 4: テストが通るのを確認**

Run: `cargo test -p snotra hex_color 2>&1 | tail -15`
Expected: PASS

- [ ] **Step 5: draw_result_row の色/サイズを RowTheme から描く**

`draw_result_row` のシグネチャに `theme: &RowTheme` を足し、ハードコード色/14/11 を置換する（アイコン slot・truncate は Task 3/5 で足すので本タスクでは現行の `painter.text` 配置のまま色/サイズだけ差し替える）:

```rust
fn draw_result_row(
    ui: &mut egui::Ui,
    result: &SearchResult,
    selected: bool,
    theme: &RowTheme,
) -> bool {
    let row_h = 30.0;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), row_h),
        egui::Sense::click(),
    );
    if selected {
        ui.painter().rect_filled(rect, 4.0, theme.selection);
        response.scroll_to_me(Some(egui::Align::Center));
    }
    let text_x = rect.left() + 28.0;
    let painter = ui.painter();
    painter.text(
        egui::pos2(text_x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        &result.name,
        egui::FontId::proportional(theme.name_size),
        theme.name_color,
    );
    painter.text(
        egui::pos2(rect.right() - 8.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        &result.path,
        egui::FontId::proportional(theme.path_size),
        theme.path_color,
    );
    response.clicked()
}
```

`update()` の描画ループ呼び出しを更新（ループ前に `let theme = self.row_theme();`）:

```rust
let theme = self.row_theme();
egui::ScrollArea::vertical().show(ui, |ui| {
    for (i, result) in results.iter().enumerate() {
        if Self::draw_result_row(ui, result, i == selected, &theme) {
            clicked = Some(i);
        }
    }
});
```

- [ ] **Step 6: update() でパネル背景・入力欄背景・selection を config visuals から設定**

`update()` の冒頭（reset 処理の後・描画の前）に visuals 適用を足す:

```rust
// §11: パネル/入力欄/選択色を config テーマから（ハードコード撤廃・runtime CLEAR_COLOR は不変）。
if let Some(s) = self.app_handle.try_state::<crate::AppState>() {
    let (bg, input_bg, sel) = {
        let engine = s.engine.lock().unwrap();
        let v = &engine.config().visual;
        (v.background_color.clone(), v.input_background_color.clone(), v.selected_row_color.clone())
    };
    let mut visuals = ui.ctx().style().visuals.clone();
    visuals.panel_fill = hex_color(&bg, egui::Color32::from_rgb(0x28, 0x28, 0x28));
    visuals.window_fill = visuals.panel_fill;
    visuals.extreme_bg_color = hex_color(&input_bg, egui::Color32::from_rgb(0x38, 0x38, 0x38)); // TextEdit 背景
    visuals.selection.bg_fill = hex_color(&sel, egui::Color32::from_rgb(0x33, 0x33, 0x33));
    ui.ctx().set_visuals(visuals);
}
```

- [ ] **Step 7: 窓生成の背景を config から（mod.rs:55）**

`egui_shell/mod.rs` の窓生成 `.background_color(tauri::window::Color(0x28, 0x28, 0x28, 0xff))` を config `background_color` から構築する。該当箇所を grep（`background_color(tauri::window::Color`）で特定し、生成関数が config を読める経路（`main.rs:580` が既に `config.visual.background_color` を読む前例）に合わせて hex→RGB 変換した `Color` を渡す:

```rust
// #RRGGBB → tauri::window::Color（過渡/リサイズ下地・SU2 のハードコード 0x282828 を config へ）。
let (r, g, b) = parse_hex_rgb(&config.visual.background_color).unwrap_or((0x28, 0x28, 0x28));
// ... .background_color(tauri::window::Color(r, g, b, 0xff))
```

`parse_hex_rgb(&str) -> Option<(u8,u8,u8)>` を `mod.rs` に小さく実装（`#` を除いて 6 桁 hex を 2 桁ずつ）。config を読む経路が生成関数に無ければ、生成の呼び出し元（setup フェーズ）から `&Config` または `background_color: String` を渡す形へ広げる。

- [ ] **Step 8: clippy/test 沈黙を確認しコミット**

Run: `cargo clippy -p snotra --all-targets 2>&1 | tail -5`
Expected: 警告/エラーなし

```bash
git add src-tauri/src/egui_shell/view.rs src-tauri/src/egui_shell/mod.rs
git commit -F <tmpfile>
```
メッセージ: `feat(egui): §11 テーマ色/font_size/窓背景を config から（#532 SU4）`

---

## Task 3: #632 legibility（name/path truncate）+ scroll gate

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`（`draw_result_row` の truncate・`last_scrolled_selected` フィールド + scroll gate）

**Interfaces:**
- Consumes（Task 2）: `RowTheme`、`draw_result_row(ui, result, selected, theme)`。
- Produces: `draw_result_row` に `scroll: bool` 引数（選択かつ scroll 要求時のみ `scroll_to_me`）。`SearchWindowView.last_scrolled_selected: Option<usize>`。

- [ ] **Step 1: last_scrolled_selected フィールドを足す**

`SearchWindowView` 構造体に追加し、`new()` で `None` 初期化:

```rust
    /// 直近に scroll_to_me した選択 index。選択変化時のみ scroll するための gate（#632）。
    last_scrolled_selected: Option<usize>,
```

`new()` の末尾フィールド群に `last_scrolled_selected: None,` を追加。

- [ ] **Step 2: draw_result_row を truncate + scroll 引数へ改修**

name を「アイコン slot 後〜行右の一定割合」に、path を残り幅に中間省略で置く。egui の galley 幅計測で name 実幅を測り、path の描画開始 x を name 右端 + gap 以降に固定して重なりを防ぐ。scroll は引数で制御:

```rust
fn draw_result_row(
    ui: &mut egui::Ui,
    result: &SearchResult,
    selected: bool,
    scroll: bool,
    theme: &RowTheme,
) -> bool {
    let row_h = 30.0;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), row_h),
        egui::Sense::click(),
    );
    if selected {
        ui.painter().rect_filled(rect, 4.0, theme.selection);
        if scroll {
            response.scroll_to_me(Some(egui::Align::Center)); // 選択変化時のみ（#632）
        }
    }
    let text_x = rect.left() + 28.0;
    let right = rect.right() - 8.0;
    let cy = rect.center().y;
    // name galley を作り、実幅から path 開始 x を決める（重なり回避）。name が幅の 60% を超えたら
    // name 側を省略幅にクリップ（egui の LayoutJob で max_width 指定）。
    let name_max = (right - text_x) * 0.6;
    let name_galley = ui.painter().layout(
        result.name.clone(),
        egui::FontId::proportional(theme.name_size),
        theme.name_color,
        name_max, // wrap width＝この幅で折り返し／省略の目安
    );
    ui.painter().galley(egui::pos2(text_x, cy - name_galley.size().y / 2.0), name_galley.clone(), theme.name_color);
    let path_x = text_x + name_galley.size().x.min(name_max) + 12.0;
    // path は右寄せ・path_x 以降に収まる幅で中間省略。egui galley は末尾省略のため、
    // 中間省略は truncate_middle（純関数）で文字列側を縮めてから描く。
    let path_avail = (right - path_x).max(0.0);
    let path_str = truncate_middle(&result.path, path_avail, theme.path_size);
    ui.painter().text(
        egui::pos2(right, cy),
        egui::Align2::RIGHT_CENTER,
        &path_str,
        egui::FontId::proportional(theme.path_size),
        theme.path_color,
    );
    response.clicked()
}
```

`truncate_middle` は文字列を利用可能幅に収まるよう中間 `…` 省略する純関数（幅は概算＝`font_size * 0.5 * chars` で見積り、正確なピクセルは galley でも可だが KISS で概算＋egui の末尾クリップに委ねる）:

```rust
/// path を avail_px におよそ収める中間省略（`C:\a\...\app.exe`）。概算幅（1 文字 ≈ size*0.55px）。
fn truncate_middle(s: &str, avail_px: f32, size: f32) -> String {
    let per = (size * 0.55).max(1.0);
    let max_chars = (avail_px / per).floor() as usize;
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars || max_chars < 4 {
        return s.to_string();
    }
    let keep = max_chars - 1; // '…' の分
    let head = keep / 2;
    let tail = keep - head;
    let mut out: String = chars[..head].iter().collect();
    out.push('…');
    out.extend(&chars[chars.len() - tail..]);
    out
}
```

- [ ] **Step 3: truncate_middle の失敗するテストを書く**

```rust
#[test]
fn truncate_middle_shortens_long_path() {
    use super::truncate_middle;
    let long = r"C:\Users\Eoh\AppData\Local\Programs\app\bin\tool.exe";
    let out = truncate_middle(long, 100.0, 11.0);
    assert!(out.chars().count() < long.chars().count(), "省略される");
    assert!(out.contains('…'), "中間省略記号を含む");
    // 短い文字列・極小幅は原文（max_chars<4 ガード）。
    assert_eq!(truncate_middle("a.exe", 1.0, 11.0), "a.exe");
    assert_eq!(truncate_middle("short", 1000.0, 11.0), "short");
}
```

- [ ] **Step 4: テストが落ちる → 実装済みなら通ることを確認**

Run: `cargo test -p snotra truncate_middle 2>&1 | tail -15`
Expected: PASS（Step 2 で実装済み。落ちる場合は truncate_middle の定義漏れを直す）

- [ ] **Step 5: update() で scroll gate を適用**

描画ループを、選択変化時のみ scroll するよう更新する:

```rust
let theme = self.row_theme();
let selected = self.state.selected();
let do_scroll = self.last_scrolled_selected != Some(selected);
egui::ScrollArea::vertical().show(ui, |ui| {
    for (i, result) in results.iter().enumerate() {
        let sel = i == selected;
        if Self::draw_result_row(ui, result, sel, sel && do_scroll, &theme) {
            clicked = Some(i);
        }
    }
});
if do_scroll {
    self.last_scrolled_selected = Some(selected);
}
```

結果リセット経路（`reset_pending` 消費・`clear_search`）で `self.last_scrolled_selected = None;` を足し、再表示後に確実に一度 scroll し直す。

- [ ] **Step 6: clippy/test 沈黙を確認しコミット**

Run: `cargo test -p snotra truncate_middle 2>&1 | tail -5`
Expected: PASS

```bash
git add src-tauri/src/egui_shell/view.rs
git commit -F <tmpfile>
```
メッセージ: `fix(egui): 結果行 legibility（name/path 重なり）+ scroll 追従 gate（#632・#532 SU4）`

---

## Task 4: icon テクスチャ層の純粋核（`icon_textures.rs` 新設）

**Files:**
- Create: `src-tauri/src/egui_shell/icon_textures.rs`
- Modify: `src-tauri/src/egui_shell/mod.rs`（`mod icon_textures;` 宣言 + 必要な re-export）
- Modify: `src-tauri/CLAUDE.md`（モジュール索引に追加）

**Interfaces:**
- Produces:
  - `enum IconMsg { Loaded(String, egui::ColorImage), Missing(String) }`
  - `fn png_to_color_image(png: &[u8]) -> Option<egui::ColorImage>` — 自前エンコードの RGBA8 PNG を ColorImage へ。
  - `fn needs_extraction(path: &str, have: &HashMap<String, egui::TextureHandle>, missing: &HashSet<String>) -> bool` — 未取得かつ未 missing なら true。
  - `fn retain_visible(textures: &mut HashMap<String, egui::TextureHandle>, visible: &HashSet<String>)` — 可視集合外の handle を drop。

- [ ] **Step 1: 純粋核の失敗するテストを書く**

`src-tauri/src/egui_shell/icon_textures.rs` を作成し、テストから書く:

```rust
//! egui メインウィンドウのアイコン・テクスチャ層（#532 SU4）。IconCache（PNG 永続層）とは別に、
//! path→TextureHandle をセッション内で保持する。純粋核（PNG→ColorImage decode・可視集合 retain・
//! 抽出要否述語）をここに置き、worker spawn / load_texture の driver は view.rs が持つ。

use std::collections::{HashMap, HashSet};

/// worker → driver のメッセージ。token は載せない——アイコンの staleness は path キー付けで
/// 構造的に無害（遅延到着 texture は現行行の path でしか引かれない・SU4 決定 2）。
pub(crate) enum IconMsg {
    Loaded(String, egui::ColorImage),
    Missing(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_to_color_image_roundtrips_rgba8() {
        // 2x2 RGBA8 PNG を png クレートで作り、decode して画素が一致することを確認。
        let mut png_buf = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png_buf, 2, 2);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().unwrap();
            // R,G,B,A の 4 画素（straight alpha）
            w.write_image_data(&[
                255, 0, 0, 255,  0, 255, 0, 255,
                0, 0, 255, 255,  255, 255, 255, 128,
            ]).unwrap();
        }
        let img = super::png_to_color_image(&png_buf).expect("decode");
        assert_eq!(img.size, [2, 2]);
        assert_eq!(img.pixels[0], egui::Color32::from_rgba_unmultiplied(255, 0, 0, 255));
        assert_eq!(img.pixels[3], egui::Color32::from_rgba_unmultiplied(255, 255, 255, 128));
    }

    #[test]
    fn needs_extraction_skips_present_and_missing() {
        let mut have: HashMap<String, egui::TextureHandle> = HashMap::new();
        let mut missing: HashSet<String> = HashSet::new();
        missing.insert("m.exe".into());
        // present を模すには TextureHandle が要るが生成に ctx が要るため、needs_extraction は
        // have.contains_key / missing.contains のみで判定する純関数ゆえ空 have + missing で検証。
        assert!(super::needs_extraction("new.exe", &have, &missing), "未知は要抽出");
        assert!(!super::needs_extraction("m.exe", &have, &missing), "missing は再抽出しない");
        let _ = &mut have; // 使用マーク
    }

    #[test]
    fn retain_visible_drops_out_of_set() {
        // retain_visible は HashMap のキー集合演算のみ（handle 生成不要な keys-only テストは
        // 別途 view の実機で担保）。ここでは空集合 retain がクラッシュしないことだけ確認。
        let mut textures: HashMap<String, egui::TextureHandle> = HashMap::new();
        let visible: HashSet<String> = HashSet::new();
        super::retain_visible(&mut textures, &visible);
        assert!(textures.is_empty());
    }
}
```

- [ ] **Step 2: モジュール宣言を追加してテストが落ちるのを確認**

`src-tauri/src/egui_shell/mod.rs` に `mod icon_textures;`（+ 必要なら `pub(crate) use icon_textures::IconMsg;`）を足す。

Run: `cargo test -p snotra icon_textures 2>&1 | tail -20`
Expected: FAIL（`png_to_color_image` / `needs_extraction` / `retain_visible` 未定義）

- [ ] **Step 3: 純粋核を実装**

`icon_textures.rs` に実装を足す:

```rust
/// 自前エンコードの RGBA8 PNG（icon.rs bgra_to_png）を ColorImage へ decode する。
/// 想定外の色種別/深度は None（自前エンコードは常に RGBA8）。
pub(crate) fn png_to_color_image(png: &[u8]) -> Option<egui::ColorImage> {
    let decoder = png::Decoder::new(png);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let (w, h) = (info.width as usize, info.height as usize);
    let rgba = &buf[..info.buffer_size()];
    let pixels: Vec<egui::Color32> = rgba
        .chunks_exact(4)
        .map(|c| egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]))
        .collect();
    if pixels.len() != w * h {
        return None;
    }
    Some(egui::ColorImage::new([w, h], pixels))
}

/// path が未取得かつ未 missing なら true（抽出 worker に積むべきか）。
pub(crate) fn needs_extraction(
    path: &str,
    have: &HashMap<String, egui::TextureHandle>,
    missing: &HashSet<String>,
) -> bool {
    !have.contains_key(path) && !missing.contains(path)
}

/// 可視集合に無い path の TextureHandle を drop（メモリを可視集合に頭打ち・SU4 決定 A メモリ境界）。
pub(crate) fn retain_visible(
    textures: &mut HashMap<String, egui::TextureHandle>,
    visible: &HashSet<String>,
) {
    textures.retain(|k, _| visible.contains(k));
}
```

- [ ] **Step 4: テストが通るのを確認**

Run: `cargo test -p snotra icon_textures 2>&1 | tail -20`
Expected: PASS（3 テスト）

- [ ] **Step 5: モジュール索引を更新**

`src-tauri/CLAUDE.md` の `egui_shell/` 節に `icon_textures.rs`（アイコン・テクスチャ層の純粋核）をファイル一覧へ足す。

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/egui_shell/icon_textures.rs src-tauri/src/egui_shell/mod.rs src-tauri/CLAUDE.md
git commit -F <tmpfile>
```
メッセージ: `feat(egui): icon テクスチャ層の純粋核（png decode・retain・needs_extraction）（#532 SU4）`

---

## Task 5: icon worker + update() 適用 + draw_result_row アイコン slot + メモリ境界

**Files:**
- Modify: `src-tauri/src/commands/icon.rs`（`ensure_icon_cache_loaded_if_enabled` を `pub(crate)` 化 + `load_icon_pngs` 追加）
- Modify: `src-tauri/src/egui_shell/view.rs`（channel/map フィールド・worker spawn・drain 適用・draw_result_row の icon 描画・clear/retain）

**Interfaces:**
- Consumes（Task 4）: `IconMsg`・`png_to_color_image`・`needs_extraction`・`retain_visible`。
- Consumes（Task 3）: `draw_result_row(ui, result, selected, scroll, theme)`。
- Produces:
  - `pub(crate) fn load_icon_pngs(state, icons, paths: Vec<String>) -> Vec<(String, Option<Vec<u8>>)>`（commands/icon.rs）。
  - `SearchWindowView` に `icon_textures: HashMap<String, egui::TextureHandle>`・`icon_missing: HashSet<String>`・`icon_tx/icon_rx`。
  - `draw_result_row` に `icon: Option<&egui::TextureHandle>` 引数。

- [ ] **Step 1: load_icon_pngs を追加（commands/icon.rs）**

`ensure_icon_cache_loaded_if_enabled` を `pub(crate)` にし、worker 用の owned-PNG 版を足す（`get_icons_batch` の 3 段ロック規律を共有・zero-copy コマンドは不変のまま）:

```rust
/// egui worker 用: paths のアイコン PNG を（キャッシュ get-or-extract-insert して）owned で返す。
/// get_icons_batch と同じ ensure-loaded + 3 段ロック規律。show_icons=false 時は全 None。
pub(crate) fn load_icon_pngs(
    state: &State<AppState>,
    icons: &State<IconCacheState>,
    paths: Vec<String>,
) -> Vec<(String, Option<Vec<u8>>)> {
    ensure_icon_cache_loaded_if_enabled(state, icons);
    // Step 1: miss 収集（1 ロック）
    let mut misses: Vec<String> = Vec::new();
    {
        let cache = icons.lock().unwrap();
        match cache.as_ref() {
            None => return paths.into_iter().map(|p| (p, None)).collect(),
            Some(c) => {
                for p in &paths {
                    if c.get(p).is_none() {
                        misses.push(p.clone());
                    }
                }
            }
        }
    }
    // Step 2: ロック外抽出（rayon・get_icons_batch と同型）
    let extracted: Vec<(String, Vec<u8>)> = misses
        .into_par_iter()
        .filter_map(|p| extract_png(&p).map(|png| (p, png)))
        .collect();
    // Step 3: 挿入して owned で返す（clone・16x16 PNG ≤8 件ゆえ許容）
    let mut cache = icons.lock().unwrap();
    if let Some(c) = cache.as_mut() {
        for (p, png) in extracted {
            c.insert(p, png);
        }
        paths.into_iter().map(|p| { let png = c.get(&p).map(|s| s.to_vec()); (p, png) }).collect()
    } else {
        paths.into_iter().map(|p| (p, None)).collect()
    }
}
```

- [ ] **Step 2: view のフィールドと channel を足す**

`SearchWindowView` に追加し、`new()` で初期化:

```rust
    icon_textures: std::collections::HashMap<String, egui::TextureHandle>,
    icon_missing: std::collections::HashSet<String>,
    icon_tx: Sender<crate::egui_shell::IconMsg>,
    icon_rx: Receiver<crate::egui_shell::IconMsg>,
```

`new()` で `let (icon_tx, icon_rx) = channel();` を足し、フィールド群へ `icon_textures: HashMap::new(), icon_missing: HashSet::new(), icon_tx, icon_rx,` を加える。

- [ ] **Step 3: worker spawn を実装**

`spawn_folder_load` の近くに `spawn_icon_load` を足す（folder と同じ thread + channel + request_repaint 構造）:

```rust
/// 現結果集合の未取得アイコンを別スレッドで抽出し IconMsg を channel へ送る（SU4）。
/// folder の per-nav thread パターン踏襲。token は載せない（staleness は path キーで無害）。
/// show_icons=false 時は呼ばない（呼び出し側でガード）。
fn spawn_icon_load(&self, paths: Vec<String>, egui_ctx: egui::Context) {
    if paths.is_empty() {
        return;
    }
    let app = self.app_handle.clone();
    let tx = self.icon_tx.clone();
    std::thread::spawn(move || {
        let (Some(state), Some(icons)) = (
            app.try_state::<crate::AppState>(),
            app.try_state::<crate::icon::IconCacheState>(),
        ) else { return };
        let loaded = crate::commands::icon::load_icon_pngs(&state, &icons, paths);
        for (path, png) in loaded {
            let msg = match png.and_then(|b| crate::egui_shell::png_to_color_image(&b)) {
                Some(img) => crate::egui_shell::IconMsg::Loaded(path, img),
                None => crate::egui_shell::IconMsg::Missing(path),
            };
            let _ = tx.send(msg);
        }
        egui_ctx.request_repaint(); // イベント駆動 runtime を起こす（folder と同理由）
    });
}
```

`crate::egui_shell::{IconMsg, png_to_color_image}` を Task 4 の `mod.rs` で `pub(crate) use` 再輸出しておく（Step の前提）。

- [ ] **Step 4: settled で spawn を呼ぶ**

`show_icons` を読むアクセサを足し、結果確定点（trailing poll の `run_search` 後・folder drain 後・changed の leading/instant 後）で「未取得 path を集めて spawn」する共通メソッドを呼ぶ。`update()` の描画直前に一括で行うのが最も単純:

```rust
fn show_icons(&self) -> bool {
    self.app_handle
        .try_state::<crate::AppState>()
        .map(|s| s.engine.lock().unwrap().config().appearance.show_icons)
        .unwrap_or(true)
}

/// 現結果の未取得アイコンを worker に積む（settled 相当・描画前に呼ぶ）。連打中は
/// debounce armed のため呼ばない（呼び出し側で is_armed ガード）。
fn request_icons_for_results(&self, ctx: &egui::Context) {
    if !self.show_icons() {
        return;
    }
    let mut wanted: Vec<String> = Vec::new();
    for r in self.state.results() {
        if !r.is_error
            && crate::egui_shell::needs_extraction(&r.path, &self.icon_textures, &self.icon_missing)
            && !wanted.contains(&r.path)
        {
            wanted.push(r.path.clone());
        }
    }
    self.spawn_icon_load(wanted, ctx.clone());
}
```

`update()` の描画直前に、連打が収まっているときだけ呼ぶ:

```rust
if !self.search_debounce.is_armed() {
    self.request_icons_for_results(&ctx);
}
```

- [ ] **Step 5: update() で IconMsg を drain して load_texture**

`update()` の folder drain の近くに icon drain を足す:

```rust
// アイコン drain（token 無し・path キーで適用）。到着したら load_texture して map へ。
let mut icon_arrived = false;
while let Ok(msg) = self.icon_rx.try_recv() {
    match msg {
        crate::egui_shell::IconMsg::Loaded(path, img) => {
            let handle = ctx.load_texture(&path, img, egui::TextureOptions::LINEAR);
            self.icon_textures.insert(path, handle);
            icon_arrived = true;
        }
        crate::egui_shell::IconMsg::Missing(path) => {
            self.icon_missing.insert(path);
        }
    }
}
if icon_arrived {
    ctx.request_repaint();
}
```

- [ ] **Step 6: draw_result_row にアイコン描画を足す**

`draw_result_row` に `icon: Option<&egui::TextureHandle>` を足し、28px slot に描く（`show_icons=false` の slot 畳みは Task 6）:

```rust
fn draw_result_row(
    ui: &mut egui::Ui,
    result: &SearchResult,
    selected: bool,
    scroll: bool,
    icon: Option<&egui::TextureHandle>,
    theme: &RowTheme,
) -> bool {
    let row_h = 30.0;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), row_h),
        egui::Sense::click(),
    );
    if selected {
        ui.painter().rect_filled(rect, 4.0, theme.selection);
        if scroll {
            response.scroll_to_me(Some(egui::Align::Center));
        }
    }
    // アイコン: 左 28px slot の中央に 16x16 を描く。
    if let Some(tex) = icon {
        let icon_size = 16.0;
        let icon_rect = egui::Rect::from_center_size(
            egui::pos2(rect.left() + 14.0, rect.center().y),
            egui::vec2(icon_size, icon_size),
        );
        ui.painter().image(
            tex.id(),
            icon_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
    let text_x = rect.left() + 28.0;
    // ...（Task 3 の name/path truncate 描画はそのまま）
    response.clicked()
}
```

`update()` の描画ループで icon を引いて渡す:

```rust
for (i, result) in results.iter().enumerate() {
    let sel = i == selected;
    let icon = self.icon_textures.get(&result.path);
    if Self::draw_result_row(ui, result, sel, sel && do_scroll, icon, &theme) {
        clicked = Some(i);
    }
}
```

- [ ] **Step 7: メモリ境界（clear-on-hide + retain-on-change）**

`update()` の `reset_pending` 消費枝に全 clear を足す:

```rust
    self.icon_textures.clear();
    self.icon_missing.clear();
```

描画前（`request_icons_for_results` の前後どちらか）に、現結果 path 集合で retain して可視集合に頭打ち:

```rust
let visible: std::collections::HashSet<String> =
    self.state.results().iter().map(|r| r.path.clone()).collect();
crate::egui_shell::retain_visible(&mut self.icon_textures, &visible);
self.icon_missing.retain(|p| visible.contains(p));
```

- [ ] **Step 8: clippy/test 沈黙を確認しコミット**

Run: `cargo clippy -p snotra --all-targets 2>&1 | tail -5`
Expected: 警告/エラーなし

```bash
git add src-tauri/src/commands/icon.rs src-tauri/src/egui_shell/view.rs
git commit -F <tmpfile>
```
メッセージ: `feat(egui): 実アイコン描画（worker + テクスチャ + メモリ境界）（#532 SU4）`

---

## Task 6: show_icons レイアウト畳み + fallback プレースホルダ

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`（`draw_result_row` の slot 幅を show_icons で切替 + 欠落 fallback）

**Interfaces:**
- Consumes（Task 5）: `draw_result_row(ui, result, selected, scroll, icon, theme)`・`show_icons()`。
- Produces: `draw_result_row` に `show_icons: bool` 引数。fallback 描画（jp_font emoji 被覆に応じて emoji or drawn placeholder）。

- [ ] **Step 1: draw_result_row に show_icons を足しテキスト起点を切替**

```rust
fn draw_result_row(
    ui: &mut egui::Ui,
    result: &SearchResult,
    selected: bool,
    scroll: bool,
    icon: Option<&egui::TextureHandle>,
    show_icons: bool,
    theme: &RowTheme,
) -> bool {
    // ... rect/selection ...
    let slot = if show_icons { 28.0 } else { 8.0 }; // off ならテキスト左端寄せ
    if show_icons {
        match icon {
            Some(tex) => { /* Step 6(Task5) の image 描画 */ }
            None => draw_icon_fallback(ui, rect, result, theme), // Step 2
        }
    }
    let text_x = rect.left() + slot;
    // ... name/path 描画（text_x 起点）...
}
```

`update()` の呼び出しに `self.show_icons()` を渡す（ループ前に `let show_icons = self.show_icons();` で 1 回読む）。worker 側は既に `request_icons_for_results` が `show_icons()` でガード済み（Task 5 Step 4）。

- [ ] **Step 2: fallback を実装（emoji 被覆を実機確認して分岐）**

まず jp_font（Yu Gothic）が 📁📄 を描けるか**実機視覚確認**する（`cargo run` with `SNOTRA_EGUI_MAIN=1`・アイコン欠落する合成パスで確認）。描ければ emoji、描けなければ drawn placeholder（単色角丸矩形）に倒す:

```rust
/// アイコン欠落時の fallback。§3.4 は 📁📄 を規定するが softbuffer + 単一 TTF で
/// 色 emoji が描けない場合は単色プレースホルダに倒す（実機確認で分岐）。
fn draw_icon_fallback(ui: &egui::Ui, rect: egui::Rect, result: &SearchResult, theme: &RowTheme) {
    let center = egui::pos2(rect.left() + 14.0, rect.center().y);
    // 実機確認で emoji が描ける場合はこちら:
    // ui.painter().text(center, egui::Align2::CENTER_CENTER,
    //     if result.is_folder { "📁" } else { "📄" },
    //     egui::FontId::proportional(theme.name_size), theme.path_color);
    // 描けない場合の drawn placeholder（単色角丸・フォルダは少し明るく）:
    let r = egui::Rect::from_center_size(center, egui::vec2(14.0, 14.0));
    let col = if result.is_folder { theme.name_color } else { theme.path_color };
    ui.painter().rect_filled(r, 2.0, col.linear_multiply(0.5));
}
```

（実機確認の結果に応じて emoji 版 / placeholder 版のどちらか一方を残す。両方コメントアウトで残さない。）

- [ ] **Step 3: 実機視覚スモークで確認**

Run: `$env:SNOTRA_EGUI_MAIN=1; cargo run`（PowerShell）
確認項目:
- アイコンが結果行に出る（exe/folder/doc）
- `show_icons=false`（config で切替）でテキストが左端寄せ・slot が畳まれる
- アイコン欠落時に fallback が出る（emoji or placeholder のどちらが正しく描けたか記録）

- [ ] **Step 4: clippy/test 沈黙を確認しコミット**

Run: `cargo clippy -p snotra --all-targets 2>&1 | tail -5`
Expected: 警告/エラーなし

```bash
git add src-tauri/src/egui_shell/view.rs
git commit -F <tmpfile>
```
メッセージ: `feat(egui): show_icons レイアウト畳み + アイコン欠落 fallback（#532 SU4）`

---

## Task 7: 統合視覚スモーク + governance:check + PR 前ゲート

**Files:**
- 変更なし（検証タスク）

- [ ] **Step 1: workspace 全体の clippy/test**

Run: `cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -10`
Expected: エラーなし

Run: `cargo test -p snotra 2>&1 | tail -15`
Expected: 全 PASS（`icon_extract_cost_probe` は `#[ignore]` ゆえ走らない）

- [ ] **Step 2: 統合実機視覚スモーク（flip 基準 2「外観維持」に直結）**

Run: `$env:SNOTRA_EGUI_MAIN=1; cargo run`
確認項目（`ui.md` トリガー: スタイル/レイアウト/テキスト表示の overflow・clipping・font レンダリング）:
- 長い name/path が重ならない（#632 解消）・中間省略が効く
- 選択行の scroll がホイールを上書きしない・index 0 で上部空白が出ない（#632 scroll gate）
- config テーマ色（背景/入力欄/テキスト/選択行/ヒント）が反映される（config.toml を編集して確認）
- **font_family 既定（Segoe UI）で混在行のベースラインがずれない**（Probe 2 の再確認）
- アイコンが出る・`show_icons=false` で畳まれる・欠落 fallback
- `msedgewebview2` 子孫プロセス 0（egui 経路・SU2 の G4 手法）

- [ ] **Step 3: governance:check（新規/削除ファイルを含むため PR 前に必須）**

Run: `npm run governance:check 2>&1 | tail -20`
Expected: PASS（`icon_textures.rs` 追加のモジュール索引・SPEC 参照整合）。失敗時は索引/参照を直す（memory [[pr-governance-check-before-pr]]）。

- [ ] **Step 4: code-review（実装後・PR 前）**

`code-reviewer` サブエージェント（`.claude/agents/`）で 3 フェーズレビュー（実装検証 / 計画判断・SPEC 同期 / パフォーマンス）。指摘を反映。特に確認: worker のスレッド安全（`load_texture` を worker で呼んでいないこと）・#579 テストの 2 枝・メモリ境界（clear/retain）の対称・DRY（`load_icon_pngs` と `get_icons_batch` の規律共有）。

- [ ] **Step 5: push して PR 作成**

```bash
git push -u origin feat/532-su4-icons-visual && gh pr create --title "SU4: アイコン + 視覚 pass + §11 テーマ消費（#532 Phase 2）" --body-file <tmpfile>
```

PR 本文に `closingIssuesReferences` の意図確認（#632 を close・#532 は OPEN 継続）を含める。マージ前に `gh pr view <PR> --json closingIssuesReferences` で意図どおりか確認（ルート CLAUDE.md「Git/GitHub 運用」の手順）。

---

## Self-Review（spec との照合）

**Spec coverage:**
- Part A（アイコン worker/texture/メモリ/show_icons/fallback）→ Task 4/5/6 ✓
- Part B（#632 truncate/scroll）→ Task 3 ✓
- Part C（5 色 + font_size + 窓背景 + font_family）→ Task 2（色/サイズ/背景）+ Task 1（font_family/fontdb）✓
- スキャフォールド始末（icon bench 残置・font spike 撤去）→ 既にコミット済み（`8b921f1`・spike 撤去 + bench 残置）✓
- 決定 2（token 無し・path キー）→ Task 4/5 の IconMsg に token 無し・retain/needs_extraction ✓
- 決定 5（runtime 不変）→ Task 2 は view visuals + mod.rs のみ・runtime 非依存 ✓
- #579 進化 → Task 1 の 2 枝テスト + CLAUDE.md/SPEC 同期 ✓
- 受け入れ条件 1-8 → 各 Task がカバー・Task 7 で統合検証 ✓

**Placeholder scan:** 各 Step に実コード。fallback（Task 6）のみ「実機確認で emoji/placeholder を分岐」を残すが、これは spec が明示した「実装時の視覚確認を要する残余」であり両版の実コードを提示済み（プレースホルダではない）。

**Type consistency:** `draw_result_row` はタスクを追うごとに引数が増える（Task2: +theme / Task3: +scroll / Task5: +icon / Task6: +show_icons）——最終形は `draw_result_row(ui, result, selected, scroll, icon, show_icons, theme)`。各 Task が前 Task の形から差分で拡張する順序で整合。`IconMsg`/`png_to_color_image`/`needs_extraction`/`retain_visible`（Task 4）→ Task 5 が消費、名前一致。`load_icon_pngs`（Task 5）の戻り `Vec<(String, Option<Vec<u8>>)>` を worker が消費、一致。
