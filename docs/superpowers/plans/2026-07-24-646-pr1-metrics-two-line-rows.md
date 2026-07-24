# #646 PR1: Metrics font 連動 + 結果行 2 行表示 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 固定 30px/52px の行高・バー高・toast 高を config 連動の `Metrics` 純粋核へ置き換え、結果行を 2 行表示(上段=名前・下段=パス)にする。

**Architecture:** spec `docs/superpowers/specs/2026-07-24-646-two-window-ui-design.md` の決定 1・2・9。`snotra-core` の `VisualConfig` に `row_padding` / `bar_padding` を追加(serde default・無移行)し、`src-tauri/src/egui_shell/layout.rs`(純粋核・egui 非依存)に `Metrics` を新設。`view.rs` の固定値 3 箇所と `mod.rs` の show 時高さリセットを `Metrics` 経由へ置換し、`draw_result_row` を 2 行描画に書き換える。窓分離・`window_gap`・実件数フィットは **PR2 であり本計画に含まない**。

**Tech Stack:** Rust / egui 0.3x(softbuffer CPU ラスタ)/ serde / TOML

## Global Constraints

- **main へ直接コミットしない**。作業ブランチ: `feat/646-pr1-metrics-two-line`(spec PR マージ後の main から作成)
- bash HEREDOC 禁止。複数行コミットメッセージは PowerShell here-string `@'...'@`(閉じ `'@` は行頭)か一時ファイル + `git commit -F`
- パス区切りは `/` で書く
- PostToolUse hook が `.rs` 編集ごとに clippy + crate テストを自動実行する。**沈黙 = 合格**(失敗時のみ会話に届く)。ただし計画の Red/Green 確認は明示コマンドで行う
- `--no-verify` 禁止
- コミットメッセージ末尾: `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`
- 式の定数(spec 決定 2 より・変更禁止): `bar_height = font_size + bar_padding` / `path_size = max(font_size × 0.78, 9.0)` / `row_height = max(font_size + path_size + row_padding + 4.0, 24.0)` / `toast_height = bar_height`。config 既定値: `row_padding = 6`・`bar_padding = 28`

---

### Task 1: VisualConfig に row_padding / bar_padding を追加(snotra-core)

**Files:**
- Modify: `snotra-core/src/config.rs`(`VisualConfig` struct・`Default` impl・default fn 群・tests)

**Interfaces:**
- Produces: `VisualConfig.row_padding: u32`(既定 6)・`VisualConfig.bar_padding: u32`(既定 28)。Task 3・4 が `engine.config().visual.row_padding` 等で読む

- [ ] **Step 1: 失敗するテストを書く**

`snotra-core/src/config.rs` の既存 `#[cfg(test)] mod tests`(`mod tests` を grep で特定)へ追加:

```rust
    /// #646 PR1: 新キー欠落の旧 config は serde default(6/28)で読める(後方互換・移行不要)。
    #[test]
    fn visual_padding_defaults_for_missing_keys() {
        let toml = r#"
[hotkey]
modifier = "alt"
key = "q"
[appearance]
window_width = 600
[paths]
"#;
        let config: Config = toml::from_str(toml).expect("parse");
        assert_eq!(config.visual.row_padding, 6);
        assert_eq!(config.visual.bar_padding, 28);
        assert_eq!(VisualConfig::default().row_padding, 6);
        assert_eq!(VisualConfig::default().bar_padding, 28);
    }
```

注: 必須フィールドは plan-review で実測済み — `hotkey.modifier`(**単数形**・serde default 無し・config.rs:104-107)と `appearance.window_width`(config.rs:308)。`[paths]` は空セクションでよい(`scan`/`additional` とも serde default 持ち)。先例は `deserialize_full_config`(config.rs:1122)。`[visual]` セクション**なし**を維持すること(このテストの主眼)。

- [ ] **Step 2: 落ちることを確認する(Red)**

