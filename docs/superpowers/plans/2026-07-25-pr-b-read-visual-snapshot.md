# PR B: `read_visual` 合成アクセサ 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 1 フレーム内に散らばった config lock を 1 回に束ね、同じ `font_size` から行高と文字サイズが導かれることを保証する。あわせて `row_theme` が毎フレーム行っている `String` 3 本の clone を消す。

**Architecture:** `src-tauri/src/egui_shell/visual.rs` を新設し、`VisualSnapshot` と `RowTheme`、および `VisualConfig → VisualSnapshot` の**純関数写像**（ユニットテスト対象）を置く。`mod.rs` には lock を取る薄いアクセサ `read_visual` だけを置く。main（`view.rs`）と results（`results_view.rs`）はフレーム冒頭で 1 回 snapshot を取り、以降はそれを読む。

**Tech Stack:** Rust / Tauri v2.11 / egui + softbuffer

**根拠となる spec:** `docs/superpowers/specs/2026-07-25-egui-window-ownership-and-event-delivery-design.md` の**決定 4**。前提サイクル: PR A（smoke 被覆）・PR A′（`ResultsWindow`）マージ済み。

## Global Constraints

- **`VisualSnapshot` の寿命は 1 フレーム。`self.` フィールドに保持しない**（毎フレーム live-read 方針・#576 / #646 決定 2 の保護）。保持した瞬間、config 変更が次フレームで反映されなくなる。
- **guard 内に確保の重い処理・I/O を置かない。** 置いてよいのは hex→`Color32` の parse と `Metrics::from_config` の算術、および `&str` 比較まで。
- **`read_metrics` は独立した projection として残す。** `show_egui_main` が show 経路で呼ぶため、show が不要な色 parse を払う形にしてはならない。
- **導出式を複製しない。** 行高は `layout::Metrics::from_config`、path 文字サイズは `layout::path_size` が唯一の正本であり、guard 内からそれを呼ぶ（式を書き写さない）。
- **`egui::Color32::from_hex` と `config_watcher::parse_hex_color` を統合しない**（下記「2 つのパーサを残す理由」）。
- 対象は**テーマ（`[visual]` 全体 + `appearance.show_icons`）に限る**。`max_results()`（`effective_visible_rows`）・`window_width()`・`lang()` は寸法/地域化であってテーマではないため**スコープ外**（現状のまま個別 lock を残す）。`ResultsView::setup()` の 1 回きり font 読みもスコープ外。

### 2 つのパーサを残す理由（実測済み・この PR では統合しない）

| パーサ | 用途 | 受理する形 |
|---|---|---|
| `egui::Color32::from_hex` | 描画色（panel_fill / 文字色 / 選択色） | `#RGB` / `#RGBA` / `#RRGGBB` / `#RRGGBBAA` |
| `config_watcher::parse_hex_color` | tao のネイティブ背景ブラシ（白フラッシュ回避） | **`#RRGGBB` のみ**（`#` 必須・6 桁・alpha 255 固定） |

`background_color = "#FFF"` のとき前者は白・後者は `None`→0x282828 へフォールバックする。**snapshot が `Color32` だけを持つ形にすると、ネイティブブラシがこの差のぶん黙って挙動を変える**——テストの無い白フラッシュ経路であり、本 PR の目的（lock の束ね）と無関係な変更である。ゆえに snapshot は main 向けに**変化したときだけ hex 文字列**を返し、main は従来どおり `parse_hex_color` を使う。

この乖離自体は既存の潜在バグ（`#FFF` 指定時に panel_fill とネイティブブラシが食い違う）だが、**本 PR では直さず follow-up issue とする**。

## テストの位置づけ（AGENTS.md ステップ 9 への回答）

純関数写像を切り出すため、**この PR は spec §6 が「cargo test で守れる」と書いた当のものを実現する**。

1. `visual.rs` のユニットテスト（下記 Task 2 Step 4。**本 PR の主たる green の根拠**）
2. `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra`
3. `npm run governance:check`（新規ファイル追加のため**必須**・#629 / #630 の再発防止）
4. `npm run smoke:egui -- -ResultsQuery <letter>`（実機・非回帰）
5. 実機目視 4 点（本 PR 向けに調整。Task 4）

