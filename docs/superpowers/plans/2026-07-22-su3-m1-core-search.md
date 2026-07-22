# SU3 M1（core 検索）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** egui メインウィンドウの placeholder view を、直 `Engine` 呼びの実検索 UI（結果リスト・キーボードナビ・起動・動的高さ）へ置き換える。SU3 spec の M1（背骨 + core）を実装する。

**Architecture:** functional core + imperative shell。純粋 `SearchState`（+ `interpret`/`compute_window_height`/`Debouncer`）が状態と遷移を持ち egui/Win32 非依存でユニットテストする。`SearchWindowView`（driver）は egui 描画・`AppState.engine` 同期呼び・window 制御を担い、結果を core へ注入する。SU2 の外殻（show/hide/blur/位置）は不変で、`update` の内側だけ作る。

**Tech Stack:** Rust / egui（immediate mode）/ softbuffer / Tauri v2（`tauri::Window`）/ `snotra-egui-runtime`（SU1）。

## Global Constraints

- **flag OFF 完全不変（G1）**: 追加は egui 経路（`SNOTRA_EGUI_MAIN=1`）のみ。WebView2 経路・IPC コマンド（`search`/`get_history_results` 等）・`tauri.conf.json`・E2E 注入を一行も触らない。`SNOTRA_EGUI_MAIN` 未設定で `smoke:startup` / `e2e:tauri` 緑。
- **view/Win32 はユニットテスト前提にしない**（`.claude/rules/src-tauri.md`）。テスト対象は純粋核（`search_state.rs`/`layout.rs`）。driver は clippy + build + trace スモークで検証。
- **IPC コマンドを削除しない**。SU3 は egui view に直 Engine 呼びを足すだけ。
- **font-first 不変条件を壊さない**（`view.rs` の `configure_japanese_font` の jp_font index 0・#579）。
- **モード判定は `view_kind()`/`interp()` 経由**（生フィールドを直接 if しない・ui ルール踏襲）。M1 では `view_kind()` は常に `Results`（folder は M2）。
- **選択・クリックは行 index で参照**（パス文字列を使わない・ui ルール）。
- 検証コマンドは `docs/build-commands.md` が SSOT。clippy: `cargo clippy -p snotra --all-targets`。テスト: `cargo test -p snotra`。純粋核は `cargo test -p snotra egui_shell::` で絞れる。
- レイアウト定数: `SEARCH_BAR_HEIGHT = 52.0`、`RESULT_ROW_HEIGHT = 30.0`、`RESULTS_PADDING = 8.0`、`UPDATE_TOAST_HEIGHT = 52.0`（toast は SU5 まで常に非表示）。debounce: search = leading + trailing 50ms、instant = 30ms trailing（M3）。

---

## File Structure

- **Create `src-tauri/src/egui_shell/search_state.rs`**（純粋核・unit-test）: `ViewKind`・`QueryIntent`・`is_instant_prefix`・`interpret`・`SearchState`（query/selected/results + 遷移）。
- **Create `src-tauri/src/egui_shell/layout.rs`**（純粋核・unit-test）: `HeightParams`・`compute_window_height`・`Debouncer`。
- **Modify `src-tauri/src/egui_shell/view.rs`**: `SearchWindowView` に `SearchState` + `Debouncer` を持たせ、TextEdit → 検索 dispatch → 結果リスト描画 → ナビ/起動 → 動的高さ を実装。
- **Modify `src-tauri/src/egui_shell/mod.rs`**: `search_state`/`layout` を mod 宣言・re-export。`EguiShellState` に `reset_pending: AtomicBool` を追加。`show_egui_main` が `reset_pending` を立てる。
- **Modify `snotra-egui-runtime/src/runtime.rs`**: `RuntimeFrame` に `set_size(w, h)` を追加（`hide_window` と同じ sanctioned チャネル。`apply_frame_commands` でイベントループスレッド上で `window.set_size` を適用）。**SU1 隣接の最小フック**。

---

## Task 1: `interpret` 純関数と QueryIntent 型

入力（query + prefix + view_kind）→ 意図（plain/command/instant）を副作用なしで返す。`ui/src/lib/interpretQuery.ts` の Rust 版。instant 判定・parse の SSOT。

**Files:**
- Create: `src-tauri/src/egui_shell/search_state.rs`
- Modify: `src-tauri/src/egui_shell/mod.rs`（`mod search_state;` + re-export）

**Interfaces:**
- Produces: `pub enum ViewKind { Results, Folder }`（`tool` は SU3.5）、`pub enum QueryIntent { Plain, Command, Instant { filter_name: String, instant_query: String } }`、`pub fn is_instant_prefix(raw_query: &str, prefix: &str) -> bool`、`pub fn interpret(raw_query: &str, prefix: &str, view_kind: ViewKind) -> QueryIntent`。

- [ ] **Step 1: `mod.rs` に module 宣言を追加**

`src-tauri/src/egui_shell/mod.rs` の先頭付近（既存 `mod lifecycle; mod view;` の隣）に追加:

```rust
mod layout;
mod search_state;
```

そして既存の `pub(crate) use lifecycle::{...}` の下に:

```rust
pub(crate) use search_state::{QueryIntent, SearchState, ViewKind, interpret, is_instant_prefix};
```

（`layout` の re-export は Task 3/4 で足す。この時点では `SearchState` は未定義だが Task 2 で作る。**Task 1 のコンパイルを通すため、この re-export 行は `interpret`/`is_instant_prefix`/`QueryIntent`/`ViewKind` のみにし、`SearchState` は Task 2 で足す**。よってこの Step では:）

```rust
pub(crate) use search_state::{QueryIntent, ViewKind, interpret, is_instant_prefix};
```

- [ ] **Step 2: 失敗するテストを書く**

`src-tauri/src/egui_shell/search_state.rs` を新規作成:

```rust
//! egui 検索ウィンドウの純粋状態核（#532 SU3）。query/選択/結果と 2 軸モード導出・遷移を
//! egui/Win32 非依存で持ち、driver（view.rs）から駆動される。ユニットテスト対象。

/// 軸1: モーダルビュースタック頂点の種類。M1 は Results のみ到達（Folder は M2、tool は SU3.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewKind {
    Results,
    Folder,
}

/// 軸2: 入力の意味。view_kind != Results のときは常に Plain。instant は parse 済みの
/// filter_name（コマンド名）/ instant_query（スペース以降）を持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryIntent {
    Plain,
    Command,
    Instant {
        filter_name: String,
        instant_query: String,
    },
}

/// instant 判定述語（instant 検出の SSOT）。空 prefix では false（全入力が instant 化するのを防ぐ）。
/// trimStart 規則もここに集約する。
pub fn is_instant_prefix(raw_query: &str, prefix: &str) -> bool {
    !prefix.is_empty() && raw_query.trim_start().starts_with(prefix)
}

/// 入力 → 意図。副作用なし。view_kind != Results は常に Plain（folder 中は非 plain 化しない）。
/// instant の parse（prefix 除去・スペース分割）を一箇所に集約する（DRY）。
pub fn interpret(raw_query: &str, prefix: &str, view_kind: ViewKind) -> QueryIntent {
    if view_kind != ViewKind::Results {
        return QueryIntent::Plain;
    }
    if is_instant_prefix(raw_query, prefix) {
        // starts_with(prefix) を通ったので trimmed[prefix.len()..] は char 境界。
        let input = &raw_query.trim_start()[prefix.len()..];
        match input.find(' ') {
            Some(idx) => QueryIntent::Instant {
                filter_name: input[..idx].to_string(),
                instant_query: input[idx + 1..].to_string(),
            },
            None => QueryIntent::Instant {
                filter_name: input.to_string(),
                instant_query: String::new(),
            },
        }
    } else if raw_query.trim_start().starts_with('/') {
        QueryIntent::Command
    } else {
        QueryIntent::Plain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_query_is_plain() {
        assert_eq!(interpret("firefox", "@", ViewKind::Results), QueryIntent::Plain);
    }

    #[test]
    fn slash_prefix_is_command() {
        assert_eq!(interpret("/r", "@", ViewKind::Results), QueryIntent::Command);
    }

    #[test]
    fn instant_prefix_without_space() {
        assert_eq!(
            interpret("@goog", "@", ViewKind::Results),
            QueryIntent::Instant { filter_name: "goog".into(), instant_query: String::new() }
        );
    }

    #[test]
    fn instant_prefix_with_space_splits_filter_and_query() {
        assert_eq!(
            interpret("@google rust egui", "@", ViewKind::Results),
            QueryIntent::Instant { filter_name: "google".into(), instant_query: "rust egui".into() }
        );
    }

    #[test]
    fn empty_prefix_never_instant() {
        // 空 prefix では全入力が instant 化しない（bootstrap 前など）。
        assert_eq!(interpret("@x", "", ViewKind::Results), QueryIntent::Plain);
        assert!(!is_instant_prefix("@x", ""));
    }

    #[test]
    fn folder_view_is_always_plain() {
        // folder 中は prefix/slash に関わらず plain（フォルダフィルタとして扱う）。
        assert_eq!(interpret("@x", "@", ViewKind::Folder), QueryIntent::Plain);
        assert_eq!(interpret("/r", "@", ViewKind::Folder), QueryIntent::Plain);
    }

    #[test]
    fn leading_whitespace_trimmed_for_detection() {
        assert_eq!(
            interpret("  @a", "@", ViewKind::Results),
            QueryIntent::Instant { filter_name: "a".into(), instant_query: String::new() }
        );
    }
}
```

- [ ] **Step 3: テストが落ちる/通ることを確認**

Run: `cargo test -p snotra egui_shell::search_state`
Expected: 実装は既に上記に含まれるため **PASS**（このタスクは純関数＋テストを同時に置く。Red を見たい場合は先にテストだけ貼って `interpret` を `todo!()` にして FAIL を確認してから本実装に差し替える）。

- [ ] **Step 4: clippy**

Run: `cargo clippy -p snotra --all-targets`
Expected: 警告なしで終了（沈黙 = 合格）。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/egui_shell/search_state.rs src-tauri/src/egui_shell/mod.rs
git commit -m "feat(su3): interpret 純関数と QueryIntent 型（M1 Task1）"
```

---

## Task 2: `SearchState` core（query/選択/結果 + 導出 + ナビ）

背骨の状態。M1 では results 軸のみ。set_query/set_results/view_kind/interp/選択ナビ（clamp）を持つ。

**Files:**
- Modify: `src-tauri/src/egui_shell/search_state.rs`
- Modify: `src-tauri/src/egui_shell/mod.rs`（re-export に `SearchState` 追加）

**Interfaces:**
- Consumes: `interpret`/`ViewKind`/`QueryIntent`（Task 1）、`snotra_core::ui_types::SearchResult`（`{ name, path, is_folder, is_error }`）。
- Produces: `SearchState`（`new()`/`query()->&str`/`set_query(String)`/`results()->&[SearchResult]`/`set_results(Vec<SearchResult>)`/`selected()->usize`/`view_kind()->ViewKind`/`interp(&str)->QueryIntent`/`move_selection(delta: i32)`/`reset()`）。

- [ ] **Step 1: 失敗するテストを書く**

`search_state.rs` の `use` に追加し、`tests` mod の上へ `SearchState` を追加（テストは `tests` mod に追記）:

```rust
use snotra_core::ui_types::SearchResult;

/// 検索ウィンドウの純粋状態。M1 は results 軸のみ（folder stack は M2）。
pub struct SearchState {
    query: String,
    results: Vec<SearchResult>,
    selected: usize,
}

