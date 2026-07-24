# SU6 統合 glue 実装計画（config 反映 + #633 + §12 IME parity）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** config 変更・index 状態変化を egui ウィンドウへ届ける glue（単一 wake + live-read）を実装し、#633（再インデックス中 stale 結果・§4.7）と §12 IME parity を egui 経路で成立させる。

**Architecture:** spec `docs/superpowers/specs/2026-07-24-su6-config-glue-design.md`（改訂版・4 レンズレビュー済み）に忠実に実装する。値は運ばない wake 合図 1 本（`config-applied` + indexing 2 イベント）→ `request_repaint` → 毎フレーム live-read。#633 は「表示ゲート（純粋核述語）+ `index_generation: AtomicU64` 世代カウンタ」。width は view を唯一の size writer 化。IME は既存 `PlatformCommand::TurnOffIme` 再利用。

**Tech Stack:** Rust / tauri 2 / egui（softbuffer runtime）。作業ブランチ: `feat/532-su6-config-glue`（作成済み・spec コミット済み）。

## Global Constraints

- **main へ直接コミットしない**（feature ブランチ上で作業・ルート CLAUDE.md）
- **bash HEREDOC（`<<EOF`）禁止**。複数行テキストは Write ツールでファイル化して `-F`/`--body-file`
- パス区切りは `/` で統一
- `*.rs` 編集後の clippy + crate テストは PostToolUse hook が自動実行（**沈黙 = 合格**。失敗時のみ会話に届く）。`*.md` は hook 対象外（沈黙 = 未検査）
- 製品 egui 経路の起動は `$env:SNOTRA_EGUI_MAIN=1; cargo run -p snotra`（`snotra-egui-mvp` はスパイク・対象外）
- spec の決定番号（決定 1〜5）を実装コメントで引用する場合は「SU6 spec 決定 N」形式
- コミットメッセージ末尾: `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`

---

### Task 1: 純粋核述語（表示ゲート + 世代トリガ）

**Files:**
- Modify: `src-tauri/src/egui_shell/search_state.rs`（末尾の `#[cfg(test)] mod tests` 内にテスト追加、本体はテストの手前に追加）
- Modify: `src-tauri/src/egui_shell/mod.rs`（re-export 1 行）

**Interfaces:**
- Consumes: 既存 `ViewKind`（`search_state.rs` の enum。`Results` / `Folder` / `Tool` variant を持つ）
- Produces: `pub fn plain_results_hidden(view_kind: ViewKind, instant_rows: bool, indexing: bool) -> bool` / `pub fn needs_index_refresh(last_seen: u64, current: u64) -> bool`（Task 4 が `crate::egui_shell::` 経由で使う）

- [ ] **Step 1: 失敗するテストを書く**

`search_state.rs` の既存 `#[cfg(test)] mod tests` に追加:

```rust
    #[test]
    fn plain_results_hidden_only_for_plain_results_view() {
        // §4.7: indexing 中は plain results のみ隠す（SolidJS shouldShowResults 鏡写し）
        assert!(plain_results_hidden(ViewKind::Results, false, true));
        // instant 行は表示継続（§4.7 instant carve-out・SPEC §4.7:181）
        assert!(!plain_results_hidden(ViewKind::Results, true, true));
        // folder/tool は index 非依存ゆえ表示継続
        assert!(!plain_results_hidden(ViewKind::Folder, false, true));
        assert!(!plain_results_hidden(ViewKind::Tool, false, true));
        // 非 indexing は常に表示
        assert!(!plain_results_hidden(ViewKind::Results, false, false));
    }

    #[test]
    fn needs_index_refresh_only_on_generation_change() {
        assert!(!needs_index_refresh(0, 0));
        assert!(needs_index_refresh(0, 1));
        // 複数回 bump がまとまっても 1 回の比較で拾う（repaint 合流パルス耐性・spec 決定 3）
        assert!(needs_index_refresh(3, 7));
    }
```

- [ ] **Step 2: 落ちることを確認（Red）**

Run: `cargo test -p snotra plain_results_hidden --no-run 2>&1 | tail -5`
Expected: コンパイルエラー `cannot find function plain_results_hidden`

- [ ] **Step 3: 最小実装**

`search_state.rs`（tests モジュールの手前）に追加:

```rust
/// §4.7 表示ゲート（#633・SU6 spec 決定 3）: 再インデックス中は plain 結果のみ隠す。
/// SolidJS `shouldShowResults`（search.ts: `interpKind()==="instant" || !indexing()`）の鏡写しで、
/// instant/folder/tool は表示継続、データと選択は保持する（クリアしない・選択リセットしない）。
/// `instant_rows` は表示中行の来歴 snapshot（`instant_rows_query.is_some()`）——live interp でなく
/// 来歴で判定するのは prefix hot-change の stale 行対策（#637 finding 0）と同じ理由。
pub fn plain_results_hidden(view_kind: ViewKind, instant_rows: bool, indexing: bool) -> bool {
    indexing && matches!(view_kind, ViewKind::Results) && !instant_rows
}

/// #633 世代トリガ（SU6 spec 決定 3）: index build 完了で bump される世代が last-seen と
/// 異なれば再検索。bool エッジ検出と違い、started/complete の repaint が 1 フレームに合流して
/// パルスが見えなくても累積カウンタは差分が残るため取りこぼさない。
pub fn needs_index_refresh(last_seen: u64, current: u64) -> bool {
    last_seen != current
}
```

`mod.rs` の re-export 行（`pub(crate) use search_state::{SlashCmd, find_slash_command};` 付近）に追加:

```rust
pub(crate) use search_state::{needs_index_refresh, plain_results_hidden};
```

- [ ] **Step 4: テストが通ることを確認（Green）**

