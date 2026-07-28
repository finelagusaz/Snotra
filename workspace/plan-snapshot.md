# plan — issue #795: config 既定値のリテラル再手打ちを潰す

前提は `workspace/research.md`。ユーザー裁定（2026-07-28）: **「置換＋不変条件のテスト」**。

## 未確定（実装前に潰す）— ラウンド 3

- [x] **既定の参照形**（`LazyLock<Config>` 静的 / `default_*()` の `pub` 化 / `Default` 実装）— **測って決めた**。**却下 1: `LazyLock<Config>` 静的** — `Config::default()` は `default_scan_paths()` でファイルシステムを叩き（`config.rs:622-640` の `.exists()`）、`default_language()` で OS ロケールを読む（`:58-68`）。窓幅 1 つのためにそれを走らせるのは過剰。**却下 2: `default_*()` の `pub` 化** — `docs/development-principles.md:58` が「**lib crate の `pub` 項目に `dead_code` は出ない**」と明記しており、`pub` にすると**到達性の検出器を失う**（この issue が扱っている問題と同じクラスの穴を新設することになる）。**採用: 不足している `Default` 実装を足し、既存の `Default` と `pub` フィールドから読む**（新しい公開関数を 1 つも増やさない）
- [x] **`window_width` に `#[serde(default = "…")]` を付けるか** — **測って決めた: 付けない**。今日は `[appearance]` に `window_width` が無い TOML は**parse 失敗**し、`.bak` 退避 + 既定起動（`RecoveredFromCorrupt`）へ落ちる（`config.rs:49`・`:900`・`:919`・`:934`）。付けると「欠落は正常」へ意味が変わり、**永続形式の後方互換の判断**（`/persistence-check` の領分）になる。この issue の不変条件 1（既定値も挙動も変えない）に反する。**不整合として残る「既定が型から導けるのに serde が使わない」は follow-up issue へ送る**
- [x] **群 8 は現存するか** — **現存**（`font_stack.rs:197` の `const DEFAULT_FONT_FAMILY: &str = "Segoe UI"`・`:205` で使用）。当初「消滅」と判定したのは**確認の grep を `| head -6` で切った**ためで、`/plan-review`「列挙の落とし穴（独立再導出が繰り返し拾った 3 クラス）」が名指しする誤りそのものだった。独立導出が訂正した
- [x] **対象は 10 群か 12 群か** — **12 群**。issue の 10 群のうち 7 のみ消滅、8 は現存。加えて (a) issue 本文が「本 issue の 11 群目にあたる」と書く `window_coordinator.rs:384` の `.unwrap_or(4)`（`window_gap`）、(b) `mod.rs:375` の `.unwrap_or(true)` 相当の `show_icons`（同ファイルの doc が「`AppearanceConfig` に `Default` が無いため型から導けずリテラルになる」と自認している）
- [x] **群 D のテストが固定すべき述語** — **UI と同じ述語を使う**。`visual.rs:202-213` の `preset_matches` は**色 5 本を `eq_ignore_ascii_case` で比較する**だけで、`config.visual.preset` は Custom カードの判定にしか使わない（`:77`）。ゆえにテストは `preset_matches(&Config::default(), &PRESETS[0])` を呼ぶ形にする——`assert_eq!` で 5 色を個別比較すると**UI が守っている不変条件より厳しい**主張になる
- [x] **色 5 本の `default_*()` を `pub` 化するか** — **不要**。`VisualConfig` の色フィールドは既に `pub`（`config.rs:405-429`）なので、テストは `VisualConfig::default().background_color` 等で読める
- [x] **`HotkeyConfig` の二重手書きを含めるか** — **含める**。`config.rs:551-552` と `:858-861` に既定 `"Alt"` / `"Q"` が二重に手書きされている（独立導出の発見）。同一ファイル内で完結し、`impl Default for HotkeyConfig` を足せば両方が参照へ変わる。機序は 12 群と同型で、別 issue へ送る理由が無い
- [x] **`window_coordinator.rs:52-55,101` の `VisualConfig::default()` 毎回構築を含めるか** — **含める**。`String` 6 本を毎回確保しており、`visual.rs:33-42` の `default_visual()` が**まさにこれを避けるために存在する**（doc に「mutex を握ったまま毎フレーム 6 本確保する」と明記）。1 行の置換で済み、放置すると `/dry-check` が二重の既定源として拾う
- [x] **`GeneralConfig::default()` を静的にするか**（`default_language()` が OS ロケールを読むため） — **しない**。4 箇所すべて `unwrap_or_else` の中に置けば、**`AppState` 不在時にしか評価されない**。その経路は `mod.rs:372` が「setup 完了前の理論経路のみ」と書くとおり実質通らない。静的を 1 つ増やすのは YAGNI
- [x] **消費者を向ける先（`*Config::default()`）自身が写しではないか** — **写しだった**（ラウンド 2 の独立導出）。`GeneralConfig::default()` は 7 フィールド、`SearchConfig::default()` は 5 フィールド、`VisualConfig::default()` は `preset`、`Config::default()` は hotkey / window_width / show_icons を、`default_*()` を呼ばずに手書きしている。**serde は 2 経路を持つ**——「セクション欠落 → `Section::default()`」と「キー欠落 → `#[serde(default = "default_X")]`」——ので、両者が食い違えば**同じ既定が経路によって変わる**。`SearchConfig::default()` が同一 impl 内で 3 フィールドだけ関数を呼んでいるのが、流儀が割れている証拠。**先にこれを直さないと、写しである SSOT へ消費者を向けることになる**。Phase 1 の先頭へ入れる
- [x] **2 経路の一致をどう固定するか** — **`toml::from_str::<Section>("") == Section::default()` を General / Search / Visual の 3 型で書く**（ラウンド 2 の独立導出）。既存の `deserialize_minimal_config_uses_defaults` は**セクションを丸ごと省略する**形であり、フィールド単位の serde 経路は現在 1 つもテストされていない。この 1 本が「serde の既定関数 ↔ `Default` 実装」の一致を全フィールド分まとめて固定する
- [x] **`PRESETS[0]` の位置依存** — **`find(|p| p.preset == ThemePreset::Obsidian)` にする**（ラウンド 2 の settings 層）。配列順への依存は「Obsidian と一致すること」という意図から乖離しうる
- [x] **`config.rs:435` の `preset: ThemePreset::Obsidian` を放置するか** — **直す**。`default_theme_preset()`（`:337`）との二重定義であり、`HotkeyConfig` に対して採る修正と同型。ラウンド 1 で指摘されたのにテストの述語だけ直して本体を放置していた（ラウンド 2 の settings 層）

