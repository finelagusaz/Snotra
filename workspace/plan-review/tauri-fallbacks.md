# src-tauri レイヤー レビュー（ラウンド 3・最終）

対象: `view.rs` / `window_coordinator.rs` / `launcher_controller.rs` / `main.rs` / `font_stack.rs` / `mod.rs`

## 観点 1: Phase 1（`*Config::default()` 手書き→ `default_*()` 化）が Phase 2 の置換先の値に影響しないか

**問題なし。** Phase 2 が読む全フィールドを個別に突き合わせた。

- `GeneralConfig::default()` の手書き→関数化対象は `hotkey_toggle`/`show_on_startup`/`auto_hide_on_focus_lost`/`show_tray_icon`/`ime_off_on_show`/`follow_cursor_monitor` の6つ（`config.rs:158-170`）。このうち Phase 2 が読むのは `hotkey_toggle`（main.rs:371）/`auto_hide_on_focus_lost`（launcher_controller.rs:586）/`ime_off_on_show`（window_coordinator.rs:243）/`follow_cursor_monitor`（window_coordinator.rs:131）の4つ。対応する `default_hotkey_toggle()`=true / `default_auto_hide_on_focus_lost()`=true / `default_ime_off_on_show()`=false / `default_follow_cursor_monitor()`=true（`config.rs:114-136`）は手書きリテラルと完全一致。値は変わらない。
- `SearchConfig::default()` が関数化する対象（`normal_mode`/`folder_mode`/`show_hidden_system`/`history_normalization`）はいずれも Phase 2 の 12 箇所で読まれていない。Phase 2 が読む `instant_command_prefix`（launcher_controller.rs:603）は Phase 1 前後どちらでも `default_instant_command_prefix()` 呼び出しのまま変化なし（`config.rs:240-241,282`）。
- `VisualConfig::default()` が関数化する対象は `preset` のみ（`config.rs:435` → `default_theme_preset()`）。Phase 2 の 12 箇所は `preset` を読まない（読むのは `background_color`/`font_family`/`font_size`/`row_padding`/`bar_padding`/`window_gap` で、これらは Phase 1 以前から `default_*()` 呼び出し済み・`config.rs:408-427`）。
- 新設される `impl Default for AppearanceConfig`（`window_width`/`show_icons`）は現行 `Config::default()` の `appearance:` ブロック（`config.rs:555-562`、`window_width: 600` / `show_icons: true`）と値が一致する。

## 観点 2: stale な行番号参照 2 件の訂正

**問題なし（訂正は正しい）。** 実際のコメントと参照先を直接読んで確認した。

- `launcher_controller.rs:598`: `/// フィールドは config.search.instant_command_prefix（config.rs:956 で確認済み）。` — `config.rs:956` は実際には `backup_invalid` の doc コメント（`snotra_core/src/config.rs:956-960`）であり、`instant_command_prefix` とは無関係。訂正主張どおりずれている。
- `window_coordinator.rs:423`: `/// ... effective_visible_rows() で既定補完する（config.rs:327）。` — `config.rs:327` は実際には `AppearanceConfig.max_history_display: Option<usize>` フィールド（`config.rs:324-327`）であり、`visible_rows`（`:312`）でも `effective_visible_rows()` メソッド（`:332-334`）でもない。訂正主張どおりずれている。
- 直し方: 両方とも行番号ではなく型・フィールド名で指す形にする（例: `launcher_controller.rs:598` → 「`SearchConfig::instant_command_prefix`」、`window_coordinator.rs:423` → 「`AppearanceConfig::effective_visible_rows()`」への自己参照、または行番号の丸ごと削除）。`.claude/rules/governance-docs.md` の禁則は本来 `.md` 間の参照が対象（frontmatter の `paths` は `.rs` を含まない）だが、「番号は構造を凍らせ、ずれても誰も気づかない」という理由付け自体はコード内コメントの行番号参照にも同型で当てはまり、この 2 件が実例。落とし先: 計画は既に Phase 4 に修正タスクとして明記済みのため追加対応不要。

## 観点 3: 12 箇所の置換でコンパイルが通らない形がないか

**問題なし。** 型を1つずつ確認した。

- `view.rs:84` — `f64::from(AppearanceConfig::default().window_width)`。`window_width: u32`（`config.rs:313`）、`f64::from(u32)` は std に実装あり（既存コードの map 枝 `f64::from(s.engine...window_width)` が同型で既に使用・`view.rs:83`）。
- `window_coordinator.rs:189` — 同じ式だが読み元は `window.inner_size()`（OS）であり config ではない。型は問題ないが、fallback だけ config 由来にする非対称は計画も認識し Phase 4 follow-up (a) へ送っている。
- `font_stack.rs:205` — `default_visual().font_family.clone()`。`font_family: String`（`config.rs:419`）、`default_visual()` は `&'static VisualConfig` を返すため `String` は `Copy` でなく `.clone()` が必須。計画の記述どおり。
- `window_coordinator.rs:384` — `default_visual().window_gap`。`window_gap: u32`（`config.rs:427`、`Copy`）のため clone 不要で計画も付けていない。整合。
- `window_coordinator.rs:430` — `AppearanceConfig::default().effective_visible_rows() as u32`。`effective_visible_rows()` は `usize` を返す（`config.rs:332-334`）ため `as u32` が必要で、計画に明記済み。同ファイル内の既存 live-read 枝（`:429`）も同じ `as u32` パターンを使っており整合。
- `window_coordinator.rs:52-55` — `(v.font_size, v.row_padding, v.bar_padding)` はいずれも `u32`（`Copy`）で clone 不要。`:101` の `background_color` は `String` のため計画が明記するとおり `.clone()` が必須（`config.rs:409`）。
- `window_coordinator.rs:131,243`／`launcher_controller.rs:586`／`main.rs:371` — 対象フィールドはいずれも `bool`。4箇所とも現状 `.unwrap_or(リテラル)`（eager）だが、計画は `GeneralConfig::default()` が `default_language()` 経由で OS ロケールを読む I/O を伴うため、置換後は全て `.unwrap_or_else(...)` へ変えると明記しており、不変条件3と整合する。
- `launcher_controller.rs:603` — `SearchConfig::default().instant_command_prefix`（`String`）を `unwrap_or_else` で受ける。現状も既に `unwrap_or_else(|| "@".to_string())` で `String` を返しており型は変わらない。
- `mod.rs:375` — `AppearanceConfig::default().show_icons`（`bool`）を `visual::visual_snapshot(...)` の第2引数に渡す。現行呼び出し（`mod.rs:370`）が同位置に `config.appearance.show_icons`（`bool`）を渡しており型が一致する。

## 軽微な懸念

- `view.rs`・`mod.rs`・`launcher_controller.rs`・`main.rs` には現状 `AppearanceConfig`/`GeneralConfig`/`SearchConfig` の `use` インポートが無い（`window_coordinator.rs` は `snotra_core::config::VisualConfig::default()` と完全修飾パスで書いており import 済みの型が無い）。計画のファイル一覧・実装順序には import 追加が明記されていない。実装時に `use snotra_core::config::{...}` を足すか完全修飾パスで書く必要がある——瑣末だが未指示なので実装者が気づかず一瞬コンパイルエラーで止まる可能性がある。落とし先: 計画修正は不要（実装時の当然の付随作業）、念のため実装エージェントへの申し送り事項として記録。