Run: `cargo test -p snotra plain_results_hidden needs_index_refresh 2>&1 | tail -5`
Expected: `test result: ok`（2 テスト pass。未使用 re-export の warning は Task 4 で消える——dead_code warning が出る場合はこの時点では許容し clippy が通ることを確認）

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/egui_shell/search_state.rs src-tauri/src/egui_shell/mod.rs
git commit -m "feat: SU6 純粋核述語（§4.7 表示ゲート + #633 世代トリガ）"
```

---

### Task 2: AppState.index_generation

**Files:**
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/main.rs`（`AppState { ... }` 構築サイト・main.rs:588）
- Modify: `src-tauri/src/commands/system.rs`（`test_state` ヘルパーの `AppState` 完全リテラル・85-92 行付近。**plan-review scout-glue が検出した 3 箇所目**——漏らすと `cargo test -p snotra` がコンパイル断）

**Interfaces:**
- Produces: `AppState.index_generation: AtomicU64`（Task 4 の view が `load(Ordering::SeqCst)` で読む）。bump は `finish_index_build()` 内（唯一のチョークポイント）

- [ ] **Step 1: 失敗するテストを書く**

`state.rs` の tests に追加:

```rust
    #[test]
    fn finish_index_build_bumps_index_generation() {
        // #633: 完了ごとに単調増加。egui view の世代比較トリガの根拠（SU6 spec 決定 3）。
        let state = test_state();
        let g0 = state.index_generation.load(Ordering::SeqCst);
        assert!(state.try_begin_index_build());
        state.finish_index_build();
        assert_eq!(state.index_generation.load(Ordering::SeqCst), g0 + 1);
    }
```

`test_state()` にもフィールド追加が要る（Step 3 と同時でよい）。

- [ ] **Step 2: 落ちることを確認（Red）**

Run: `cargo test -p snotra finish_index_build_bumps --no-run 2>&1 | tail -5`
Expected: コンパイルエラー `no field index_generation`

- [ ] **Step 3: 最小実装**

`state.rs`:

```rust
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
```

`AppState` にフィールド追加:

```rust
    /// index build 完了ごとに単調増加する世代（#633・SU6 spec 決定 3）。egui view が
    /// last-seen と比較して再検索をトリガするアキュムレータ。panic/spawn 失敗経路の finish でも
    /// bump されるが、無変化 index への再検索は同一結果になるだけで無害（意図的に単純化）。
    pub index_generation: AtomicU64,
```

`finish_index_build()` の末尾に追加:

```rust
        self.index_generation.fetch_add(1, Ordering::SeqCst);
```

`AppState { ... }` 構築サイトは **3 箇所すべて**に `index_generation: AtomicU64::new(0),` を追加する: (1) `state.rs` の `test_state()`、(2) `main.rs:588` の本番構築、(3) `src-tauri/src/commands/system.rs:85-92` の `test_state(indexing: bool)`（`..` 更新構文を使わない完全リテラル）。grep `AppState {` で全列挙してから編集すること。main.rs / system.rs 側は `use` に `AtomicU64` が無ければ追加する。

注: バックグラウンド再スキャン（`setup_background_rescan`・§3.3）も index を swap するが bump しない——WebView2 も rescan 完了で `indexing-complete` を emit せずフロントは refresh しない（parity・意図的 non-goal）。

- [ ] **Step 4: テストが通ることを確認（Green）**

Run: `cargo test -p snotra finish_index_build 2>&1 | tail -5`
Expected: `test result: ok`（既存 finish 系テスト含め全 pass）

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/state.rs src-tauri/src/main.rs
git commit -m "feat: AppState.index_generation（#633 世代カウンタ・finish_index_build で bump）"
```

---

### Task 3: wake 配線（config-applied emit + listener 登録）

**Files:**
- Modify: `src-tauri/src/config_watcher.rs`（`apply_config_change` 末尾）
- Modify: `src-tauri/src/egui_shell/mod.rs`（listener 関数追加）
- Modify: `src-tauri/src/main.rs`（setup の egui block・679-687 付近）

**Interfaces:**
- Consumes: `EguiShellState.egui_ctx: Mutex<Option<egui::Context>>`（SU5 実装済み・`mod.rs:51`）
- Produces: イベント `"config-applied"`（payload なし）/ `register_config_wake_listeners(app: &tauri::AppHandle)`

純粋核が無い glue タスクのため TDD 対象外（検証は clippy/既存テスト + Task 7 スモーク）。

- [ ] **Step 1: config_watcher.rs — emit 追加**

`apply_config_change` の末尾（width リサイズ分岐 `if width_changed ...` ブロックの直後、関数の閉じ括弧の前）に追加:

```rust
    // SU6 spec 決定 1: egui 窓への単一 wake（値は運ばない・受信側は次フレームの live-read が拾う）。
    // WebView2 側に listener は無く flag OFF では無害なので無条件 emit。update_config（上）より
    // 後に置く——先に起こすと旧 config を描いてから二度目の wake が要る。
    let _ = app.emit("config-applied", ());
```

- [ ] **Step 2: mod.rs — listener 関数追加**

`register_hide_listener` の直後に追加。repaint ブロックは既存 2 箇所（`spawn_update_check` 末尾 mod.rs:120-125・view.rs `spawn_install` 内）と同型のため `wake_view` ヘルパーに集約する（/dry-check・plan-review 独立導出の指摘）:

```rust
/// 可視中の view を起こす（egui_ctx 未登録＝setup〜初フレーム、hidden 中は無害な no-op）。
/// WebView2 経路（flag OFF）では EguiShellState が manage されておらず自然に no-op。
pub(crate) fn wake_view(app: &tauri::AppHandle) {
    if let Some(sh) = app.try_state::<EguiShellState>()
        && let Ok(guard) = sh.egui_ctx.lock()
        && let Some(ctx) = guard.as_ref()
    {
        ctx.request_repaint();
    }
}