---

### Task 1: `show_icons` の 5 回目の lock を潰す（引数化）

**なぜ最初か:** spec 決定 4 が「**必須条件**」と名指しした項目であり、単独でコンパイルが通り、単独でレビューできる。ここを残したまま snapshot を入れると「束ねた読み」と「はぐれた読み」が 1 フレーム内で食い違う——修正が新しいフレーム内不整合を作る。

**現状（実測）**: `ResultsView::update()` の config lock は **5 回**。`:401` font_family / `:408` `row_theme` / `:409` `read_metrics` / `:410` `show_icons` / **`:121` `show_icons`（`request_icons_for_results` 冒頭）**。`:410` で読んだ値が `:121` で読み直されている。

**Files:**
- Modify: `src-tauri/src/egui_shell/results_view.rs`
- Test: なし（この Step は純粋な引数の付け替え。守りはコンパイラ）

**Interfaces:**
- Produces: `ResultsView::request_icons_for_results(&mut self, rows: &[SearchResult], show_icons: bool, ctx: &egui::Context)`

- [ ] **Step 1: シグネチャに `show_icons` を足し、冒頭の読みを引数へ差し替える**

`results_view.rs:120-123` を置き換える:

```rust
    fn request_icons_for_results(
        &mut self,
        rows: &[SearchResult],
        show_icons: bool,
        ctx: &egui::Context,
    ) {
        // #673 / spec 決定 4: **ここで config を読み直さない。** 呼び出し側（update()）が
        // フレーム冒頭で読んだ値を渡す——同一フレーム内で 2 度読むと、間に config_watcher の
        // 適用が挟まったとき「アイコンを積むかどうか」と「アイコン枠を描くかどうか」が
        // 食い違う（描画は枠を出したのに抽出は走らない、の 1 フレーム）。
        if !show_icons {
            return;
        }
```

- [ ] **Step 2: 呼び出し側を直す**

`request_icons_for_results` の呼び出し（`ResultsView::update()` 内）に、`:410` で読んだ `show_icons` を渡す。

Run: `grep -n "request_icons_for_results" src-tauri/src/egui_shell/results_view.rs`
で呼び出し行を特定してから編集する（行番号は本計画作成時点のもので、編集でずれる）。

- [ ] **Step 3: 検証**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: 警告 0（引数を渡し忘れればコンパイルエラーになる）

Run: `cargo test -p snotra`
Expected: 既存件数のまま pass（140 passed）

- [ ] **Step 4: `show_icons` の残存読み回数を数える**

Run: `grep -n "show_icons(" src-tauri/src/egui_shell/results_view.rs`
Expected: 自由関数の**定義 1 行**と、`update()` からの**呼び出し 1 行**のみ（`:121` の読みが消えている）。

- [ ] **Step 5: コミット**

```
refactor: #673 results の show_icons 5 回目の lock を引数化で潰す

request_icons_for_results が冒頭で config を読み直していた（同一フレームで 2 度読み）。
spec 決定 4 が read_visual の必須条件として名指しした箇所——束ねた読みとはぐれた読みが
1 フレーム内で食い違うと、アイコン枠は描いたのに抽出は走らない状態が生じる。
```

---

### Task 2: `visual.rs` 新設と全読み取り点の移行

**なぜ 1 タスクか:** PR A′ Task 1 と同じ理由。`-D warnings` 下で未使用の新 API は `dead_code` エラーになり、旧アクセサ（`row_theme` / `show_icons`）を残せば導出式が 2 箇所になる。レビュアーが片方だけ棄却できる境界が無い。

**Files:**
- Create: `src-tauri/src/egui_shell/visual.rs`
- Modify: `src-tauri/src/egui_shell/mod.rs`（`mod` 宣言 + re-export + `read_visual`）
- Modify: `src-tauri/src/egui_shell/results_view.rs`（`RowTheme` / `row_theme` / `show_icons` の撤去、`update()` の移行）
- Modify: `src-tauri/src/egui_shell/view.rs`（5 箇所の移行）
- Test: `src-tauri/src/egui_shell/visual.rs` の `#[cfg(test)] mod tests`

