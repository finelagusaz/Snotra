# SolidJS 設定 UI → egui (eframe 0.31) 移植: 技術課題調査

## 対応方針サマリー（確定）

| # | 課題 | 方針 | 妥協 | 備考 |
|---|------|------|------|------|
| 1 | モーダルダイアログ | `egui::Modal`（0.29+ 組み込み） | なし | |
| 2 | ファイルピッカー | `rfd` 同期版 + 別スレッド spawn + `Arc<Mutex>` | なし | Phase 2 前に PoC で検証 |
| 3 | カラーピッカー | `color_edit_button_srgba` + `Color32::from_hex` | なし | |
| 4 | フォントプレビュー | フォント名表示のみ、保存時にロード | なし | 現行 SolidJS 版と同等。`list_system_fonts()` 実装済みで追加依存不要 |
| 5 | リスト並べ替え | ↑↓ボタン + ループ後操作適用 | なし | |
| 6 | Opener グループ化 | **snotra-settings 内** (`opener_group.rs`) に配置 | なし | メインアプリは不使用。UI 表示用変換は設定アプリの責務 |
| 7 | rfd ブロッキング | 課題 2 と同一。別スレッド方式で解決 | なし | |

### 次のアクション
- rfd PoC（課題 2/7 の検証）→ Phase 2 実装

## 1. モーダルダイアログ (Index/Opener tabs)

### 結論

egui 0.29+ に組み込みの `egui::Modal` がある。中央配置 + 半透明バックドロップ + 背景 UI への入力ブロックをネイティブに提供するため、SolidJS のモーダルと同等の UX を実現できる。妥協は不要。

### 推奨アプローチ

`egui::Modal` を使う。状態管理は `bool` フラグ（`show_index_modal` 等）を App 構造体に持ち、ボタンクリックで `true` にして `update()` 内で条件描画する。

### コード例

```rust
struct SettingsApp {
    show_index_modal: bool,
    // ...
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if ui.button("インデックス設定…").clicked() {
                self.show_index_modal = true;
            }
        });

        if self.show_index_modal {
            let modal = egui::Modal::new(egui::Id::new("index_modal"))
                .backdrop_color(egui::Color32::from_black_alpha(120));

            let response = modal.show(ctx, |ui| {
                ui.heading("インデックス設定");
                ui.separator();

                // モーダル本体の UI
                ui.label("スキャン対象フォルダ:");
                // ... フォルダリスト等

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        // 保存処理
                        self.show_index_modal = false;
                    }
                    if ui.button("キャンセル").clicked() {
                        self.show_index_modal = false;
                    }
                });
            });

            // バックドロップクリックで閉じたい場合
            if response.should_close() {
                self.show_index_modal = false;
            }
        }
    }
}
```

### 代替案

- `egui-modal` クレート: `Modal::new(ctx, "id")` + `.open()` / `.show()` パターンでヘルパー関数（タイトル/ボディ/ボタン）付き。機能は豊富だが、組み込み `egui::Modal` で十分なら外部依存は不要。

### 参考