/// config 変更・index 状態変化の wake 合図（#532 SU6 spec 決定 1）。値は運ばず request_repaint
/// のみ——次フレームの live-read が最新値を拾う。空振りは benign（初 show フレームの live-read が
/// 最新を描く）。**「値を運ばない」はこの benign 性の load-bearing 前提**——将来イベントに値を
/// 載せる変更はこの前提を壊す（spec 決定 1）。
pub(crate) fn register_config_wake_listeners(app: &tauri::AppHandle) {
    for event in ["config-applied", "indexing-started", "indexing-complete"] {
        let handle = app.clone();
        app.listen(event, move |_| {
            wake_view(&handle);
        });
    }
}
```

`spawn_update_check` 末尾の同型ブロック（mod.rs:119-126）も `wake_view(&handle);` に置換して重複を消す（view.rs `spawn_install` 側は ctx を直接持つ別形のため対象外）。

- [ ] **Step 3: main.rs — 登録サイト（位置 pin）**

setup の egui block 内、`egui_shell::register_hide_listener(&app_handle);` の直後に追加:

```rust
                // config 変更・indexing 状態変化の wake（SU6 spec 決定 1）。config_watcher 起動
                // （下の setup_config_watcher）と setup_startup_display より前に登録し、可視窓が
                // 合図を取りこぼす窓を作らない（位置は spec が pin・並行性レビュー）。
                egui_shell::register_config_wake_listeners(&app_handle);
```

- [ ] **Step 4: ビルド確認**

Run: `cargo clippy -p snotra --all-targets 2>&1 | tail -5`
Expected: エラー・警告なし（PostToolUse hook の沈黙でも可）

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/config_watcher.rs src-tauri/src/egui_shell/mod.rs src-tauri/src/main.rs
git commit -m "feat: SU6 wake 配線（config-applied emit + egui wake listener 3 本）"
```

---

### Task 4: view 統合 — #633（世代再検索 + 表示ゲート）

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`

**Interfaces:**
- Consumes: Task 1 の `crate::egui_shell::{needs_index_refresh, plain_results_hidden}`・Task 2 の `AppState.index_generation`・既存 `self.indexing()`（view.rs:629）・`self.state.view_kind()`・`self.instant_rows_query`
- Produces: なし（view 内で完結）

- [ ] **Step 1: フィールド追加**

`SearchWindowView` struct（`launching` フィールド付近）に追加:

```rust
    /// #633: index build 完了世代の last-seen（AppState.index_generation と比較・SU6 spec 決定 3）。
    /// 差分で現クエリを再検索（SolidJS `indexing-complete`→runRefresh parity）。bool エッジ検出で
    /// ないのは started/complete の repaint が 1 フレームに合流するとパルスが見えないため。
    last_seen_index_generation: u64,
```

`new()` に `last_seen_index_generation: 0,` を追加。

- [ ] **Step 2: update() に世代検知ブロック追加**

`update()` 内、reset_pending 消費ブロック（`if let Some(sh) = ... reset_pending.swap ...`）の直後・`let ctx = ui.ctx().clone();` の後に追加（`ctx` を使わないので位置は reset ブロック直後でよいが、他ブロックと揃えて `ctx` 定義後に置く）:

```rust
        // #633: index build 完了の世代検知 → 現クエリで再検索（runRefresh parity・SU6 spec 決定 3）。
        // reset_pending 消費の後に置く（show 直後は reset 済み空クエリの no-op になるだけ）。
        // folder 中は fs 由来 cache の再フィルタ、tool 中は no-op——run_search が view_kind で分岐済み。
        if let Some(s) = self.app_handle.try_state::<crate::AppState>() {
            let gen = s.index_generation.load(Ordering::SeqCst);
            if crate::egui_shell::needs_index_refresh(self.last_seen_index_generation, gen) {
                self.last_seen_index_generation = gen;
                self.run_search();
            }
        }
```

`Ordering` は view.rs 冒頭で `use std::sync::atomic::Ordering;` 済みか確認（reset_pending.swap が使用中なので既にあるはず）。

- [ ] **Step 3: show_results に表示ゲート**

view.rs:1605-1606 の

```rust
        // 結果リスト（shouldShowResults 相当。results 軸〔plain〕と folder 軸を描く。空なら描かない）。
        let show_results = !self.state.results().is_empty();
```

を以下に置換:

```rust
        // 結果リスト（shouldShowResults 相当）。§4.7: 再インデックス中は plain 結果のみ隠す
        // （instant/folder/tool carve-out・SU6 spec 決定 3）。データと選択は保持——クリアしない
        // （SolidJS parity: setIndexing は結果を触らず派生 memo が非表示を担う）。indexing 中の
        // 案内は空クエリ=hint・非空クエリ=overlay（Task 7・spec 追補 1）が担い、高さは
        // show_results=false で 52px に折りたたまれる。
        let show_results = !self.state.results().is_empty()
            && !crate::egui_shell::plain_results_hidden(
                self.state.view_kind(),
                self.instant_rows_query.is_some(),
                self.indexing(),
            );
```

- [ ] **Step 4: ビルド + 全テスト確認**

Run: `cargo test -p snotra 2>&1 | tail -5`
Expected: `test result: ok`（全 pass・hook 沈黙）

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/egui_shell/view.rs
git commit -m "feat: #633 egui 再インデックス表示ゲート + 世代再検索（§4.7）"
```

---

### Task 5: view 統合 — font/width/native 背景ブラシ（live-read 例外 3 つ）

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`

**Interfaces:**
- Consumes: 既存 `configure_japanese_font(context, &font_family)`（view.rs・setup で使用中）・`crate::config_watcher::parse_hex_color`（mod.rs:143 で使用実績あり・可視性 pub(crate) 確認済み）
- Produces: なし（view 内で完結）。`window_width()` の意味変更（inner_size 読み → config live-read）

- [ ] **Step 1: フィールド追加 + setup() 初期化**

struct に追加:

```rust
    /// SU6 spec 決定 2: 適用済み font_family。config 値と毎フレーム比較し差分で再ロード。
    /// **解決の成否に依らず config 値へ無条件更新する**——未解決名（typo・未インストール）で
    /// 毎フレーム load_system_fonts（数十 ms）が走る perf cliff を避ける（並行性レビュー）。
    applied_font_family: String,
    /// SU6 spec 決定 2: 適用済み native 背景ブラシ（hex 文字列）。painted panel は live-read だが
    /// リサイズ時に露出する native surface の色は生成時ブラシ由来のため実行時追従が要る（codex 反証）。
    applied_background_hex: String,
    /// SU6 spec 決定 2: 直近 set_size の幅。view が唯一の size writer（幅は config live-read）。
    last_set_width: f64,
