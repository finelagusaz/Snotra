# #795 独立導出（3 回目）

**方向（ユーザー裁定 2026-07-28）**: 置換＋不変条件のテスト。`try_state` が `None` を返す経路の見直しは採らない。

## 混入の開示

指示どおり `workspace/plan.md` / `workspace/research.md` / `workspace/plan-snapshot.md` / `workspace/plan-review/` は一切読んでいない（grep も `--exclude-dir=workspace` で除外した）。ただし次の 2 行が全文 grep に混ざった。**根拠には使っていない**（内容は「`.unwrap_or_else(|| "Segoe UI".to_string())`」の 1 行で、独立に読んだ `font_stack.rs:205` と同じ事実）:

- `.superpowers/sdd/task-3-brief.md:73`
- `.superpowers/sdd/task-4-brief.md:38`

また `docs/superpowers/plans/*.md` は過去の計画書だが、`docs/` は禁止指定に含まれないため grep 結果に現れた。こちらも根拠には使わず、コードと issue のみで導出した。

## 実測の記録

作業用に `snotra-core/tests/tmp_probe_defaults.rs` を一時作成して `cargo test -p snotra-core --test tmp_probe_defaults -- --nocapture` を実行し、**実行後に削除済み**（リポジトリは clean）。出力は §3 に転記した。

---

## 0. 前提: issue の 10 群表は HEAD に対して古い

issue #795 は PR #791 のレビュー時点の観測である。その後 `docs/superpowers/specs/2026-07-28-config-background-color-design.md` に基づく変更が入り、**群 7 は既に解消している**。以下はすべて HEAD（`14ebd36`）で読み直した結果である。

| # | issue の主張 | HEAD での実測 | 判定 |
|---|---|---|---|
| 1 | `view.rs` `.unwrap_or(600.0)` | `src-tauri/src/egui_shell/view.rs:84` に現存 | **残存** |
| 2 | `window_coordinator.rs` `.unwrap_or(600.0)` | `window_coordinator.rs:189` に現存。ただし読み元は `window.inner_size()`（OS の現在値）であり **config ではない** | **残存・ただし別種**（§6） |
| 3 | `.unwrap_or(8)` | `window_coordinator.rs:430`（`max_results()`） | **残存** |
| 4 | `.unwrap_or_else(\|\| "@".to_string())` | `launcher_controller.rs:603` | **残存** |
| 5 | `.unwrap_or(true)` ×2 | `launcher_controller.rs:586`（`auto_hide_on_focus_lost`）/ `main.rs:371`（`hotkey_toggle`）。両方に `// config.rs 既定と一致` のコメント付き | **残存** |
| 6 | `window_coordinator.rs` ×2 | `:131`（`follow_cursor_monitor` = `true`）/ `:243`（`ime_off_on_show` = `false`、`// config.rs の既定値と一致`） | **残存** |
| 7 | `tauri::window::Color(0x28,0x28,0x28,0xff)` ×2 | **消滅**。`window_coordinator.rs:73` の `native_brush_color(color: egui::Color32)` が `Color32` から変換し、`mod.rs:269-271` と `visual.rs:118-122` が「`#282828` を再手打ちしない」と明示。`window_coordinator.rs:101` は `.unwrap_or_else(\|\| VisualConfig::default().background_color)` | **解消済み** |
| 7' | `renderer.rs` の `CLEAR_COLOR` | `snotra-egui-runtime/src/renderer.rs:13` に `pub const CLEAR_COLOR: u32 = 0x0028_2828;` として現存（grep `282828` に**当たらない**書式）。ただし `window_coordinator.rs:553-559` の `runtime_fallback_matches_config_default_background` が既に**機構で pin 済み** | **写しは残るが検出器あり**（§7） |
| 8 | `font_stack.rs` の `DEFAULT_FONT_FAMILY` | `font_stack.rs:197` に現存 | **残存** |
| 9 | `impl Default for SearchOptions` | `snotra-core/src/search.rs:66-75` に現存（`Disabled` / `0.30` / `false` / `2`） | **残存** |
| 10 | `snotra-settings` `PRESETS[Obsidian]` | `snotra-settings/src/tabs/visual.rs:20-29` に現存 | **残存・置換不能**（§8） |
| 11 | `.unwrap_or(4)`（#680 の 2） | `window_coordinator.rs:384` | **残存** |

**issue が数え落としている群が 2 つある**（§1 と §2）。

---

## 1. `config.rs` 内部の写し（最重要・全列挙）

issue は「`config.rs` の `default_*()` と**他ファイル**のリテラル」だけを数えているが、**`config.rs` 自身の `impl Default` が `default_*()` を呼ばずに手書きしている**箇所がある。距離が 20 行なので目には入るが、機構は他群とまったく同じくゼロである。

### 1-a. `impl Default for GeneralConfig`（`config.rs:158-171`）— 7 件中 6 件が手書き

| フィールド | 手書き値 | 対応する `default_*()` | 行 |
|---|---|---|---|
| `language` | `default_language()` | あり | :161（**唯一 SSOT 参照**） |
| `hotkey_toggle` | `true` | `default_hotkey_toggle()` :114 | :162 |
| `show_on_startup` | `false` | `default_show_on_startup()` :118 | :163 |
| `auto_hide_on_focus_lost` | `true` | `default_auto_hide_on_focus_lost()` :122 | :164 |
| `show_tray_icon` | `true` | `default_show_tray_icon()` :126 | :165 |
| `ime_off_on_show` | `false` | `default_ime_off_on_show()` :130 | :166 |
| `follow_cursor_monitor` | `true` | `default_follow_cursor_monitor()` :134 | :167 |
| `auto_update` | `AutoUpdateMode::Full` | `default_*` は無いが `#[default] Full`（:36-37）が SSOT。`AutoUpdateMode::default()` へ | :168 |

