# SU3 M3 instant + slash Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** egui/softbuffer メインウィンドウに、WebView2(SolidJS) と parity なインスタントコマンド（`@`前方一致・同期直フィルタ・実行）とスラッシュコマンド（`/r /o /s /q`・完全一致即実行）を直 Engine で足す。

**Architecture:** 設計 SSOT は `docs/superpowers/specs/2026-07-22-su3-search-experience-design.md`「M3 実装確定」節。instant fetch は 30ms debounce を撤廃し `filter_instant_commands` を毎打鍵同期実行、instant 実行は同期直呼び（`launch_item_core`/`launch_exec_core`・ブロックリスクは #631）。slash は TextEdit changed エッジで edge-trigger（`/r` のみ結果注入型で `run_search` の Command 分岐が冪等に扱う）。Enter/クリックの activate は changed 処理より**後**で interp 判定する（codex 発見 4）。Escape ラダーは M2 の 2 段のまま（ClearMode 段は parity 乖離ゆえ削除済み）。

**Tech Stack:** Rust / egui 0.35 / Tauri v2 / snotra-core（`instant.rs`）/ arboard。

## Global Constraints

- **flag OFF（`SNOTRA_EGUI_MAIN` 未設定）で WebView2 経路・IPC コマンド・E2E 注入は完全不変（G1）。** IPC の `get_instant_commands`/`execute_instant_command` は触らない。egui 経路は crate 内の core fn（`filter_instant_commands`/`expand_instant_command`/`launch_*_core`）を直接呼ぶだけ（additive）。
- **instant 実行は同期直呼び**（spec M3 実装確定）。`run_launch_blocking`（spawn_blocking + 4s）は通さない——SPEC §19.6 乖離は受容済み残余（#631 コメント 2026-07-23 に記録済み）。
- **失敗通知は M1 同型**: hide しない + `trace_main` のみ。通知 UI を建てない（#631 一本化）。`/s` の indexing 中 Err は無音（#434 parity）。
- **純粋核（`search_state.rs`/`layout.rs`）はユニットテスト。view.rs はユニットテスト前提にしない**（clippy + trace スモーク・`.claude/rules/src-tauri.md`）。
- **各 green でコミット。** `cargo test -p snotra` / `cargo clippy -p snotra --all-targets -- -D warnings`（`docs/build-commands.md`）。実機スモークは flag ON で `msedgewebview2.exe` 子孫 0。

---

## File Structure

- `src-tauri/src/egui_shell/search_state.rs`（Modify）: `SlashCmd` enum + `find_slash_command` 純関数（trim 後完全一致・§15.3 判定の SSOT）、`SearchState::reset_selection`（毎打鍵 selected=0 の SolidJS parity・M1 持ち越し gap の是正）。ユニットテスト対象。
- `src-tauri/src/egui_shell/layout.rs`（Modify）: `Debouncer::cancel`（instant/command 突入時に armed trailing を掃除する・SolidJS `cancelDebounce` parity）。ユニットテスト対象。
- `src-tauri/src/egui_shell/mod.rs`（Modify）: `SlashCmd`/`find_slash_command` の re-export 1 行。
- `src-tauri/src/egui_shell/view.rs`（Modify）: `run_search` の Command/Instant 分岐、changed エッジ dispatch 再構成、`execute_slash`/`execute_instant_selected`/`activate_or_execute`、Enter 後置、← の instant ガード。clippy + trace スモーク検証。

---

## Task 1: 純粋核（`search_state.rs` + `layout.rs`）