```

`new()` に追加: `applied_font_family: String::new(), applied_background_hex: String::new(), last_set_width: 0.0,`

`setup()` の `configure_japanese_font(context, &font_family);` の直後に追加:

```rust
        self.applied_font_family = font_family;
```

（`font_family` の move でコンパイルが通るよう `configure_japanese_font` 呼び出しは `&font_family` のまま）。`applied_background_hex` は空のままでよい——初フレームの Step 2 ブロックが config 値との差分で一度 set_background_color を呼ぶ（生成時ブラシと同値の再設定・無害）。

- [ ] **Step 2: update() の §11 テーマブロックを拡張（lock 1 回に集約）**

既存ブロック（view.rs:1189-1202）:

```rust
        // §11: パネル/入力欄/選択色を config テーマから（ハードコード撤廃・runtime CLEAR_COLOR は不変）。
        if let Some(s) = self.app_handle.try_state::<crate::AppState>() {
            let (bg, input_bg, sel) = {
                let engine = s.engine.lock().unwrap();
                let v = &engine.config().visual;
                (v.background_color.clone(), v.input_background_color.clone(), v.selected_row_color.clone())
            };
            let mut visuals = ctx.style_of(ctx.theme()).visuals.clone();
            visuals.panel_fill = hex_color(&bg, egui::Color32::from_rgb(0x28, 0x28, 0x28));
            visuals.window_fill = visuals.panel_fill;
            visuals.extreme_bg_color = hex_color(&input_bg, egui::Color32::from_rgb(0x38, 0x38, 0x38)); // TextEdit 背景
            visuals.selection.bg_fill = hex_color(&sel, egui::Color32::from_rgb(0x33, 0x33, 0x33));
            ctx.set_visuals(visuals);
        }
```

を以下に置換（font_family も同じ lock で読む・エッジ検出 2 つを続ける）:

```rust
        // §11: パネル/入力欄/選択色を config テーマから（ハードコード撤廃・runtime CLEAR_COLOR は不変）。
        // font_family / native 背景ブラシのエッジ検出も同一 lock で読む（SU6 spec 決定 2・lock 1 回/フレーム）。
        if let Some(s) = self.app_handle.try_state::<crate::AppState>() {
            let (bg, input_bg, sel, font_family) = {
                let engine = s.engine.lock().unwrap();
                let v = &engine.config().visual;
                (
                    v.background_color.clone(),
                    v.input_background_color.clone(),
                    v.selected_row_color.clone(),
                    v.font_family.clone(),
                )
            };
            let mut visuals = ctx.style_of(ctx.theme()).visuals.clone();
            visuals.panel_fill = hex_color(&bg, egui::Color32::from_rgb(0x28, 0x28, 0x28));
            visuals.window_fill = visuals.panel_fill;
            visuals.extreme_bg_color = hex_color(&input_bg, egui::Color32::from_rgb(0x38, 0x38, 0x38)); // TextEdit 背景
            visuals.selection.bg_fill = hex_color(&sel, egui::Color32::from_rgb(0x33, 0x33, 0x33));
            ctx.set_visuals(visuals);

            // SU6 spec 決定 2: font_family hot-reload（WebView2 の --font-family CSS 変数即時反映 parity）。
            // applied は解決成否に依らず無条件更新（フィールド doc 参照）。
            if font_family != self.applied_font_family {
                self.applied_font_family = font_family.clone();
                configure_japanese_font(&ctx, &font_family);
                ctx.request_repaint(); // set_fonts は次フレーム適用——欠くと新フォントが 1 イベント遅れる
            }

            // SU6 spec 決定 2: native 背景ブラシ追従（生成時一度きり → 実行時変更へ・codex 反証）。
            if bg != self.applied_background_hex {
                self.applied_background_hex = bg.clone();
                if let Some(window) = self.app_handle.get_window("main") {
                    let color = crate::config_watcher::parse_hex_color(&bg)
                        .unwrap_or(tauri::window::Color(0x28, 0x28, 0x28, 0xff));
                    let _ = window.set_background_color(Some(color));
                }
            }
        }
```

**API 確認**: `tauri::Window::set_background_color` が使用中の tauri バージョンに存在するかは `cargo check -p snotra` が判定する（Step 4）。**存在しない場合**は背景ブラシブロックを削除し、代わりに SPEC §11 に受容残余を 1 行追記する（「egui 経路の native 背景ブラシは生成時一度きりで、実行時のテーマ変更後は次回起動までリサイズ過渡の下地色が旧色のままとなる（受容・#532 SU6）」）——spec 決定 2 の fallback 分岐。

- [ ] **Step 3: window_width() を config live-read へ差し替え**

既存（view.rs:654-665 付近、doc コメント含む）を以下に置換:

```rust
    /// ウィンドウ論理幅は config live-read（SU6 spec 決定 2: **view が唯一の size writer**）。
    /// 旧実装の inner_size() 読みは「幅を維持」だったが、config_watcher（notify スレッド）の幅
    /// set_size と 2 次元 read-modify-write で潰し合う race の片翼だった——config を正本にすれば
    /// cross-thread writer 自体が消える（初版 spec の watcher flag 分岐案は却下・並行性レビュー）。
    /// なお flag ON では config_watcher の幅 set_size は get_webview_window=None で元々 no-op。
    fn window_width(&self) -> f64 {
        self.app_handle
            .try_state::<crate::AppState>()
            .map(|s| f64::from(s.engine.lock().unwrap().config().appearance.window_width))
            .unwrap_or(600.0)
    }