### 1-b. `impl Default for SearchConfig`（`config.rs:274-292`）— **半分だけ変換済み**

この「まだら」こそが所見である。同じ `impl` の中で 2 つの流儀が同居している。

- SSOT 参照済み: `fuzzy_history_cap_ratio: default_fuzzy_history_cap_ratio()`（:281）/ `instant_command_prefix: default_instant_command_prefix()`（:282）/ `migemo_min_chars: default_migemo_min_chars()`（:284）
- 手書き: `normal_mode: SearchModeConfig::Fuzzy`（:277）/ `folder_mode: SearchModeConfig::Fuzzy`（:278）— 両方 `default_search_mode()`（:189）がある / `show_hidden_system: false`（:279）← `default_show_hidden_system()`（:193）/ `history_normalization: SearchHistoryNormalizationConfig::Disabled`（:280）← `default_history_normalization()`（:197）
- **手書きだが欠陥ではないもの**: `migemo_enabled: false`（:283）と `include_path_env: false`（:289）は属性が素の `#[serde(default)]`（:245, :270）＝ `bool::default()` なので、`false` は既に SSOT と同一の導出である。`result_limit` / `recent_limit` / `top_n_history` / `max_history_display` の `None` は sentinel であって既定値ではない（`snotra-core/CLAUDE.md`「`Option<T>` フィールドを migration の sentinel に使う場合」が `None` を返すことを要求する）。**ここを「揃えて」`Some(default)` にしてはならない**——legacy 移行が死ぬ。

### 1-c. `impl Default for VisualConfig`（`config.rs:432-449`）— 1 件だけ手書き

`preset: ThemePreset::Obsidian`（:435）だけが `default_theme_preset()`（:337）を呼んでいない。残り 10 件はすべて SSOT 参照済み。

### 1-d. `impl Default for Config`（`config.rs:547-587`）

- `show_icons: true`（:558）← `default_show_icons()`（:185）が存在するのに手書き
- `window_width: 600`（:557）← **対応する `default_*()` が無い**（§2）
- `hotkey: HotkeyConfig { modifier: "Alt", key: "Q" }`（:550-553）← `default_*()` が無く、**同じ 2 リテラルが `fallback_hotkey_if_system_shortcut()`（`config.rs:858-861`）にもう 1 組ある**。`config.rs` 内に閉じた 2 コピーの重複であり、`default_*()` を新設しなくても `const DEFAULT_HOTKEY` か `HotkeyConfig::default_hotkey()` で 1 本にできる
- `instant_commands`（:570-585）の Google / GitHub 2 件、`paths`（:564-567）、`openers: Vec::new()` — 対応する `default_*()` は無い（§2）

---

## 2. 対応する `default_*()` が存在しないフィールド（＝写しを消そうにも参照先が無い）

`config.rs` の `default_*()` は :114 から :383 の 21 本で、**全部 private `fn`（`pub fn` は 1 本も無い）**。以下のフィールドはそこに項目を持たない。

| フィールド | 現在の既定の在り処 | 帰結 |
|---|---|---|
| `appearance.window_width` | `impl Default for Config` の `600`（:557）**のみ** | `AppearanceConfig` に `Default` が無いため、`Section::default().window_width` が**書けない**。群 1 の置換先が存在しない |
| `appearance.show_icons` | `default_show_icons()` はあるが private。`impl Default for Config` は手書き | crate 外からは参照経路なし（`mod.rs:373-375` のコメントが既にこれを名指ししている） |
| `appearance.visible_rows` / `search.result_limit` / `search.recent_limit` | `None` sentinel + `effective_*()` アクセサ | **既に SSOT 参照の正しい形**。群 3 は `effective_visible_rows()` を呼んでいて、問題は `try_state` が None のときの `.unwrap_or(8)` だけ |
| `hotkey.modifier` / `hotkey.key` | `impl Default for Config`（:551-552）と `fallback_hotkey_if_system_shortcut`（:859-860）の 2 コピー | `HotkeyConfig` に `Default` が無い |
| `general.auto_update` | `#[derive(Default)]` + `#[default] Full`（:33-37） | derive が SSOT。`AutoUpdateMode::default()` で参照可能 |
| `search.migemo_enabled` / `search.include_path_env` | `#[serde(default)]` → `bool::default()` | 既に SSOT 相当（前述） |
| `visual.custom_theme` | `None`（`Option`） | 既定の概念なし |
| `paths.additional` / `paths.scan` | `Vec::new()` / `Config::default_scan_paths()`（`pub fn`・環境依存） | `PathsConfig` に `Default` は無いが、`Vec::new()` は自明。`default_scan_paths()` は `pub` かつ環境依存で、`PathsConfig::default()` を derive すると **scan が空になり `Config::default()` と食い違う**（§10 の注意） |
| `instant_commands` | `impl Default for Config`（:570-585） | 参照先なし |

**この節が示す構造**: 「`Section::default().field` を参照する」という既存の作法（`window_coordinator.rs:53, 101` / `visual.rs:37` の `DEFAULT_VISUAL`）は、`GeneralConfig` / `SearchConfig` / `VisualConfig` にしか適用できない。`AppearanceConfig` / `HotkeyConfig` には `Default` が無い。

---

## 3. serde の 2 経路と、`toml::from_str::<Section>("")` の実測

### 3-a. 2 経路とは何か

`Config`（:91-106）は `general` / `visual` / `search` / `openers` / `instant_commands` に `#[serde(default)]` を持ち、`hotkey` / `appearance` / `paths` は持たない（`paths` は struct 級 default が無く、フィールドが全部 `#[serde(default)]`）。したがって:

- **経路 A（セクションごと欠落）**: `[general]` が config.toml に無い → `#[serde(default)]`（:94）→ `GeneralConfig::default()` → §1-a の**手書きリテラル**が使われる
- **経路 B（キーだけ欠落）**: `[general]` はあるが `hotkey_toggle =` が無い → `#[serde(default = "default_hotkey_toggle")]`（:142）→ **`default_*()` 関数**が使われる

**食い違うと何が起きるか**: `default_hotkey_toggle()` を `false` に変えて `impl Default` の `true` を直し忘れると、**同じユーザーが `[general]` セクションを書いているか否かだけで既定値が変わる**。`[general]` を丸ごと書いていないユーザー（＝ほぼ全員。`save_to_dir` は全セクションを書き出すので、実際には初回保存後は経路 B）は `true`、`[general]` はあるがキーが無いユーザー（手編集で一部だけ書いた・新キー追加直後の旧 config）は `false`。config.toml の**形**が値を決めるという、TOML の仕様にも SPEC にも書かれていない挙動になる。しかも `Config::save_to_dir` は全キーを書き戻すので、**一度保存された瞬間に経路 B の値が固定されて永続化される**——読み込みごとに違う値になるのではなく、その時点の形が焼き付く。

この 2 経路は `config.rs:1259-1304` の 2 本のテスト（`visual_padding_defaults_for_missing_keys` / `visual_field_defaults_apply_when_section_present`）が既に区別しており、後者の doc コメントは「前者は属性を外しても通ってしまう」と明記している。**つまりこの構造は既に認識されているが、`VisualConfig` の padding 3 キーについてしか測られていない。**

### 3-b. `toml::from_str::<Section>("")` の実測（一時テストで測定・出力そのまま）

```
general: Ok(GeneralConfig { language: Ja, hotkey_toggle: true, show_on_startup: false, auto_hide_on_focus_lost: true, show_tray_icon: true, ime_off_on_show: false, follow_cursor_monitor: true, auto_update: Full })
search:  Ok(SearchConfig { normal_mode: Fuzzy, folder_mode: Fuzzy, show_hidden_system: false, history_normalization: Disabled, fuzzy_history_cap_ratio: 0.3, instant_command_prefix: "@", migemo_enabled: false, migemo_min_chars: 2, result_limit: None, recent_limit: None, top_n_history: None, max_history_display: None, include_path_env: false })
visual:  Ok(VisualConfig { preset: Obsidian, background_color: "#282828", input_background_color: "#383838", text_color: "#E0E0E0", selected_row_color: "#505050", hint_text_color: "#808080", font_family: "Segoe UI", font_size: 15, row_padding: 6, bar_padding: 28, window_gap: 4, custom_theme: None })
paths:   Ok(PathsConfig { additional: [], scan: [] })
appearance: Err(Error { message: "missing field `window_width`", input: Some(""), keys: [], span: Some(0..0) })
hotkey:  Err(Error { message: "missing field `modifier`", input: Some(""), keys: [], span: Some(0..0) })
config:  false
--- eq check ---
general eq: Ok(true)
search eq: Ok(true)
visual eq: Ok(true)
paths eq: Ok(true)
```

**結論**:

- **成功する**: `GeneralConfig` / `SearchConfig` / `VisualConfig` / `PathsConfig`
- **失敗する**: `AppearanceConfig`（`window_width: u32`・`config.rs:313` に属性なし）、`HotkeyConfig`（`modifier` / `key` とも属性なし・:110-111）、`Config` 全体
- 4 型とも `PartialEq` を derive している（:138 / :228 / :404 / :459）ので `assert_eq!` が書ける
- `general eq: true` は `language` を含んで成立している。**両辺とも `default_language()` を通る**（属性経由 / `impl Default` 経由）ので、この等価テストはロケール非依存に緑になる——`sys_locale` の値が何であれ両辺が同じ関数を呼ぶ

### 3-c. ゆえに置ける不変条件テストと、置けないセクションの代替形

**置ける（最高レバレッジ・これ 1 つで 2 経路の乖離クラスが全滅する）**:

```rust
#[test] fn general_section_absent_equals_default() {
    assert_eq!(toml::from_str::<GeneralConfig>("").unwrap(), GeneralConfig::default());
}
// SearchConfig / VisualConfig も同型（3 本）
```

これは §1-a / 1-b / 1-c の手書きを**全部**カバーする（フィールドを 1 つ増やして片方だけ直し忘れても落ちる）。しかも将来フィールドを足したときに自動で母集団に入る——列挙を人が保守しない形になる。

**置けない `AppearanceConfig` / `HotkeyConfig` の代替形**（`Default` も空 parse も無いので、`Config` 経由で測るしかない）:

```rust
/// `[appearance]` は `window_width` が必須のため空 parse できない。
/// キーだけ欠落させた最小 Config で「Config::default() と同値」を測る。
#[test] fn appearance_keys_absent_match_config_default() {
    let d = Config::default();
    let toml = format!(
        "[hotkey]\nmodifier=\"Alt\"\nkey=\"Q\"\n[appearance]\nwindow_width={}\n[paths]\n",
        d.appearance.window_width);
    let c: Config = toml::from_str(&toml).unwrap();
    assert_eq!(c.appearance, d.appearance);   // 構造体まるごと
    assert_eq!(c.hotkey, d.hotkey);
}
```

**フィールドを個別に並べず、構造体まるごとで比較すること。** `AppearanceConfig`（:306）も `HotkeyConfig`（:108）も `PartialEq, Eq` を derive している。個別 assert では将来足したフィールドが母集団に入らない——**列挙を人が保守しない形にする**のが、§3-c 前半の 3 本と同じこの節の狙いである。