**Interfaces:**
- Produces:
  - `pub(crate) struct VisualSnapshot`（フィールドは下記）
  - `pub(crate) struct RowTheme`（`results_view.rs` から移設）
  - `pub(crate) struct VisualApplied<'a>`
  - `pub(crate) fn visual_snapshot(v: &VisualConfig, show_icons: bool, applied: VisualApplied) -> VisualSnapshot`（**純関数**）
  - `pub(crate) fn read_visual(app: &tauri::AppHandle, applied: VisualApplied) -> VisualSnapshot`（`mod.rs`・lock を取る薄い皮）
- Consumes: Task 1 の `request_icons_for_results(rows, show_icons, ctx)`

- [ ] **Step 1: 移行前に「読む時刻」の監査を書き出す**

**PR A′ の教訓を明示的に 1 度走らせる。** A′ は「冗長に見えた読みが、**いつ読むか**によって荷重を持っていた」ために壊れた。本 PR は 1 フレーム内の読みを時間方向に束ねる。移行する 9 箇所それぞれについて、「この読みが**後で**行われることに依存しているものがあるか」を 1 行で答え、答えを計画のこの欄に書き込んでから実装に移る。

| # | 箇所 | 読む値 | 後で読むことへの依存 |
|---|---|---|---|
| 1 | `view.rs:1025` `read_metrics` | font_size / row_padding / bar_padding | （記入する） |
| 2 | `view.rs:1097-1132` | background / input_bg / selection / font_family | （記入する） |
| 3 | `view.rs:1317` `row_theme` | text / hint / selection / font_size | （記入する） |
| 4 | `view.rs:1410-1418`（**条件付き**: overlay 表示時のみ） | input_bg / hint | （記入する） |
| 5 | `view.rs:1443` `row_theme`（**条件付き**: toast 表示時のみ） | 同 3 | （記入する） |
| 6 | `results_view.rs:399-407` | font_family | （記入する） |
| 7 | `results_view.rs:408` `row_theme` | 同 3 | （記入する） |
| 8 | `results_view.rs:409` `read_metrics` | 同 1 | （記入する） |
| 9 | `results_view.rs:410` `show_icons` | show_icons | （記入する） |

**とくに 4 と 5 を疑う**——現在は overlay / toast が存在するときだけ、しかも `ctx.set_visuals`（`:1113`）より**後**で読まれている。「先に読むと何かが変わるか」を確かめること。全部 clear だと予想するが、**予想を根拠にしない。**

- [ ] **Step 2: `visual.rs` を新規作成する（型と純関数写像）**

