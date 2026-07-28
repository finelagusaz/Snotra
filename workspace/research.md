# research — issue #795: config 既定値のリテラル再手打ちを潰す

## issue の要約

`config.rs` の `default_*()` が返す値と同じリテラルが、コードのあちこちに手書きでコピーされている。今日はすべて一致しているため無害だが、一致を保っている機構は 1 つも無い（コメントの規範だけ）。

**ユーザー裁定（2026-07-28）**: 「置換＋不変条件のテスト」。1〜6 のリテラルを `Config::default()` 由来の参照へ置換し、9 は死んだ実装として扱い、置換で消せない 10（既定 = Obsidian プリセット）は一致をテストで固定する。

## issue の棚卸しは古い — 2 群は既に解消済み

| 群 | 現状（main = `14ebd36` で grep 実測） |
|---|---|
| 7（`tauri::window::Color(0x28, 0x28, 0x28, 0xff)` ×2） | **消滅**。`grep "0x28, 0x28, 0x28"` は 0 件。`src-tauri/src/egui_shell/mod.rs:270` に「フォールバックの正本は `VisualConfig::default()` である——`#282828` のリテラルをここへ再手打ちしない（spec 決定 4）」と明記済み |
| 8（`font_stack.rs` の `const DEFAULT_FONT_FAMILY: &str = "Segoe UI"`） | **現存**（`font_stack.rs:197,205`）。**当初「消滅」と書いたのは誤りである**——確認の grep を `\| head -6` で切り、アルファベット順で後ろに来る `src-tauri/...` を母集団から落とした。`/plan-review`「列挙の落とし穴（独立再導出が繰り返し拾った 3 クラス）」が名指しする「出力を切るオプションを列挙に使わない」の実例（#666 と同型）。`/plan-review` の独立導出が訂正した |

**残る対象は 10 群である**（issue の 10 群のうち 7 のみ消滅・8 は現存）。**加えて issue 本文が「本 issue の 11 群目にあたる」と書いている `window_gap` の群も対象に含める**（下記）。

## 対象の確定リスト（すべて grep で実在確認済み）

### A. `AppState` 不在時の fallback リテラル（置換可能・6 群 8 箇所）

| # | 位置 | リテラル | 一致する既定 |
|---|---|---|---|
| 1 | `src-tauri/src/egui_shell/view.rs:84` | `.unwrap_or(600.0)` | `appearance.window_width` = 600 |
| 2 | `src-tauri/src/egui_shell/window_coordinator.rs:189` | `.unwrap_or(600.0)` | 同上。**ただし読み元が非対称**——`view.rs` は config、ここは `window.inner_size()`（OS の現在値）の失敗時 fallback |
| 3 | `src-tauri/src/egui_shell/window_coordinator.rs:430` | `.unwrap_or(8)` | `effective_visible_rows()`（= `visible_rows.unwrap_or_else(default_visible_rows)`・`config.rs:332-334`） |
| 4 | `src-tauri/src/egui_shell/launcher_controller.rs:603` | `.unwrap_or_else(\|\| "@".to_string())` | `default_instant_command_prefix()` = `"@"`（`config.rs:205-207`） |
| 5a | `src-tauri/src/egui_shell/launcher_controller.rs:586` | `.unwrap_or(true) // config.rs 既定と一致` | `default_auto_hide_on_focus_lost()` |
| 5b | `src-tauri/src/main.rs:371` | `.unwrap_or(true) // config.rs 既定と一致` | `default_hotkey_toggle()` |
| 6a | `src-tauri/src/egui_shell/window_coordinator.rs:131` | `.unwrap_or(true)` | `default_follow_cursor_monitor()` |
| 6b | `src-tauri/src/egui_shell/window_coordinator.rs:243` | `.unwrap_or(false) // config.rs の既定値と一致` | `default_ime_off_on_show()` |

| 8 | `src-tauri/src/egui_shell/font_stack.rs:197,205` | `const DEFAULT_FONT_FAMILY: &str = "Segoe UI"` | `default_font_family()`（`config.rs:361-363`） |
| 11 | `src-tauri/src/egui_shell/window_coordinator.rs:384` | `.unwrap_or(4)` | `default_window_gap()` = 4（`config.rs:381`）。**issue 本文が「本 issue の 11 群目にあたる」と明記している**（#680 部分 2） |

### A′. 根本原因 — `AppearanceConfig` に `Default` 実装が無い

群 1・2・3 と `mod.rs:375` のリテラルは、**`AppearanceConfig` が `Default` を持たないこと**の帰結である（`config.rs:306` に `derive(Default)` も `impl Default` も無く、`Config::default()` が `:555-562` でフィールドを直接埋めている）。`mod.rs:373-375` の doc がそれを自認している。`HotkeyConfig` も同様で、既定 `"Alt"` / `"Q"` が `config.rs:551-552` と `:858-861` に二重に手書きされている。

