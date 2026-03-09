---
paths:
  - "snotra-settings/**/*.rs"
---

# snotra-settings egui 実装の注意点

## API の型に注意

- `egui::Key::ALL` は `&[Key]`（`&&[Key]` ではない）。`for &key in egui::Key::ALL` が正しい
- `color_edit_button_srgba` は `&mut Color32` を取る。一時変数に変換して渡すと変更が反映されない
- `egui::Stroke::new()` に `StrokeKind` が必要（egui 0.31+）
- `ThemePreset` は `Copy`。`.clone()` ではなく値コピーで渡す

## Win キーの制限

egui の `Modifiers` は `ctrl` / `alt` / `shift` / `mac_cmd` / `command` のみ。Win キーは検出できない。ホットキーキャプチャでは Ctrl/Alt/Shift のみサポートする。

## フレームごとの重い処理を避ける

egui は毎フレーム `update()` を呼ぶ（60fps）。`list_system_fonts()` のような Win32 API 呼び出しをフレームごとに実行するとパフォーマンスが劣化する。初期化時に一度だけ取得して `SettingsApp` のフィールドにキャッシュする。