Run: `cargo test -p snotra-core visual_padding_defaults_for_missing_keys`
Expected: FAIL(`no field row_padding` のコンパイルエラー)

- [ ] **Step 3: 最小実装**

`VisualConfig`(config.rs:384 付近)へフィールド追加(`custom_theme` の前):

```rust
    #[serde(default = "default_row_padding")]
    pub row_padding: u32,
    #[serde(default = "default_bar_padding")]
    pub bar_padding: u32,
```

default fn 群(`default_font_size` の隣)へ:

```rust
/// #646 PR1: 行高の余白(row_height = font_size + path_size + row_padding + 4)。
fn default_row_padding() -> u32 {
    6
}

/// #646 PR1: バー高の余白(bar_height = font_size + bar_padding)。28 は現行 52px を
/// 「font_size=24 でのチューニング結果」と読み直した値(24 + 28 = 52)。
fn default_bar_padding() -> u32 {
    28
}
```

`impl Default for VisualConfig`(config.rs:405 付近)へ:

```rust
            row_padding: default_row_padding(),
            bar_padding: default_bar_padding(),
```

- [ ] **Step 4: 通ることを確認する(Green)**

Run: `cargo test -p snotra-core`
Expected: 全 PASS(既存テスト含む)

- [ ] **Step 5: コミット**

```powershell
git add snotra-core/src/config.rs && git commit -m @'
feat: VisualConfig に row_padding / bar_padding を追加(#646 PR1)

serde default(6/28)で旧 config 無移行。Metrics 連動式の入力になる。

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 2: layout.rs に Metrics 純粋核を新設

**Files:**
- Modify: `src-tauri/src/egui_shell/layout.rs`(`HeightParams` の前に追加・tests 追記)

**Interfaces:**
- Consumes: なし(純関数のみ)
- Produces: `pub fn path_size(font_size: u32) -> f64` / `pub struct Metrics { pub bar_height: f64, pub row_height: f64, pub toast_height: f64 }` / `Metrics::from_config(font_size: u32, row_padding: u32, bar_padding: u32) -> Metrics`。Task 3・4 が使う

- [ ] **Step 1: 失敗するテストを書く**

`layout.rs` の `#[cfg(test)] mod tests` へ追加:

```rust
    /// #646 決定 2: bar_padding=28 は font 24 で現行 52px をピクセル再現する(後方互換の要)。
    #[test]
    fn metrics_bar_reproduces_current_at_font24() {
        let m = Metrics::from_config(24, 6, 28);
        assert_eq!(m.bar_height, 52.0);
        assert_eq!(m.toast_height, 52.0);
    }

    /// #646 決定 2・9: row_height は 2 行積算(name 行 + path 行 + 行間 4 + row_padding)。
    #[test]
    fn metrics_row_is_two_line_sum() {
        let m = Metrics::from_config(15, 6, 28);
        assert_eq!(m.bar_height, 43.0);
        // path_size = max(15*0.78, 9) = 11.7 → 15 + 11.7 + 6 + 4 = 36.7
        assert!((m.row_height - 36.7).abs() < 1e-9, "row={}", m.row_height);
    }

    /// #646 決定 2: 下限 24(アイコン 16px + 余白)。8 + 9 + 0 + 4 = 21 → 24 へ床上げ。
    #[test]
    fn metrics_row_floor_is_24() {
        assert_eq!(Metrics::from_config(8, 0, 28).row_height, 24.0);
    }

    /// path_size は RowTheme と同係数(0.78・下限 9)。
    #[test]
    fn path_size_matches_row_theme_coefficient() {
        assert_eq!(path_size(8), 9.0);
        assert!((path_size(15) - 11.7).abs() < 1e-9);
    }
```

- [ ] **Step 2: 落ちることを確認する(Red)**

Run: `cargo test -p snotra metrics_`
Expected: FAIL(`Metrics` 未定義のコンパイルエラー)