**この 5 本には副次的な性質がもう 1 つある。** 将来 `GeneralConfig` / `SearchConfig` / `VisualConfig` に **serde default を持たないフィールド**を足すと、`toml::from_str::<Section>("")` が `Err` になり `.unwrap()` が panic して落ちる。つまりこれらは既定値の一致だけでなく、**受理する config 形式が黙って狭まることの検出器**でもある（`/persistence-check` が問う当の性質）。テストの doc にこれを書いておく。

`window_width` 自体は**この形では測れない**（必須なので fixture に書かざるを得ず、書いた値が返るだけのトートロジーになる）。`hotkey` も同じ。**測るには型を変えるしかない**——§9 の決定事項。

---

## 4. src-tauri / snotra-settings / snotra-egui-runtime の写し（全列挙・`head` で切っていない）

`grep -rn "unwrap_or" --include=*.rs src-tauri/src snotra-egui-runtime/src snotra-settings/src`（`unwrap_or_default()` のみ除外）の全 46 行を 1 件ずつ判定した。

### 4-a. 置換対象（config 既定のリテラル写し）— 8 件

| # | `file:line` | 現在 | 置換先 |
|---|---|---|---|
| A1 | `src-tauri/src/egui_shell/view.rs:84` | `.unwrap_or(600.0)` | `appearance.window_width` の既定。**参照先が無い**（§2・§9 の決定 1 に依存） |
| A2 | `src-tauri/src/egui_shell/window_coordinator.rs:131` | `.unwrap_or(true)` | `GeneralConfig::default().follow_cursor_monitor` |
| A3 | `src-tauri/src/egui_shell/window_coordinator.rs:243` | `.unwrap_or(false) // config.rs の既定値と一致` | `GeneralConfig::default().ime_off_on_show` |
| A4 | `src-tauri/src/egui_shell/window_coordinator.rs:384` | `.unwrap_or(4)` | `VisualConfig::default().window_gap`（#680 の 2） |
| A5 | `src-tauri/src/egui_shell/window_coordinator.rs:430` | `.unwrap_or(8)` | `AppearanceConfig::default().effective_visible_rows()`。**フィールドを読んではならない**——`visible_rows` は `None` sentinel（§1-b / §2）。`effective_visible_rows()` は `pub`（:332）ゆえ src-tauri から呼べる |
| A6 | `src-tauri/src/egui_shell/launcher_controller.rs:586` | `.unwrap_or(true) // config.rs 既定と一致` | `GeneralConfig::default().auto_hide_on_focus_lost` |
| A7 | `src-tauri/src/egui_shell/launcher_controller.rs:603` | `.unwrap_or_else(\|\| "@".to_string())` | `SearchConfig::default().instant_command_prefix` |
| A8 | `src-tauri/src/main.rs:371` | `.unwrap_or(true) // config.rs 既定と一致` | `GeneralConfig::default().hotkey_toggle` |
| A9 | `src-tauri/src/egui_shell/font_stack.rs:197,205` | `const DEFAULT_FONT_FAMILY: &str = "Segoe UI"` + `.unwrap_or_else(\|\| DEFAULT_FONT_FAMILY.to_string())` | `VisualConfig::default().font_family`（`DEFAULT_VISUAL` 相当を使えば clone 1 回） |
| A10 | `src-tauri/src/egui_shell/mod.rs:376` | `visual::visual_snapshot(visual::default_visual(), true, ..)` の `true` | `show_icons` の既定。**参照先が無い**（コメント :373-375 が自ら名指し済み） |

### 4-b. 置換してはいけない — `try_state` の `unwrap_or` だが config 既定ではない

| `file:line` | 値 | 理由 |
|---|---|---|
| `mod.rs:193` | `.unwrap_or(AutoUpdateMode::Disabled)` | **config 既定は `Full`。意図的な fail-safe**（`// 勝手に更新を始めない Disabled へ倒す（#648 F）`）。置換すると設定を読めていない状態で更新を始める |
| `launcher_controller.rs:620` | `.unwrap_or(Language::Ja)` | 既定は `default_language()`＝ OS ロケール依存。**定数と一致しえない**。`GeneralConfig::default().language` へ置換すると `sys_locale::get_locale()` を極初期フレームで呼ぶことになり、意味も費用も変わる |
| `window_coordinator.rs:189` | `.unwrap_or(600.0)` | 読み元が `window.inner_size()`（**OS の現在値**）であって config ではない。`600.0` は「たまたま既定幅と同じ数」。§6 参照 |
| `launcher_controller.rs:191` | `.unwrap_or(false)` | `EguiShellState.hide_pending` の swap 戻り値 |
| `launcher_controller.rs:594` | `.unwrap_or(false)` | `SettingsProcessState` の存否 |
| `launcher_controller.rs:611` | `.unwrap_or(false)` | `AppState.indexing`（AtomicBool） |
| `window_coordinator.rs:478` | `.unwrap_or(false)` | `AppState.main_visible`（AtomicBool） |
| `main.rs:358` | `.unwrap_or(0)` | `hotkey_generation` |
| `main.rs:363` | `.unwrap_or(false)` | `main_visible` |
| `main.rs:387` | `.unwrap_or(0)` | 同上（世代） |
| `window_coordinator.rs:188` | `.unwrap_or(1.0)` | `scale_factor()` の DPI 既定 |
| `layout.rs:67` | `.unwrap_or(0.0)` ×2 | 高さ加算の `Option` 畳み込み |
| `icon_textures.rs:60` | `.unwrap_or(0)` | 試行回数マップの既定 |
| `trace.rs:48` | `.unwrap_or(0)` | seq |
| `icon.rs:424, 441` / `font_stack.rs:129` / `visual.rs:130` / `renderer.rs:108` / `runtime.rs:427` / `app.rs:161,328,347,589` / `font.rs:197` / `backup.rs:177,206` / `opener.rs:50,383` / `tabs/visual.rs:177,299` | — | config 既定と無関係（lock poison 回収・パース失敗・UI ロジック） |

