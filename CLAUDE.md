# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Snotra is a Windows-only keyboard launcher application written in Rust. It runs as a system tray resident app, invoked via a global hotkey (default: Alt+Q) to display a floating frameless search window for finding and launching applications.

## Build & Run

```bash
cargo build          # Debug build
cargo build --release # Release build (optimized, stripped)
cargo run            # Run in debug mode
cargo test           # Run all unit tests
```

No linter beyond `cargo check` / `cargo clippy`.

## Architecture

Pure Rust with direct Win32 API calls via the `windows` crate (Microsoft official). No web frontend, no GUI framework.

### Module Structure

- `main.rs` — Entry point, Win32 message loop (`GetMessage` / `DispatchMessage`). Owns application state (previously `app.rs`), indexes apps at startup, wires 5 callbacks (`on_query_changed`, `on_launch`, `on_folder_expand`, `on_folder_navigate`, `on_folder_filter`), registers hotkey with Alt+Q fallback
- `config.rs` — TOML config load/save from `%APPDATA%\Snotra\config.toml` via `dirs` crate. `AppearanceConfig` has serde defaults for `top_n_history`, `max_history_display`, `show_icons`. `PathsConfig` has `additional` (legacy `.lnk`-only paths) and `scan` (`Vec<ScanPath>` with per-path extensions and `include_folders`). Alt+Space hotkey is silently rewritten to Alt+Q on load
- `hotkey.rs` — Global hotkey registration via `RegisterHotKey` / `UnregisterHotKey`
- `tray.rs` — System tray icon via `Shell_NotifyIconW`, context menu
- `window.rs` — Frameless popup search window (`WS_POPUP`), Edit control (child HWND), custom-painted result list. Exports `SearchResult` and `FolderExpansionState` types. Handles `WM_PAINT`, `WM_COMMAND` (EN_CHANGE), `WM_KEYDOWN`, `WM_ACTIVATE`. State stored in `thread_local! { static WINDOW_STATE }`
- `indexer.rs` — Scans Start Menu, Desktop, and additional paths for `.lnk` shortcuts; scans `ScanPath` entries for files matching per-path extensions (with optional folder entry registration). `AppEntry { name, target_path, is_folder }` with `Serialize`/`Deserialize`. Deduplicates by lowercased name. Binary index cache (`index.bin`) with config hash for invalidation; `load_or_scan()` loads cache or rebuilds. `rebuild_and_save()` for forced rebuild from settings dialog
- `icon.rs` — Icon extraction via `SHGetFileInfoW`, BGRA pixel data extraction via `GetDIBits`. `IconCache` stores `HashMap<String, IconData>` (path → BGRA pixels), persisted as `icons.bin` via bincode. Runtime `HashMap<String, HICON>` rebuilt from pixel data using `CreateIconIndirect`. `draw()` renders 16×16 icons via `DrawIconEx`
- `search.rs` — Fuzzy matching with `fuzzy-matcher` (`SkimMatcherV2`). Scoring: `fuzzy_score + (global_count × 5) + (query_count × 20)`. Also provides `recent_history()` for empty-query mode
- `history.rs` — `HistoryStore` backed by `HistoryData` (bincode-serialized). Tracks global launch counts, per-query launch counts, and folder expansion counts. Atomic write via `.bin.tmp` rename. `prune()` keeps top-N entries. Shared across callbacks via `Rc<RefCell<HistoryStore>>`
- `folder.rs` — `list_folder(dir, filter, history, max_results)` reads a directory, applies case-insensitive substring filter, sorts: folders first → expansion count descending → alphabetical
- `launcher.rs` — Launches selected app via `ShellExecuteW`

### Key Patterns

- Window state is stored in `thread_local! { static WINDOW_STATE: RefCell<Option<WindowState>> }` and accessed via closure. No `GWLP_USERDATA` usage
- The search window is created hidden at startup and toggled visible/hidden on hotkey
- `WM_HOTKEY` messages are received on the message loop thread since `RegisterHotKey` is thread-bound
- `HistoryStore` is shared across 5 callbacks via `Rc<RefCell<HistoryStore>>`. Each callback clones the `Rc`; `borrow()` / `borrow_mut()` is called inline
- Folder expansion uses a single-depth push/pop state machine. Entering a folder saves `(results, selected, query)` into `FolderExpansionState`; Escape restores it. Left-arrow navigates to OS-level parent (capped at drive root)
- Scoring formula: `combined = fuzzy_score + (global_count × GLOBAL_WEIGHT) + (query_count × QUERY_WEIGHT)` where `GLOBAL_WEIGHT = 5`, `QUERY_WEIGHT = 20`
- Atomic write in `history.rs`: serialize → write `.bin.tmp` → remove `.bin` → rename
- Alt+Space hotkey is auto-rewritten to Alt+Q in `Config::load()` (OS reserves Alt+Space). Rewrite is persisted immediately
- Index cache uses config hash (from `PathsConfig`) for invalidation; if hash changes, index is rebuilt from scratch
- Icon extraction: `SHGetFileInfoW` → `GetIconInfo` → `GetDIBits` → BGRA pixels. Restored via `CreateIconIndirect`. `IconCache` is `Rc`-shared and stored in `WindowState`
- `AppEntry` has `Serialize`/`Deserialize` for binary index cache. `is_folder` field distinguishes folder entries from file entries