注(pre-flight 是正): `--lib` は使えない — `cargo test -p snotra --lib` は `no library targets found in package 'snotra'` で失敗する(SU6/SU6.5 ledger の環境実測)。positional フィルタのみで絞る。

- [ ] **Step 3: 最小実装**

`layout.rs` の `HeightParams` 定義の直前へ:

```rust
/// path 行のフォントサイズ(#646 決定 9)。view.rs `RowTheme::path_size` と同係数——
/// 正本はここ(layout の Metrics が同じ値で行高を積算するため。二重定義は行高と描画の
/// 不一致バグになる)。
pub fn path_size(font_size: u32) -> f64 {
    (font_size as f64 * 0.78).max(9.0)
}

/// 行高・バー高・toast 高の算出値(#646 決定 2)。config `visual` から毎フレーム導出し
/// キャッシュしない(font_size と同じ live-read 方針)。
pub struct Metrics {
    /// font_size + bar_padding。既定(15+28)=43、font 24 で現行 52 を再現。
    pub bar_height: f64,
    /// 2 行表示(決定 9)の積算: font_size + path_size + row_padding + 行間 4。下限 24。
    pub row_height: f64,
    /// bar_height と同値(§20.3 の toast 行)。
    pub toast_height: f64,
}

impl Metrics {
    pub fn from_config(font_size: u32, row_padding: u32, bar_padding: u32) -> Self {
        let f = font_size as f64;
        let bar_height = f + bar_padding as f64;
        let row_height = (f + path_size(font_size) + row_padding as f64 + 4.0).max(24.0);
        Self { bar_height, row_height, toast_height: bar_height }
    }
}
```

- [ ] **Step 4: 通ることを確認する(Green)**

Run: `cargo test -p snotra`
Expected: 全 PASS(`compute_window_height` の既存テストは 52.0/30.0 リテラル注入のため無変更で通る)

- [ ] **Step 5: コミット**

```powershell
git add src-tauri/src/egui_shell/layout.rs && git commit -m @'
feat: layout.rs に Metrics 純粋核を追加(#646 PR1・決定 2)

bar/row/toast 高を font_size + config 余白から導出。font 24 で現行値を再現。

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 3: view.rs — 2 行描画 + 固定値 3 箇所の Metrics 化

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`
  - `row_theme()`(1000 行付近)— `path_size` を layout の正本から取る
  - 新 helper `metrics()` を `row_theme()` の隣に追加
  - `draw_result_row`(900 行付近)— 2 行描画 + `row_h` 引数化
  - 結果リストループ(1712-1740 付近)— 呼び出しへ `metrics` を渡す
  - toast ブロック(1592-1638 付近)— `52.0` と y 定数を `toast_height` 比例へ
  - `compute_window_height` 呼び出し(1750 付近)— `Metrics` 値を注入

**Interfaces:**
- Consumes: Task 1 の `visual.row_padding` / `visual.bar_padding`、Task 2 の `Metrics::from_config` / `path_size`
- Produces: `fn metrics(&self) -> Metrics`(Task 4 は使わない。mod.rs は自前で `Metrics::from_config` を呼ぶ)

- [ ] **Step 1: row_theme の path_size を layout 正本へ寄せる**

`row_theme()` 内(view.rs:1015 付近):

```rust
            // 変更前: path_size: (size as f32 * 0.78).max(9.0), // WebView2 の name>path 比を踏襲
            path_size: crate::egui_shell::layout::path_size(size) as f32, // 正本は layout(#646)
```

- [ ] **Step 2: metrics() helper を追加**

`row_theme()` の直後へ:

```rust
    /// 実行中 config から Metrics を都度導出する(#646 決定 2)。row_theme と同じ
    /// live-read 方針(キャッシュしない・config-applied wake で次フレームに反映)。
    fn metrics(&self) -> crate::egui_shell::layout::Metrics {
        let (f, rp, bp) = self
            .app_handle
            .try_state::<crate::AppState>()
            .map(|s| {
                let engine = s.engine.lock().unwrap();
                let v = &engine.config().visual;
                (v.font_size, v.row_padding, v.bar_padding)
            })
            .unwrap_or((15, 6, 28));
        crate::egui_shell::layout::Metrics::from_config(f, rp, bp)
    }
```