```

（`window_width` の型が u32 でない場合は `as f64` へ調整。config.rs で確認のこと。）

- [ ] **Step 4: set_size 条件を幅差分にも反応させる**

既存（view.rs:1655-1663）:

```rust
        if (height - self.last_set_height).abs() > 0.5 {
            self.last_set_height = height;
            if let Some(window) = self.app_handle.get_window("main") {
                let _ = window.set_size(tauri::LogicalSize::new(self.window_width(), height));
            }
            ui.ctx().request_repaint();
        }
```

を以下に置換:

```rust
        // 幅は config live-read（SU6 spec 決定 2）。hidden 中の幅変更は wake 空振りでも、次 show の
        // 初フレームでこの差分が検知して是正する（show_egui_main の inner_size 幅 52px collapse とは独立）。
        let width = self.window_width();
        if (height - self.last_set_height).abs() > 0.5 || (width - self.last_set_width).abs() > 0.5 {
            self.last_set_height = height;
            self.last_set_width = width;
            if let Some(window) = self.app_handle.get_window("main") {
                let _ = window.set_size(tauri::LogicalSize::new(width, height));
            }
            // 新サイズでの再描画を即要求し 1 フレームの空きを詰める（背景 config 色で
            // フラッシュは緩和済みだが空き自体が G-RESIZE のちらつき機構・advisor 指摘）。
            ui.ctx().request_repaint();
        }
```

（既存の G-RESIZE 経緯コメントは上のとおり保持する——plan-review scout-egui の指摘。）

- [ ] **Step 5: ビルド + 全テスト確認**

Run: `cargo test -p snotra 2>&1 | tail -5`
Expected: `test result: ok`。`set_background_color` が存在しない場合はここでコンパイルエラー → Step 2 の fallback 分岐（ブロック削除 + SPEC §11 追記）を実行して再実行

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/egui_shell/view.rs
git commit -m "feat: SU6 live-read 例外 3 つ（font hot-reload / width view 単独 writer / 背景ブラシ追従）"
```

---

### Task 6: §12 IME parity（show 経路）

**Files:**
- Modify: `src-tauri/src/egui_shell/mod.rs`（`show_egui_main`）

**Interfaces:**
- Consumes: `crate::platform::{PlatformBridge, PlatformCommand}`（`PlatformCommand::TurnOffIme(usize)` は生 HWND を取る・platform/mod.rs 定義）・`AppState.engine.config().general.ime_off_on_show`
- Produces: なし

- [ ] **Step 1: show_egui_main に IME オフを追加**

`show_egui_main` 内、残留 Alt 解除ブロック（`if !crate::is_alt_pressed() { crate::send_alt_key_up(); }`）の直後・`trace_main("egui_show:done", ...)` の前に追加:

```rust
    // §12: 表示時 IME オフ（設定有効時・復元なし・SU6 spec 決定 4）。ime_off_on_show は実行中
    // config から都度読み（キャッシュしない・#576 同型——config_watcher の hot-reload が diff/event
    // 追加なしに届く）。**focus 同期（上の SendMessageTimeoutW）より後に置く**——前だと IME オフが
    // 対象窓に効かない（WebView2 apply_ime_control doc の警告条件）。Win32 は PlatformBridge 経由
    // （rule）。TurnOffIme は生 HWND(usize) を取るため窓型非依存で &Window 一般化は不要。
    #[cfg(windows)]
    {
        let ime_control = app
            .try_state::<crate::AppState>()
            .map(|s| s.engine.lock().unwrap().config().general.ime_off_on_show)
            .unwrap_or(false); // config.rs の既定値と一致
        if ime_control
            && let Some(bridge) = app.try_state::<std::sync::Mutex<crate::platform::PlatformBridge>>()
            && let Ok(b) = bridge.lock()
            && let Ok(hwnd) = window.hwnd()
        {
            b.send_command(crate::platform::PlatformCommand::TurnOffIme(hwnd.0 as usize));
            crate::trace_main("egui_show:ime_control", serde_json::json!({}));
        }
    }
```

（trace はスモーク（Task 9 Step 3 項目 6）の客観確認点。managed state 型は main.rs:796 で `Mutex<PlatformBridge>`＝`std::sync::Mutex` と確認済み・scout-egui。）

（`PlatformBridge` の managed state 型は main.rs の `apply_ime_control`（main.rs:450-459）と同一形。パスが `crate::platform::` でなく `crate::` 直下 re-export の場合は main.rs の use を確認して合わせる。）

- [ ] **Step 2: ビルド確認**

Run: `cargo clippy -p snotra --all-targets 2>&1 | tail -5`
Expected: エラーなし

- [ ] **Step 3: コミット**

```bash
git add src-tauri/src/egui_shell/mod.rs
git commit -m "feat: §12 IME parity — egui show 経路で表示時 IME オフ（PlatformBridge 経由）"
```

---

### Task 7: 通知 parity — indexing overlay + hotkey 失敗通知（spec 追補 1/2）

**Files:**
- Modify: `src-tauri/src/egui_shell/notify.rs`（純粋核: `overlay_kind` + `NOTICE_HOTKEY`）
- Modify: `src-tauri/src/egui_shell/strings.rs`（`hotkey_change_failed`）
- Modify: `src-tauri/src/egui_shell/mod.rs`（`EguiShellState` フィールド + listener）
- Modify: `src-tauri/src/egui_shell/view.rs`（overlay 分岐置換 + pending 消費）

**Interfaces:**
- Consumes: Task 3 の listener 登録パターン・既存 `NoticeSlot`（`set(message, now, duration)`・notify.rs:24）・既存 overlay 描画（view.rs:1458-1495）
- Produces: `pub enum OverlayKind { Indexing, Launching, Notice }` / `pub fn overlay_kind(indexing: bool, query_empty: bool, launching: bool, has_notice: bool) -> Option<OverlayKind>` / `pub const NOTICE_HOTKEY: Duration` / `pub fn hotkey_change_failed(l: Language, hotkey: &str) -> String` / `EguiShellState.pending_hotkey_failure: Mutex<Option<String>>`

- [ ] **Step 1: 失敗するテストを書く（純粋核 2 つ）**

`notify.rs` の tests に追加:

```rust
    #[test]
    fn overlay_kind_priority_ladder() {
        use super::OverlayKind::*;
        // 優先順 indexing > launching > notice（WebView2 Switch 先頭一致 parity・SU5 確立の不変）
        assert_eq!(overlay_kind(true, false, true, true), Some(Indexing));
        // 空クエリの indexing は hint_text が描くため overlay は出さない（二重描画回避・spec 追補 1）
        assert_eq!(overlay_kind(true, true, true, true), None);
        assert_eq!(overlay_kind(false, true, true, true), Some(Launching));
        assert_eq!(overlay_kind(false, true, false, true), Some(Notice));
        assert_eq!(overlay_kind(false, true, false, false), None);
    }
```

`strings.rs` の tests（既存 parity テスト群と同じ場所）に追加:

```rust
    #[test]
    fn hotkey_change_failed_matches_i18n() {
        // i18n.ts の該当キー値と一字一句一致させる（実装前に ui/src/lib/i18n.ts を読み、
        // {hotkey} 置換を含む正確な文字列をコピーしてこの期待値を書くこと）
        assert_eq!(
            hotkey_change_failed(Language::Ja, "Alt+Q"),
            "ホットキー (Alt+Q) の登録に失敗しました。元のホットキーを維持します"
        );
        assert!(hotkey_change_failed(Language::En, "Alt+Q").contains("Alt+Q"));
    }
```

**実装前に `ui/src/lib/i18n.ts` の hotkey 失敗キー（60-61/94-95 行付近）を必ず読み、期待値文字列を実物へ合わせて修正する**（上の日本語文字列は独立導出の引用であり、一字一句の正は i18n.ts）。

- [ ] **Step 2: 落ちることを確認（Red）**

Run: `cargo test -p snotra overlay_kind hotkey_change_failed --no-run 2>&1 | tail -5`
Expected: コンパイルエラー（未定義シンボル）

- [ ] **Step 3: 純粋核の実装**

`notify.rs`（NoticeSlot の近く）:

```rust
/// hotkey 登録失敗通知の表示時間（SolidJS `setHotkeyFailureNotice` の 5000ms parity・
/// launchNotice.ts で確認のこと）。
pub const NOTICE_HOTKEY: Duration = Duration::from_millis(5000);

/// 検索バー overlay の優先ラダー（WebView2 SearchWindow.tsx の Switch 先頭一致 parity）。
/// `indexing` は「indexing 中かつ Results ビュー」を呼び出し側で評価して渡す。
/// 空クエリの indexing は TextEdit の hint_text が描くため None（二重描画回避・spec 追補 1）。
/// 非空クエリの indexing は表示ゲート（§4.7）で結果が消えるため overlay が唯一の案内になる。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OverlayKind {
    Indexing,
    Launching,
    Notice,
}

pub fn overlay_kind(
    indexing: bool,
    query_empty: bool,
    launching: bool,
    has_notice: bool,
) -> Option<OverlayKind> {
    if indexing {
        if query_empty { None } else { Some(OverlayKind::Indexing) }
    } else if launching {
        Some(OverlayKind::Launching)
    } else if has_notice {
        Some(OverlayKind::Notice)
    } else {
        None
    }
}
```

`strings.rs`（既存関数群と同形式・i18n.ts 実物から文字列コピー）:

```rust
/// ホットキー登録失敗通知（i18n.ts の該当キーと一字一句一致・{hotkey} は書式挿入）。
pub fn hotkey_change_failed(l: Language, hotkey: &str) -> String {
    match l {
        Language::Ja => format!("ホットキー ({hotkey}) の登録に失敗しました。元のホットキーを維持します"),
        Language::En => format!("Failed to register hotkey ({hotkey}). Keeping the previous hotkey."),
    }
}
```

`mod.rs` の re-export に `overlay_kind` / `OverlayKind` / `NOTICE_HOTKEY` を追加（既存 `NoticeSlot` の re-export 行に倣う）。

- [ ] **Step 4: テストが通ることを確認（Green）**

Run: `cargo test -p snotra overlay_kind hotkey_change_failed 2>&1 | tail -5`
Expected: `test result: ok`

- [ ] **Step 5: driver 配線（mod.rs + view.rs）**

`EguiShellState`（mod.rs:44-52）にフィールド追加:

```rust
    /// hotkey 登録失敗の pending payload（spec 追補 2）。config_watcher の
    /// `hotkey-registration-failed` listener が格納し view が消費時に lang() live-read で整形する。
    /// **この listener は wake しない**——wake は config-applied（update_config 後）だけにし、
    /// 言語同時変更時に旧言語で整形する競合窓を閉じる（「language-changed が先」不変条件の egui 版）。
    pub(crate) pending_hotkey_failure: Mutex<Option<String>>,
```

`register_config_wake_listeners` の直後に listener 追加（同関数内に足してもよいが wake しない listener なので分離する）:

```rust
/// hotkey 登録失敗の payload 受け口（spec 追補 2・wake は config-applied に委ねる）。
pub(crate) fn register_hotkey_failure_listener(app: &tauri::AppHandle) {
    let handle = app.clone();
    app.listen("hotkey-registration-failed", move |event| {
        // emit 側は String を渡すため payload は JSON 文字列（引用符付き）。
        let hotkey: String = serde_json::from_str(event.payload()).unwrap_or_default();
        if let Some(sh) = handle.try_state::<EguiShellState>() {
            *sh.pending_hotkey_failure.lock().unwrap() = Some(hotkey);
        }
    });
}
```

main.rs の egui block（`register_config_wake_listeners` の直後）に `egui_shell::register_hotkey_failure_listener(&app_handle);` を追加。

`view.rs` update() の世代検知ブロック（Task 4 Step 2）の直後に消費を追加:

```rust
        // hotkey 登録失敗の pending 消費（spec 追補 2）。reset_pending 消費より後（順序不変条件）。
        // 整形はここで lang() live-read——config-applied wake のフレームは update_config 後なので
        // 言語同時変更でも新言語で整形される。hidden 中の失敗は次 show のこの消費で表示される
        //（WebView2 は hidden 中に期限切れ・改善方向の受容差異・spec 追補 2）。
        if let Some(sh) = self.app_handle.try_state::<crate::egui_shell::EguiShellState>()
            && let Some(hk) = sh.pending_hotkey_failure.lock().unwrap().take()
        {
            let msg = crate::egui_shell::ui_strings::hotkey_change_failed(self.lang(), &hk);
            self.notice.set(msg, self.notice_base.elapsed(), crate::egui_shell::NOTICE_HOTKEY);
            ctx.request_repaint();
        }
```

`view.rs` の overlay 分岐（view.rs:1465-1471）を純関数消費に置換:

```rust
        // 優先順は WebView2 SearchWindow.tsx の Switch 先頭一致 parity: indexing > 起動中 > 通知。
        // 空クエリの indexing は hint が描く。非空クエリの indexing は表示ゲート（§4.7）で結果が
        // 消えるため overlay が唯一の案内（spec 追補 1・ladder は overlay_kind に抽出しテスト固定）。
        let overlay_text: Option<String> = match crate::egui_shell::overlay_kind(
            self.indexing() && self.state.view_kind() == ViewKind::Results,
            self.state.query().trim().is_empty(),
            self.launching.is_some(),
            self.notice.message().is_some(),
        ) {
            Some(crate::egui_shell::OverlayKind::Indexing) => {
                Some(crate::egui_shell::ui_strings::indexing_hint(self.lang()).to_string())
            }
            Some(crate::egui_shell::OverlayKind::Launching) => {
                Some(crate::egui_shell::ui_strings::launching(self.lang()).to_string())
            }
            Some(crate::egui_shell::OverlayKind::Notice) => self.notice.message().map(|m| m.to_string()),
            None => None,
        };
```

- [ ] **Step 6: ビルド + 全テスト確認**

Run: `cargo test -p snotra 2>&1 | tail -5`
Expected: `test result: ok`

- [ ] **Step 7: コミット**

```bash
git add src-tauri/src/egui_shell/notify.rs src-tauri/src/egui_shell/strings.rs src-tauri/src/egui_shell/mod.rs src-tauri/src/egui_shell/view.rs src-tauri/src/main.rs
git commit -m "feat: SU6 通知 parity — 非空クエリ indexing overlay + hotkey 失敗通知（spec 追補 1/2）"
```

---

### Task 8: docs 同期 + stale コメント是正（#648(B)）+ 再変換 defer issue

**Files:**
- Modify: `src-tauri/src/egui_shell/strings.rs`（`//!` 2-3 行目）
- Modify: `src-tauri/src/egui_shell/mod.rs`（re-export 行の直前コメント）
- Modify: `src-tauri/CLAUDE.md`（config_watcher イベント一覧・egui_shell 節の strings.rs 記述）
- Modify: `SPEC.md`（§7.5 末尾に additive 追記）

**Interfaces:** なし（文書のみ。`*.md` は hook 沈黙 = 未検査であることに注意——Task 9 の governance:check が捕捉）

- [ ] **Step 1: strings.rs の `//!` 是正（#648(B)）**

2-3 行目の

```
//! させる（parity の正本は i18n.ts）。言語は config `general.language` を起動時に一回読む
//! 静的解決（hot-reload＝`language-changed` 追従は SU6 の config 反映で拡張・spec 決定 10）。
```

を以下に置換:

```
//! させる（parity の正本は i18n.ts）。言語は呼び出しごとに引数で受ける——view.rs の `lang()` が
//! config `general.language` を毎フレーム live-read するため、config-applied wake（SU6）で
//! 言語切替が次フレームから反映される（起動時一回読みではない・#648(B) で旧記述を是正）。
```

- [ ] **Step 2: mod.rs の stale コメント是正**

`// view.rs が UI 文言（hint/overlay/toast）で消費する（#532 SU5・言語は起動時一回読み）` を
`// view.rs が UI 文言（hint/overlay/toast）で消費する（#532 SU5・言語は lang() が毎フレーム live-read）` に置換。

- [ ] **Step 3: src-tauri/CLAUDE.md 更新（2 箇所）**

1. `config_watcher.rs` の「発火するイベント」行の末尾に `/ config-applied（egui wake・値なし・SU6）` を追加
2. egui_shell 節の `strings.rs` 記述 `言語は config 起動時読み` を `言語は view.rs lang() の毎フレーム live-read` に置換。同節の `mod.rs` 責務列挙 `hide listener` を `hide/config-wake listener` に置換

- [ ] **Step 3b: SPEC §7.5 追記文へ通知 2 点を反映 + ロードマップ進捗節**

Step 4 の追記文の末尾に「hotkey 登録失敗は `hotkey-registration-failed` の payload を保持し表示時に整形・通知する（§7.5 ホットキー項の egui parity）」の一文を足す。また `docs/superpowers/specs/2026-07-21-phase2-softbuffer-migration-roadmap.md` の進捗節に SU6 完了行（PR 番号はマージ後に確定するため「PR #NN」placeholder のままにせず、この Step ではロードマップを触らず **Task 9 Step 5 のマージ後確認と同時に 1 行追記**でもよい——実装順はどちらでも可、忘れないことが要点）。

- [ ] **Step 4: SPEC §7.5 に as-built 追記**

§7.5 の末尾（`設定の読み込み失敗時の扱い` 項目の後）に追加:

```markdown
- egui 経路（`SNOTRA_EGUI_MAIN`・#532 SU6）: config_watcher は適用完了後に `config-applied` を発火し、egui ウィンドウはこれを再描画の合図としてのみ消費する（値は運ばず、毎フレーム実行中 config を live-read）。`indexing-started` / `indexing-complete` も同様に合図として消費し、index build 完了世代（`index_generation`）の差分で現クエリを再検索する（§4.7・#633）。font_family・ウィンドウ幅・ネイティブ背景ブラシはフレーム内のエッジ検出で追従する
```

- [ ] **Step 5: 再変換 defer issue を起票**