### B. 置換の対象外（同じ `.unwrap_or` でも config 既定の写しではない）

`try_state` の `.unwrap_or` は他にもあるが、**次は状態フラグの「不在なら false」であって config 既定と無関係**である（誤って巻き込まない）:

- `launcher_controller.rs:191`（`hide_pending.swap`）・`:594`（`settings_running`）・`:611`（`indexing`）
- `window_coordinator.rs:478`（`main_visible`）
- `snotra-core/src/folder.rs:71`・`snotra-settings/src/app.rs:328,589`（config でも `try_state` でもない）

### C. `impl Default for SearchOptions`（`snotra-core/src/search.rs:66-75`）

```rust
normalization: SearchHistoryNormalizationConfig::Disabled,
fuzzy_history_cap_ratio: 0.30,
migemo_enabled: false,
migemo_min_chars: 2,
```

**issue は「本番の呼び出し元は現在ゼロ」と書くが、削除はできない**——テストが使っている（実測: `search/tests/basic.rs:291,340`、`search/tests/migemo.rs:8,11,26,171,174,376,379` で `SearchOptions::default()` / `..SearchOptions::default()`）。本番経路は `SearchOptions::from(&config.search)`（`search.rs:77`）。

### D. `PRESETS[Obsidian]`（`snotra-settings/src/tabs/visual.rs:20-28`）

`bg` / `input_bg` / `text` / `selected` / `hint` の 5 色が `#282828` / `#383838` / `#E0E0E0` / `#505050` / `#808080`。対応する既定は `config.rs:341-359` の 5 本。**`default_theme_preset()` = `ThemePreset::Obsidian`**（`config.rs:337-339`）なので、「既定 config が Obsidian カードを選択中に見せる」という UI の挙動は**この 5 本 + preset 既定の一致に依存している**。プリセットは既定値の写しではなく独立した概念であり、**置換では消せない**。

## 既存パターン — 先例は `visual.rs` にある

```rust
// src-tauri/src/egui_shell/visual.rs:33-42
/// `VisualConfig::default()` は色 5 本 + font_family の `String` を毎回ヒープ確保する。
/// `visual_snapshot` は engine の guard 内で呼ばれるため、呼び出しごとに作ると
/// **mutex を握ったまま毎フレーム 6 本確保する**（レビュー Important 1）。
static DEFAULT_VISUAL: LazyLock<VisualConfig> = LazyLock::new(VisualConfig::default);

/// AppState 不在時に渡す既定 config（確保ゼロ・`DEFAULT_VISUAL` の doc 参照）。
pub(crate) fn default_visual() -> &'static VisualConfig { &DEFAULT_VISUAL }
```

**この形が本 issue の解そのものである**——「フォールバックの正本は `*::default()`」を `LazyLock` の静的で持ち、`&'static` で配る。`read_metrics` / `visual_snapshot` / `mod.rs` の背景色が既に採用している（群 7・8 が消えたのはこの適用による）。

`Config` / `VisualConfig` / `SearchConfig` はいずれも `Default` を持つ（`config.rs:547` / `:432` / `:274`）。

## 技術的制約

- **確保コスト**: `Config::default()` は色 5 本・`font_family`・`instant_command_prefix` の `String` を確保する。群 1（`view.rs:84 window_width()`）と群 3（`max_results`）は**描画フレームごとに呼ばれる**ため、呼び出しごとの `Config::default()` は上の doc が禁じた形と同じになる。`LazyLock` の静的で `&'static` を配る形が要る
- **群 2 は他と性質が違う**: `.ok().map(...)` の対象が `window.inner_size()`（OS 呼び出し）であり、`AppState` 不在ではなく **API 失敗時**の fallback である。config の既定へ倒すのが正しいかは自明でない
- Win32 API・IPC 境界・メッセージポンプの制約はこの変更に無い（読み取り値の出所を変えるだけで、呼び出し順序も窓の生成も触らない）

## 未解決の疑問

`plan.md` の「未確定（実装前に潰す）」節へ送る（この research では解かない）:

- 静的既定の置き場所（`snotra-core` に置くか、`src-tauri` の `visual.rs` の隣に置くか。既存 `default_visual()` と統合するか）
- 群 2 の fallback 先を config 既定にしてよいか（読み元の非対称をこの issue で解くのか、受容して別 issue へ送るのか）
- 群 3 の一致（`Config::default().appearance.effective_visible_rows()` が本当に 8 か）の実測
- `impl Default for SearchOptions` を `SearchConfig::default()` から導出する形に書き換えたとき、テスト 9 箇所の期待値が変わらないか
- 群 D のテストをどこへ置くか（`snotra-settings` のユニットテスト）と、`theme_preset` 既定も含めるか