- [ ] **Step 3: draw_result_row を 2 行描画へ書き換える**

シグネチャへ `row_h: f32` を追加し、本体の name/path 部(909-993 行付近)を置換。選択ハイライト・scroll_to_me・アイコン slot は不変:

```rust
    fn draw_result_row(
        ui: &mut egui::Ui,
        result: &SearchResult,
        selected: bool,
        scroll: bool,
        icon: Option<&egui::TextureHandle>,
        show_icons: bool,
        theme: &RowTheme,
        row_h: f32,
    ) -> bool {
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), row_h),
            egui::Sense::click(),
        );
        // …選択ハイライト・scroll_to_me・アイコン描画は現行のまま(row_h = 30.0 の行のみ削除)…

        // 2 行表示(#646 決定 9): 上段 = 名前(全幅・末尾省略)、下段 = パス(全幅・左寄せ・
        // 幅超過時のみ中間省略)。#632 の「name 60% 制限 + path 右寄せ + 実測幅の重なり回避」は
        // 2 行化で name と path が幅を取り合わなくなったため廃止。
        let text_x = rect.left() + slot;
        let avail = (rect.right() - 8.0 - text_x).max(0.0);
        let mut name_job = egui::text::LayoutJob::single_section(
            result.name.clone(),
            egui::TextFormat {
                font_id: egui::FontId::proportional(theme.name_size),
                color: theme.name_color,
                ..Default::default()
            },
        );
        name_job.wrap = egui::text::TextWrapping::truncate_at_width(avail);
        let name_galley = ui.painter().layout_job(name_job);
        // path 空(エラー行等)は名前 1 行を縦中央に単独描画
        if result.path.is_empty() {
            ui.painter().galley(
                egui::pos2(text_x, rect.center().y - name_galley.size().y / 2.0),
                name_galley,
                theme.name_color,
            );
            return response.clicked();
        }
        let path_font = egui::FontId::proportional(theme.path_size);
        let path_full = ui.painter().layout_no_wrap(
            result.path.clone(),
            path_font.clone(),
            theme.path_color,
        );
        let path_str = if path_full.size().x <= avail {
            result.path.clone()
        } else {
            // per-char 幅は実 galley から実測(CJK 過小評価対策・#632 の方針を継承)
            let per_char_px = path_full.size().x / (result.path.chars().count().max(1) as f32);
            truncate_middle(&result.path, avail, per_char_px)
        };
        let path_galley = ui.painter().layout_no_wrap(path_str, path_font, theme.path_color);
        // 鏡像ケース(folder 列挙エラー行・snotra-core/src/folder.rs の error_result は
        // name 空・path 非空): 上段を空白にせず path 1 行を縦中央に単独描画
        //(上の path 空分岐と対称・plan-review scout-egui 指摘)。
        if result.name.is_empty() {
            ui.painter().galley(
                egui::pos2(text_x, rect.center().y - path_galley.size().y / 2.0),
                path_galley,
                theme.path_color,
            );
            return response.clicked();
        }
        // 2 行ブロックを rect 縦中央へ(行間 4.0 は Metrics::row_height の +4.0 と対)
        let total_h = name_galley.size().y + 4.0 + path_galley.size().y;
        let top = rect.center().y - total_h / 2.0;
        let name_h = name_galley.size().y;
        ui.painter().galley(egui::pos2(text_x, top), name_galley, theme.name_color);
        ui.painter().galley(
            egui::pos2(text_x, top + name_h + 4.0),
            path_galley,
            theme.path_color,
        );
        response.clicked()
    }
```