Write ツールでスクラッチパッドに body ファイルを作成（内容は spec 決定 5 の転記: 実装不在の根拠〔windows_ime.rs は START/COMPOSITION/END のみ・WM_IME_REQUEST 応答ゼロ〕・「製品 IMM32 コード移植」表現の訂正〔WebView2 が無償提供・移植元なし・新規実装〕・ロードマップ決定 2〔WANT・切替非ブロック〕との紐付け・flip 後判断）。

```bash
gh issue create --title "egui 経路の IME 再変換（IMR_RECONVERTSTRING）は未実装（WANT・flip 非ブロック・#532）" --body-file <スクラッチパッドのファイル>
```

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/egui_shell/strings.rs src-tauri/src/egui_shell/mod.rs src-tauri/CLAUDE.md SPEC.md
git commit -m "docs: SU6 as-built 同期（§7.5 wake 機構・strings 言語 live-read 是正 #648(B)）"
```

---

### Task 9: 検証（governance + 実機スモーク）+ PR

**Files:** なし（検証と PR のみ）

- [ ] **Step 1: governance:check**

Run: `npm run governance:check`
Expected: pass（新規ファイルは plan/spec の 2 md のみだが、CLAUDE.md/SPEC.md を触ったので必須・memory [[pr-governance-check-before-pr]]）

- [ ] **Step 2: 全テスト + clippy 最終確認**

Run: `cargo test -p snotra 2>&1 | tail -3` および `cargo clippy --workspace --all-targets 2>&1 | tail -3`
Expected: ok / 警告なし

- [ ] **Step 3: 実機スモーク（ユーザー協働・チェックリストは spec「テストと受け入れ」節）**

`$env:SNOTRA_TRACE=1; $env:SNOTRA_EGUI_MAIN=1; cargo run -p snotra` で起動し、settings サイドカー（`/o`）から変更して確認:

1. テーマ色（背景/選択行/文字色）変更が**本体可視のまま**反映される（wake 実証・従来は打鍵まで stale）
2. window_width 変更が反映される
3. font_family 変更が反映される（対象は family + 結果行 font_size。**入力欄 font_size は #643 領分で対象外**）
4. スキャンパス変更 → 再インデックス中: plain 結果が消え、**非空クエリでは overlay の「再構築中…」案内**（空クエリでは hint）・instant（`@`）候補は表示継続 → 完了後: 現クエリの結果が自動復帰
5. 小さい scan 集合で速い再構築 → stale 結果が残らない（世代カウンタのパルス耐性）
6. `ime_off_on_show=true` で Alt+Q show 時に IME がオフ
7. トレイ Exit → trace で flush（history/icon 保存）を確認。**Alt+F4 の挙動を flag ON/OFF 両方で観察**（対称なら受容・非対称なら報告）
8. `/o` → settings 起動 → alwaysOnTop 解除/復帰 → 保存 → 本体反映の end-to-end
9. hotkey 変更が次回押下から効く
10. 無効な hotkey（他アプリと衝突する等・登録に失敗する組合せ）へ変更 → 検索バー overlay に失敗通知が 5 秒表示される（spec 追補 2）

- [ ] **Step 4: push + PR 作成**

```bash
git push -u origin HEAD && gh pr create --title "SU6: 統合 glue — config 反映 wake + #633 表示ゲート/世代カウンタ + §12 IME parity（#532 Phase 2）" --body-file <スクラッチパッドの PR body ファイル>
```

PR body には `Closes #633` を含める（#633 は SU6 で close が正）。**`Part of #532` は closing keyword を使わない**（`Closes #532` と書かない——#532 は継続）。また #633 本文は「結果をクリア/再評価」と書くが実装は「クリアせず表示ゲート + 世代カウンタ」——**設計変更の経緯（SolidJS parity・instant carve-out・パルス見逃し）を PR body に 2〜3 行で明記**し、将来 #633 を読む人の誤読を防ぐ（scout-docs 提案）。

- [ ] **Step 5: マージ前 closing 確認（ルート CLAUDE.md の squash 手順 1〜4 を必ず実施）**

`gh pr view <PR> --json closingIssuesReferences` で #633 のみであることを確認してからマージ。マージ後: #532 が OPEN のまま・#633 closed・「知らない close」なし、の 3 点確認。

---

## Self-Review 結果（作成時に実施・plan-review 反映後に更新）

- **Spec coverage**: 決定 1→Task 3、決定 2→Task 5、決定 3→Task 1/2/4、決定 4→Task 6、決定 5→Task 8 Step 5、追補 1/2→Task 7、確認項目→Task 9 Step 3、付随作業→Task 8。全決定 + 追補に対応タスクあり
- **Placeholder**: なし（背景ブラシ API は tauri 2.11.4 ソースで実在確認済み・scout-egui。i18n 文言は「実装前に i18n.ts 実物から転記」を Red ステップに組込み済み）
- **型整合**: `plain_results_hidden(ViewKind, bool, bool)` / `needs_index_refresh(u64, u64)` / `index_generation: AtomicU64` / `overlay_kind(bool, bool, bool, bool) -> Option<OverlayKind>` / `hotkey_change_failed(Language, &str) -> String` は定義タスクと消費タスクで一致

## plan-review 結果の反映記録（2026-07-24）

- 要対処 1（scout-glue）: `commands/system.rs` の `AppState` リテラル 3 箇所目 → Task 2 に反映済み
- 独立導出の gap 2（一次検証済み）: hotkey 失敗通知の listener ゼロ・非空クエリ indexing の無言化 → spec 追補 1/2 + Task 7 新設
- 独立導出との不一致 2 件は計画側の根拠で維持: width の watcher 分岐案（cross-thread race・並行性レビュー）/ font の watcher diff + AtomicBool 案（dirty flag 配管を作らない・spec 決定 2）——いずれも導出側は race 分析・spec 却下履歴を持たないため
- 採用した細部: `wake_view` 集約（/dry-check）・IME trace・G-RESIZE コメント保持・PR body への #633 経緯記載・ロードマップ進捗行
