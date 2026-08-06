# research — issue #824（`window_width` に serde の既定関数を足すか）

## issue の要約

#795 で射程外とした 3 件のうち、**未決は項目 3 だけ**（項目 1・2 は #877 で解消済み・本文の再検討は禁止と issue 側が明記）。

項目 3: `AppearanceConfig` の 6 フィールドのうち `window_width` だけが `#[serde(default = "…")]` を持たない。`[appearance]` セクションはあるが `window_width` が無い TOML は **parse 失敗**し、`config.toml.bak` 退避 + 既定起動（`RecoveredFromCorrupt`）へ落ちる。

判断が要る点（issue 本文）: 足すか。足すなら「手で編集した部分的な TOML が通るようになる」ことを受け入れるか。

## 関連ファイル・シンボル（すべて grep で実在確認済み）

| 位置 | 内容 |
|---|---|
| `snotra-core/src/config.rs:319` | `pub window_width: u32,` — 属性なし（必須フィールド） |
| `snotra-core/src/config.rs:320-321` | `#[serde(default = "default_show_icons")] pub show_icons: bool` — 足す場合に倣う前例 |
| `snotra-core/src/config.rs:336-353` | `impl Default for AppearanceConfig`。doc が 3 つの主張を持つ（後述）。`window_width: 600` のリテラルはここ 1 か所 |
| `snotra-core/src/config.rs:928-978` | `load_from_dir_reporting` — parse 失敗 → `backup_invalid` → `RecoveredFromCorrupt`（保存しない） |
| `snotra-core/src/config.rs:984-` | `backup_invalid` — `config.toml` を `config.toml.bak` へ **rename**（元ファイルは canonical path から消える） |
| `snotra-core/src/config.rs:1681-1682` | テスト doc が「`AppearanceConfig` / `HotkeyConfig` は必須フィールド（`window_width` / `modifier` / `key`）を持ち空文字列から parse できない」と記録 |
| `snotra-core/src/hotkey.rs:14-29` | `HotkeyConfig { modifier: String, key: String }` — 両方とも属性なし。`impl Default` の doc が「必須フィールド（serde の既定関数を持たない）」と明記 |
| `snotra-core/src/config.rs:880-893` | `fallback_invalid_hotkey` |
| `snotra-settings/src/tabs/backup.rs:299` | インポートの入口 `Config::from_toml_str(&content)` |
| `src-tauri/src/main.rs:563`, `src-tauri/src/config_watcher.rs:131` | `RecoveredFromCorrupt` → トレイバルーン通知 |
| `docs/adr/ADR-config-default-fallback-references.md:18` | #795 時点の却下理由 |
| `docs/adr/ADR-config-default-fallback-references.md:32` | 「`window_width` の `#[serde(default = "…")]`（#824 の 3）は**未決のまま**である」 |
| `SPEC.md:665` | **「欠損キーはデフォルト補完」** |
| `SPEC.md:666` | 「未知キーは無視して読み込み継続」 |

## 実測した事実

1. **`SPEC.md`「13.1 設定データ」は既に「欠損キーはデフォルト補完」と宣言している**（`SPEC.md:665`）。`window_width` / `hotkey.modifier` / `hotkey.key` の 3 フィールドはこの宣言に**従っていない**。ADR の却下理由「足すと受理する config 形式の変更になる」は、この既存宣言を参照せずに書かれている。**#877 が項目 2 を裁いたときと同型**（`SPEC.md`「7.6 起動時の設定初期化」が既に宣言していた挙動への追随ゆえ仕様変更ではなく修正、と ADR:30 が記録）。
2. **`deny_unknown_fields` はコードベースに 1 件も無い**（全文 grep・0 件）。ゆえに未知キーは黙って無視される（`SPEC.md:666` と一致）。**帰結: `window_widht = 900` と打ち間違えた TOML は「未知キー無視 + 必須キー欠落」となり parse 失敗 → 設定ファイル全体が `.bak` へ退避される。** 1 キーのタイプミスが全設定の喪失（`.bak` からの手動復旧が要る）を招く。
3. **`window_width` は Tauri 移行コミット（`65d34e3`）から存在する**（`git log -S window_width -- snotra-core/src/config.rs --reverse` の最初のヒット）。**出荷済みのどのバージョンも `window_width` の無い config を書いたことがない**——旧 config の移行は動機として存在しない。動機は手編集・インポート・外部ツール生成の robustness に限られる。
4. **同じ欠陥が `HotkeyConfig.modifier` / `key` にもある。** `fallback_invalid_hotkey`（`config.rs:880`）は `apply_migrations()` の中にあり、`toml::from_str`（`config.rs:931`）の**後**に走る。ゆえに救えるのは「在るが不正」なホットキーだけで、**キー欠落は救えない**（parse 段階で落ちる）。既定値は `HotkeyConfig::default()`（Alt/Q）として既にある。
5. **インポート経路は「受理する形式」の永続化する消費者である。** `snotra-settings/src/tabs/backup.rs:299` は `Config::from_toml_str` で parse し、成功すると `config.toml` へ**上書き保存**する（`SPEC.md`「13.3 バックアップ」）。今日は `window_width` 欠落のファイルをインポートすると**ローカライズされたエラーで拒否され、何も書かれない**。既定を足すと**インポートが成功し `window_width = 600` が書き込まれる**（元ファイルに幅の指定が無かったことが 600 として恒久化する）。`AGENTS.md`「検証の作法（全タスク共通）」が名指す #755/#801 のクラス。

## 再利用できる既存パターン

- 足す場合の形は `default_show_icons` に倣う: 名前つき既定関数 + `impl Default` がそれを呼ぶ（`config.rs:320` / `:347`）。**`impl Default` に `600` を残したまま属性だけ新設すると、#795 が消した写しを復活させ、`config.rs:337` の doc「既定リテラルはここ 1 か所だけである」が偽になる。**
- ADR への追記の作法: `ADR-config-default-fallback-references.md:24-32`「後日の決定（#824 の 1 と 2）」が前例。**却下の本文は書き換えず追記する**（`:26` が明示）。

## 技術的制約

- ADR 本文内の参照は `governance:check` の照合対象外（`ADR-adr-frozen-history`）。ただし `SPEC.md` / `CLAUDE.md` 等の生きた層から ADR を引く場合は正準形が要る。
- `snotra-core/CLAUDE.md`「データ永続化の注意」: serde 表現を変えるときは**旧オンディスク形式の deserialize テスト**を新形式の往復とは別に置く。今回は「今日 parse 可能なものが引き続き parse でき、値が変わらない」ことの固定が該当する。
- `AppearanceConfig` / `HotkeyConfig` は空文字列から parse できないため `empty_section_deserializes_to_default_*` の 3 テスト（`config.rs:1683-1699`）の対象外になっている（`config.rs:1681-1682` の doc がその理由を記録）。**足せばこの制約が消え、同型テストを追加できる**——現在の doc はその時点で stale になる。

## 未解決の疑問（ユーザーへ問う）

1. 足すか、足さないか（issue を NOT_PLANNED で閉じるか）。**上記の実測 1（SPEC §13.1 が既に宣言済み）により、これは「仕様変更」ではなく「仕様への追随（修正）」として扱える。**
2. 足すなら射程は `window_width` だけか、`HotkeyConfig.modifier` / `key` も同時か（実測 4 の同型欠陥）。
3. 実測 5（インポートが通るようになる）を受け入れるか。