### 4-c. `snotra-settings` は fallback を持たない

設定 UI は常に読み込み済みの `Config` を持つため `try_state` 相当の経路が無い。`grep` した 46 行のうち `snotra-settings` の 10 行はすべて 4-b の分類。**唯一の写しは `PRESETS`（§8）だけ**である。

`tabs/search.rs:44,52` と `tabs/visual.rs:151` は `effective_result_limit()` / `effective_recent_limit()` / `effective_visible_rows()` を呼んでおり、**既に SSOT 参照の正しい形**（DragValue の `get_or_insert` 用の既定を config から取っている）。`.range(10..=1000)` `.range(1..=50)` `.range(8..=48)` `.range(300..=1200)` は既定値ではなく**入力の値域**であり、`config.rs:1021` の `window_width < 200` とも一致しない（UI は 300、validate は 200）。これは本 issue の対象外だが、置換の巻き添えにしないよう明示しておく。

---

## 5. `snotra-core/src/search.rs` の `impl Default for SearchOptions`（群 9）

`search.rs:66-75` は `Disabled` / `0.30` / `false` / `2` を手書きしている。`impl From<&SearchConfig> for SearchOptions`（:77-86）が本番経路で、`engine.rs:126` が `SearchOptions::from(&self.config.search)` を呼ぶ。**`SearchOptions::default()` の本番呼び出し元はゼロ**（grep で確認）。

置換形は 2 つある:

- (a) `SearchOptions::from(&SearchConfig::default())` を `Default` の本体にする — 4 値すべてが 1 本の導出になり、**フィールドを足したときに自動で追従する**
- (b) 各値を `SearchConfig::default().x` で引く — 4 回書くので追従しない

(a) が構造的に強い。**`SearchOptions` に `Default` が要らないなら消す**という選択肢もあるが、これは「置換＋不変条件のテスト」の裁定の外なので、判断は決定事項（§9 決定 4）に回す。

---

## 6. 群 2 は「リテラルの写し」ではなく「読み元の非対称」である

`window_coordinator.rs:186-190`:

```rust
let width = window.inner_size().ok()
    .map(|s| s.to_logical::<f64>(window.scale_factor().unwrap_or(1.0)).width)
    .unwrap_or(600.0);
```

- `view.rs:84` は **config の `window_width`** を読み、`600.0` はその既定の写し → 群 1（置換対象）
- ここは **OS が持つ現在の窓幅**を読み、`600.0` は「取れなかったときの適当な値」→ 置換先が「config 既定」であるとは**論理的に決まらない**

issue も「幅の読み元が非対称」と書いている。**この非対称の是正（両方 config を読む / `inner_size()` を落とす）は挙動変更であり、リテラル置換の範囲を超える。** 本 issue では「`600.0` を config 既定へ揃える」だけに留めるか、非対称の是正ごと行うかを決める必要がある（§9 決定 2）。前者を選ぶなら「fallback の値としてたまたま同じ数を選び続ける」という**新しい写しを作る**ことになるので、私の結論は**触らず、コメントで読み元の違いを明記する**である。

---

## 7. `CLEAR_COLOR`（群 7 の残り）

`snotra-egui-runtime/src/renderer.rs:10-13`:

```rust
/// **`snotra-core` の `default_background_color()` と同値だが、この crate は同 crate に依存しない**
/// ——一致は機構ではなく規約であり、乖離したときに落ちる検査は無い（受容する残余）。
pub const CLEAR_COLOR: u32 = 0x0028_2828;
```

**この doc コメントは HEAD で既に偽である。** `src-tauri/src/egui_shell/window_coordinator.rs:550-559` の `runtime_fallback_matches_config_default_background` が、両 crate に依存できる唯一の crate（src-tauri）から `VisualConfig::default().background_color` と `CLEAR_COLOR` の一致を assert している。

- SU1 の crate 境界（`snotra-egui-runtime` は `snotra-core` に依存しない）は維持される。テストの置き場は src-tauri で正しい
- **やるべきは置換でも新テストでもなく、`renderer.rs:11-12` の「落ちる検査は無い（受容する残余）」を訂正すること**（§10）
- ただし `docs/development-principles.md:66` は「既定 `#282828` が `CLEAR_COLOR` と一致するため、既定のままでは正常に見え続ける」＝**一致していること自体が別のバグ（描画経路の消費者ゼロ）を隠す**と述べている。「pin して等しさを固定する」のが正しい不変条件かは自明でない。**既に pin されている**ので現状維持でよいが、pin の意味（「等しくあれ」ではなく「等しさが変わったら気づけ」）をテスト doc に書くべきである

---

## 8. 群 10（`snotra-settings` の `PRESETS`）は**置換不能**

`snotra-settings/src/tabs/visual.rs:9-29`:

```rust
struct PresetDef { preset: ThemePreset, label: &'static str, bg: &'static str, ... }
const PRESETS: &[PresetDef] = &[ PresetDef { preset: ThemePreset::Obsidian, bg: "#282828", ... }, ... ];
```

- フィールドは `&'static str`、`PRESETS` は `const`。`default_background_color()` は `String` を返す。**`const` を壊さずに置換する方法は無い**（`LazyLock<Vec<PresetDef>>` へ変えるのは「置換」ではなく型の作り替え）
- したがってここは**不変条件のテストで固定する**側に倒れる。裁定文「置換で消せないものは不変条件のテストで固定する」がまさにこの群を指す