```rust
//! テーマ（config `[visual]` + `appearance.show_icons`）の 1 フレーム分の読み取り値と、
//! その純粋な導出（#673 spec 決定 4）。
//!
//! **なぜ束ねるか**: 従来は 1 フレームの中で `row_theme`（文字色・font_size）と
//! `read_metrics`（font_size・padding）が別々に lock を取っていた。その間に
//! `config_watcher` の `update_config` が挟まると、**新しい font_size を旧い行高で描く
//! 1 フレーム**が生じる。次フレームで自然に直る cosmetic な窓だが、同じ値を 2 度読む
//! 構造そのものが窓の原因である。
//!
//! **保持しないこと**: `VisualSnapshot` の寿命は 1 フレームである。`self.` へ持つと
//! config 変更が反映されなくなる（毎フレーム live-read 方針・#576 / #646 決定 2）。

use snotra_core::config::VisualConfig;

use crate::egui_shell::layout::{self, Metrics};

/// 1 結果行の描画テーマ（`results_view.rs` から移設）。main のバー行・toast 行も同じ型を使う。
pub(crate) struct RowTheme {
    pub name_color: egui::Color32,
    pub path_color: egui::Color32,
    pub selection: egui::Color32,
    pub name_size: f32,
    pub path_size: f32,
}

/// 呼び出し側が既に適用済みの値。**guard 内で比較し、変化したときだけ clone する**ための入力。
pub(crate) struct VisualApplied<'a> {
    /// 各窓の `applied_font_family`（ctx は窓ごとに独立ゆえ main と results で別々に持つ）。
    pub font_family: &'a str,
    /// main の `applied_background_hex`。**results は `None` を渡す**——ネイティブ背景ブラシの
    /// 追従は main だけが行うため、比較そのものを行わない。
    pub background_hex: Option<&'a str>,
}

/// 1 フレーム分のテーマ値。main と results の要求の**和集合**である（窓ごとの projection に
/// 分けない——分けると導出式が再び 2 箇所になる・spec 決定 4）。
pub(crate) struct VisualSnapshot {
    /// パネル背景（main のみ使用）。
    pub background: egui::Color32,
    /// TextEdit / overlay 背景（main のみ使用）。
    pub input_bg: egui::Color32,
    /// 選択色（main の `visuals.selection.bg_fill` と行の選択帯）。
    pub selection: egui::Color32,
    /// ヒント文字色（main の overlay・行の path）。
    pub hint: egui::Color32,
    /// 行テーマ（main のバー・toast、results の各行）。
    pub row: RowTheme,
    /// 行高・バー高・toast 高・バー内余白。
    pub metrics: Metrics,
    /// アイコン枠を描くか（results のみ使用）。
    pub show_icons: bool,
    /// `applied.font_family` と異なるときだけ `Some`。呼び出し側は Some のときだけ
    /// `configure_japanese_font` を呼び、`applied` を更新する。
    pub font_family_changed: Option<String>,
    /// `applied.background_hex` と異なるときだけ `Some`（`None` を渡したときは常に `None`）。
    /// main はこの hex を `config_watcher::parse_hex_color` に食わせる——**描画色の
    /// `egui::Color32::from_hex` とは受理する形が違うため、統合しない**（計画の
    /// 「2 つのパーサを残す理由」）。
    pub background_hex_changed: Option<String>,
}

/// `VisualConfig` から 1 フレーム分の値を導く**純関数**（lock を持たない・テスト対象）。
///
/// フォールバックは `VisualConfig::default()` が正本である（リテラルを再手打ちしない・
/// `read_metrics` と同方針）。hex の parse に失敗した色だけが既定色へ落ちる。
pub(crate) fn visual_snapshot(
    v: &VisualConfig,
    show_icons: bool,
    applied: VisualApplied<'_>,
) -> VisualSnapshot {
    let d = VisualConfig::default();
    let text = hex_or(&v.text_color, &d.text_color);
    let hint = hex_or(&v.hint_text_color, &d.hint_text_color);
    let selection = hex_or(&v.selected_row_color, &d.selected_row_color);
    VisualSnapshot {
        background: hex_or(&v.background_color, &d.background_color),
        input_bg: hex_or(&v.input_background_color, &d.input_background_color),
        selection,
        hint,
        row: RowTheme {
            name_color: text,
            path_color: hint,
            selection,
            name_size: v.font_size as f32,
            // 正本は layout（行高が同じ係数で積算するため。二重定義は行高と描画の不一致）。
            path_size: layout::path_size(v.font_size) as f32,
        },
        // 導出式を書き写さない——行高の正本は Metrics::from_config である。
        metrics: Metrics::from_config(v.font_size, v.row_padding, v.bar_padding),
        show_icons,
        font_family_changed: (v.font_family != applied.font_family)
            .then(|| v.font_family.clone()),
        background_hex_changed: applied
            .background_hex
            .filter(|a| *a != v.background_color)
            .map(|_| v.background_color.clone()),
    }
}

/// `#RRGGBB` 等を `Color32` へ。parse 失敗時は既定値（の parse 結果）へ落ちる。
/// 既定値まで parse に失敗することは無い（`config.rs` の `default_*` は妥当な 6 桁 hex）が、
/// release は `panic="abort"` ゆえ unwrap しない——最後は黒へ落とす。
fn hex_or(s: &str, default_hex: &str) -> egui::Color32 {
    egui::Color32::from_hex(s)
        .or_else(|_| egui::Color32::from_hex(default_hex))
        .unwrap_or(egui::Color32::BLACK)
}
```

**フォールバックの統一で 1 つだけ挙動が変わる。** 現行 `row_theme` の手書きフォールバックは選択色が `#333333` だが、config の既定（`config.rs` の `default_selected_row_color`）は **`#505050`** である。派生コピーのドリフトであり、SSOT は `config.rs` ゆえ `#505050` へ揃える。**この差は計画に明記し、PR 本文でも述べる**（他の 4 色 `#282828` / `#383838` / `#E0E0E0` / `#808080` は既定と一致しており変化しない・実測済み）。