## 変更ファイル一覧

| ファイル | 変更内容 |
|---|---|
| `snotra-core/src/config.rs` | (1) **`GeneralConfig` / `SearchConfig` / `VisualConfig` の既存 `Default` 実装が手書きしている値を `default_*()` 呼び出しへ揃える**（写しである SSOT を先に直す）、(2) `impl Default for AppearanceConfig` を新設、(3) `impl Default for HotkeyConfig` を新設し `:551-552` と `:854-869` の二重手書きを参照へ、(4) serde の 2 経路の一致を固定するテストを追加 |
| `snotra-core/src/search.rs` | `impl Default for SearchOptions` を `Self::from(&SearchConfig::default())` へ（4 リテラルが消える） |
| `src-tauri/src/egui_shell/view.rs` | `:84` の `.unwrap_or(600.0)` |
| `src-tauri/src/egui_shell/window_coordinator.rs` | `:189` の `600.0`・`:430` の `8`・`:131` の `true`・`:243` の `false`・`:384` の `4`（群 11）・`:52-55,101` の `VisualConfig::default()` → `default_visual()` |
| `src-tauri/src/egui_shell/launcher_controller.rs` | `:586` の `true`・`:603` の `"@"` |
| `src-tauri/src/egui_shell/font_stack.rs` | `:197` の `const DEFAULT_FONT_FAMILY`（群 8）を削除し、`:205` を `default_visual().font_family` 由来へ |
| `src-tauri/src/egui_shell/mod.rs` | `:375` の `show_icons` リテラル `true` を `AppearanceConfig::default().show_icons` へ。`:372-375` の「`Default` が無いため型から導けず」という doc も更新する |
| `src-tauri/src/main.rs` | `:371` の `true` |
| `snotra-settings/src/tabs/visual.rs` | `#[cfg(test)] mod tests` を新設（現在ゼロ）し、群 10 の不変条件を固定 |

**SPEC.md 更新: 不要**（既定値・挙動・設定キー・デシリアライズ可能な入力集合のいずれも変えない。`serde(default)` を付けない判断がこの主張を支えている）。

## 実装順序

### Phase 1 — snotra-core（**まず SSOT 自身の写しを消す**）

**この Phase を先にやる理由**: 消費者を `*Config::default()` へ向ける前に、その `*Config::default()` が既定関数を呼ぶ形になっていなければ、**写しである SSOT へ消費者を向けることになる**。serde の 2 経路（セクション欠落 → `Section::default()` / キー欠落 → `#[serde(default = "…")]`）が食い違ったままになる。