**書くべきテストの形**: 5 本の文字列比較を並べるのではなく、**実際の述語を通す**。

```rust
#[test] fn obsidian_preset_matches_config_default() {
    // 既定 config でどのカードも強調されない/「カスタムとして保存」が最初から出る、
    // という初回起動の観測可能な帰結を、UI が実際に使う述語で固定する。
    let c = Config::default();
    let obsidian = PRESETS.iter().find(|p| p.preset == ThemePreset::Obsidian).unwrap();
    assert!(preset_matches(&c, obsidian));
    assert_eq!(c.visual.preset, ThemePreset::Obsidian);
}
```

`preset_matches`（:202-214）は `eq_ignore_ascii_case` を使うので、`#E0E0E0` vs `#e0e0e0` の差では落ちない——**5 本の `assert_eq!` を並べるより弱く見えて、実際の UI 挙動を正確に測る**。UI の観測可能な帰結（カードが強調される）を守るのが目的であり、文字列が bit 一致することではない。

`PRESETS[0]` の添字ではなく `find(|p| p.preset == Obsidian)` で引くこと（配列順の変更で黙って別のプリセットを測らないため）。

---

## 9. 決めなければならないこと（実装前の決定事項）

### 決定 1: `AppearanceConfig` に `Default` を与えるか（群 1 / 3 / A10 の可否がここに懸かる）

`window_width` / `show_icons` / `visible_rows` の 3 群は、`AppearanceConfig::default()` が無い限り置換先を持たない。選択肢:

- **(1-a) `impl Default for AppearanceConfig` を手書きする**（`#[serde(default)]` を `window_width` に付けない）。`Config` 側の `#[serde(default)]` も付けない。`toml::from_str::<Config>` の受理集合は**不変**（`[appearance] window_width` は必須のまま）。`impl Default for Config` の `appearance:` ブロック（:555-562）は `AppearanceConfig::default()` へ縮む
- **(1-b) `default_window_width()` を新設し `#[serde(default = "default_window_width")]` を付ける**。→ **`[appearance]` に `window_width` が無い config.toml が parse 成功に変わる**。今まで「壊れた config」として `.bak` 退避 + 既定起動していた入力が、黙って通るようになる。**受理する config 形式の変更**であり `/persistence-check` の対象（`snotra-core/CLAUDE.md`「データ永続化の注意」）。後方互換の方向としては緩和なので既存ユーザーは壊れないが、**`Config::validate()` の `window_width < 200` チェックを通らない経路が増えるわけではない**（validate は保存前に走る）
- **(1-c) `default_*()` 群を `pub` にする**（`pub fn default_window_width()` 等）。crate 外から関数を直接呼べるので `Default` 実装が要らない。ただし `snotra-core` は lib crate ゆえ `pub` 項目に `dead_code` が出ない（`docs/development-principles.md` が明記）＝**未使用の `pub fn` が増えても検出されない**

**私の結論は (1-a)**。受理形式を変えず、`Section::default()` という**既に repo 内で確立した作法**（`visual.rs:37` の `DEFAULT_VISUAL`、`window_coordinator.rs:53,101`）に揃う。(1-b) は本 issue の目的（リテラルの重複排除）に対して永続形式の変更という副作用が大きすぎる。

**同じ判断が `HotkeyConfig` にも要る**（`impl Default for HotkeyConfig` → `"Alt"`/`"Q"` を 1 か所へ）。こちらは `config.rs` 内 2 コピー（:551-552 と :859-860）の解消がそのまま得られる。

### 決定 2: `window_coordinator.rs:189` の `600.0` を触るか

§6 のとおり、私は**触らない**（読み元が config ではない）。触るなら非対称の是正まで含める別 issue。

### 決定 3: `GeneralConfig::default()` の呼び出しコスト

`GeneralConfig::default()` は `default_language()` → `sys_locale::get_locale()` を呼ぶ。A2 / A3 / A6 / A8 の置換で `.unwrap_or(GeneralConfig::default().x)` と書くと:

- **`unwrap_or` は引数を eager 評価する**——`AppState` が**在る**通常経路でも毎回 locale を引く。`auto_hide_enabled()` は blur ごと、`hotkey_toggle` はホットキー押下ごとに走る
- 対策は 2 つ。**(3-a) `unwrap_or_else(|| GeneralConfig::default().x)` を徹底する**（lazy 化。それでも `None` 経路では locale を引くが、そこは起動極初期の理論経路のみ）／**(3-b) `visual.rs:31-42` の `DEFAULT_VISUAL` / `default_visual()` と同型の `LazyLock<GeneralConfig>` を置く**（`DEFAULT_VISUAL` はまさに「毎回 6 本ヒープ確保する」ことを避けるために置かれた・doc :31-36）

**私の結論は (3-b)**。`visual.rs` に前例があり、`unwrap_or` / `unwrap_or_else` の取り違えという**同じ種類の見落とし**を構造的に消す。置き場は `egui_shell/` 内（`visual.rs` の `DEFAULT_VISUAL` の隣か、`config` 系の小さなモジュール）。

**この規律は `GeneralConfig` に限らない。§4-a の 10 件すべてに掛かる:**