- [ ] **Step 3: `mod.rs` に `read_visual` を置く**

`mod.rs` の `mod` 宣言群へ `mod visual;` を足し、re-export を書く:

```rust
// view.rs / results_view.rs が毎フレームの描画で消費する（#673 spec 決定 4）。
pub(crate) use visual::{RowTheme, VisualApplied, VisualSnapshot};
```

`read_metrics` の**すぐ下**に置く（対の projection であることを並びで示す）:

```rust
/// 1 フレーム分のテーマ値を **lock 1 回**で読み切る（#673 spec 決定 4）。導出は純関数
/// `visual::visual_snapshot` が持ち、この関数は lock と AppState 不在の面倒だけを見る。
///
/// **`read_metrics` は残す**（統合しない）——`show_egui_main` が show 経路で高さだけを要り、
/// 色 parse を払わせないため。両者とも `Metrics::from_config` を正本とするので導出は 1 つ。
///
/// AppState 不在（setup 完了前の理論経路のみ）は `VisualConfig::default()` から導出する。
pub(crate) fn read_visual(
    app: &tauri::AppHandle,
    applied: VisualApplied<'_>,
) -> VisualSnapshot {
    match app.try_state::<crate::AppState>() {
        Some(s) => {
            let engine = s.engine.lock().unwrap();
            let config = engine.config();
            // guard 内で行うのは hex parse と算術と &str 比較まで。I/O や重い確保を足さないこと。
            visual::visual_snapshot(&config.visual, config.appearance.show_icons, applied)
        }
        None => visual::visual_snapshot(
            &snotra_core::config::VisualConfig::default(),
            // `AppearanceConfig` には `Default` 実装が無い（`VisualConfig` にはある）ため、
            // ここだけは既定値を型から導けずリテラルになる。SSOT は `snotra-core` の
            // `config.rs::default_show_icons`（= true）であり、現行 `show_icons()` の
            // `.unwrap_or(true)` と同値——挙動は変わらない。
            true,
            applied,
        ),
    }
}
```

**注意 1**: `engine.config()` の戻り値の借用が guard 生存中であることを確認する。現行 `read_metrics` と同じ形（`let engine = s.engine.lock().unwrap(); let v = &engine.config().visual;`）に倣うこと。

**注意 2（実測済み）**: `AppearanceConfig` は `Default` を実装していない（`derive` は `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`）。**`AppearanceConfig::default()` と書くとコンパイルできない。** `VisualConfig` は `impl Default` を持つ（`config.rs`）ので、そちらはそのまま使える。