- [ ] `impl Default for GeneralConfig`（`config.rs:158`）の手書き 7 フィールドを、対応する `default_*()` の呼び出しへ置換する。**値は 1 つも変えない**
- [ ] `impl Default for SearchConfig`（`:274`）の手書き 5 フィールドを同様に置換する（同 impl 内で既に 3 フィールドだけ関数を呼んでおり、流儀が割れている）
- [ ] `impl Default for VisualConfig`（`:432`）の `preset: ThemePreset::Obsidian`（`:435`）を `default_theme_preset()` へ置換する
- [ ] `impl Default for AppearanceConfig` を追加し、`Config::default()` の `appearance:` ブロック（`config.rs:555-562`）を `AppearanceConfig::default()` へ置換する。**legacy な `Option` 3 本（`max_results` / `top_n_history` / `max_history_display`）が `None` のままであることを 1 行ずつ目視で突き合わせる**——誤って `Some(v)` を入れると `migrate_legacy_count_params`（`:785-814`）が黙って `visible_rows` へ昇格させる
- [ ] `impl Default for HotkeyConfig` を追加し、`Config::default():551-552` と `fallback_hotkey_if_system_shortcut`（`:854-869`）の `"Alt"` / `"Q"` を参照へ置換する
- [ ] `impl Default for SearchOptions`（`search.rs:66-75`）を `Self::from(&SearchConfig::default())` へ書き換える
- [ ] **2 経路の一致を固定するテストを追加する**: `toml::from_str::<GeneralConfig>("")` / `<SearchConfig>("")` / `<VisualConfig>("")` が、それぞれ `Section::default()` と等しいこと。**これがフィールド単位の serde 経路を初めて覆う**（既存の `deserialize_minimal_config_uses_defaults` はセクションを丸ごと省略する形で、この経路を通らない）
- [ ] `cargo test -p snotra-core` が緑であることを確認する

### Phase 2 — src-tauri（fallback の置換 10 箇所）

- [ ] `view.rs:84` — `.unwrap_or(f64::from(AppearanceConfig::default().window_width))`
- [ ] `window_coordinator.rs:189` — 同上（**読み元の非対称は触らない**・下記 Phase 4 で follow-up 起票）
- [ ] `window_coordinator.rs:430` — `.unwrap_or(AppearanceConfig::default().effective_visible_rows() as u32)`
- [ ] `window_coordinator.rs:131` — `.unwrap_or_else(|| GeneralConfig::default().follow_cursor_monitor)`
- [ ] `window_coordinator.rs:243` — `.unwrap_or_else(|| GeneralConfig::default().ime_off_on_show)`
- [ ] `window_coordinator.rs:384` — `.unwrap_or(default_visual().window_gap)`（群 11）
- [ ] `window_coordinator.rs:52-55,101` — `VisualConfig::default()` の毎回構築を `default_visual()` へ寄せる。**`:101`（`read_background`）は `default_visual().background_color.clone()` と `.clone()` が要る**（`&'static` から所有値を返すため）
- [ ] `launcher_controller.rs:586` — `.unwrap_or_else(|| GeneralConfig::default().auto_hide_on_focus_lost)`
- [ ] `launcher_controller.rs:603` — `.unwrap_or_else(|| default_visual_search_prefix())` 相当（`SearchConfig::default().instant_command_prefix`。**`String` ゆえ `unwrap_or_else` が必須**）
- [ ] `font_stack.rs:197,205` — `const DEFAULT_FONT_FAMILY` を削除し `default_visual().font_family.clone()` 由来へ（群 8）
- [ ] `mod.rs:375` — `true` を `AppearanceConfig::default().show_icons` へ。`:372-375` の doc から「`Default` 実装が無いため型から導けず」の記述を消す（**この変更で前提が偽になる**）
- [ ] `main.rs:371` — `.unwrap_or_else(|| GeneralConfig::default().hotkey_toggle)`
- [ ] 「`config.rs` 既定と一致」系のコメント 3 箇所（`launcher_controller.rs:586`・`window_coordinator.rs:243`・`main.rs:371`）を削除する——**参照になった以上、一致を主張する注記は無意味であり、残すと「規範で守っている」という誤読を生む**

### Phase 3 — 不変条件のテスト（置換で消せない群 10）

- [ ] `snotra-settings/src/tabs/visual.rs` に `#[cfg(test)] mod tests` を新設し、**`preset_matches(&Config::default(), <Obsidian の PresetDef>)` が true** であることを固定する（UI と同じ述語を使う。`assert_eq!` で 5 色を個別比較しない）
- [ ] 対象は **`PRESETS.iter().find(|p| p.preset == ThemePreset::Obsidian)`** で引く。**`PRESETS[0]` と添字で書かない**——配列順への依存は「Obsidian と一致すること」という意図から乖離しうる
- [ ] 同テストの失敗メッセージに「既定色を変えるなら Obsidian プリセットも同じ変更で直すこと」と書く
- [ ] `snotra-settings/CLAUDE.md` のテスト方針の例外リストを**更新する**（`font.rs` の `face_index_valid` に続く 2 件目。既存の理由文は `font.rs` 固有の書き方なので流用せず、この件の理由を書く）。`*.md` を触るので `npm run governance:check` を走らせる