- `SearchConfig::default()`（A7）は `String` を 1 本確保する。`instant_prefix()` は「キャッシュしない・毎回読む」設計（`launcher_controller.rs:598` の doc）＝**フレームごとに走る**
- `VisualConfig::default()`（A4 / A9）は色 5 本 + font_family の計 6 本を確保する。`visual.rs:31-36` の `DEFAULT_VISUAL` はまさにこれを避けるために置かれている。A9（`font_stack.rs`）は同 crate・同 `egui_shell` モジュール配下なので `super::visual::default_visual()`（`pub(crate)`・`visual.rs:40`）へ到達でき、確保 6 本が `clone()` 1 本になる
- `AppearanceConfig::default()`（A1 / A5 / A10）は 3 legacy `Option` + 2 スカラで確保ゼロだが、規律は同じにする

**規則は 1 本に固定する: 置換はすべて `unwrap_or_else` を使うか、`DEFAULT_VISUAL` と同型の `LazyLock` を経由する。`unwrap_or` を残さない。** 半分だけ守るのは、この issue を生んだ形そのものである。

### 決定 4: `SearchOptions::default()` を残すか消すか

本番呼び出し元ゼロ。`impl From<&SearchConfig>` があるので `Default` は `SearchOptions::from(&SearchConfig::default())` で 1 行にできる。裁定は「置換」なのでそれで足りるが、`docs/development-principles.md`「消費者ゼロ」の観点では削除も筋。**削除は別判断**として issue に残すのが安全。

---

## 10. この変更で偽になる／古くなるコメント・doc

| 場所 | 現在の記述 | 変更後の状態 |
|---|---|---|
| `src-tauri/src/egui_shell/launcher_controller.rs:586` | `.unwrap_or(true) // config.rs 既定と一致` | **削除する**。置換後は「一致」ではなく「同一」なので、コメントが規範として残ると「写しを置いてコメントを添える」作法を再生産する |
| `src-tauri/src/main.rs:371` | `.unwrap_or(true) // config.rs 既定と一致` | 同上 |
| `src-tauri/src/egui_shell/window_coordinator.rs:243` | `.unwrap_or(false) // config.rs の既定値と一致` | 同上 |
| `src-tauri/src/egui_shell/mod.rs:373-375` | `AppearanceConfig` には `Default` 実装が無いため show_icons だけは型から導けずリテラルになる——SSOT は snotra-core の `default_show_icons`（= true） | **決定 1 で (1-a) を採ると偽になる**。`AppearanceConfig::default().show_icons` へ置換し、コメントごと撤去 |
| `snotra-egui-runtime/src/renderer.rs:11-12` | `一致は機構ではなく規約であり、乖離したときに落ちる検査は無い（受容する残余）` | **HEAD で既に偽**（`window_coordinator.rs:553` が pin 済み）。本 issue の変更に関係なく訂正が要る。「検査は src-tauri の `runtime_fallback_matches_config_default_background` にある（crate 境界ゆえ両 crate に依存できる src-tauri に置く）」へ |
| `docs/development-principles.md:71`（「既定値のリテラルを写さない」の箇所） | `.unwrap_or(600.0) のような再手打ちは今日たまたま一致しているだけである（#680 の 2）` | **規範としては真のまま。例示が古くなる**（`.unwrap_or(600.0)` は 2 か所あり、片方（`view.rs:84`）は消え、もう片方（`window_coordinator.rs:189`）は config 読みではない）。例を `#680 の 2` の `window_gap` へ差し替えるか、「解消済み」と注記する |
| `docs/development-principles.md:66` | `既定 #282828 が snotra-egui-runtime の CLEAR_COLOR と一致するため、既定のままでは正常に見え続ける` | **偽にならない**（描画経路の消費者に関する記述であり、pin の有無とは別の主張）。ただし「一致に検査が無い」と読める文脈なので、pin の存在に触れると誤読が減る |
| `docs/build-commands.md:71` | `既定色での確認はこの検証にならない。config の既定 #282828 は CLEAR_COLOR と一致する` | **偽にならない**。検証手順の話で、pin とは独立 |
| `snotra-core/src/config.rs:1279-1284` | `visual_field_defaults_apply_when_section_present` の doc（「前者は属性を外しても通ってしまう」） | **偽にならないが、§3-c の等価テストを足すと役割が変わる**。等価テストは「セクション欠落 == `Default`」を測り、この 2 本は「キー欠落で属性が効く」を測る。**両方要る**（等価テストだけでは、両経路が同じように壊れた場合を検出できない）。doc に補足を足すのが望ましい |
| `src-tauri/src/egui_shell/window_coordinator.rs:428` | `visible_rows は Option<usize> のため effective_visible_rows() で既定補完する（config.rs:327）` | 行番号参照が既に stale（`effective_visible_rows` は現在 `config.rs:332`）。触る箇所なので**ついでに行番号を外す**（`.claude/rules/` の「位置はファイル名・行で断定せず見出し名・シンボル名で grep」・#588） |
| `src-tauri/src/egui_shell/launcher_controller.rs:599` | `フィールドは config.search.instant_command_prefix（config.rs:956 で確認済み）` | 同上（現在 :241）。A7 で触る行の直上なので同時に外す |

---

## 11. 変更集合（まとめ）

### snotra-core

1. `config.rs` — `impl Default for GeneralConfig` の 7 値を `default_*()` / `AutoUpdateMode::default()` 参照へ（:162-168）
2. `config.rs` — `impl Default for SearchConfig` の残り 4 値（`normal_mode` / `folder_mode` / `show_hidden_system` / `history_normalization`）を `default_*()` 参照へ（:277-280）
3. `config.rs` — `impl Default for VisualConfig` の `preset` を `default_theme_preset()` へ（:435）
4. `config.rs` — `impl Default for AppearanceConfig` を新設（決定 1-a）。`window_width` / `show_icons` / 3 legacy `None` / `visible_rows: None`。`impl Default for Config` の該当ブロック（:555-562）を `AppearanceConfig::default()` へ。**`window_width` に `#[serde(default = ...)]` は付けない**（受理形式を変えない）
5. `config.rs` — `impl Default for HotkeyConfig` を新設。`impl Default for Config`（:550-553）と `fallback_hotkey_if_system_shortcut`（:858-861）の 2 コピーを畳む
6. `config.rs` — 新テスト 3 本: `toml::from_str::<{General,Search,Visual}Config>("") == Default::default()`（§3-c）
7. `config.rs` — 新テスト 2 本（`AppearanceConfig` / `HotkeyConfig` は空 parse 不可のため `Config` 経由・§3-c 後半）
8. `search.rs` — `impl Default for SearchOptions` を `SearchOptions::from(&SearchConfig::default())` へ（:66-75）