### Config Format (TOML)

```toml
[hotkey]
modifier = "Alt"
key = "Q"              # Alt+Space is auto-rewritten to Alt+Q on first load

[appearance]
max_results = 8
window_width = 600
top_n_history = 200        # max entries kept in history.bin (default: 200)
max_history_display = 8    # max items shown when query is empty (default: 8)
show_icons = true          # show file/folder icons in results (default: true)

[paths]
additional = []            # legacy: paths scanned for .lnk only

[[paths.scan]]             # new: per-path extension filtering
path = "C:\\Tools"
extensions = [".exe", ".bat"]
include_folders = true     # register folders as searchable entries (default: false)
```

## Data Files

| File           | Location                         | Format  |
|----------------|----------------------------------|---------|
| `config.toml`  | `%APPDATA%\Snotra\config.toml`   | TOML    |
| `history.bin`  | `%APPDATA%\Snotra\history.bin`   | bincode |
| `index.bin`    | `%APPDATA%\Snotra\index.bin`     | bincode |
| `icons.bin`    | `%APPDATA%\Snotra\icons.bin`     | bincode |

`history.bin` contains `HistoryData`: global launch map, per-query map, folder expansion map. Written atomically via `.bin.tmp` intermediary. Pruned to `top_n_history` entries on every save.

`index.bin` contains `IndexCache`: version, timestamp, `Vec<AppEntry>`, config hash. Invalidated when config hash changes. Written atomically via `.bin.tmp`.

`icons.bin` contains `IconCacheData`: `HashMap<String, IconData>` mapping target paths to 16×16 BGRA pixel data. Rebuilt when index is rescanned.

## Implementation Status

- [x] Phase 1: History & priority system (launch counts, query-weighted scoring, empty-query recents, bincode persistence)
- [x] Phase 2: Folder expansion (right/left arrow navigation, in-folder filter, Escape to restore, expansion count ranking)
- [x] Phase 3: Index extension (per-path extensions via `ScanPath`, folder entries, icon extraction/cache via `SHGetFileInfoW`, binary index cache with config hash invalidation)
- [ ] Phase 4: Search mode extension (prefix / substring / fuzzy, per-mode config)
- [ ] Phase 5: Settings dialog (Win32 dialog, tab UI, `/o` command)
- [ ] Phase 6: Visual & misc (preset themes, IME control, hotkey toggle, titlebar, window position memory, tray toggle)

## Development Principles

### TDD

Test pure-logic modules inline with `#[cfg(test)] mod tests { ... }`. Win32 modules (`window.rs`, `hotkey.rs`, `tray.rs`, `launcher.rs`, `main.rs`) are not unit-testable as they require a running message loop or real HWNDs.

Testable modules:

- `search.rs` — fuzzy ranking, history boosting, `recent_history()`, edge cases (empty index, empty query)
- `history.rs` — increment counts, `prune()`, query-specific tracking, folder expansion, bincode roundtrip
- `config.rs` — TOML deserialization, default field injection, `ScanPath` parsing, Alt+Space → Alt+Q rewrite
- `folder.rs` — `list_folder()` with temp directories, filter, sort order
- `indexer.rs` — extension filtering, folder registration, deduplication, `IndexCache` bincode roundtrip, config hash
- `icon.rs` — `IconData` and `IconCacheData` bincode roundtrip

### KISS

- `main.rs` must not grow further. New subsystems get their own module
- Callbacks stay as thin wrappers — no business logic inside closures in `main.rs`
- Avoid adding new `thread_local` statics; consolidate into `WindowState`

### DRY

- Scoring logic lives exclusively in `search.rs` (`GLOBAL_WEIGHT`, `QUERY_WEIGHT`)
- Filter/sort logic lives exclusively in `folder.rs`
- Do not duplicate these in `window.rs` or `main.rs`