- [ ] **Step 4: `visual.rs` にユニットテストを書く**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> VisualConfig {
        VisualConfig::default()
    }

    /// **本 PR が存在する理由の不変条件**（spec 決定 4 の効果）: 1 つの snapshot の中で、
    /// 行高（metrics）と文字サイズ（row）が**同じ font_size** から導かれる。
    /// 別々に lock を取っていた頃は、間に config 適用が挟まると新 font を旧行高で描く
    /// 1 フレームが生じえた。
    #[test]
    fn metrics_and_row_sizes_derive_from_the_same_font_size() {
        let mut v = cfg();
        v.font_size = 24;
        let s = visual_snapshot(&v, true, VisualApplied { font_family: &v.font_family, background_hex: None });
        assert_eq!(s.row.name_size, 24.0);
        assert_eq!(s.row.path_size, layout::path_size(24) as f32);
        let m = Metrics::from_config(24, v.row_padding, v.bar_padding);
        assert_eq!(s.metrics.row_height, m.row_height);
        assert_eq!(s.metrics.bar_height, m.bar_height);
    }

    /// hex の parse 失敗は既定色へ落ちる（既定リテラルの再手打ちを持たない）。
    #[test]
    fn invalid_hex_falls_back_to_config_default() {
        let mut v = cfg();
        v.text_color = "not-a-color".into();
        let s = visual_snapshot(&v, true, VisualApplied { font_family: "", background_hex: None });
        let d = VisualConfig::default();
        assert_eq!(s.row.name_color, egui::Color32::from_hex(&d.text_color).unwrap());
    }

    /// font_family は変化フレームだけ clone する（毎フレームの String 確保を避ける）。
    #[test]
    fn font_family_reported_only_when_changed() {
        let v = cfg();
        let same = visual_snapshot(&v, true, VisualApplied { font_family: &v.font_family, background_hex: None });
        assert!(same.font_family_changed.is_none());
        let diff = visual_snapshot(&v, true, VisualApplied { font_family: "Other", background_hex: None });
        assert_eq!(diff.font_family_changed.as_deref(), Some(v.font_family.as_str()));
    }

    /// 背景 hex は main だけが比較する。results は None を渡し、常に None が返る
    /// （**`Some("")` を渡すと毎フレーム「変化した」と誤報する**——比較しない意図を型で表す）。
    #[test]
    fn background_hex_change_is_opt_in() {
        let v = cfg();
        let opt_out = visual_snapshot(&v, true, VisualApplied { font_family: "", background_hex: None });
        assert!(opt_out.background_hex_changed.is_none());
        let unchanged = visual_snapshot(&v, true, VisualApplied { font_family: "", background_hex: Some(&v.background_color) });
        assert!(unchanged.background_hex_changed.is_none());
        let changed = visual_snapshot(&v, true, VisualApplied { font_family: "", background_hex: Some("#000000") });
        assert_eq!(changed.background_hex_changed.as_deref(), Some(v.background_color.as_str()));
    }
}
```

Run: `cargo test -p snotra visual::` — Expected: 4 本 pass。

- [ ] **Step 5: `results_view.rs` を移行する**

1. `RowTheme` の定義（`results_view.rs:353-361` 付近）を削除し、`crate::egui_shell::RowTheme` を使う
2. `row_theme` 自由関数（`:175-192`）と `show_icons` 自由関数（`:199-205`）と `hex_color`（`:195-197`）を削除する
3. `update()` の `:399-410` を snapshot 1 回に置き換える:

```rust
        let visual = crate::egui_shell::read_visual(
            &self.app_handle,
            crate::egui_shell::VisualApplied {
                font_family: &self.applied_font_family,
                // ネイティブ背景ブラシの追従は main の責務——results は比較しない。
                background_hex: None,
            },
        );
        // font_family hot-reload（ctx は窓ごとに独立ゆえ main 側の適用はこの窓に効かない）。
        if let Some(name) = &visual.font_family_changed {
            crate::egui_shell::view::configure_japanese_font(ui.ctx(), name);
            self.applied_font_family = name.clone();
        }
        let theme = &visual.row;
        let metrics = &visual.metrics;
        let show_icons = visual.show_icons;
```

4. `request_icons_for_results(rows, show_icons, ctx)` へ `visual.show_icons` を渡す（Task 1 で引数化済み）
5. `draw_row` などへ渡している `theme` / `metrics` の型・参照を合わせる

- [ ] **Step 6: `view.rs` を移行する**

1. `:1025` の `read_metrics` を snapshot 取得へ置き換える:

```rust
        let visual = crate::egui_shell::read_visual(
            &self.app_handle,
            crate::egui_shell::VisualApplied {
                font_family: &self.applied_font_family,
                background_hex: Some(&self.applied_background_hex),
            },
        );
        let metrics = &visual.metrics;