- [egui::Modal ドキュメント](https://docs.rs/egui/latest/egui/containers/modal/struct.Modal.html)
- [egui Discussion #1740: How can we do modal window in egui?](https://github.com/emilk/egui/discussions/1740)
- [egui-modal クレート](https://docs.rs/egui-modal)


## 2. フォルダ/ファイルピッカー (rfd crate + eframe)

### 結論

**`rfd::FileDialog::pick_folder()` は eframe のイベントループをブロックする**。Windows では「応答なし」ポップアップが出る場合がある。解決策は2つ: (A) 別スレッドで同期版を呼ぶ、(B) `rfd::AsyncFileDialog` を使う。いずれも実用的に動作するが、(A) の方がシンプル。

### 推奨アプローチ: 別スレッドで同期版を実行

```rust
use std::sync::{Arc, Mutex};
use std::path::PathBuf;

struct SettingsApp {
    picked_folder: Arc<Mutex<Option<PathBuf>>>,
    picker_active: bool,
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // ピッカー実行中はボタンを無効化
            let button = egui::Button::new("フォルダを選択…");
            if ui.add_enabled(!self.picker_active, button).clicked() {
                self.picker_active = true;
                let result = Arc::clone(&self.picked_folder);
                let repaint_ctx = ctx.clone();
                std::thread::spawn(move || {
                    let path = rfd::FileDialog::new()
                        .set_title("フォルダを選択")
                        .pick_folder();
                    *result.lock().unwrap() = path;
                    repaint_ctx.request_repaint(); // UI 更新をトリガー
                });
            }

            // 結果を回収
            if let Some(path) = self.picked_folder.lock().unwrap().take() {
                self.picker_active = false;
                // path を使って設定に反映
            }
        });
    }
}
```

### 代替案: `rfd::AsyncFileDialog` + `poll_promise`

`poll_promise` クレートと組み合わせると、async 版を `update()` ループ内でポーリングできる。ただし tokio ランタイムの設定が必要になる場合があり、単純なスレッド spawn の方が取り回しやすい。

```rust
// poll_promise パターン（参考）
use poll_promise::Promise;

struct SettingsApp {
    folder_promise: Option<Promise<Option<PathBuf>>>,
}

// update() 内:
if ui.button("フォルダを選択…").clicked() {
    self.folder_promise = Some(Promise::spawn_thread("pick_folder", || {
        rfd::FileDialog::new().pick_folder()
    }));
}

if let Some(promise) = &self.folder_promise {
    if let Some(result) = promise.ready() {
        // result を処理
        self.folder_promise = None;
    }
}
```

### Windows 固有の挙動

- rfd は Windows で COM の `IFileDialog` を使用する
- 同期版はメッセージポンプをブロックするため、eframe のウィンドウ再描画が止まる
- 別スレッドで呼べば COM ダイアログは独立したメッセージループで動くため問題なし
- `ctx.request_repaint()` でダイアログ完了時に UI 更新をトリガーする必要がある

### 参考

- [egui Discussion #5621: How to avoid RFD file dialogs hanging egui?](https://github.com/emilk/egui/discussions/5621)
- [egui PR #5697: Update file dialog example to be non-blocking](https://github.com/emilk/egui/pull/5697)
- [egui Discussion #3092: Using rfd::FileDialog from a drop-down menu](https://github.com/emilk/egui/discussions/3092)
- [rfd リポジトリ](https://github.com/PolyMeilex/rfd)


## 3. カラーピッカー (Visual tab)

### 結論

egui にビルトインのカラーピッカーウィジェットがあり、十分な機能を提供する。`Color32` は `from_hex` / `to_hex` をサポートするため、CSS hex 文字列との相互変換も容易。プリセットテーマのスウォッチ表示も `show_color` + ボタンで実装可能。妥協不要。

### 推奨アプローチ

`color_edit_button_srgba` でカラーボタン + フルピッカーを表示し、隣に hex テキスト入力を配置する。

### コード例

```rust
use egui::Color32;
use egui::widgets::color_picker::{color_edit_button_srgba, Alpha};

struct ThemeEditor {
    bg_color: Color32,
    hex_input: String,
}

impl ThemeEditor {
    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("背景色:");

            // カラーピッカーボタン（クリックでフルピッカー表示）
            if color_edit_button_srgba(ui, &mut self.bg_color, Alpha::Opaque).changed() {
                // Color32 → 6桁 hex に変換
                self.hex_input = format!(
                    "#{:02x}{:02x}{:02x}",
                    self.bg_color.r(), self.bg_color.g(), self.bg_color.b()
                );
            }

            // hex テキスト入力
            let resp = ui.text_edit_singleline(&mut self.hex_input);
            if resp.lost_focus() {
                if let Ok(color) = Color32::from_hex(&self.hex_input) {
                    self.bg_color = color;
                }
            }
        });
    }

    /// プリセットテーマのスウォッチ表示
    fn preset_swatches(&mut self, ui: &mut egui::Ui) {
        let presets = [
            ("Dark", "#1e1e2e"),
            ("Light", "#eff1f5"),
            ("Nord", "#2e3440"),
        ];

        ui.horizontal(|ui| {
            for (name, hex) in &presets {
                let color = Color32::from_hex(hex).unwrap();
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(24.0, 24.0),
                    egui::Sense::click(),
                );
                if ui.is_rect_visible(rect) {
                    ui.painter().rect_filled(rect, 4.0, color);
                    ui.painter().rect_stroke(rect, 4.0, (1.0, Color32::GRAY));
                }
                if response.clicked() {
                    self.bg_color = color;
                    self.hex_input = hex.to_string();
                }
                response.on_hover_text(*name);
            }
        });
    }
}
```

### `Color32` hex 変換 API

| メソッド | シグネチャ | 備考 |
|----------|-----------|------|
| `from_hex` | `pub fn from_hex(hex: &str) -> Result<Color32, ParseHexColorError>` | `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa` 対応 |
| `to_hex` | `pub fn to_hex(&self) -> String` | 常に 8桁 `#rrggbbaa` を返す（ロスレス） |

CSS で使う6桁 `#rrggbb` が必要な場合は `format!("#{:02x}{:02x}{:02x}", r, g, b)` で自前変換する。

### 代替案

特になし。egui ビルトインで十分。

### 参考

- [egui::widgets::color_picker モジュール](https://docs.rs/egui/latest/egui/widgets/color_picker/index.html)
- [Color32 ドキュメント](https://docs.rs/egui/latest/egui/struct.Color32.html)
- [Color32 from hex issue #1492](https://github.com/emilk/egui/issues/1492)


## 4. システムフォント列挙 + プレビュー (Visual tab)

### 結論

**ライブプレビューは高コスト**。`ctx.set_fonts()` は呼び出しのたびに内部フォントアトラスを再構築するため、フォント選択ドロップダウンのホバーでリアルタイムプレビューを行うと重い。**フォント名をデフォルトフォントで表示し、適用ボタンで初めてフォントをロードする**方式が現実的。

### `ctx.set_fonts()` の挙動

- 呼び出しのたびにフォントアトラスを再構築する（グリフキャッシュが破棄される）
- フォントデータは `Arc<[u8]>` で保持されるため、`FontDefinitions` のクローンは軽い
- しかしアトラス再構築は重い（特に CJK フォントの場合、数百ミリ秒かかりうる）
- 新しいフォントは次フレームから有効になる

### 推奨アプローチ: フォント名リスト + 適用ボタン

```rust
use font_kit::source::SystemSource;

struct FontSettings {
    available_fonts: Vec<String>,  // 起動時にシステムフォントを列挙
    selected_font_name: String,
    fonts_loaded: bool,
}

impl FontSettings {
    fn load_system_font_names(&mut self) {
        // font-kit でシステムフォント名を列挙（軽量）
        if let Ok(families) = SystemSource::new().all_families() {
            self.available_fonts = families;
            self.available_fonts.sort();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.label("フォント:");

        // フォント名をデフォルトフォントで表示（プレビューなし）
        egui::ComboBox::from_id_salt("font_select")
            .selected_text(&self.selected_font_name)
            .show_ui(ui, |ui| {
                for name in &self.available_fonts {
                    ui.selectable_value(
                        &mut self.selected_font_name,
                        name.clone(),
                        name,
                    );
                }
            });

        if ui.button("適用").clicked() {
            self.apply_font(ctx);
        }
    }

    fn apply_font(&self, ctx: &egui::Context) {
        // font-kit でフォントファイルを読み込み
        let source = SystemSource::new();
        if let Ok(handle) = source.select_best_match(
            &[font_kit::family_name::FamilyName::Title(
                self.selected_font_name.clone(),
            )],
            &font_kit::properties::Properties::new(),
        ) {
            if let Ok(font_data) = handle.load() {
                if let Some(data) = font_data.copy_font_data() {
                    let mut fonts = egui::FontDefinitions::default();
                    fonts.font_data.insert(
                        "custom".to_owned(),
                        egui::FontData::from_owned((*data).to_vec()),
                    );
                    fonts.families
                        .entry(egui::FontFamily::Proportional)
                        .or_default()
                        .insert(0, "custom".to_owned());
                    ctx.set_fonts(fonts);
                }
            }
        }
    }
}
```

### 代替案: ライブプレビュー（制限付き）

選択変更のたびに `ctx.set_fonts()` を呼ぶ。CJK フォントは重いため、以下の緩和策を取る:

1. フォントファイルサイズで事前フィルタ（> 50MB はスキップ）
2. デバウンス: 選択後 500ms 待ってからロード
3. ローディングインジケータを表示

ただし UX 的には適用ボタン方式の方が安定する。

### 依存クレート

| クレート | 用途 |
|----------|------|
| `font-kit` | システムフォント列挙 + ファイルパス取得 |
| `font-loader` | 代替（メンテナンスが少ない） |

### 参考

- [egui Issue #7068: Add Support for Dynamically Creating New Font Families at Runtime](https://github.com/emilk/egui/issues/7068)
- [egui Discussion #2169: Loading fonts at runtime](https://github.com/emilk/egui/discussions/2169)
- [egui Issue #5233: Automatically load system fonts when needed](https://github.com/emilk/egui/issues/5233)
- [egui Discussion #1420: How to set custom font](https://github.com/emilk/egui/discussions/1420)


## 5. リスト並べ替え (Opener tab - up/down buttons)

### 結論

egui には組み込みの並べ替えリストウィジェットはないが、`ScrollArea` + ループ + ボタンで簡単に実装できる。ドラッグ&ドロップが必要なら `egui_dnd` クレートがある。Opener 設定程度のリストなら上下ボタン方式で十分。

### 推奨アプローチ: ScrollArea + 上下ボタン

```rust
struct OpenerListEditor {
    entries: Vec<GroupedOpenerEntry>,
}

impl OpenerListEditor {
    fn ui(&mut self, ui: &mut egui::Ui) {
        let mut swap: Option<(usize, usize)> = None;
        let mut remove_idx: Option<usize> = None;

        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                let len = self.entries.len();
                for i in 0..len {
                    let entry = &self.entries[i];
                    ui.horizontal(|ui| {
                        // 上下ボタン
                        ui.vertical(|ui| {
                            if ui.add_enabled(i > 0, egui::Button::new("▲").small())
                                .clicked()
                            {
                                swap = Some((i, i - 1));
                            }
                            if ui.add_enabled(i < len - 1, egui::Button::new("▼").small())
                                .clicked()
                            {
                                swap = Some((i, i + 1));
                            }
                        });

                        // エントリ内容
                        ui.vertical(|ui| {
                            ui.label(&entry.tool.name);
                            ui.label(
                                egui::RichText::new(&entry.tool.exe)
                                    .small()
                                    .color(egui::Color32::GRAY),
                            );
                        });

                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui.button("🗑").clicked() {
                                    remove_idx = Some(i);
                                }
                                if ui.button("編集").clicked() {
                                    // 編集モーダルを開く
                                }
                            },
                        );
                    });
                    ui.separator();
                }
            });

        // Immediate mode: ループ外で状態変更
        if let Some((a, b)) = swap {
            self.entries.swap(a, b);
        }
        if let Some(idx) = remove_idx {
            self.entries.remove(idx);
        }
    }
}
```

### 代替案: egui_dnd によるドラッグ&ドロップ

```rust
// Cargo.toml: egui_dnd = "0.13"
use egui_dnd::dnd;

fn ui(&mut self, ui: &mut egui::Ui) {
    dnd(ui, "opener_list")
        .with_animation_time(0.2)
        .show_vec(&mut self.entries, |ui, entry, handle, _state| {
            ui.horizontal(|ui| {
                handle.ui(ui, |ui| {
                    ui.label("≡"); // ドラッグハンドル
                });
                ui.label(&entry.tool.name);
                ui.label(&entry.tool.exe);
            });
        });
}
```

### イディオム

egui の immediate mode では、ループ中に `Vec` を変更できないため、変更操作（swap/remove）を一旦変数に記録し、ループ後に適用するパターンが定石。上のコード例の `swap` / `remove_idx` がその例。

### 参考

- [egui_dnd クレート](https://docs.rs/egui_dnd/latest/egui_dnd/)
- [egui Discussion #1530: Advanced drag and drop prototype](https://github.com/emilk/egui/discussions/1530)
- [egui Discussion #3869: How to implement drag and drop?](https://github.com/emilk/egui/discussions/3869)


## 6. Opener グループ化ロジックの置き場所

### 現状の構造

| ファイル | 役割 |
|----------|------|
| `snotra-core/src/config.rs` | `OpenerRule` / `OpenerTool` 構造体、`find_matching_tools()` |
| `ui/src/lib/openerGroups.ts` | `buildGroupedOpeners()` (flatten) / `serializeGroupedOpeners()` (rebuild) |
| `src-tauri/src/commands/launch.rs` | `find_matching_tools()` を使って Opener を解決（グループ化ロジックは使っていない） |

### 分析

`openerGroups.ts` の `buildGroupedOpeners` / `serializeGroupedOpeners` は **純粋にUI表示のための変換ロジック**:

- `buildGroupedOpeners`: `OpenerRule[]` → ツール単位の `GroupedOpenerEntry[]` にフラット化（同一ツールの拡張子をマージ）
- `serializeGroupedOpeners`: 編集後の `GroupedOpenerEntry[]` → `OpenerRule[]` に再構築

`src-tauri` 側（メインアプリ）はこのグループ化ロジックを**一切使っていない**。`find_matching_tools()` は `OpenerRule[]` を直接走査するだけ。

### 結論: snotra-core に移動すべき

**理由:**

1. **テスト可能性**: `snotra-core` は純ロジック crate でユニットテスト可能。TypeScript のテストを Rust テストに移植できる
2. **設定 UI が複数箇所から使われる可能性**: 将来 egui 設定 UI と Web UI が共存する場合、ロジックの重複を防げる
3. **型安全性**: Rust 側に置けば `OpenerRule` ↔ `GroupedOpenerEntry` 変換の型整合が compile-time で保証される

**ただし、メインアプリ（src-tauri）はこのロジックを使わない**ため、`snotra-core` 内で `#[cfg(feature = "settings-ui")]` のようなフィーチャーゲートにする必要はない。関数が存在しても呼ばれなければバイナリサイズへの影響は LTO で除去される。

### 推奨構成

```
snotra-core/src/
  config.rs          # OpenerRule, OpenerTool, find_matching_tools() (既存)
  opener_group.rs    # GroupedOpenerEntry, build_grouped_openers(), serialize_grouped_openers() (新規)
```

### トレードオフ

| 選択肢 | メリット | デメリット |
|--------|---------|-----------|
| **snotra-core に置く** | テスト可能、型安全、再利用可能 | メインアプリでは不使用のコードが増える（ただし LTO で除去） |
| **snotra-settings (UI crate) に置く** | 設定 UI 専用で依存が明確 | 将来別の設定 UI を作る場合にロジック重複 |

### 参考

- `ui/src/lib/openerGroups.ts`: フラット化・再構築ロジックの TypeScript 実装
- `snotra-core/src/config.rs`: `OpenerRule` / `OpenerTool` 定義と `find_matching_tools()`
- `src-tauri/src/commands/launch.rs`: メインアプリでの Opener 使用箇所（グループ化不使用を確認済み）


## 7. rfd のブロッキング問題の詳細調査

### Windows での rfd の挙動

| 項目 | 詳細 |
|------|------|
| Win32 API | `IFileDialog` (COM) を使用 |
| メッセージポンプ | COM ダイアログは**独自のメッセージループ**を内部で回す |
| 呼び出しスレッド | 呼び出し元スレッドを**ブロック**する（ダイアログが閉じるまで戻らない） |
| eframe への影響 | メインスレッドで呼ぶと `update()` が呼ばれなくなり、ウィンドウが「応答なし」になる |

### 既知の問題

1. **「応答なし」ポップアップ**: メインスレッドブロック時に Windows が「応答なし」と表示する ([Discussion #3092](https://github.com/emilk/egui/discussions/3092))
2. **ダイアログが背面に回る**: 2回目以降の呼び出しでファイルダイアログが eframe ウィンドウの背面に開くことがある ([Discussion #3092](https://github.com/emilk/egui/discussions/3092))
3. **`request_repaint_after` との競合**: ダイアログ表示中に repaint リクエストが発火するとパニックする場合がある ([Discussion #3499](https://github.com/emilk/egui/discussions/3499))
4. **ドロップダウンメニューの残留**: メニューから呼んだ場合、ダイアログ終了後もメニューが表示されたままになる ([Discussion #3092](https://github.com/emilk/egui/discussions/3092))

### 推奨パターン: スレッド spawn + Arc\<Mutex\> + request_repaint

```rust
use std::sync::{Arc, Mutex};
use std::path::PathBuf;

struct App {
    folder_result: Arc<Mutex<Option<Option<PathBuf>>>>,
    // None = ピッカー未起動 or 結果回収済み
    // Some(None) = キャンセルされた
    // Some(Some(path)) = フォルダ選択された
    picker_active: bool,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 結果をポーリング（ロックは短時間）
        if self.picker_active {
            if let Ok(mut guard) = self.folder_result.try_lock() {
                if let Some(result) = guard.take() {
                    self.picker_active = false;
                    if let Some(path) = result {
                        // path を処理
                    }
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            if ui.add_enabled(!self.picker_active, egui::Button::new("フォルダを選択…"))
                .clicked()
            {
                self.picker_active = true;
                let result = Arc::clone(&self.folder_result);
                let repaint = ctx.clone();
                std::thread::spawn(move || {
                    let path = rfd::FileDialog::new()
                        .set_title("フォルダを選択")
                        .pick_folder();
                    *result.lock().unwrap() = Some(path);
                    repaint.request_repaint();
                });
            }

            if self.picker_active {
                ui.spinner();
                ui.label("フォルダを選択中…");
            }
        });
    }
}
```

### なぜ AsyncFileDialog ではなく同期版 + スレッドか

| 方式 | メリット | デメリット |
|------|---------|-----------|
| **同期版 + `std::thread::spawn`** | tokio 不要、シンプル、eframe と相性良好 | スレッド管理が必要（とはいえ単純） |
| **`AsyncFileDialog` + tokio** | async/await で書ける | tokio ランタイムが必要、eframe の `update()` との統合が煩雑 |
| **`AsyncFileDialog` + `poll_promise`** | tokio 不要で async を使える | 追加クレート依存、内部的にはスレッド spawn と同等 |

eframe アプリでは **同期版 + スレッド spawn が最もシンプルで推奨される**。emilk 自身も「ブロッキング rfd 呼び出しはメインスレッドで行う必要がある」と述べた上で、「ノンブロッキングにしたければ async 版を使え」と助言している。しかし実際のところ、同期版を別スレッドで呼ぶのが最も安定している。

### 参考

- [egui Discussion #5621: How to avoid RFD file dialogs hanging egui?](https://github.com/emilk/egui/discussions/5621)
- [egui Discussion #3499: request_repaint_after + rfd panic](https://github.com/emilk/egui/discussions/3499)
- [egui Discussion #3092: rfd from drop-down menu issues](https://github.com/emilk/egui/discussions/3092)
- [egui PR #5697: Non-blocking file dialog example](https://github.com/emilk/egui/pull/5697)
- [rfd GitHub](https://github.com/PolyMeilex/rfd)