doc コメント(892-899 行)も 2 行表示の記述へ更新する(「name galley の実幅を測って path 開始 x を決める」の一文を「上段名前・下段パス。行高は Metrics::row_height(呼び出し側注入)」へ)。

- [ ] **Step 4: 呼び出し側 3 箇所を Metrics 化**

結果リストループ(1712 付近)— `let theme = self.row_theme();` の隣で `let metrics = self.metrics();` を取り、`draw_result_row(..., &theme)` → `draw_result_row(..., &theme, metrics.row_height as f32)`。

toast ブロック(1592 付近)— `let m = self.metrics();` を取り:

```rust
            let toast_h = m.toast_height as f32;
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), toast_h),
                egui::Sense::hover(),
            );
```

行 1 の y `rect.top() + 13.0` → `rect.top() + toast_h * 0.25`、ボタンの `btn_y = rect.top() + 39.0` → `rect.top() + toast_h * 0.75`(52px 時の 13/39 と同比率)。

`compute_window_height` 呼び出し(1750 付近):

```rust
        let m = self.metrics();
        let height = compute_window_height(&HeightParams {
            show_results,
            max_results: self.max_results(),
            has_update_toast: has_toast,
            search_bar_height: m.bar_height,
            result_row_height: m.row_height,
            results_padding: 8.0,
            update_toast_height: m.toast_height,
        });
```

(toast ブロックの `m` とスコープが切れているため再取得。engine lock は毎フレーム複数回取得が既存パターン——`row_theme` / `show_icons` と同じ。)

さらに**虚偽化するコメント 2 箇所を同時更新する**(plan-review scout-egui / 独立導出の一致指摘):

- `view.rs:1482-1487`「**バー高さ 52px は据え置く**…(SU6.5 決定 3)」→「#646 決定 2: バー高は `font_size + bar_padding`(Metrics)。SU6.5 決定 3 の 52px 据え置きは WebView2 parity 制約下の判断で、SU7 の WebView2 撤去により失効」の旨へ書き換え
- `view.rs:1448` 付近「window は 52px のまま」→「window は bar_height のまま」へ

**注意(独立導出の警告)**: `bar_padding` 既定 28 とアイコン slot 幅 `28.0`(view.rs:924)は**無関係な偶然の一致**。grep 置換の対象にしないこと。`view.rs:207` の `last_set_height: 52.0` 初期値は据え置き(初回フレームの diff 検知で是正される設計・view.rs:1759-1760 コメントに明記)。indexing overlay の `FontId 15.0`(view.rs:1587)は pre-existing の別軸でスコープ外(PR 本文に follow-up 候補として記す)。

- [ ] **Step 5: ビルドとテストが通ることを確認する**

Run: `cargo clippy -p snotra --all-targets -- -D warnings` → 警告 0
Run: `cargo test -p snotra` → 全 PASS
(PostToolUse hook の沈黙も同じ検査の合格を意味する)

- [ ] **Step 6: コミット**

```powershell
git add src-tauri/src/egui_shell/view.rs && git commit -m @'
feat: 結果行を 2 行表示にし固定高 3 箇所を Metrics 連動へ(#646 PR1・決定 2/9)

- draw_result_row: 上段名前(末尾省略)・下段パス(全幅・超過時のみ中間省略)。
  #632 の name60%/右寄せ/重なり回避の座標計算は 2 行化で廃止
- toast 行と compute_window_height を Metrics(bar/row/toast)経由に
- RowTheme::path_size の係数は layout::path_size を正本に

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 4: mod.rs — show 時の高さリセットを bar_height へ

**Files:**
- Modify: `src-tauri/src/egui_shell/mod.rs:196-209`(`show_egui_main` の高さリセット)と `create()` の `inner_size` コメント(169 行付近)

**Interfaces:**
- Consumes: Task 1 の config キー・Task 2 の `Metrics::from_config`

- [ ] **Step 1: 高さリセットを Metrics 化**

`show_egui_main` の `set_size` ブロック(201-209 行)を置換:

```rust
    #[cfg(windows)]
    {
        let width = window
            .inner_size()
            .ok()
            .map(|s| s.to_logical::<f64>(window.scale_factor().unwrap_or(1.0)).width)
            .unwrap_or(600.0);
        // 折りたたみ高 = bar_height(#646 決定 2)。52 固定だと font 連動後の実バー高と
        // ずれ、position クランプが誤った高さで効く(コメント 196-200 行の機構と同じ理由)。
        let bar_h = app
            .try_state::<crate::AppState>()
            .map(|s| {
                let engine = s.engine.lock().unwrap();
                let v = &engine.config().visual;
                layout::Metrics::from_config(v.font_size, v.row_padding, v.bar_padding).bar_height
            })
            .unwrap_or(52.0);
        let _ = window.set_size(tauri::LogicalSize::new(width, bar_h));
    }