impl SearchState {
    pub fn new() -> Self {
        Self { query: String::new(), results: Vec::new(), selected: 0 }
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn set_query(&mut self, q: String) {
        self.query = q;
    }

    pub fn results(&self) -> &[SearchResult] {
        &self.results
    }

    /// 結果を差し替える。選択は範囲内へクランプ（空なら 0）。
    pub fn set_results(&mut self, results: Vec<SearchResult>) {
        self.results = results;
        self.selected = clamp_selected(self.results.len(), self.selected);
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// 軸1。M1 は常に Results（Folder stack は M2）。
    pub fn view_kind(&self) -> ViewKind {
        ViewKind::Results
    }

    /// 軸2。prefix は driver が config live-read で渡す。
    pub fn interp(&self, prefix: &str) -> QueryIntent {
        interpret(&self.query, prefix, self.view_kind())
    }

    /// ↑↓ ナビ。delta 移動して結果範囲へクランプ（空なら 0 のまま・端で飽和）。
    pub fn move_selection(&mut self, delta: i32) {
        if self.results.is_empty() {
            self.selected = 0;
            return;
        }
        let max = self.results.len() as i32 - 1;
        let next = (self.selected as i32 + delta).clamp(0, max);
        self.selected = next as usize;
    }

    /// resetForShow 相当。show のたびに driver が呼ぶ（query/結果/選択を初期化）。
    pub fn reset(&mut self) {
        self.query.clear();
        self.results.clear();
        self.selected = 0;
    }
}

impl Default for SearchState {
    fn default() -> Self {
        Self::new()
    }
}

/// 選択 index を [0, len) へクランプ（len==0 は 0）。folderNav.clampSelectedIndex 相当。
pub(crate) fn clamp_selected(len: usize, idx: usize) -> usize {
    if len == 0 {
        0
    } else {
        idx.min(len - 1)
    }
}
```

`tests` mod に追記:

```rust
    fn res(name: &str) -> SearchResult {
        SearchResult { name: name.into(), path: format!("C:/{name}.exe"), is_folder: false, is_error: false }
    }

    #[test]
    fn set_results_clamps_selection() {
        let mut s = SearchState::new();
        s.set_results(vec![res("a"), res("b"), res("c")]);
        s.move_selection(2); // selected = 2
        assert_eq!(s.selected(), 2);
        s.set_results(vec![res("a")]); // 縮小 → クランプ
        assert_eq!(s.selected(), 0);
    }

    #[test]
    fn move_selection_saturates_at_bounds() {
        let mut s = SearchState::new();
        s.set_results(vec![res("a"), res("b")]);
        s.move_selection(-1); // 端で 0 飽和
        assert_eq!(s.selected(), 0);
        s.move_selection(1);
        assert_eq!(s.selected(), 1);
        s.move_selection(1); // 端で 1 飽和
        assert_eq!(s.selected(), 1);
    }

    #[test]
    fn move_selection_on_empty_is_zero() {
        let mut s = SearchState::new();
        s.move_selection(1);
        assert_eq!(s.selected(), 0);
    }

    #[test]
    fn reset_clears_everything() {
        let mut s = SearchState::new();
        s.set_query("abc".into());
        s.set_results(vec![res("a"), res("b")]);
        s.move_selection(1);
        s.reset();
        assert_eq!(s.query(), "");
        assert!(s.results().is_empty());
        assert_eq!(s.selected(), 0);
    }

    #[test]
    fn interp_reads_current_query() {
        let mut s = SearchState::new();
        s.set_query("@g".into());
        assert_eq!(s.interp("@"), QueryIntent::Instant { filter_name: "g".into(), instant_query: String::new() });
    }
```

- [ ] **Step 2: テストを実行して確認**

Run: `cargo test -p snotra egui_shell::search_state`
Expected: PASS（Red を見るには一時的に `move_selection` を `todo!()` にして FAIL 確認 → 本実装へ）。

- [ ] **Step 3: `mod.rs` の re-export に `SearchState` を追加**

`src-tauri/src/egui_shell/mod.rs` の re-export を更新:

```rust
pub(crate) use search_state::{QueryIntent, SearchState, ViewKind, interpret, is_instant_prefix};
```

- [ ] **Step 4: clippy**

Run: `cargo clippy -p snotra --all-targets`
Expected: 沈黙（合格）。`interpret`/`is_instant_prefix` が Task 2 時点で未使用でも re-export 済みなので警告は出ない。出たら `#[allow(unused)]` でなく Task 5 の view 統合まで待つ判断をコメントで残す。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/egui_shell/search_state.rs src-tauri/src/egui_shell/mod.rs
git commit -m "feat(su3): SearchState core（query/選択/結果/ナビ）（M1 Task2）"
```

---

## Task 3: `compute_window_height` 純関数

結果表示可否・行数からウィンドウ論理高さを算出。`ui/src/lib/windowHeight.ts` の Rust 版。

**Files:**
- Create: `src-tauri/src/egui_shell/layout.rs`
- Modify: `src-tauri/src/egui_shell/mod.rs`（`mod layout;` は Task1 で宣言済み。re-export 追加）

**Interfaces:**
- Produces: `pub struct HeightParams { show_results: bool, max_results: u32, has_update_toast: bool, search_bar_height: f64, result_row_height: f64, results_padding: f64, update_toast_height: f64 }`、`pub fn compute_window_height(p: &HeightParams) -> f64`。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/egui_shell/layout.rs` を新規作成:

```rust
//! egui 検索ウィンドウの純粋レイアウト/タイミングヘルパー（#532 SU3）。ウィンドウ高さ算出と
//! 検索 debounce の判定を egui/Win32 非依存で持つ。ユニットテスト対象。

/// compute_window_height の入力。SU5 まで has_update_toast は常に false。
pub struct HeightParams {
    pub show_results: bool,
    pub max_results: u32,
    pub has_update_toast: bool,
    pub search_bar_height: f64,
    pub result_row_height: f64,
    pub results_padding: f64,
    pub update_toast_height: f64,
}

/// ウィンドウ論理高さ。show_results なら bar + max*row + pad、否なら bar。toast は加算。
/// windowHeight.ts の computeWindowHeight と同一。
pub fn compute_window_height(p: &HeightParams) -> f64 {
    let content = if p.show_results {
        p.search_bar_height + p.max_results as f64 * p.result_row_height + p.results_padding
    } else {
        p.search_bar_height
    };
    content + if p.has_update_toast { p.update_toast_height } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(show: bool, max: u32) -> HeightParams {
        HeightParams {
            show_results: show,
            max_results: max,
            has_update_toast: false,
            search_bar_height: 52.0,
            result_row_height: 30.0,
            results_padding: 8.0,
            update_toast_height: 52.0,
        }
    }

    #[test]
    fn collapsed_is_search_bar_only() {
        assert_eq!(compute_window_height(&params(false, 8)), 52.0);
    }

    #[test]
    fn expanded_is_bar_plus_rows_plus_padding() {
        // 52 + 8*30 + 8 = 300
        assert_eq!(compute_window_height(&params(true, 8)), 300.0);
    }

    #[test]
    fn toast_adds_height() {
        let mut p = params(false, 8);
        p.has_update_toast = true;
        assert_eq!(compute_window_height(&p), 52.0 + 52.0);
    }
}
```

- [ ] **Step 2: mod.rs に re-export を追加**

`src-tauri/src/egui_shell/mod.rs`:

```rust
pub(crate) use layout::{HeightParams, compute_window_height};
```

- [ ] **Step 3: テストを実行**

Run: `cargo test -p snotra egui_shell::layout`
Expected: PASS。

- [ ] **Step 4: clippy**

Run: `cargo clippy -p snotra --all-targets`
Expected: 沈黙。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/egui_shell/layout.rs src-tauri/src/egui_shell/mod.rs
git commit -m "feat(su3): compute_window_height 純関数（M1 Task3）"
```

---

## Task 4: `Debouncer`（leading + trailing・決定7）

打鍵 coalescing。leading（バースト先頭で即時）+ trailing（interval 経過で再発火）。clock は driver が注入。search=leading+trailing、instant=trailing のみ（M3）。

**Files:**
- Modify: `src-tauri/src/egui_shell/layout.rs`
- Modify: `src-tauri/src/egui_shell/mod.rs`（re-export）

**Interfaces:**
- Produces: `pub struct Debouncer { interval: std::time::Duration, leading: bool, armed: bool }`、`Debouncer::new(interval, leading)`、`on_input(&mut self) -> bool`（leading 発火可否）、`poll(&mut self, elapsed_since_input: Duration) -> bool`（trailing 発火可否）、`interval(&self) -> Duration`。

- [ ] **Step 1: 失敗するテストを書く**

`layout.rs` の `HeightParams`/`compute_window_height` の下（tests mod の上）へ追加:

```rust
use std::time::Duration;

/// 打鍵 debounce（決定7）。時刻は driver が注入する（純粋・テスト可能）。
/// - on_input: 入力フレーム。leading（バースト先頭）なら true を返し、以後 armed。
/// - poll: 各フレーム。armed かつ elapsed>=interval で trailing 発火し disarm。
pub struct Debouncer {
    interval: Duration,
    leading: bool,
    armed: bool,
}

impl Debouncer {
    pub fn new(interval: Duration, leading: bool) -> Self {
        Self { interval, leading, armed: false }
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// 入力があったフレームで呼ぶ。leading 有効かつバースト先頭なら true（＝今すぐ検索）。
    /// いずれにせよ armed にして trailing を予約する。
    pub fn on_input(&mut self) -> bool {
        let fire_leading = self.leading && !self.armed;
        self.armed = true;
        fire_leading
    }

    /// 入力の無いフレームで呼ぶ。前回入力からの経過が interval 以上なら trailing 発火し disarm。
    pub fn poll(&mut self, elapsed_since_input: Duration) -> bool {
        if self.armed && elapsed_since_input >= self.interval {
            self.armed = false;
            return true;
        }
        false
    }
}
```

`tests` mod に追記:

```rust
    use std::time::Duration;

    #[test]
    fn leading_fires_on_burst_start_then_trailing() {
        let mut d = Debouncer::new(Duration::from_millis(50), true);
        assert!(d.on_input(), "バースト先頭は leading 発火");
        assert!(!d.on_input(), "連打中は leading 抑止");
        assert!(!d.poll(Duration::from_millis(30)), "50ms 未満は trailing 抑止");
        assert!(d.poll(Duration::from_millis(50)), "50ms 経過で trailing 発火");
        assert!(!d.poll(Duration::from_millis(100)), "発火後は disarm");
    }

    #[test]
    fn no_leading_mode_only_trailing() {
        let mut d = Debouncer::new(Duration::from_millis(30), false);
        assert!(!d.on_input(), "leading なしは即時発火しない");
        assert!(d.poll(Duration::from_millis(30)), "trailing のみ発火");
    }
```

- [ ] **Step 2: mod.rs の re-export を更新**

```rust
pub(crate) use layout::{Debouncer, HeightParams, compute_window_height};
```

- [ ] **Step 3: テスト実行**

Run: `cargo test -p snotra egui_shell::layout`
Expected: PASS。

- [ ] **Step 4: clippy + commit**

Run: `cargo clippy -p snotra --all-targets`（沈黙）

```bash
git add src-tauri/src/egui_shell/layout.rs src-tauri/src/egui_shell/mod.rs
git commit -m "feat(su3): Debouncer（leading+trailing・決定7）（M1 Task4）"
```

---

## Task 5: driver 統合 — TextEdit → 同期検索 dispatch

`SearchWindowView` に `SearchState` + search `Debouncer` を持たせ、TextEdit の変更を state へ流し、`results + plain + !indexing` のとき `engine.search` を同期呼びして結果を注入する。**この Task では debounce の leading のみ配線し（毎打鍵で即時 search）、trailing/`request_repaint_after` は Task 8**。結果リスト描画は Task 6（この Task では結果件数を trace で確認）。

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`

**Interfaces:**
- Consumes: `SearchState`/`QueryIntent`/`ViewKind`（Task 2）、`crate::AppState`（`engine: Mutex<Engine>`、`indexing: AtomicBool`）、`snotra_core::ui_types::SearchResult`。
- Produces: `SearchWindowView` が `state: SearchState` を持つ（M2/M3 が拡張）。

- [ ] **Step 1: `SearchWindowView` に state と debouncer を追加**

`view.rs` の struct 定義（現 `query: String` を持つ）を置換:

```rust
use std::time::{Duration, Instant};
use crate::egui_shell::{Debouncer, SearchState};

pub(crate) struct SearchWindowView {
    app_handle: tauri::AppHandle,
    was_focused: bool,
    unfocus_at: Option<Instant>,
    state: SearchState,
    search_debounce: Debouncer,
    last_input_at: Instant,
    // query フィールドは SearchState.query へ移譲（削除）。
}
```

`new()` を更新:

```rust
    pub(crate) fn new(app_handle: tauri::AppHandle) -> Self {
        Self {
            app_handle,
            was_focused: false,
            unfocus_at: None,
            state: SearchState::new(),
            search_debounce: Debouncer::new(Duration::from_millis(50), true),
            last_input_at: Instant::now(),
        }
    }
```

- [ ] **Step 2: config から instant prefix を live-read するヘルパーを追加**

`view.rs` の `impl SearchWindowView` に追加（既存 `auto_hide_enabled` と同型）:

```rust
    /// instant prefix を実行中 config から都度読む（キャッシュしない・#576 と同設計）。
    /// フィールドは config.search.instant_command_prefix（config.rs:956 で確認済み）。
    fn instant_prefix(&self) -> String {
        self.app_handle
            .try_state::<crate::AppState>()
            .map(|s| s.engine.lock().unwrap().config().search.instant_command_prefix.clone())
            .unwrap_or_else(|| "@".to_string())
    }

    /// index 構築中か（AppState.indexing: AtomicBool・state.rs:14 で確認済み）。
    fn indexing(&self) -> bool {
        self.app_handle
            .try_state::<crate::AppState>()
            .map(|s| s.indexing.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(false)
    }
```

- [ ] **Step 3: 検索 dispatch を実装（同期・leading のみ）**

`view.rs` の `impl SearchWindowView` に検索実行ヘルパーを追加:

```rust
    /// 現在の state.query に対して検索を実行し結果を注入する（同期・直 Engine）。
    /// results + plain + !indexing のみ通常検索。空クエリは結果クリア（§4.6）。
    /// instant/command/folder は M3/M2 で分岐を足す（現状 plain のみ実装）。
    fn run_search(&mut self) {
        let prefix = self.instant_prefix();
        match self.state.interp(&prefix) {
            QueryIntent::Plain => {
                if self.state.query().trim().is_empty() || self.indexing() {
                    self.state.set_results(Vec::new());
                    return;
                }
                let query = self.state.query().to_string();
                let results = {
                    let state = match self.app_handle.try_state::<crate::AppState>() {
                        Some(s) => s,
                        None => return,
                    };
                    let mut engine = state.engine.lock().unwrap();
                    engine.search(&query)
                }; // lock 解放
                self.state.set_results(results);
            }
            // command/instant は M2/M3。M1 では結果を出さない（空維持）。
            _ => {
                self.state.set_results(Vec::new());
            }
        }
    }
```

`use` に `crate::egui_shell::QueryIntent;` を追加。

- [ ] **Step 4: `update` の TextEdit を state 駆動に置換**

`view.rs` の `update` 末尾の placeholder TextEdit ブロック（`self.query` を使う箇所）を置換:

```rust
        // 検索入力欄。state.query を編集し、変化があれば debounce leading で同期検索。
        let mut buf = self.state.query().to_string();
        let response = ui.add(
            egui::TextEdit::singleline(&mut buf)
                .hint_text("検索…")
                .desired_width(f32::INFINITY),
        );
        if response.changed() {
            self.state.set_query(buf);
            self.last_input_at = Instant::now();
            if self.search_debounce.on_input() {
                self.run_search(); // leading（Task 8 で trailing を足す）
            }
        }
        // 窓に focus があるのに入力欄が focus を持たないなら移す（Alt+Q 直後に打てる）。
        if focused && !response.has_focus() {
            response.request_focus();
        }

        // 開発時の結果件数 trace（Task 6 でリスト描画に置換）。
        crate::trace_main(
            "egui_search:dispatch",
            serde_json::json!({ "query_len": self.state.query().chars().count(), "results": self.state.results().len() }),
        );
```

**注**: 旧 `self.query` を参照する全箇所を `self.state.query()` に更新。struct から `query` フィールドが消えたことでコンパイルエラーになる箇所を潰す。

- [ ] **Step 5: build + clippy**

Run: `cargo clippy -p snotra --all-targets`
Expected: 沈黙（合格）。エラーが出たらフィールド参照・型を修正。

- [ ] **Step 6: trace スモーク（手動・Win32 依存）**

Run（PowerShell・アプリを egui フラグ + trace で起動して打鍵・SSOT は `docs/build-commands.md`）:

```
$env:SNOTRA_EGUI_MAIN=1; $env:SNOTRA_TRACE=1; cargo run -p snotra
```

Expected: 検索欄に "firefox" 等を打つと `egui_search:dispatch` trace に `results` > 0 が出る（実インデックスにヒットがあれば）。`msedgewebview2.exe` 子孫 0。確認後アプリ終了。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/egui_shell/view.rs
git commit -m "feat(su3): TextEdit→同期検索 dispatch（leading・M1 Task5）"
```

---

## Task 6: 結果リスト描画（ScrollArea + 行 + 選択 + scroll 追従）

`state.results()` を ScrollArea で描き、行は `[アイコンスロット] 名前 · 淡色パス [フォルダバッジ]`。`state.selected()` 行をハイライトし `scroll_to_me`。マウス: シングルクリック＝起動要求（Task 7 で配線）、ダブルクリック＝選択更新。

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`

**Interfaces:**
- Consumes: `state.results()`/`state.selected()`（Task 2）。
- Produces: `update` 内でクリック行 index を捕捉する仕組み（`clicked_index: Option<usize>`・Task 7 が消費）。

- [ ] **Step 1: 行描画ヘルパーを追加**

`view.rs` に、1 行を描き「クリック種別」を返すヘルパーを追加:

```rust
    /// 1 行を描画。selected ならハイライト + scroll_to_me。
    /// 返り値: (single_clicked, double_clicked)。
    fn draw_result_row(
        &self,
        ui: &mut egui::Ui,
        index: usize,
        result: &SearchResult,
        selected: bool,
    ) -> (bool, bool) {
        let row_h = 30.0;
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), row_h),
            egui::Sense::click(),
        );
        if selected {
            ui.painter().rect_filled(rect, 4.0, ui.visuals().selection.bg_fill);
            response.scroll_to_me(Some(egui::Align::Center));
        }
        // アイコンスロット（SU4 が埋める）: 左に 24px 空ける。
        let text_x = rect.left() + 28.0;
        let name_color = ui.visuals().text_color();
        let path_color = ui.visuals().weak_text_color(); // 淡色パス
        let painter = ui.painter();
        painter.text(
            egui::pos2(text_x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            &result.name,
            egui::FontId::proportional(14.0),
            name_color,
        );
        // 名前の右にパスを淡色で（簡易・galley 省略は egui 既定に委ねる）。
        painter.text(
            egui::pos2(rect.right() - 8.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            &result.path,
            egui::FontId::proportional(11.0),
            path_color,
        );
        let _ = index;
        (response.clicked(), response.double_clicked())
    }
```

**注**: `SearchResult` の import（`use snotra_core::ui_types::SearchResult;`）を追加。淡色パスの右寄せ・省略は M1 では簡易実装（重なり回避の厳密なレイアウトは G-RESIZE 目視で調整）。フォルダバッジ（`is_folder`）は M2 で追加（M1 は通常検索でファイル主体のため後回し可）。

- [ ] **Step 2: `update` に結果リストを描画**

Task 5 で入れた trace ブロックを、リスト描画へ置換。TextEdit の後に:

```rust
        // 結果リスト（shouldShowResults 相当。M1: results 軸・plain のみ。空なら描かない）。
        let show_results = !self.state.results().is_empty();
        let mut clicked: Option<usize> = None;
        let mut dbl_clicked: Option<usize> = None;
        if show_results {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let selected = self.state.selected();
                for (i, result) in self.state.results().iter().enumerate() {
                    let (single, double) = self.draw_result_row(ui, i, result, i == selected);
                    if single {
                        clicked = Some(i);
                    }
                    if double {
                        dbl_clicked = Some(i);
                    }
                }
            });
        }
        // クリック処理は Task 7。ここでは捕捉のみ（未使用警告を避けるため Task 7 まで let _ =）。
        let _ = (clicked, dbl_clicked);
```

**注**: `self.state.results()` の借用と `self.draw_result_row(&self, ...)` の同時借用に注意。`draw_result_row` は `&self` を取るが、ループ内で `self.state.results()` を借用しつつ `self.draw_result_row` を呼ぶと二重借用になる。回避: 描画前に `let results = self.state.results().to_vec();`（clone）してループする、または `draw_result_row` を `&self` でなく関連関数（`Self::draw_result_row(ui, ...)`）にして `self` 借用を切る。**関連関数化を推奨**（`&self` を使わない描画にする）。

- [ ] **Step 3: build + clippy**

Run: `cargo clippy -p snotra --all-targets`
Expected: 沈黙。借用エラーが出たら Step 2 注の関連関数化で解消。

- [ ] **Step 4: trace/目視スモーク**

Run: `$env:SNOTRA_EGUI_MAIN=1; cargo run -p snotra`
Expected: 打鍵で結果リストが展開し、行に名前 + 淡色パスが出る。↑↓ はまだ効かない（Task 7）。`msedgewebview2.exe` 子孫 0。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/egui_shell/view.rs
git commit -m "feat(su3): 結果リスト描画（ScrollArea + 行 + 選択）（M1 Task6）"
```

---

## Task 7: キーボードナビ + 起動

↑↓ で `move_selection`、Enter/シングルクリックで選択項目を起動 → hide、ダブルクリックで選択更新（§4.8）。起動は `launch_item_core`（ShellExecuteW・ロック外）→ 成功なら全起動経路の共通末尾 `record_and_save`（履歴記録 + 保存）を再利用（DRY・§4.3/§5 の履歴 parity）。

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`
- Modify: `src-tauri/src/commands/launch.rs`（`record_and_save` を `fn` → `pub(crate) fn` に昇格。egui 経路から再利用）

**Interfaces:**
- Consumes: `state.move_selection`/`state.selected`/`state.results`（Task 2）、`crate::commands::launch::launch_item_core`（`pub(crate) fn launch_item_core(path: &str) -> LaunchResult`）、`crate::commands::launch::record_and_save(state: &AppState, path: &str, query: &str)`（本 Task で pub(crate) 化・launch.rs:84）、`crate::commands::launch::LaunchStatus`（`Ok` が成功・launch.rs:16）、`emit("egui-hide-requested")`（SU2 の hide 合流点）。
- Produces: 起動 → 履歴記録 → hide の配線。`launch_item_core` はエンジンロックを保持せず呼ぶ（ShellExecuteW・launch.rs:226 の制約）。

- [ ] **Step 1: `record_and_save` を pub(crate) 化**

`src-tauri/src/commands/launch.rs:84` の共通末尾を egui 経路から再利用できるよう昇格:

```rust
pub(crate) fn record_and_save(state: &AppState, path: &str, query: &str) {
```

（本体は不変。可視性のみ変更。`launch_item_core` は既に `pub(crate)`。）

- [ ] **Step 2: ↑↓ ナビを追加**

`update` の Escape 検出の近く（TextEdit の前）に、キー入力を拾って選択移動:

```rust
        // ↑↓ ナビ（結果があるとき）。TextEdit より前に ctx から拾い、入力欄 focus 中も効かせる。
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
            self.state.move_selection(1);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
            self.state.move_selection(-1);
        }
```

- [ ] **Step 3: 起動ヘルパーを追加（起動 → 履歴記録 → hide）**

`impl SearchWindowView` に:

```rust
    /// index 行を起動し、成功なら履歴記録して hide 要求を出す（§4.8 シングルクリック / Enter）。
    /// launch_item_core は ShellExecuteW（エンジンロック外で呼ぶ・launch.rs:226）。成功時のみ
    /// record_and_save で履歴を記録（§4.3/§5 の query_count 加点・全起動経路の共通末尾を再利用）。
    /// エラー行（is_error）は起動しない。
    fn activate(&self, index: usize) {
        use crate::commands::launch::{LaunchStatus, launch_item_core, record_and_save};
        let Some(result) = self.state.results().get(index) else { return };
        if result.is_error {
            return;
        }
        let path = result.path.clone();
        let query = self.state.query().to_string();
        let outcome = launch_item_core(&path); // ロック外・ShellExecuteW
        crate::trace_main(
            "egui_launch",
            serde_json::json!({ "index": index, "status": format!("{:?}", outcome.status) }),
        );
        if matches!(outcome.status, LaunchStatus::Ok) {
            if let Some(state) = self.app_handle.try_state::<crate::AppState>() {
                record_and_save(&state, &path, &query); // 履歴記録 + 保存（ロックは内部で最小保持）
            }
            // 起動成功時のみ hide（SU2 の hide 合流点へ・view から window を直接触らない）。
            self.emit_hide();
        }
    }
```

**注**: `record_and_save` は内部で `engine.lock()` → `record_launch` → `prepare_history_save_if_dirty(5)` → ロック外 `save()` を行う（launch.rs:84-94）。query は正規化前の生クエリを渡す（`record_launch` 内で `normalize_history_query_key` される・WebView2 経路と同じ）。M1 は plain のみゆえ `state.query()` が検索クエリそのもの（instant/folder の query 差異は M2/M3）。

- [ ] **Step 4: Enter / クリックを配線**

Task 6 の `let _ = (clicked, dbl_clicked);` を置換。加えて Enter 処理を追加:

```rust
        // Enter: 選択項目を起動（結果があるとき）。TextEdit の Enter より先に ctx で拾う。
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !self.state.results().is_empty() {
            self.activate(self.state.selected());
        }
        if let Some(i) = clicked {
            self.activate(i); // シングルクリック＝起動（§4.8）
        }
        if let Some(i) = dbl_clicked {
            // ダブルクリック＝選択更新のみ（起動しない）。single と両方立つ環境では
            // activate が先行しうるが、起動で hide するため実害なし（§4.8 は排他前提）。
            self.state.move_selection(i as i32 - self.state.selected() as i32);
        }
```

- [ ] **Step 5: build + clippy**

Run: `cargo clippy -p snotra --all-targets`
Expected: 沈黙。

- [ ] **Step 6: trace スモーク**

Run: `$env:SNOTRA_EGUI_MAIN=1; $env:SNOTRA_TRACE=1; cargo run -p snotra`
Expected: 打鍵 → ↑↓ で選択ハイライト移動 → Enter で `egui_launch` trace + アプリ起動 + ウィンドウ hide。クリックでも起動。`msedgewebview2.exe` 子孫 0。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/egui_shell/view.rs src-tauri/src/commands/launch.rs
git commit -m "feat(su3): キーボードナビ + 起動→履歴記録→hide（M1 Task7）"
```

---

## Task 8: debounce trailing 配線（request_repaint_after）

Task 5 の leading のみを leading + trailing 50ms に拡張。入力の無いフレームでも trailing 発火のため `request_repaint_after(50ms)` を積み、`poll` で経過判定して検索を回す。

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`

**Interfaces:**
- Consumes: `Debouncer::on_input`/`poll`/`interval`（Task 4）、`self.last_input_at`（Task 5）。

- [ ] **Step 1: 入力時に trailing を予約**

Task 5 の `if response.changed()` ブロックを更新:

```rust
        if response.changed() {
            self.state.set_query(buf);
            self.last_input_at = Instant::now();
            if self.search_debounce.on_input() {
                self.run_search(); // leading
            }
            // trailing 発火のため interval 後に再描画を要求する（SU2 blur と同じ egui idiom）。
            ctx.request_repaint_after(self.search_debounce.interval());
        }
```

- [ ] **Step 2: 毎フレーム poll で trailing 検索**

`update` の TextEdit 描画の後（結果リスト描画の前）に:

```rust
        // trailing debounce: 連打が収まって interval 経過したら最終クエリで検索し直す。
        if self.search_debounce.poll(self.last_input_at.elapsed()) {
            self.run_search();
        }
```

- [ ] **Step 3: build + clippy**

Run: `cargo clippy -p snotra --all-targets`
Expected: 沈黙。

- [ ] **Step 4: trace スモーク**

Run: `$env:SNOTRA_EGUI_MAIN=1; $env:SNOTRA_TRACE=1; cargo run -p snotra`
Expected: 高速連打すると `egui_search:dispatch` が leading（先頭）と trailing（収束後）で出る（毎打鍵で全走査しない）。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/egui_shell/view.rs
git commit -m "feat(su3): debounce trailing 配線（request_repaint_after・M1 Task8）"
```

---

## Task 9: 動的ウィンドウ高さ + show 順序 + reset-on-show

`compute_window_height` で結果表示可否 × max_results から高さを算出し `RuntimeFrame::set_size` で反映。`show_egui_main` が `EguiShellState.reset_pending` を立て、view が消費して `state.reset()`（resetForShow 相当）。

**Files:**
- Modify: `snotra-egui-runtime/src/runtime.rs`（`RuntimeFrame::set_size` + `apply_frame_commands`）
- Modify: `src-tauri/src/egui_shell/mod.rs`（`EguiShellState.reset_pending` + `show_egui_main`）
- Modify: `src-tauri/src/egui_shell/view.rs`（高さ算出 → `frame.set_size` + reset 消費）

**Interfaces:**
- Produces: `RuntimeFrame::set_size(&mut self, width: f64, height: f64)`（SU1 最小フック・`hide_window` と同じ sanctioned チャネル）。
- Consumes: `compute_window_height`/`HeightParams`（Task 3）、`EguiShellState`（SU2）。

- [ ] **Step 1: runtime に `set_size` フックを追加**

`snotra-egui-runtime/src/runtime.rs` の `RuntimeFrame` を拡張:

```rust
pub struct RuntimeFrame {
    close_requested: bool,
    hide_requested: bool,
    drag_requested: bool,
    resize_to: Option<(f64, f64)>,
}
```

メソッド追加:

```rust
    /// 論理サイズへのリサイズを要求する（apply_frame_commands でイベントループ上適用）。
    /// hide_window と同じ sanctioned チャネル。view から window を直接触らない。
    pub fn set_size(&mut self, width: f64, height: f64) {
        self.resize_to = Some((width, height));
    }
```

`render()` 内の `RuntimeFrame { ... }` 初期化に `resize_to: None,` を追加。`apply_frame_commands` に適用を追加（`hide` より前に置く＝リサイズしてから hide しない順序は任意だが、drag/hide/close と並べる）:

```rust
    fn apply_frame_commands(&mut self, frame: RuntimeFrame) -> Result<(), RuntimeError> {
        if let Some((w, h)) = frame.resize_to {
            self.window
                .set_size(tauri::LogicalSize::new(w, h))?;
        }
        if frame.drag_requested {
            self.window.start_dragging()?;
        }
        // ...既存 hide/close...
    }
```

**注**: `tauri::LogicalSize` の import を追加。`window.set_size` の戻りは `tauri::Result<()>` ＝ `RuntimeError::Tauri`（`#[from]`）で `?` 可。runtime テスト（`runtime.rs` の tests）はこのフックの純粋部分が無いため追加不要（SU2 と同じく view/window は smoke）。

- [ ] **Step 2: `EguiShellState` に reset_pending を追加**

`src-tauri/src/egui_shell/mod.rs` の `EguiShellState`:

```rust
#[derive(Default)]
pub(crate) struct EguiShellState {
    pub(crate) hotkey_generation: AtomicU64,
    pub(crate) hide_pending: AtomicBool,
    pub(crate) reset_pending: AtomicBool, // show が立て、view が消費して state.reset()
}
```

`show_egui_main` に、`hide_pending` をクリアしている箇所の隣で:

```rust
    if let Some(sh) = app.try_state::<EguiShellState>() {
        sh.hide_pending.store(false, Ordering::SeqCst);
        sh.reset_pending.store(true, Ordering::SeqCst); // resetForShow を view に指示
    }
```

- [ ] **Step 3: view が reset を消費 + 高さを反映**

`view.rs` の `update` 冒頭（focus 観測の前後）に reset 消費:

```rust
        // show 直後の resetForShow（EguiShellState.reset_pending を消費）。
        if let Some(sh) = self.app_handle.try_state::<crate::egui_shell::EguiShellState>() {
            if sh.reset_pending.swap(false, std::sync::atomic::Ordering::SeqCst) {
                self.state.reset();
                self.search_debounce = Debouncer::new(Duration::from_millis(50), true);
            }
        }
```

`update` 末尾（結果リスト描画の後）に高さ反映:

```rust
        // 動的ウィンドウ高さ（§4.5/§4.7）。show_results 可否 × max_results から算出し set_size。
        let show_results = !self.state.results().is_empty();
        let max_results = self.max_results();
        let height = crate::egui_shell::compute_window_height(&crate::egui_shell::HeightParams {
            show_results,
            max_results,
            has_update_toast: false, // SU5
            search_bar_height: 52.0,
            result_row_height: 30.0,
            results_padding: 8.0,
            update_toast_height: 52.0,
        });
        if (height - self.last_set_height).abs() > 0.5 {
            self.last_set_height = height;
            _frame.set_size(600.0_f64.max(self.window_width()), height);
        }
```

struct に `last_set_height: f64` と（幅取得用）を追加。`max_results()`/`window_width()` ヘルパーを追加:

```rust
    fn max_results(&self) -> u32 {
        // visible_rows は Option<usize>。effective_visible_rows() で既定補完（config.rs:327）。
        self.app_handle
            .try_state::<crate::AppState>()
            .map(|s| s.engine.lock().unwrap().config().appearance.effective_visible_rows() as u32)
            .unwrap_or(8)
    }
```

**注**: `window_width()` は現行ウィンドウ幅（`self.app_handle.get_window("main").and_then(|w| w.inner_size().ok())` を論理化、または固定 600.0）。M1 では幅は不変ゆえ `600.0` 固定でも可（SU2 が幅を config から生成済み・変えない）。`_frame` は `update(&mut self, ui, frame)` の frame 引数——現在 `_frame` 名なので `frame` に改名して使う。

- [ ] **Step 4: build + clippy（両 crate）**

Run:
```
cargo clippy -p snotra-egui-runtime --all-targets
cargo clippy -p snotra --all-targets
```
Expected: 両方沈黙。

- [ ] **Step 5: G-RESIZE 目視スモーク**

Run: `$env:SNOTRA_EGUI_MAIN=1; cargo run -p snotra`
Expected: 打鍵で結果が出るとウィンドウが下へ伸び、クエリを消すと 52px に戻る。**展開/折りたたみで reflow/ちらつき/位置ずれが目に見えて悪くないこと**（G-RESIZE）。Alt+Q hide → 再 show でクエリが空にリセットされ 52px（reset-on-show）。悪ければ present タイミングを単一ウィンドウ内で調整（2 ウィンドウ化しない）。

- [ ] **Step 6: Commit**

```bash
git add snotra-egui-runtime/src/runtime.rs src-tauri/src/egui_shell/mod.rs src-tauri/src/egui_shell/view.rs
git commit -m "feat(su3): 動的高さ + show順序 + reset-on-show（RuntimeFrame::set_size・M1 Task9）"
```

---

## Task 10: indexing overlay + 空クエリ整合

index 構築中は通常結果を出さず「構築中」を表示（§4.7）。空クエリは結果非表示（§4.6・Task 5 で実装済みの再確認 + overlay 追加）。

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`

- [ ] **Step 1: indexing overlay を描画**

`update` の結果リスト描画部を、indexing 分岐で拡張:

```rust
        let show_results = !self.state.results().is_empty();
        if self.indexing() && self.state.query().trim().is_empty() {
            // 構築中かつ空クエリ: 案内を出す（結果は無い）。i18n は既存キーに合わせる。
            ui.label("インデックス構築中…");
        } else if show_results {
            // ...Task 6 の ScrollArea...
        }
```

**注**: `run_search`（Task 5）は既に `indexing()` 時に plain 検索を抑止しているため、構築中は results が空。overlay は「空クエリ + 構築中」で案内を出す（§4.7: 通常結果ビューは非インデックス時に表示）。表示文字列は既存 i18n（WebView2 側の "indexing" 相当）に合わせるか、暫定文字列で M1 を通し M3/SU6 で i18n 統一（**暫定文字列は M1 のみ許容し TODO を残さない——このタスクで確定文字列を置く**）。

- [ ] **Step 2: build + clippy**

Run: `cargo clippy -p snotra --all-targets`（沈黙）

- [ ] **Step 3: trace スモーク**

Run: 初回起動（index 構築中）に `$env:SNOTRA_EGUI_MAIN=1; cargo run -p snotra` で「構築中」表示 → 構築完了後に検索が効くことを確認。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/egui_shell/view.rs
git commit -m "feat(su3): indexing overlay + 空クエリ整合（M1 Task10）"
```

---

## Task 11: M1 検証ゲート（G-SYNC / G-RESIZE / G1）

M1 の受け入れを接地する。ユニット緑・flag OFF 完全不変・同期 search のフレームコスト実測・動的リサイズ目視。

**Files:**
- なし（検証のみ・必要なら trace 追加）

- [ ] **Step 1: 純粋核ユニット緑**

Run: `cargo test -p snotra egui_shell::`
Expected: `search_state`（interpret 表・selection clamp・reset・interp 導出）と `layout`（height・Debouncer）が全 PASS。

- [ ] **Step 2: G1（flag OFF 完全不変）**

Run（`SNOTRA_EGUI_MAIN` 未設定で）:
```
cargo test -p snotra
npm run smoke:startup
npm run e2e:tauri
```
Expected: すべて緑。SU3 の追加が WebView2 経路・IPC・E2E 注入を触っていないこと（diff で `commands/`・`main.rs` の WebView2 分岐・`tauri.conf.json` が無改変か確認）。

- [ ] **Step 3: G-SYNC（同期 search のフレームコスト実測）**

`run_search` に計測 trace を仕込む（DEV のみ）:

```rust
                let t0 = std::time::Instant::now();
                let results = { /* engine.search */ };
                crate::trace_main("egui_search:cost", serde_json::json!({ "ms": t0.elapsed().as_secs_f64() * 1000.0, "n": results.len() }));
```

Run: 実データ規模のインデックスで `$env:SNOTRA_TRACE=1` 起動し打鍵、`egui_search:cost` の ms を観測。
Expected: trailing 1 回の search がフレームを詰まらせない（体感カクつきなし）。詰まるなら spec のリスク節に従い間隔調整か `spawn_blocking`（別タスク・要相談）。**計測 trace は確認後 revert するか DEV ガードのまま残すか判断**（残すなら `import.meta.env.DEV` 相当の Rust 側 `SNOTRA_TRACE` ゲート内なのでコスト無し）。

- [ ] **Step 4: G-RESIZE（動的リサイズ目視・Task 9 の再確認）**

Task 9 Step 5 の目視を M1 完了時点で再確認。展開/折りたたみ/再 show が滑らか。

- [ ] **Step 5: M1 完了コミット（必要なら計測 trace の後始末）**

```bash
git add -A
git commit -m "chore(su3): M1 検証ゲート（G-SYNC/G-RESIZE/G1）確認"
```

---

## Self-Review（この計画の点検）

**Spec coverage（SU3 spec の M1 該当）:**
- `SearchState` 骨格（results 軸） → Task 2 ✓
- `interpret` / `compute_window_height` / `should_run_search`（＝`Debouncer`） → Task 1/3/4 ✓
- 描画（検索バー・結果リスト・行） → Task 5/6 ✓
- 同期 search → Task 5 ✓
- debounce（leading+trailing 50ms） → Task 4/8 ✓
- ↑↓ナビ/選択/scroll 追従 → Task 6/7 ✓
- Enter/クリック起動 + hide → Task 7 ✓
- 空クエリ / indexing overlay → Task 5/10 ✓
- 動的高さ / show 順序 → Task 9 ✓
- G-RESIZE / G-SYNC / G1 → Task 9/11 ✓

**裏取り済み（plan 執筆時に grep で確認・実装で推測しない）:**
- prefix は `config().search.instant_command_prefix`（config.rs:956）— `general` ではない
- `visible_rows` は `Option<usize>`、`config().appearance.effective_visible_rows()` で読む（config.rs:327）
- `AppState.indexing: AtomicBool`（state.rs:14）
- `LaunchResult.status: LaunchStatus`、成功は `LaunchStatus::Ok`（launch.rs:16,33,56）
- `launch_item_core` は履歴を記録しない。共通末尾 `record_and_save(state, path, query)`（launch.rs:84）が記録 + 保存。egui 経路も再利用（Task 7 で pub(crate) 化）

**実装時に確認が残る箇所（egui/Tauri API の細部・plan 内に注記済み）:**
- egui の描画 API 細部（`allocate_exact_size`/`painter().text`/`ScrollArea`/`scroll_to_me`）— 使用 egui バージョンで署名確認（Task 6）
- `RuntimeFrame::set_size` 追加は SU1 隣接の最小フック（**要相談**・Task 9）。runtime を触るため合意してから実装する
- `window.set_size` の `tauri::LogicalSize` import と `RuntimeError::Tauri` への `?` 変換（Task 9）

**Placeholder scan:** 「暫定文字列」は Task 10 で確定文字列を置く指示に置換済み（TODO を残さない）。計測 trace（Task 11）は残置/revert の判断を明示。

**Type consistency:** `SearchState` の API（`set_query`/`set_results`/`move_selection`/`view_kind`/`interp`/`reset`）は Task 2 定義と Task 5–10 の呼び出しで一致。`Debouncer`（`on_input`/`poll`/`interval`）は Task 4 定義と Task 8 呼び出しで一致。`compute_window_height`/`HeightParams` は Task 3 定義と Task 9 で一致。`RuntimeFrame::set_size` は Task 9 で定義即使用。

**Scope:** M1 のみ。folder（M2）/ instant・slash（M3）/ tool（SU3.5）/ アイコン実体（SU4）は含まない。`run_search` の command/instant 分岐は空（M2/M3 で埋める seam）。