### Phase 4 — 検証と後始末

- [ ] `docs/build-commands.md` カテゴリ A（Rust 変更）の必須コマンドをすべて実行する
- [ ] Phase 3 で `*.md` を触った場合のみ `npm run governance:check` を実行する
- [ ] follow-up issue を 1 本起票する。内容は 3 つ——(a) 群 2 の読み元の非対称（`view.rs:84` は config、`window_coordinator.rs:189` は OS の `inner_size()`）、(b) `launcher_controller.rs:620` の `Language::Ja` が `default_language()`（OS ロケール依存）と一致しない既存の取り違え、(c) `window_width` に serde default が無い不整合
- [ ] 触るファイル内の stale な行番号参照を直す: `launcher_controller.rs:598` が指す `config.rs:956`（実際は `backup_invalid`）と `window_coordinator.rs:423` が指す `config.rs:327`（実際は `max_history_display`）。**どちらも本 PR で触る行の近傍であり、行番号ではなく名前で指す形へ直す**（`.claude/rules/governance-docs.md`「序数で他を指してはならない」）
- [ ] 独立導出が見つけた**別クラスの発見**（「CLEAR_COLOR の一致に検査は無い」「`background_color` は消費者ゼロ」という**stale な主張が 4 箇所に写っている**: `renderer.rs:11-12`・`snotra-egui-runtime/CLAUDE.md:38`・`docs/development-principles.md:66`・`scripts/governance-check.mjs:1067-1068`）を別 issue として起票する。**値ではなく主張の写しであり、本 issue と同型だが変更集合が別である**

## 不変条件

1. **既定値・挙動・デシリアライズ可能な入力集合は 1 つも変わらない。** この変更は「同じ値をどこから読むか」だけを変える
2. **`AppState` 不在時の fallback 値は、置換の前後で同一である**（10 箇所すべて）
3. **確保・I/O を伴う既定は `unwrap_or_else` で受ける。** `unwrap_or` は引数を eager 評価するため、通常経路でも毎回走る。`GeneralConfig::default()` は `default_language()` 経由で OS ロケールを読み、`SearchConfig::default()` は `String` を確保する
4. **新しい公開関数を 1 つも増やさない。** `pub` 化ではなく `Default` 実装で解く（`docs/development-principles.md:58`——lib crate の `pub` に `dead_code` は出ないため、公開面を増やすことは検出器を失うことである）
5. **`AppearanceConfig::default()` は確保も I/O もしない**（数値・bool・`Option` のみ）。ゆえに毎フレーム経路（`view.rs:84`・`window_coordinator.rs:430`）で `unwrap_or` のまま使える
6. この変更は状態・プロセス・スレッド・ウィンドウを 1 つも導入しない

## 破壊不変条件と検知手段

| 壊れたら即アウトな不変条件 | 検知手段 |
|---|---|
| **fallback 値が変わる**（例: `.unwrap_or(true)` を誤って `ime_off_on_show`（false）へ繋ぐ） | 置換ごとに「その行が読んでいた config フィールド」と対応することを 1 行ずつ突き合わせる。`cargo test -p snotra` と `-p snotra-core` |
| **`Config::default()` の返り値が変わる**（`impl Default` へ移す途中でフィールドを取りこぼす） | 既存テスト（`config.rs:1676` の `window_width`、`:1354`・`:1667` 系の visual 既定）。**移設前後で `Config::default()` を比較するテストは書かない**——同じ構造体の同語反復になるため |
| **既定 config と Obsidian プリセットの一致が崩れ、初回起動でどのカードも強調されない** | Phase 3 のテスト（現在この不変条件を守る機構は 1 つも無い） |
| **I/O・確保を伴う既定を eager 評価して通常経路で走らせる** | 不変条件 3 を Phase 2 のチェック項目で固定（`unwrap_or_else` の使い分け） |

## テスト方針

- 追加: `snotra-settings/src/tabs/visual.rs` に `#[cfg(test)] mod tests`（群 10 の不変条件 1 本）
- 既存で検算するもの: `config.rs:1676`（`window_width` 既定）、`SearchOptions::default()` を使うテスト群（群 9 の等価性）、`window_coordinator.rs:554` の `CLEAR_COLOR` 不変条件テスト
- **置換で消える群にはテストを書かない**——SSOT を直接参照する形になれば乖離しうる 2 つ目の表現が存在しなくなる。そこにテストを足すと**3 つ目の写し**になる
- 検証コマンド: `docs/build-commands.md` カテゴリ A（Rust 変更）

## セルフレビュー

（収束後に 1 度だけ記録する）