```

2. `:1097-1132` のブロックを snapshot 経由へ置き換える。**`if let Some(s) = try_state` の枠は外す**——snapshot は AppState 不在でも既定値を返すため、visuals は常に適用される（従来は AppState 不在時に visuals をまったく設定しなかった。AppState は常に manage されているので実挙動は不変）:

```rust
        let mut visuals = ctx.style_of(ctx.theme()).visuals.clone();
        visuals.panel_fill = visual.background;
        visuals.window_fill = visuals.panel_fill;
        visuals.extreme_bg_color = visual.input_bg; // TextEdit 背景
        visuals.selection.bg_fill = visual.selection;
        ctx.set_visuals(visuals);

        // SU6 spec 決定 2: font_family hot-reload。applied は解決成否に依らず無条件更新。
        if let Some(name) = &visual.font_family_changed {
            self.applied_font_family = name.clone();
            configure_japanese_font(&ctx, name);
            ctx.request_repaint(); // set_fonts は次フレーム適用——欠くと新フォントが 1 イベント遅れる
        }

        // SU6 spec 決定 2: native 背景ブラシ追従。**描画色とは別のパーサを使う**
        //（`parse_hex_color` は `#RRGGBB` 厳格。計画「2 つのパーサを残す理由」）。
        if let Some(hex) = &visual.background_hex_changed {
            self.applied_background_hex = hex.clone();
            if let Some(window) = self.app_handle.get_window("main") {
                let color = crate::config_watcher::parse_hex_color(hex)
                    .unwrap_or(tauri::window::Color(0x28, 0x28, 0x28, 0xff));
                let _ = window.set_background_color(Some(color));
            }
        }
```

3. `:1317` `let bar_theme = results_view::row_theme(&self.app_handle);` → `let bar_theme = &visual.row;`
4. `:1410-1418` の overlay 用 2 色 → `visual.input_bg` / `visual.hint`（`hex_color` 呼び出しも消える）
5. `:1443` `let theme = results_view::row_theme(&self.app_handle);` → `let theme = &visual.row;`
6. `view.rs` の `hex_color` ヘルパーが未使用になれば削除する（`grep -n "hex_color" src-tauri/src/egui_shell/view.rs` で残存を確認してから）

**借用に注意**: `visual` はフレーム冒頭のローカルであり、`&mut self` を取るメソッド呼び出しと寿命が重なる。E0502 が出たら、`visual` から必要な値を先に `Copy` で取り出す（`Color32` / `f32` / `f64` はすべて `Copy`）か、`RowTheme` を `Clone` にして渡す。**`self.` へ保持して回避してはならない**（Global Constraints）。

- [ ] **Step 7: 検証**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: 警告 0。旧アクセサを削除しているため移行漏れはコンパイルエラーになる。

Run: `cargo test -p snotra`
Expected: **144 passed**（140 + 新規 4）

- [ ] **Step 8: lock 回数を数える**

Run: `grep -n "try_state::<crate::AppState>" src-tauri/src/egui_shell/results_view.rs`
Expected: `setup()` の 1 箇所（スコープ外）と worker（`spawn_icon_load`）の 1 箇所のみ。`update()` 経路からは 0。

Run: `grep -rn "row_theme\|fn show_icons" src-tauri/src/egui_shell/`
Expected: **0 件**。**新しい doc コメントにこれらの識別子を書かないこと**——書くと grep が 0 にならない（PR A / A′ で 2 度踏んだ自己参照の罠。3 度目を作らない）。

- [ ] **Step 9: コミット**

```
refactor: #673 テーマの読みを 1 フレーム 1 lock へ束ねる（read_visual）

visual.rs に VisualSnapshot と純関数写像を新設し、main / results が毎フレーム
1 回だけ config lock を取る形にする。row_theme が毎フレーム String を 3 本 clone
してから lock の外で parse していたのを、guard 内 parse に変えて確保を消す。

- 行高（Metrics::from_config）と文字サイズ（row）が同じ font_size から導かれることを
  ユニットテストで固定する（本 PR が存在する理由の不変条件）