```

`layout` が `mod.rs` スコープに無ければ `use` 追記ではなく `crate::egui_shell::layout::Metrics` のフル修飾で書く(既存の修飾スタイルに合わせる)。196-200 行の既存コメント内の「52px」表現は「bar_height(既定 43px)」へ更新。`create()` の `.inner_size(window_width, 52.0)` は**据え置き**(visible:false の初期値で、初回 show が正すため)——ただしコメントへ「初期値。実高は show 時に Metrics で再設定(#646)」を追記。

- [ ] **Step 2: ビルドとテストを確認する**

Run: `cargo clippy -p snotra --all-targets -- -D warnings` → 警告 0
Run: `cargo test -p snotra` → 全 PASS

- [ ] **Step 3: コミット**

```powershell
git add src-tauri/src/egui_shell/mod.rs && git commit -m @'
feat: show 時の折りたたみ高を bar_height 連動に(#646 PR1)

52 固定のままだと font 連動後の実バー高とずれ、position クランプが誤差で効く。

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 5: SPEC.md 同期(仕様変更の文書化)

**Files:**
- Modify: `SPEC.md:183`(show 時リセット)・`SPEC.md:513-514`(結果行 1 行表示 + 中間省略の子 bullet)・`SPEC.md:517`(バー高 52px 固定)・`SPEC.md:1015-1023`(§20.3 toast 高)
- Modify: `docs/architecture.md:89`(show 時 52px リセット)・`docs/architecture.md:147`(トースト 52px)— plan-review で scout-docs と独立導出が独立に一致した漏れ
- Modify: `src-tauri/CLAUDE.md`「show の操作順序制約」節(「高さリセット(52px)」の具体値)— 規範文書は自動配送されないため見出し語で grep して特定する

**Interfaces:** なし(文書のみ)

- [ ] **Step 1: 4 箇所を as-built へ同期**

行番号はドリフトしうるため**引用文で grep してから**編集する:

1. 「show 時に毎回検索バー高さ(52px)にリセット」→「show 時に毎回検索バー高さ(`font_size + bar_padding`・既定 43px)にリセットしてから結果に応じて拡張する(#646)」
2. 「検索結果はフルパスの1行表示」→「検索結果は 2 行表示: 上段 = 表示名(`font_size`・末尾省略)、下段 = フルパス(`font_size × 0.78`・左寄せ・幅超過時のみ中間省略)(#646)」。**直下の子 bullet(SPEC.md:514「長いパスは中間セグメント省略…」)は置換文と内容重複するため統合し、515(フォルダ末尾 `\` 区別)は残す**(scout-docs 指摘)
3. 「**検索バー高さは 52px 固定**(font_size 非連動の as-built。行高との連動は #646)」→「検索バー高さは `font_size + bar_padding`(既定 15+28=43px)、結果行高は `font_size + path行 + row_padding + 4`(下限 24px)。`row_padding` / `bar_padding` は `[visual]` の config キー(既定 6 / 28・#646)」
4. §20.3 の「高さ 52px(2行 × 26px)」および「`--update-toast-height` (52px) を加算」「検索バー直下の 52px 行」→ toast 高は `bar_height` と同値(既定 43px)に連動する旨へ(3 文とも。行内の y 配置は高さ比 0.25 / 0.75)
5. `docs/architecture.md`: 「show 時に 52px へリセット」(89 行付近)→「show 時に bar_height(`font_size + bar_padding`・既定 43px)へリセット」、「トースト UI は…52px で表示」(147 行付近)→「bar_height と同高で表示」
6. `src-tauri/CLAUDE.md`「show の操作順序制約」: 「高さリセット(52px)」→「高さリセット(bar_height・既定 43px)」(順序制約自体は不変)
7. SPEC §11 の管理項目列挙(509-511 行)への `row_padding` / `bar_padding` 追記は**しない** — あの列挙は GUI 設定タブの項目一覧であり、新キーは GUI 非露出(scout-docs の裏取りで確定)

- [ ] **Step 2: governance:check を実行する**

Run: `npm run governance:check`
Expected: PASS(SPEC § 参照の整合。`*.md` は hook 検査対象外——沈黙は「何も走らなかった」なので明示実行が必須)

- [ ] **Step 3: コミット**

```powershell
git add SPEC.md docs/architecture.md src-tauri/CLAUDE.md && git commit -m @'
docs: SPEC/architecture/CLAUDE.md を Metrics 連動 + 2 行表示の as-built へ同期(#646 PR1)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 6: スモーク検証 + PR 作成

**Files:** なし(検証と PR のみ)

- [ ] **Step 1: egui スモークを実行する**

Run: `npm run smoke:egui`(コマンド実体は `docs/build-commands.md` カテゴリ C を参照)
Expected: PASS(trace イベント名・hotkey 前提は本 PR で不変)

- [ ] **Step 2: 実機 GUI スモーク(人間ペア)**

ユーザーに依頼: 起動して次を目視 — (a) 既定 font 15 でバー 43px・結果行 2 行(上段名前・下段パス)になっている (b) config.toml の `font_size = 24` でバーが従来と同じ 52px(バー/toast のみのピクセル一致が受け入れ条件・spec 決定 8) (c) `row_padding` / `bar_padding` を書き換えて保存すると再起動なしで反映 (d) toast(`SNOTRA_EGUI_FAKE_UPDATE` の既存流儀)で 2 ボタンが枠内に収まる

- [ ] **Step 3: push して PR を作る**

```powershell
git push -u origin HEAD && gh pr create --title "#646 PR1: Metrics font 連動 + 結果行 2 行表示" --body-file <一時ファイル>
```

PR 本文の注意(ルート CLAUDE.md「Git/GitHub 運用」): closing keyword を書かない(`Refs #646` のみ。#646 は PR2 まで OPEN を保つ)。マージ直前に `gh pr view <PR> --json closingIssuesReferences` で閉じる issue が空であることを確認する。

---

## Self-Review(記入済み)

- **Spec coverage**: 決定 1(PR 分割)= 本計画が PR1 のみ担当 ✓ / 決定 2(Metrics + config 2 キー。`window_gap` は PR2)= Task 1・2 ✓ / 決定 9(2 行表示)= Task 3 ✓ / 決定 8 の PR1 受け入れ条件(バー/toast ピクセル一致・行は目視)= Task 6 Step 2 ✓ / SPEC 同期 = Task 5(spec の「計画時に grep で確定」を果たした: 183・513・517・1015-1023)✓
- **Placeholder scan**: TBD/TODO なし。全コードブロックは実体 ✓
- **Type consistency**: `Metrics::from_config(u32, u32, u32)` を Task 2 で定義し Task 3・4 が同名同型で消費 ✓ / `path_size(u32) -> f64` を Task 2 で定義し Task 3 Step 1 が `as f32` で消費 ✓ / `draw_result_row` の追加引数 `row_h: f32` と呼び出し側 `metrics.row_height as f32` ✓