**Files:**
- Modify: `src-tauri/src/egui_shell/search_state.rs`
- Modify: `src-tauri/src/egui_shell/layout.rs`
- Modify: `src-tauri/src/egui_shell/mod.rs`（re-export）
- Test: 各ファイルの `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: 既存 `SearchState`（`selected` field・`clamp_selected`）、既存 `Debouncer`（`armed` field）。
- Produces:
  - `SlashCmd { History, OpenSettings, RebuildIndex, Quit }`（`Debug, Clone, Copy, PartialEq, Eq`）
  - `find_slash_command(query: &str) -> Option<SlashCmd>`（trim 後完全一致・大文字小文字区別）
  - `SearchState::reset_selection(&mut self)`（selected=0）
  - `Debouncer::cancel(&mut self)`（disarm・trailing 取り消し）
  - `mod.rs` re-export: `pub(crate) use search_state::{SlashCmd, find_slash_command};`

- [ ] **Step 1: `find_slash_command` の failing test を書く**

`search_state.rs` の tests に追加（`ui/src/lib/commands.ts` の `findCommand`（`c.command === trimmed`）parity）:

```rust
#[test]
fn find_slash_command_exact_match_with_trim() {
    assert_eq!(find_slash_command("/r"), Some(SlashCmd::History));
    assert_eq!(find_slash_command(" /o "), Some(SlashCmd::OpenSettings)); // trim 後一致
    assert_eq!(find_slash_command("/s"), Some(SlashCmd::RebuildIndex));
    assert_eq!(find_slash_command("/q"), Some(SlashCmd::Quit));
}

#[test]
fn find_slash_command_rejects_partial_case_and_args() {
    assert_eq!(find_slash_command("/"), None);        // 部分入力
    assert_eq!(find_slash_command("/x"), None);       // 未知コマンド
    assert_eq!(find_slash_command("/O"), None);       // 大文字は不一致（findCommand === parity）
    assert_eq!(find_slash_command("/o extra"), None); // 引数付きは不一致（完全一致のみ）
    assert_eq!(find_slash_command(""), None);
}
```

- [ ] **Step 2: 落ちるのを確認**

Run: `cargo test -p snotra find_slash_command`
Expected: FAIL（`find_slash_command` 未定義でコンパイルエラー）

- [ ] **Step 3: `SlashCmd` + `find_slash_command` を実装 + re-export**

`search_state.rs` に追加（`EscapeOutcome` の近く）:

```rust
/// slash コマンドの写像（§15.2）。History(`/r`) だけは結果注入型（履歴を表示して留まる）で、
/// driver が run_search の Command 分岐へ振る。他 3 つは fire-once の副作用型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashCmd {
    History,
    OpenSettings,
    RebuildIndex,
    Quit,
}

/// trim 後の完全一致で slash コマンドを引く（§15.3 即実行の判定・commands.ts findCommand parity・
/// 大文字小文字は区別する）。部分入力・引数付きは None（候補表示なし・§15.3）。
pub fn find_slash_command(query: &str) -> Option<SlashCmd> {
    match query.trim() {
        "/r" => Some(SlashCmd::History),
        "/o" => Some(SlashCmd::OpenSettings),
        "/s" => Some(SlashCmd::RebuildIndex),
        "/q" => Some(SlashCmd::Quit),
        _ => None,
    }
}
```

`mod.rs` の既存 `pub(crate) use search_state::{interpret, is_instant_prefix};` の近くに追加:

```rust
pub(crate) use search_state::{SlashCmd, find_slash_command};
```

- [ ] **Step 4: 通るのを確認**

Run: `cargo test -p snotra find_slash_command`
Expected: PASS

- [ ] **Step 5: `reset_selection` の failing test を書く**

```rust
#[test]
fn reset_selection_returns_to_top() {
    // SolidJS parity: 毎打鍵 setSelected(0)（handlePlainQueryInput / instant fetch / slash とも）。
    // M1 は set_results の clamp のみで、打鍵後も旧 selected が残っていた（M3 で是正）。
    let mut s = SearchState::new();
    s.set_results(vec![res("a"), res("b"), res("c")]);
    s.move_selection(2);
    assert_eq!(s.selected(), 2);
    s.reset_selection();
    assert_eq!(s.selected(), 0);
}
```

- [ ] **Step 6: 落ちるのを確認 → 実装 → 通す**

Run: `cargo test -p snotra reset_selection_returns_to_top`
Expected: FAIL（未定義）

`SearchState` impl に追加:

```rust
    /// 選択を先頭へ戻す。driver が打鍵（changed エッジ）ごとに呼ぶ（SolidJS の毎打鍵
    /// setSelected(0) parity・#532 SU3 M3）。
    pub fn reset_selection(&mut self) {
        self.selected = 0;
    }
