<p align="right">
  English | <a href="README.md">日本語</a>
</p>

<p align="center">
  <img src="src-tauri/icons/icon.png" width="128" height="128" alt="Snotra icon">
</p>

<h1 align="center">Snotra</h1>

<p align="center">
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

## Features

- Global hotkey (Alt+Q) for instant activation
- Three-tier search: prefix match, substring match, and fuzzy match
- History-based smart ranking
- Folder expand and navigation with arrow keys (right to expand, left to go up)
- Slash commands (`/o` settings · `/r` history · `/s` rebuild index · `/q` quit, and more)
- Icon display (on-demand extraction, toggleable in settings)
- CSS custom property-based theme system
- Automatic IME control
- System tray integration

## Getting Started

### Prerequisites

- **Windows 10/11**
- **Rust** (stable toolchain)
- **Node.js** >= 22

### Development

```bash
npm install
npm run tauri dev
```

To run type checking manually, use `npm run typecheck`. In CI, type checking is always run via `prebuild` when `npm run build` is executed.

### Release Build

```bash
npm run tauri build
```

### Tests

```bash
cargo test -p snotra-core
npm test
npm run smoke:startup
# Playwright runner + Tauri Driver
npm run e2e:tauri:setup
npm run e2e:tauri
```

## Architecture

```
Snotra/
  Cargo.toml            # Workspace (snotra-core, src-tauri)
  snotra-core/          # Pure logic library crate
  src-tauri/            # Tauri v2 binary crate (Win32 integration)
  ui/                   # SolidJS frontend
    src/
      components/       # SearchWindow, ResultRow, Settings
      stores/           # Reactive state management
      lib/              # Types, IPC wrappers, theme utilities
  .github/workflows/    # CI/CD (release pipeline)
```

- Detailed spec and state diagram: [SPEC.md](SPEC.md)

## Codex Automation

An issue-driven workflow is available that automates the full cycle from Codex implementation to Draft PR creation.
See [.github/codex-automation.md](.github/codex-automation.md) for configuration and usage rules.

## Tech Stack

<p>
  <img src="https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/Tauri_v2-24C8D8?logo=tauri&logoColor=white" alt="Tauri">
  <img src="https://img.shields.io/badge/SolidJS-2C4F7C?logo=solid&logoColor=white" alt="SolidJS">
  <img src="https://img.shields.io/badge/TypeScript-3178C6?logo=typescript&logoColor=white" alt="TypeScript">
  <img src="https://img.shields.io/badge/Vite-646CFF?logo=vite&logoColor=white" alt="Vite">
</p>

## License

This project is licensed under the [MIT License](LICENSE).

## Setup (Windows)

- **Prerequisites**: In Visual Studio 2022 (or Build Tools), enable the "Desktop development with C++" workload and the Windows SDK. Make sure `git`, `rustup`, and `node`/`npm` are on your PATH.
- **Rust**: Install the stable toolchain via rustup and add the MSVC target.
  - Commands:
    - Run `rustup-init.exe` to install
    - `rustup default stable`
    - `rustup target add x86_64-pc-windows-msvc`
- **Node.js / npm**: Install Node.js LTS (>= 22 as required). Verify with `node -v` / `npm -v`.
- **Install dependencies**: From the project root, run:
  - `npm ci` (or `npm install`)
  - To install the frontend separately: `cd ui && npm ci`
- **Tauri CLI**: Install if needed (global install is fine).
  - `npm install -g @tauri-apps/cli` or `cargo install tauri-cli`
- **Start development**:
  - Frontend only (manual): `cd ui && npm run dev`
  - Full Tauri dev from root: `npm run tauri dev`

### Troubleshooting (Common Issues)

- **`EPERM: operation not permitted, unlink ... esbuild.exe`**
  - Cause: `esbuild.exe` is locked by another process (dev server, editor extension, antivirus, etc.).
  - Fix:
    - Close all dev servers, terminals, and editor terminals.
    - Check with `tasklist | findstr /I "esbuild node"` and stop with `taskkill /F /IM esbuild.exe` or `Get-Process node | Stop-Process -Force`.
    - If the handle persists, use Sysinternals Process Explorer (Ctrl+F) or `handle.exe` to identify and close it.
    - If caused by antivirus, exclude the project folder.

- **`failed to remove file target\debug\snotra.exe` (os error 5 / Access denied)**
  - Cause: `snotra.exe` from a previous build is still running, preventing the file from being deleted.
  - Fix:
    - Check for the running process: `Get-Process -Name snotra -ErrorAction SilentlyContinue` / `tasklist | findstr /I snotra`
    - Terminate it: `taskkill /F /IM snotra.exe` or `Get-Process -Name snotra | Stop-Process -Force`.
    - If a handle remains, close it via Process Explorer / `handle.exe`.
    - Clean up: `Remove-Item .\target\debug\snotra.exe -Force` / `cargo clean`.
    - If needed, restart the terminal as administrator.

- **`linker not found` / MSVC-related build errors**
  - Fix: Make sure the "Desktop development with C++" workload and Windows SDK are installed in Visual Studio, then restart your terminal and rebuild.

- **`tauri` CLI not found / command fails**
  - Fix: Run `npm install -g @tauri-apps/cli` or `cargo install tauri-cli`. Note that the project may manage the CLI as a local devDependency, so prefer `npm run tauri dev`.

### Quick Checklist

- **Verify environment**: `node -v`, `npm -v`, `rustc --version`, `cargo --version`, `git --version`
- **Install dependencies**: `npm ci` (and `cd ui && npm ci` if needed)
- **Start**: `npm run tauri dev`
