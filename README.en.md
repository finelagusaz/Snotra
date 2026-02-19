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
- In-folder navigation with arrow keys
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

### Release Build

```bash
npm run tauri build
```

### Tests

```bash
cargo test -p snotra-core
```

## Architecture

```
Snotra/
  snotra-core/          # Pure logic library crate
  src-tauri/            # Tauri v2 binary crate (Win32 integration)
  ui/                   # SolidJS frontend
    src/
      components/       # SearchWindow, ResultRow, Settings
      stores/           # Reactive state management
      lib/              # Types, IPC wrappers, theme utilities
  .github/workflows/    # CI/CD (release pipeline)
```

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
