<p align="right">
  English | <a href="README.md">日本語</a>
</p>

<p align="center">
  <img src="src-tauri/icons/icon.png" width="128" height="128" alt="Snotra icon">
</p>

<h1 align="center">Snotra</h1>

<p align="center">
  <b>Type less, launch more.</b><br>
  <i>A fast, keyboard-driven application launcher for Windows</i>
</p>

<p align="center">
  <a href="https://github.com/finelagusaz/Snotra/actions/workflows/release.yml"><img src="https://github.com/finelagusaz/Snotra/actions/workflows/release.yml/badge.svg" alt="Build"></a>
  <img src="https://img.shields.io/badge/platform-Windows-0078D4?logo=windows" alt="Platform">
  <img src="https://img.shields.io/badge/Rust-2024_edition-DEA584?logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/Tauri-v2-24C8D8?logo=tauri&logoColor=white" alt="Tauri">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="License"></a>
</p>

---

<!-- TODO: Add demo GIF here -->

## Features

- **Global hotkey** (Alt+Q) — summon the launcher from anywhere
- **Three-tier search**: prefix match, substring match, and fuzzy match
- **History-based smart ranking**: frequently used apps rise to the top
- **Folder navigation**: right arrow to expand, left arrow to go up
- **Slash commands**: `/o` settings · `/r` history · `/s` rebuild index · `/q` quit
- **Instant commands**: `@` prefix to run user-defined commands instantly (with variable expansion)
- **Custom openers**: define rules to open files with any tool you choose
- Icon display, theme customization, automatic IME control, system tray integration

## Installation

1. Download the latest `Snotra-vX.X.X.zip` from [Releases](https://github.com/finelagusaz/Snotra/releases/latest)
2. Extract the ZIP to any folder
3. Run `snotra.exe`

## Basic Usage

| Action | Result |
|--------|--------|
| `Alt+Q` | Open the search window |
| Type text | Search the index |
| `↑` / `↓` | Navigate candidates |
| `Enter` | Launch the selected app or file |
| `→` (on a folder) | Expand the folder |
| `←` | Go up to the parent folder |
| `Shift+Enter` | Choose which custom opener tool to use |
| `Escape` | Close the search window |
| `/o` | Open settings |
| `/r` | Show recent launch history |
| `/s` | Rebuild the index |
| `/q` | Quit the app |

### Instant Commands

Type `@` followed by a command name to instantly run a user-defined command.

| Input | Result |
|-------|--------|
| `@google SolidJS` | Opens a URL with `{query}` expanded |
| `@clip` | Runs a command using clipboard content (`{clip}`) |

Add or edit commands from the settings screen (`/o`).

### Direct folder access via path input

Type a path such as `C:\` or `D:\Projects\` directly into the search box to browse that folder's contents immediately.

## License

This project is licensed under the [MIT License](LICENSE).

---

Development & contributing: [CONTRIBUTING.md](CONTRIBUTING.md)
