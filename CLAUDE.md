# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Snotra is a Windows-only keyboard launcher application written in Rust. It runs as a system tray resident app, invoked via a global hotkey (default: Alt+Space) to display a floating frameless search window for finding and launching applications.

## Build & Run

```bash
cargo build          # Debug build
cargo build --release # Release build (optimized, stripped)
cargo run            # Run in debug mode
```

No test framework is set up yet. No linter beyond `cargo check` / `cargo clippy`.

## Architecture

Pure Rust with direct Win32 API calls via the `windows` crate (Microsoft official). No web frontend, no GUI framework.

### Module Structure

- `main.rs` — Entry point, Win32 message loop (`GetMessage` / `DispatchMessage`)
- `app.rs` — Application state struct, ties all modules together
- `config.rs` — TOML config load/save from `%APPDATA%\Snotra\config.toml`
- `hotkey.rs` — Global hotkey registration via `RegisterHotKey` / `UnregisterHotKey`
- `tray.rs` — System tray icon via `Shell_NotifyIconW`, context menu
- `window.rs` — Frameless popup search window (`WS_POPUP`), Edit control, custom-painted result list. Handles `WM_PAINT`, `WM_COMMAND`, `WM_KEYDOWN`, `WM_ACTIVATE`
- `indexer.rs` — Scans Start Menu and Desktop for `.lnk` shortcuts, extracts display name and target path
- `search.rs` — Fuzzy matching with `fuzzy-matcher` (`SkimMatcherV2`)
- `launcher.rs` — Launches selected app via `ShellExecuteW`

### Key Patterns

- Window state is stored in `GWLP_USERDATA` on the `HWND` and retrieved in `WndProc`
- The search window is created hidden at startup and toggled visible/hidden on hotkey
- `WM_HOTKEY` messages are received on the message loop thread since `RegisterHotKey` is thread-bound
- `.lnk` files are parsed with the `lnk` crate (not COM `IShellLink`)

### Config Format (TOML)

```toml
[hotkey]
modifier = "Alt"
key = "Space"

[appearance]
max_results = 8
window_width = 600

[paths]
additional = []
```
