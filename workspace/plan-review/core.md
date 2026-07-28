## 観点1: 新テストは書けるか・意味があるか

**問題なし。** `GeneralConfig`（config.rs:139-156）・`SearchConfig`（:229-272）・`VisualConfig`（:405-430）は全フィールドが `#[serde(default)]` または `#[serde(default = "fn")]` を持ち、必須フィールドが無い。`toml::from_str::<T>("")` は空テーブルとして各フィールドの serde default を呼び成功する。3型とも `PartialEq` を derive 済み（:138, :228, :404 相当の derive 行）で `assert_eq!` が使える。

計画がテスト対象を `GeneralConfig`/`SearchConfig`/`VisualConfig` の3型に絞り、`AppearanceConfig`・`HotkeyConfig` を含めなかったのは正しい判断である——`AppearanceConfig.window_width`（:313、`u32`, serde default 属性なし）と `HotkeyConfig.modifier`/`key`（:110-111、`String`, 同属性なし）はいずれも必須フィールドで、この2型を対象に含めていたら `toml::from_str::<T>("")` は parse 失敗していた（未確定欄「`window_width` に `serde(default)` を付けるか」の裁定＝付けない、と整合）。

## 観点2: `impl Default` 手書き値と `default_*()` の突き合わせ

**問題なし（値の一致）:**
- General 7フィールド: `hotkey_toggle`(true)/`show_on_startup`(false)/`auto_hide_on_focus_lost`(true)/`show_tray_icon`(true)/`ime_off_on_show`(false)/`follow_cursor_monitor`(true) は対応する `default_*()`（config.rs:114-136）と1対1で値一致。
- Visual: `preset: ThemePreset::Obsidian`（:435）は `default_theme_preset()`（:337-339）と一致。
- AppearanceConfig 新設時の legacy Option 3本（`max_results`/`top_n_history`/`max_history_display`）は現行 `Config::default()`（:559-561）で全て `None`——計画の「1行ずつ目視で突き合わせる」対象と一致。加えて `normalized_default_resolves_all_migration_sentinels`（:1449-1461）が同じ3フィールドの `None` を直接アサートしており、目視確認漏れがあっても自動検知される二重防御になっている。

**軽微な懸念（対応する `default_*()` が無いフィールド、値は変えなくてよいが計画の文言が想定していない）:**
- General の `auto_update: AutoUpdateMode::Full`（:168）には named `default_*()` 関数が無い。`AutoUpdateMode` 自体が `#[derive(Default)]` + `#[default] Full`（:35-40）を持つのみ。置換先は `AutoUpdateMode::default()` になる（値は変わらない）。計画「7フィールド」の数自体はこれを含めて正しいが、Phase1 箇条書き（plan.md:43）の「対応する `default_*()` の呼び出しへ置換する」という表現は7番目のこのフィールドにだけ当てはまらない。
- AppearanceConfig 新設時の `window_width: 600`（:557）には `default_*()` が存在しない（未確定欄の「付けない」裁定通り、意図的）。実装時は他フィールドと違い literal `600` のまま残す必要があるが、Phase1 箇条書き（plan.md:46）はこれを明記していない。

**要対処（計画の修正）:**
SearchConfig の「手書き5フィールドを同様に置換する」（plan.md:44）は数が合わない。既存の named `default_*()` に対応し置換可能なのは4つだけ——`normal_mode`/`folder_mode` → `default_search_mode()`、`show_hidden_system` → `default_show_hidden_system()`、`history_normalization` → `default_history_normalization()`（config.rs:189-199 vs :277-280）。残る手書き literal のうち `migemo_enabled: false`（:283）と `include_path_env: false`（:289）には対応する `default_*()` が無く、`#[serde(default)]`（:245-246, :270-271）は `bool::default()` を素通しするだけである。値は変わらないため実装は壊れないが、「5フィールド」という数字がどの5番目を指すか不明で、実装者が不要な `default_migemo_enabled()`/`default_include_path_env()` を新設する（不変条件4「新しい公開関数を1つも増やさない」には反しないが計画外の変更集合が増える）誤読を招きうる。**Phase1 の当該箇条書きを「4フィールド（normal_mode/folder_mode/show_hidden_system/history_normalization）+ migemo_enabled・include_path_env は対応する `default_*()` が無いため literal のまま残す」に修正することを推奨する。**

## 観点3: 既存テストの赤化リスク

**問題なし。** 値を1つも変えない前提が保たれる限り、以下は緑のまま:
- `default_config_has_expected_values`(:1671-1690) — `Config::default()` の個別フィールド値をアサート。値不変なので影響なし。
- `deserialize_minimal_config_uses_defaults`(:1632-1668) — セクション欠落時の struct-level default 経路。今回の変更は field-level default 経路の内部実装（Default impl の書き方）のみで、この経路の挙動自体は変えない。
- `visual_padding_defaults_for_missing_keys`(:1261-1277) / `visual_field_defaults_apply_when_section_present`(:1286-1304) — `VisualConfig` の値アサート。preset置換は無関係のフィールド。
- `normalized_default_matches_default_plus_manual_migrations`(:1464-1468) — `Config::default()` と `apply_migrations()` 後の完全一致を見る。`Default` 実装移設でフィールドを1つでも取りこぼせばここが即赤化する（破壊不変条件表の検知手段と一致）。
- `normalized_default_resolves_all_migration_sentinels`(:1449-1461) — 観点2で述べた通り、AppearanceConfig の legacy Option が誤って `Some` になれば検知する。

`search.rs` 側: `impl Default for SearchOptions` → `Self::from(&SearchConfig::default())`（plan.md 該当箇条書き）は本番ホットパスに影響しない。`Engine::search`（engine.rs:124-131）は既に `SearchOptions::from(&self.config.search)` を直接呼んでおり `SearchOptions::default()` を経由しない。`SearchOptions::default()` の呼び出し元は `SearchEngine::search()` という便宜API（search.rs:219-233、本番未使用・テストのみ・search/tests/*.rs 全件）のみで、`SearchConfig::default()` 経由の `String` 確保（`instant_command_prefix`）はテストコードの誤差でしかない。値4項目（normalization/fuzzy_history_cap_ratio/migemo_enabled/migemo_min_chars）は現行 literal と一致確認済み。