### src-tauri

**9〜15 に共通する規律（決定 3）: `unwrap_or` を残さない。すべて `unwrap_or_else` か `LazyLock` 経由にする。**

9. `egui_shell/` に `LazyLock<GeneralConfig>`（決定 3-b）を追加し、A2 / A3 / A6 / A8 を置換 + 3 つの `// config.rs 既定と一致` コメントを削除
10. `window_coordinator.rs:384` の `.unwrap_or(4)` → `visual::default_visual().window_gap`（**#680 が完全に閉じる**——部分 1（hex パーサ 2 本立て）は spec 決定 4 で既に撤去済みで、`config_watcher.rs:19` が「`parse_hex_color` はここに無い」と明記する。PR 本文で `closingIssuesReferences` に #680 が載ることを**意図として選ぶ**か、載せないなら本文から参照を外す・ルート `CLAUDE.md`「Git/GitHub 運用」手順 1〜2）
11. `window_coordinator.rs:430` の `.unwrap_or(8)` → `.unwrap_or_else(|| AppearanceConfig::default().effective_visible_rows() as u32)`。**`visible_rows` フィールドを読まない**（`None` sentinel）+ `:428` の行番号参照の除去
12. `view.rs:84` の `.unwrap_or(600.0)` → `.unwrap_or_else(|| f64::from(AppearanceConfig::default().window_width))`（決定 1 に依存）
13. `mod.rs:376` の `true` → `AppearanceConfig::default().show_icons` + コメント :373-375 の撤去
14. `launcher_controller.rs:603` の `"@"` → `SearchConfig::default().instant_command_prefix`（`unwrap_or_else` のまま）+ :599 の行番号参照の除去
15. `font_stack.rs:197,205` の `DEFAULT_FONT_FAMILY` 撤去 → `super::visual::default_visual().font_family.clone()`（`LazyLock` 経由・確保 6 本を回避）

### snotra-settings

16. `tabs/visual.rs` — `PRESETS` は**置換せず**、`obsidian_preset_matches_config_default` を追加（§8）

### snotra-egui-runtime

17. `renderer.rs:11-12` の doc 訂正のみ（`CLEAR_COLOR` は変えない・§7）

### 触らない

18. `config.rs` の全テスト fixture（`assert_eq!(config.visual.background_color, "#282828")` :1354/:1667/:1697-1702、`default_config_has_expected_values` :1671-1716、:2814-2836 の `[visual]` ブロック等）。**これらが pin である。** `default_background_color()` へ置換するとトートロジーになり、既定値を変えたときに「気づかせる」機能が消える
19. `mod.rs:193` の `AutoUpdateMode::Disabled`（意図的 fail-safe）
20. `launcher_controller.rs:620` の `Language::Ja`（ロケール依存の既定とは一致しえない）
21. `window_coordinator.rs:189` の `600.0`（読み元が OS・§6）
22. `snotra-settings` の DragValue `.range(...)`（値域であって既定値ではない）
23. `SYSTEM_SHORTCUTS`（:1114-1120）の `"alt","f4"` 等（既定値ではなく禁止リスト）
24. **`52.0`**（`mod.rs:275` の `inner_size(window_width, 52.0)` / `view.rs:68` の `last_set_height: 52.0` / `layout.rs:315-316` のテスト）。既定値**ではない**——`default_*()` で 52 を返すものは 1 本も無く、実際の既定 bar_height は 43 である（`default_bar_padding()` の doc（:374-377）が「52 は font_size=24 でのチューニング結果（24+28）」と記す）。窓生成の初期値・memo 初期値であり、本 issue の母集団外。`window_coordinator.rs:39` の `(/simplify: 独立実装 2 箇所でフォールバックが 52.0/43.0 に乖離していた)` が示すとおり**過去に別 issue で扱われた系統**である

---

## 12. 検証（`docs/build-commands.md` の該当カテゴリ）

- `cargo test -p snotra-core`（新テスト 5 本 + 既存 pin が緑）
- `cargo clippy --workspace --all-targets -- -D warnings`（新 `impl Default` が未使用で `dead_code` にならないか。`AppearanceConfig::default()` / `HotkeyConfig::default()` は src-tauri が使うので lib crate の `pub` 経路で問題なし）
- `cargo test -p snotra`（`runtime_fallback_matches_config_default_background` / `visual.rs` の hex テスト群）
- `cargo test -p snotra-settings`（新 preset テスト）
- `npm run governance:check`（doc 参照の追加・削除がある）
- **`cargo run -p snotra` の目視は不要**（フォント登録の**規則**は変えず、`DEFAULT_FONT_FAMILY` の値を同値へ置換するだけ。ただし A9 は `font_stack.rs` を触るため `src-tauri/CLAUDE.md`「フォント登録」の規範に照らすと**視覚スモークを 1 回走らせておくのが安全側**）
- **`/persistence-check`**: 決定 1-a を採れば受理形式は不変なので不要。**1-b を採る場合は必須**