```

Run: `cargo test -p snotra reset_selection_returns_to_top`
Expected: PASS

- [ ] **Step 7: `Debouncer::cancel` の failing test を書く**

`layout.rs` の tests に追加:

```rust
    #[test]
    fn cancel_disarms_pending_trailing() {
        let mut d = Debouncer::new(Duration::from_millis(50), true);
        d.on_input();
        assert!(d.is_armed());
        d.cancel();
        assert!(!d.is_armed());
        assert!(!d.poll(Duration::from_millis(100)), "cancel 後は trailing 発火しない");
        // cancel 後の次入力はバースト先頭扱い（leading 再発火）
        assert!(d.on_input());
    }
```

- [ ] **Step 8: 落ちるのを確認 → 実装 → 通す**

Run: `cargo test -p snotra cancel_disarms_pending_trailing`
Expected: FAIL（`cancel` 未定義）

`Debouncer` impl に追加:

```rust
    /// armed を解除し予約済み trailing を取り消す（SolidJS cancelDebounce parity・#532 SU3 M3）。
    /// instant/command モード突入時に driver が呼ぶ——モード外で予約された検索が
    /// モード中に遅延発火する経路を塞ぐ（run_search は再導出ゆえ実害は無いが無駄撃ちを消す）。
    pub fn cancel(&mut self) {
        self.armed = false;
    }
```

Run: `cargo test -p snotra cancel_disarms_pending_trailing`
Expected: PASS

- [ ] **Step 9: clippy + 全テスト**

Run: `cargo clippy -p snotra --all-targets -- -D warnings && cargo test -p snotra egui_shell`
Expected: clean + PASS（既存 M1/M2 テスト含め全緑）

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/egui_shell/search_state.rs src-tauri/src/egui_shell/layout.rs src-tauri/src/egui_shell/mod.rs
git commit -m "feat(egui): M3 純粋核（SlashCmd/find_slash_command・reset_selection・Debouncer::cancel）#532 SU3 M3"
```

---

## Task 2: driver 配線（`view.rs`）

**注**: `view.rs` は egui/Win32 依存ゆえユニットテスト前提にしない（`.claude/rules/src-tauri.md`）。検証は `cargo clippy` + flag ON の trace スモーク。各ステップは concrete edit。

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`

**Interfaces:**
- Consumes: Task 1（`SlashCmd`/`find_slash_command`/`reset_selection`/`Debouncer::cancel`）、既存 `QueryIntent`（`interp`）、`snotra_core::instant::{filter_instant_commands, expand_instant_command}`、`snotra_core::config::InstantAction`、`crate::commands::launch::{InstantCommandDto, launch_item_core, launch_exec_core, LaunchStatus}`、`crate::commands::{open_settings, rebuild_index}`（`main.rs:setup_open_settings_listener` の直呼び前例と同型）、`Engine::recent_history`。

- [ ] **Step 1: import を追加**

`view.rs` 冒頭の `use crate::egui_shell::{...}` に `SlashCmd, find_slash_command` を追加:

```rust
use crate::egui_shell::{
    Debouncer, EscapeOutcome, HeightParams, QueryIntent, SearchState, SlashCmd, ViewKind,
    // …既存の他項目は不変…
    find_slash_command,
};
```

- [ ] **Step 2: `run_search` の Command/Instant 分岐を実装**

現 `run_search` の `// command/instant は M3。M1/M2 では結果を出さない（空維持）。` の `_ =>` 腕を削除し、明示 2 腕へ置換:

```rust
                    QueryIntent::Instant { filter_name, .. } => {
                        // §19.5: 前方一致フィルタ。毎打鍵同期（30ms debounce 撤廃・spec M3 実装確定）。
                        // indexing を見ない（§19.7: instant はインデックス非依存ゆえ構築中でも使用可）。
                        let rows = {
                            let state = match self.app_handle.try_state::<crate::AppState>() {
                                Some(s) => s,
                                None => return,
                            };
                            let engine = state.engine.lock().unwrap();
                            snotra_core::instant::filter_instant_commands(
                                &engine.config().instant_commands,
                                &filter_name,
                            )
                            .into_iter()
                            .map(|c| {
                                let dto = crate::commands::launch::InstantCommandDto::from(c);
                                SearchResult {
                                    name: dto.name,
                                    // §19.5: description 設定時は優先、無ければ display（URL / exe args）
                                    path: if dto.description.is_empty() { dto.display } else { dto.description },
                                    is_folder: false,
                                    is_error: false,
                                }
                            })
                            .collect::<Vec<_>>()
                        }; // lock 解放
                        self.state.set_results(rows);
                    }
                    QueryIntent::Command => {
                        // §15.2 /r: 履歴を注入して留まる（冪等ゆえ trailing 再発火も無害）。
                        // 他（部分入力・実行済み直後）は候補なしクリア（§15.3: command 中は検索しない）。
                        if matches!(find_slash_command(self.state.query()), Some(SlashCmd::History)) {
                            let rows = {
                                let state = match self.app_handle.try_state::<crate::AppState>() {
                                    Some(s) => s,
                                    None => return,
                                };
                                let engine = state.engine.lock().unwrap();
                                engine.recent_history()
                            };
                            self.state.set_results(rows);
                        } else {
                            self.state.set_results(Vec::new());
                        }
                    }
```

（`recent_history` が `&mut self` を要求してコンパイルエラーになる場合は `let mut engine` にする——`commands/search.rs:get_history_results` と同じ呼び方に合わせる。）

- [ ] **Step 3: `execute_slash` を追加**

`SearchWindowView` impl に追加（`activate` の近く）:

```rust
    /// slash コマンドを実行する（§15.3 即実行・#532 SU3 M3）。SolidJS handleCommandQueryInput と
    /// 同順: クエリ/結果クリア（clearCommandModeState 相当）→ action。`/r`（History）は結果注入型で
    /// ここへ来ない（changed ハンドラが run_search へ振る）。失敗通知は建てない（trace のみ・#631 一本化）。
    fn execute_slash(&mut self, cmd: SlashCmd) {
        crate::trace_main("egui_slash", serde_json::json!({ "cmd": format!("{cmd:?}") }));
        self.state.set_query(String::new());
        self.state.set_results(Vec::new());
        self.search_debounce.cancel();
        let app = self.app_handle.clone();
        match cmd {
            SlashCmd::History => {} // 到達しない（呼び出し側 match が run_search へ振る）
            SlashCmd::OpenSettings => {
                // indexing 中の Err（ERR_INDEXING_IN_PROGRESS）は trace のみ（spec M3 実装確定・
                // クエリクリア後は検索バーの indexing hint が可視＝degraded な理由提示）。
                if let Err(e) = crate::commands::open_settings(app.state(), app.clone()) {
                    crate::trace_main("egui_slash_error", serde_json::json!({ "cmd": "/o", "error": e }));
                }
            }
            SlashCmd::RebuildIndex => {
                // SolidJS /s parity: hide してから rebuild（hide は emit 合流・順序は視覚のみで
                // rebuild は backend スレッド）。indexing 中の Err は意図的無音（#434 parity）。
                self.emit_hide();
                if let Err(e) = crate::commands::rebuild_index(app.state(), app.clone()) {
                    crate::trace_main("egui_slash_error", serde_json::json!({ "cmd": "/s", "error": e }));
                }
            }
            SlashCmd::Quit => {
                // quit_app（commands/system.rs）と同一実体: exit-requested listener が
                // history/icon flush → exit（main.rs）。egui 経路も同じ合流点を使う。
                let _ = app.emit("exit-requested", ());
            }
        }
    }
```

- [ ] **Step 4: `execute_instant_selected` を追加**