- フォールバックは VisualConfig::default() を正本に統一。selected_row_color の
  手書きフォールバック #333333 は既定 #505050 とドリフトしていたため揃える
- 描画色（egui の from_hex）とネイティブ背景ブラシ（parse_hex_color・#RRGGBB 厳格）の
  2 パーサは統合しない。統合すると白フラッシュ経路の挙動が黙って変わる
```

---

### Task 3: 文書の同期

**Files:**
- Modify: `src-tauri/CLAUDE.md`（`egui_shell/` のファイル索引 + 毎フレーム読みの不変条件）

- [ ] **Step 1: モジュール索引に `visual.rs` を足す**

`src-tauri/CLAUDE.md`「モジュール構成」の `egui_shell/` 箇条書きの構成列挙へ `visual.rs` を追加し、責務列挙にも 1 句足す（責務の散文の正本はファイルの `//!`・#562）。

- [ ] **Step 2: 毎フレーム読みの不変条件を書く**

`egui_shell/` の「イベント駆動 wake の不変条件」の隣に、次を足す:

```markdown
  - **テーマの読みは 1 フレーム 1 回（#673 spec 決定 4）**: `read_visual` が返す `VisualSnapshot` を各 view の `update()` 冒頭で 1 回だけ取り、以降はそれを読む。**同じ値を後段で config から読み直さない**——間に `config_watcher` の適用が挟まると、同じフレームの中で新旧が混ざる（新 font_size を旧行高で描く等）。**snapshot を `self.` へ保持してもならない**（毎フレーム live-read が config 変更の反映経路そのもの・#576 / #646 決定 2）。導出式の正本は `layout::Metrics::from_config` と `layout::path_size` であり、guard 内からそれを呼ぶ
```

- [ ] **Step 3: ガバナンス検査**

Run: `npm run governance:check`
Expected: G1..G10 passed。**新規ファイルを含む PR では PR 作成前に必ず走らせる**（#629 / #630 の同型再発）。

- [ ] **Step 4: コミット**

```
docs: #673 visual.rs をモジュール索引へ追加し、1 フレーム 1 lock の不変条件を書く
```

---

### Task 4: 検証（実機）

**Files:** なし

- [ ] **Step 1: GUI smoke（非回帰）**

Run: `npm run smoke:egui -- -ResultsQuery <索引に当たる 1 文字>`
Expected: PASS（`egui_results:show` / `:hide` の観測 + PR A′ で足した orphan 検出）。

**注意**: 実機のフォアグラウンドへキーを注入する。実行前に他の操作を止めること。

- [ ] **Step 2: 実機目視 4 点（**本 PR 向け**。A′ の 4 点の流用ではない）**

本 PR が触るのは「テーマ値がどう読まれるか」ゆえ、**config を実際に書き換えて live-reload を見る**のが要点である。`config.toml` を編集（または設定画面で変更）して:

1. **`font_family` 変更が両窓に即時反映される**（ctx は窓ごとに独立ゆえ、main だけ変わって results が変わらない失敗があり得る）
2. **色の変更（`text_color` / `selected_row_color` / `input_background_color`）が即時反映される**
3. **`show_icons` の切り替えでアイコン枠が畳まれ、かつアイコン抽出も止まる/再開する**（Task 1 の引数化がここに効く）
4. **`background_color` の変更にネイティブ背景ブラシが追従する**（hide → hotkey で再表示したときに白フラッシュが出ない）。**Task 2 の 2 パーサ判断がここに着地する**——`#RRGGBB` 形式で確認すること

- [ ] **Step 3: 結果を PR 本文へ記録する**

「追加/更新テスト名 + 検証した不変条件」（AGENTS.md ステップ 9）。未実施があれば未実施と書く。

- [ ] **Step 4: follow-up issue を立てる**

`background_color` に `#FFF` 等の 3 桁 hex を書くと、panel_fill（egui parser）とネイティブ背景ブラシ（`parse_hex_color`）が食い違う——既存の潜在バグ。本 PR では直さないため issue にする（本文に本計画の「2 つのパーサを残す理由」を引く）。