```rust
    /// 選択中の instant コマンドを同期実行する（§19.6・#532 SU3 M3）。IPC の
    /// execute_instant_command（spawn_blocking + 4s）と同じ手順（action 抽出をロック内・
    /// clipboard 読みをロック外）を、イベントループで同期直呼びに畳む（spec M3 実装確定・
    /// ブロックリスクは #631 スコープ）。instant は履歴を記録しない（IPC 経路 parity）。
    /// 成功: クエリ/結果クリア + hide（§19.6）。失敗: 据え置き + trace（M1 起動失敗と同型）。
    fn execute_instant_selected(&mut self, index: usize, instant_query: &str) {
        use crate::commands::launch::{LaunchStatus, launch_exec_core, launch_item_core};
        use snotra_core::config::InstantAction;
        use snotra_core::instant::expand_instant_command;
        let Some(sel) = self.state.results().get(index) else { return };
        if sel.is_error {
            return;
        }
        let name = sel.name.clone();
        let Some(state) = self.app_handle.try_state::<crate::AppState>() else { return };
        let Some(action) = ({
            let engine = state.engine.lock().unwrap();
            engine
                .config()
                .instant_commands
                .iter()
                .find(|c| c.name == name)
                .map(|c| c.action.clone())
        }) else {
            return;
        };
        // clipboard 読み（Win32）はロック外（commands/instant.rs と同順）。
        let clipboard = arboard::Clipboard::new()
            .and_then(|mut cb| cb.get_text())
            .unwrap_or_default();
        let outcome = match action {
            InstantAction::Url { url } => {
                launch_item_core(&expand_instant_command(&url, instant_query, &clipboard))
            }
            InstantAction::Exec { exe, args } => {
                launch_exec_core(&exe, &args, instant_query, &clipboard)
            }
            // load 後は移行済みで到達しないが、防御的に Url 扱い（IPC 経路と同じ）
            InstantAction::Legacy { command } => {
                launch_item_core(&expand_instant_command(&command, instant_query, &clipboard))
            }
        };
        crate::trace_main(
            "egui_instant",
            serde_json::json!({ "name": name, "status": format!("{:?}", outcome.status) }),
        );
        if matches!(outcome.status, LaunchStatus::Ok) {
            self.state.set_query(String::new());
            self.state.set_results(Vec::new());
            self.search_debounce.cancel();
            self.emit_hide();
        }
    }

    /// Enter/クリックの単一 dispatch（§19.6/§4.8・#532 SU3 M3）。instant モード中は選択コマンドを
    /// 実行、それ以外（plain・/r 履歴・folder）は通常起動（activate）。行 index で参照
    /// （パス文字列を使わない・ui ルール踏襲）。Shift+Enter も同じ Enter として届くため
    /// §19.6「Shift+Enter=Enter」は追加コードなしで成立する（tool-selection は SU3.5）。
    fn activate_or_execute(&mut self, index: usize) {
        let prefix = self.instant_prefix();
        if let QueryIntent::Instant { instant_query, .. } = self.state.interp(&prefix) {
            self.execute_instant_selected(index, &instant_query);
        } else {
            self.activate(index);
        }
    }
```

- [ ] **Step 5: changed エッジの dispatch を interp 分岐へ再構成**

現 `if response.changed() { ... }` の else（Results）側を置換:

```rust
        if response.changed() {
            if in_folder {
                self.state.set_folder_filter(buf);
                self.run_search(); // folder は同期フィルタ（debounce 不要・I/O 無し）
            } else {
                self.state.set_query(buf);
                self.state.reset_selection(); // SolidJS parity: 毎打鍵 selected=0（M1 gap 是正）
                let prefix = self.instant_prefix();
                match self.state.interp(&prefix) {
                    QueryIntent::Plain => {
                        self.last_input_at = Instant::now();
                        if self.search_debounce.on_input() {
                            self.run_search(); // leading
                        }
                        ctx.request_repaint_after(self.search_debounce.interval());
                    }
                    QueryIntent::Instant { .. } => {
                        // 同期直フィルタ（30ms debounce 撤廃・spec M3 実装確定）。
                        // plain 由来の armed trailing は掃除（cancelDebounce parity）。
                        self.search_debounce.cancel();
                        self.run_search();
                    }
                    QueryIntent::Command => {
                        // §15.3: debounce をキャンセルして即実行（changed エッジ＝query 変化時
                        // のみゆえ immediate-mode でも fire-once）。/r と部分入力は run_search の
                        // Command 分岐（冪等: /r=履歴注入・他=結果クリア）。
                        self.search_debounce.cancel();
                        match find_slash_command(self.state.query()) {
                            Some(SlashCmd::History) | None => self.run_search(),
                            Some(cmd) => self.execute_slash(cmd),
                        }
                    }
                }
            }
        }
```

- [ ] **Step 6: Enter 処理を changed 処理の後へ移し dispatch を統一**

現在 TextEdit より前にある Enter ブロック（`// Enter: 選択項目を起動（結果があるとき）。TextEdit の Enter より先に ctx で拾う。` + `if ctx.input(...Enter...) { self.activate(...) }`）を**削除**し、trailing debounce ブロック（`self.search_debounce.poll(...)` と `is_armed()` の再要求）の**直後**（`let show_results = ...` の前）に挿入:

```rust
        // Enter: 選択項目を起動/実行。TextEdit の changed 処理より後で判定する——同一フレームに
        // 入力確定（貼り付け・IME 確定）と Enter が入ったとき、旧 state の interp/選択で起動
        // しないため（codex 発見 4・spec M3 実装確定）。egui の input はフレーム内で不変
        // （読む順序は消費に影響しない）ため後置しても Enter は取りこぼさない。
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !self.state.results().is_empty() {
            self.activate_or_execute(self.state.selected());
        }
```

クリック合流点 `if let Some(i) = clicked { self.activate(i); }` を差し替え:

```rust
        if let Some(i) = clicked {
            self.activate_or_execute(i);
        }
```

- [ ] **Step 7: ← の instant ガードを追加**

ArrowLeft の `ViewKind::Results` 腕の条件に interp ガードを足す（§19.7: instant 中の ←→ 無効。
→ は instant 行が `is_folder=false` ゆえ既存ガードで構造的に無反応・追加不要）:

```rust
                ViewKind::Results => {
                    // instant/command 中は ← 無効（§19.7）。instant 行の path は description/display
                    // ゆえ compute_parent_dir が偶然 Some を返して bogus folder 突入しうるのを塞ぐ。
                    if matches!(self.state.interp(&self.instant_prefix()), QueryIntent::Plain)
                        && let Some(sel) = self.state.results().get(self.state.selected())
                        && !sel.is_error
                        && let Some(parent) = compute_parent_dir(&sel.path)
                    {
                        let tok = self.state.enter_folder(parent.clone());
                        // ← from Results は enterFolderExpansion(parent) 相当・記録する。
                        self.record_folder_expansion(&parent);
                        self.folder_cache = None;
                        self.folder_error = None;
                        self.spawn_folder_load(tok, parent, ctx.clone());
                    }
                }
```

- [ ] **Step 8: clippy**

Run: `cargo clippy -p snotra --all-targets -- -D warnings`
Expected: clean

- [ ] **Step 9: flag OFF 完全不変（G1）確認**

Run: `cargo test -p snotra && cargo test -p snotra-core`（flag 未設定）
Expected: 既存テスト全緑（WebView2 経路・IPC・core とも不変。M3 の追加は egui view と純粋核のみ）

- [ ] **Step 10: trace スモーク（flag ON・実機）**

Run: `$env:SNOTRA_EGUI_MAIN=1; $env:SNOTRA_TRACE=1; cargo run -p snotra --release`（`docs/build-commands.md` の起動手順に従う）
確認（目視 + trace）:

- `@` のみ → 登録コマンド全件表示（既定 `g`/`gh`）・アイコンスロット無しでも行が読める
- `@g` → 前方一致絞り込み・**打鍵と同フレームで候補更新**（30ms 遅延なし）
- `@g rust` + Enter → 既定ブラウザで Google 検索・クエリクリア・hide（trace `egui_instant` status=Ok）
- `@unknown` → 結果空（noResults 表示なし・§19.5）
- instant 中 `←`/`→` 無反応（folder 突入しない）・Shift+Enter=Enter
- instant 行クリック → Enter と同じ実行
- `/s` 直後（indexing 中）に `@g` が使える（§19.7）・plain 検索は空のまま
- `/r` → 履歴表示・**留まる**（クエリは `/r` のまま）・↑↓ 選択 + Enter で履歴項目起動
- `/o` → 設定起動 + クエリクリア。indexing 中は開かず、空クエリの bar に indexing hint が可視（trace `egui_slash_error`）
- `/s` → hide + 再構築開始（trace `egui_slash` cmd=RebuildIndex）
- `/x`（部分入力） → 結果空・検索走らず
- instant/command 中の Escape → **即 hide**（ClearMode なし・SolidJS/SPEC §8.6 parity）
- 打鍵で selected が先頭へ戻る（plain でも・M1 gap 是正の確認）
- `/q` → 終了（trace で flush 経路確認）※最後に実行
- `msedgewebview2.exe` 子孫 0（`/q` 前に確認）

- [ ] **Step 11: Commit**

```bash
git add src-tauri/src/egui_shell/view.rs
git commit -m "feat(egui): instant+slash driver 配線（同期直フィルタ・edge-trigger 即実行・Enter 後置 dispatch）#532 SU3 M3"
```

---

## Self-Review

**Spec coverage（spec「M3 実装確定」節 + §15/§19 → タスク）:**
- instant 同期直呼び + #631 残余 → Task 2 Step 4（`execute_instant_selected` doc に明記）。
- 30ms debounce 撤廃 → Task 2 Step 2/5（同期直フィルタ・`Debouncer::cancel` は Task 1 Step 7-8）。
- ClearMode 削除 → コード変更なし（M2 の `on_escape` のまま）。スモーク Step 10 で「instant/command 中 Escape=即 hide」を明示確認。
- 失敗通知 M1 同型 + `/o` indexing hint 緩和 → Task 2 Step 3（trace のみ・hint はクエリクリアで自然可視）。
- dispatch 再構成（changed → Enter）→ Task 2 Step 5/6。クリックも `activate_or_execute` へ統一（Step 6）。
- slash edge-trigger・`/r` 注入・`/o` `/s` `/q` 写像 → Task 2 Step 2/3/5。`/q` は `emit("exit-requested")`（`quit_app` と同一実体）。
- instant フィルタ/DTO 変換/アイコンスキップ（§19.5）→ Task 2 Step 2（アイコンは M1 から placeholder スロットのみ＝スキップ相当。SU4 で instant 非表示を実装）。
- ←→ 無効（§19.7）→ Task 2 Step 7（← ガード・→ は is_folder=false で構造的無反応）。Shift+Enter=Enter（§19.6）→ egui は修飾キー無視で Enter が届くため追加コードなし（Step 4 doc に記載）。
- indexing 非対称（§19.7）→ run_search の Plain 分岐のみ indexing ガード（既存・Step 2 は見ない）。
- selected リセット parity → Task 1 Step 5-6 + Task 2 Step 5（受け入れ条件に含める・スモークで確認）。
- resetForShow の instant 解除（§19.7）→ 既存 `reset()`（query クリアで interp が Plain に戻る・追加不要）。

**Placeholder scan:** なし（全ステップ concrete code / command / 期待値付き）。

**Type consistency:** `SlashCmd`/`find_slash_command` は Task 1 定義 → mod.rs re-export → Task 2 が `crate::egui_shell` 経由で消費。`execute_instant_selected(index: usize, instant_query: &str)` は `activate_or_execute` からのみ呼ばれ、`instant_query` は `QueryIntent::Instant { instant_query, .. }`（owned String）から借用。`Debouncer::cancel` は `search_debounce`（既存 field）に対して呼ぶ。`InstantCommandDto::from(&InstantCommand)` は既存 impl（launch.rs:376）。

**実装時確認点（着手時に潰す）:**
- `recent_history` の `&self`/`&mut self`（Step 2 の注記どおり呼び方を合わせる）。
- `open_settings`/`rebuild_index` の `app.state()` 直呼びが view から通るか（`main.rs:setup_open_settings_listener` 前例あり。`use tauri::Manager` が view.rs に既存か確認）。
- `arboard` は `commands/instant.rs` で使用済み（同一 crate 依存・追加不要）。
