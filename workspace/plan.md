# plan — issue #824 項目 3（欠損キー/セクションを既定へ落とす）

## 目的

`SPEC.md`「13.1 設定データ」が既に宣言している **「欠損キーはデフォルト補完」** に、実装を追随させる。今日この宣言に従っていない箇所を全数塞ぐ。**仕様変更ではなく修正**である（#877 が項目 2 を裁いたのと同型——`SPEC.md`「7.6 起動時の設定初期化」への追随ゆえ修正、と `ADR-config-default-fallback-references` が記録する）。

今日の欠陥: `window_widht = 900` のような **1 キーのタイプミス**が「未知キー無視 + 必須キー欠落」となり parse 失敗 →`config.toml` 全体が `.bak` へ rename され、全設定が既定に戻る。

## 受け入れ条件

1. `toml::from_str::<Config>("")` が **成功**し、`hotkey` = Alt+Q・`appearance.window_width` = 600・`paths.scan` = **空**になる
2. `[appearance]` があり `window_width` が無い `config.toml` の読み込みが `LoadOutcome::Loaded` を返し、**`.bak` が作られない**
3. `[hotkey]` に `modifier` だけがある TOML が parse でき、`key` = `"Q"` になる。`[visual.custom_theme]` に 1 色だけ書いた TOML が parse でき、残り 4 色が既定になる
4. **今日 parse できる TOML はすべて同じ値へ parse され続ける**（受理集合を広げるだけの片方向変更）
5. 既定リテラル（`600` / `"Alt"` / `"Q"` / 5 色）の定義点が**各 1 か所**のまま（#795 が消した写しを復活させない）
6. `cargo test -p snotra-core` / `cargo clippy --workspace --all-targets -- -D warnings` / `npm run governance:check` が green

## 決定事項（実装者は再導出しない）

| 論点 | 決定 | 理由 |
|---|---|---|
| 射程 | セクション 3（`Config.hotkey` / `appearance` / `paths`）+ スカラーキー 8（`window_width`・`hotkey.modifier`・`hotkey.key`・`CustomTheme` の 5 色） | ユーザー裁定（2026-08-06）。`SPEC.md`「13.1 設定データ」に従わない箇所の**全数**。`CustomTheme` は独立導出レビューが検出した漏れ（`config.rs:421-427`・既定関数 `default_background_color()` 以下 5 本が既にある） |
| `ScanPath` / `InstantCommand` / `OpenerRule` / `OpenerTool` の必須フィールド | **射程外** | 補完先の値が**存在しない**——`ScanPath.path` を補完するなら `""` を発明することになり、それは値ではなく書き損じである。しかも `Config::validate()` が `ScanPathEmpty` として既に拾う（`config.rs:1040-1044`）ので、既定化はエラーの出どころを parse から validate へ動かすだけで利用者の得が無い |
| **`Config.paths` の既定** | **seed しない**——`PathsConfig` へ `#[derive(Default)]`（`scan` は空）、`Config.paths` は素の `#[serde(default)]` | ユーザー裁定（2026-08-06・当初案からの変更）。`default_scan_paths()` で seed すると「セクションごと欠落 → 探索パスあり」「`[paths]` あり `scan` 無し → 空」となり、**同じ未指定が TOML の書き方で違う値になる**（`config.rs:1673-1676` が名指しし #795 が塞いだ乖離クラス）。seed しなければ parse 経路の 2 つが一致し、受け入れ条件 4 が無傷で保たれる |
| `default_scan_paths()` の役割 | **`Config::default()` 専用の seed**（first-run と `RecoveredFromCorrupt`）へ純化する | 上の裁定の帰結。`Config::default()` と parse 経路の既定が `scan` について食い違うのは**意図**であり、`Config::default()` 側の doc に理由を書く（書かないと次の読者が「`PathsConfig::default()` へ寄せ忘れ」と誤読して seed 案へ戻す） |
| 既定関数の形 | 名前つき private fn がリテラルを持ち、`impl Default` がそれを呼ぶ | 同じ struct 内の前例 `default_show_icons`（`config.rs:320` 属性 / `:347` Default が呼ぶ）に揃える。**属性だけ新設して `impl Default` に `600` を残すと写しが復活し、`config.rs:337` の doc が偽になる。**逆向き（fn が `AppearanceConfig::default().window_width` を読む）にもしない——属性から呼ばれる fn が struct 全体を構築するのは無駄で、依存の向きを 2 度読ませる |
| 新しい既定関数の可視性 | `pub` にしない | `ADR-config-default-fallback-references` 却下 1 の維持（lib crate の `pub` 項目には `dead_code` が出ない＝到達性の検出器を失う） |
| `impl Default for CustomTheme` | **新設する**（`#[derive(Default)]` は使えない——`String::default()` は `""` であって色ではない） | 4 兄弟テストを同じ形（`toml::from_str::<T>("") == T::default()`）で書けるようにする。trait 実装なので `-D warnings` 下で `dead_code` にならない |
| `snotra-settings` の `localize_toml_error` | **触らない** | `missing field` の局所化は射程外とした配列要素（`ScanPath.path` 等）でなお到達する（`backup.rs:214-218`） |
| `SPEC.md` | **2 行の追記が要る** | (1)「13.1 設定データ」の宣言は字面上すべてのキーを覆うが、上表のとおり配列要素は補完しないと決めたので線引きを書く。(2)「13.3」のインポート規定「不正値やシステムショートカット競合を既定値へ置換して成功扱いにはしない」と、`[hotkey]` **不在**が Alt+Q になることの非衝突を明記する（不正値と不在は別で、不在は 13.1 が覆う） |

### 意図的に残す非対称（doc とテストで固定する）

- `Config::default()`（first-run / 破損復旧のシード）→ `scan` は `Config::default_scan_paths()` で埋まる
- **parse 経路**（`[paths]` 欠落・`scan` キー欠落のいずれも）→ `scan` は**空**

これは「設定ファイルが無い/読めない」と「設定ファイルはあり、そこに何も書いていない」の区別であって、**同一経路の中の乖離ではない**。parse 経路の 2 つは一致する。

## 変更ファイルと対象シンボル

| ファイル | 対象 |
|---|---|
| `SPEC.md` | 「13.1 設定データ」へ線引きを 1 行 / 「13.3」との非衝突を 1 行 |
| `snotra-core/src/config.rs` | `Config.hotkey`・`Config.appearance`・`Config.paths`（属性追加）/ `PathsConfig`（`#[derive(Default)]` 追加）/ `impl Default for Config` の `paths:` リテラル（`..Default::default()` 経由 + doc）/ `AppearanceConfig.window_width`（属性追加）/ 新 `fn default_window_width()` / `impl Default for AppearanceConfig`（委譲 + doc）/ `CustomTheme` の 5 フィールド（属性追加）/ 新 `impl Default for CustomTheme` / テスト群 |
| `snotra-core/src/hotkey.rs` | `HotkeyConfig.modifier`・`.key`（属性追加）/ 新 `fn default_hotkey_modifier()`・`fn default_hotkey_key()` / `impl Default for HotkeyConfig`（委譲 + doc） |
| `docs/adr/ADR-config-default-fallback-references.md` | 「後日の決定（#824 の 3）」を**追記**（却下本文と「未決のまま」の行は書き換えない——`ADR-adr-frozen-history` と同 ADR が明記する作法） |
| `snotra-core/CLAUDE.md` | `config.rs` 節へ操作規範を 1 行（新しいセクション/設定キーには serde 既定を付ける。正本は `SPEC.md`「13.1 設定データ」） |

**触らないと確認したもの**: `PathsConfig {` の構築点は `config.rs:579` のみ、`CustomTheme {` の構築点は `snotra-settings/src/tabs/visual.rs:95` のみ（5 フィールドすべてを明示構築しているため `Default` 新設の影響を受けない）。全数 grep 済み。

## 実装順序

### Phase 0 — SPEC の線引き（`AGENTS.md`「開発ワークフロー」の SPEC → コード の順）

- [ ] `SPEC.md`「13.1 設定データ」へ、補完対象の線引き（配列要素の識別フィールドは補完しない）を 1 行足す
- [ ] `SPEC.md`「13.3」のインポート規定との非衝突（**不正値**の既定置換禁止であって**不在**の補完は 13.1 が覆う）を 1 行で記録する

### Phase 1 — Red（契約を先に書き換える）

- [ ] 既存テストを新契約へ書き換える。**今日の「必須フィールド」契約を明示的に符号化しているものが正本になる**
  - [ ] `partial_toml_falls_back_to_default_via_unwrap_or_default`（`config.rs:2984`・中心アサートは `:2995`）→ 中心アサーションが反転する。`[hotkey]` だけの TOML が parse **成功**し、`hotkey` は Ctrl+Space が**保たれ**、他セクションが既定になる形へ。テスト名とコメント（`:2985`）ごと作り直す
  - [ ] `from_toml_str_fills_defaults`（`config.rs:3275`）→ コメント `:3276`「hotkey, appearance, paths are required; …」が偽になる。**コメントだけ**の修正（アサーションは全セクションを書いているため通り続ける）
  - [ ] `from_toml_str_rejects_missing_required_section`（`config.rs:3352`）→ 欠損セクションが既定補完されることを検証する形へ（テスト名も改める）
- [ ] 新規テスト **(a) Phase 2 の前に落ちなければならないもの**（契約の反転を測る）
  - [ ] `empty_section_deserializes_to_default_appearance` / `_hotkey` / `_paths` / `_custom_theme` — `toml::from_str::<T>("") == T::default()`。既存 3 兄弟（`config.rs:1684-1699`）と同じ形。**この 4 本が射程を機械的に符号化する**。`_paths` は**空 `Vec` リテラルではなく `PathsConfig::default()` と比較する**（derive と parse 経路を互いに固定する）
  - [ ] `config_parses_with_all_sections_omitted` — `toml::from_str::<Config>("")` が Ok。**`Config::default()` との全体比較はしない**（`paths.scan` が意図的に食い違う・`general.language` は OS ロケール依存）。各セクションを対応する `Default` と比較し、`paths` は **`PathsConfig::default()` と比較**して、上記「意図的に残す非対称」をコメントで pin する。**将来 必須フィールドが混入したら落ちる検知器**でもある
  - [ ] `appearance_window_width_default_applies_when_key_missing` — `[appearance]` は**書き**、`window_width` だけ書かない。**同じテストで `show_icons = false` を sentinel として置き、その値が残ることも検証する**（セクションごと省くと親の `#[serde(default)]` が struct 丸ごと既定へ落とし、フィールド属性が一度も実行されない false-green になる。同じ理由で別建てされた先例が `visual_field_defaults_apply_when_section_present`・`config.rs:1277`）
  - [ ] `hotkey_key_default_applies_when_key_missing` — `modifier = "Ctrl"` を sentinel に `key` だけ省く → `key` = `"Q"`・`modifier` = `"Ctrl"`
  - [ ] `custom_theme_field_default_applies_when_key_missing` — `[visual]` の下に `[visual.custom_theme]` を書き `background_color` だけ（sentinel 兼）→ 残り 4 色が既定。**アサートは `custom_theme` が `Some` であることを確かめてから中身に対して行う**（`Option` なので `None` のまま素通りする形にしない）
  - [ ] `load_from_dir_missing_section_is_loaded_not_recovered` — `[hotkey]` だけの `config.toml` を書いた一時ディレクトリで `load_from_dir_reporting` が `Loaded` を返し `config.toml.bak` が**存在しない**こと。既存の `.bak` テスト群（`config.rs:3059-3200`）の対の位置に置く
- [ ] 新規テスト **(b) Phase 2 の前後どちらでも通らなければならないもの**（不変であることを測る。**Red にならないのが正しい**）
  - [ ] `full_config_parse_is_unchanged` — **今日受理されている完全形**の TOML（触る全箇所に**非既定値**: `hotkey` = Ctrl+Space、`window_width` = 900、`[paths].scan` に 1 件、`[visual.custom_theme]` に 5 色）を parse し、全値がそのまま残ること。**後方互換の証明はこの向き**（新形式の往復ではなく、現行形式を新コードで読む・`snotra-core/CLAUDE.md`「データ永続化の注意」）
  - [ ] `paths_section_without_scan_key_stays_empty` — `[paths]` に `additional` を sentinel に `scan` を省く → `scan` は空・`additional` は保たれる。`scan` は今日すでに `#[serde(default)]` を持つので、これは**変わらないことの pin**である
- [ ] `cargo test -p snotra-core` を実行し、**(a) 群と書き換えた 2 本のアサーションが落ち、(b) 群が通ること**を確認する（Red の実測）。**(b) 群が落ちたら計画の前提（片方向の拡大）が崩れている**ので、実装へ進まず原因を報告する

**既存の `.bak` テスト 7 本は無傷である**（`config.rs:3059` / `3076` / `3103` / `3120` / `3136` / `3160` / `3188`。seed はいずれも構文エラー・非 UTF-8・ファイル不在・値の不正であって、セクション欠落で破損を作っているものは 1 本も無い——独立導出レビューが全数を読んで確認済み）。

**`window_width = 600` / `[paths]` / `[hotkey]` を「パーサを満足させるためだけに」書いている既存テスト（20 数本）は 1 本も触らない**——省略できるようになるが、回ると差分が数百行に膨らみレビューの信号対雑音比が壊れる。

### Phase 2 — Green（実装）

- [ ] `config.rs`: `fn default_window_width() -> u32 { 600 }` を `default_show_icons` の隣に置き、`window_width` へ `#[serde(default = "default_window_width")]`、`impl Default for AppearanceConfig`（`config.rs:346`）を `window_width: default_window_width(),` へ
- [ ] `hotkey.rs`: `fn default_hotkey_modifier() -> String` / `fn default_hotkey_key() -> String` を `impl Default` の直前に新設、両フィールドへ属性、`impl Default for HotkeyConfig` を委譲へ
- [ ] `config.rs`: `CustomTheme` の 5 フィールドへ既存の `default_*_color()` を指す属性を追加し、`impl Default for CustomTheme` を同じ 5 本経由で新設
- [ ] `config.rs`: `PathsConfig`（`config.rs:485-490`）へ `#[derive(Default)]` を追加
- [ ] `config.rs`: `Config.hotkey` / `Config.appearance` / `Config.paths` へ素の `#[serde(default)]`
- [ ] `config.rs`: `impl Default for Config` の `paths:` リテラル（`config.rs:579-582`）を `PathsConfig { scan: Self::default_scan_paths(), ..Default::default() }` へ
- [ ] `cargo test -p snotra-core` が green

### Phase 3 — doc・ADR

- [ ] `impl Default for AppearanceConfig` の doc（`config.rs:337-339`）——「serde の既定関数を持たない」「`[appearance]` に無い TOML は parse 失敗」「意図的に足していない」の 3 文がすべて偽になる。#824 の決着として書き直し、リテラルが `default_window_width` へ移ったことを書く（issue コメントが指摘していた「項目 3 の現場だけ `#824` マーカーが無い」もここで解消する）
- [ ] `impl Default for HotkeyConfig` の doc（`hotkey.rs:20-22`）——「必須フィールド（serde の既定関数を持たない）」が偽になる。「既定リテラルはここ 1 か所」はリテラルの所在を 2 fn へ移して真のまま保つ
- [ ] `empty_section_deserializes_to_default_*` 群の doc（`config.rs:1673-1682`）——「`AppearanceConfig` / `HotkeyConfig` は…ここでは対象にできない」が 3 つとも成立しなくなる。段落を削る
- [ ] `impl Default for Config` の `paths:` の隣に、`default_scan_paths()` が `Config::default()` 専用 seed である理由（parse 経路の既定ではない）を書く
- [ ] `ADR-config-default-fallback-references.md` へ「後日の決定（#824 の 3）」を追記する。書く内容: (i) SPEC が既に宣言していたこと・却下時にそれを参照していなかったこと、(ii) 「受理する config 形式の変更」への回答＝**受理集合を広げるだけの片方向変更**であること、(iii) `Config.paths` の 3 案と却下理由（seed 案 2 つ＝否定の知識。置かないと次に触る人が再発明する）、(iv) 残余（下記）、(v) **値レベルの個別フォールバックを射程外とした決定と理由**（下記）
- [ ] `snotra-core/CLAUDE.md` の `config.rs` 節へ 1 行（新セクション/新設定キーには serde 既定を付ける）
- [ ] 実装差分を確定させる（`cargo clippy --workspace --all-targets -- -D warnings` と `npm run governance:check` が green）

## 不変条件と異常系

- **既定リテラルの定義点は各 1 か所**（受け入れ条件 5）。`impl Default` と serde 既定関数の二重定義を作らない
- **既に parse できる入力の解釈を変えない**（受け入れ条件 4）。今回の変更は受理集合を**広げるだけ**である。`#[serde(default)]` は既存キーの解釈に触れない
- **データ保全は改善方向のみ**: 今日 `.bak` へ退避されていた入力の一部が退避されなくなる。破壊的フォールバックを新設しない
- **`window_width = 0` 等の明示的な不正値の扱いは不変**: 既定補完が効くのは**欠落**だけで、明示値は従来どおり `Config::validate()` の担当（`config.rs:2960` が pin）
- **書き戻しは起きない**: `load_from_dir_reporting`（`config.rs:933-937`）は `apply_migrations()` が true のときだけ save する。欠落自体は migration の対象ではないので、**欠落したままの config.toml はディスク上で欠落したまま残る**。設定 GUI が保存したときに全キーが materialize される
- **`fallback_invalid_hotkey` との相互作用**: `[hotkey]` 欠落 → Alt+Q は `validate_hotkey()` を通る有効値なので何もしない（`changed` が立たない）
- **`-D warnings` 下の `dead_code`**: 新設 3 fn は serde 属性の文字列から呼ばれ、`impl Default for CustomTheme` と `#[derive(Default)] for PathsConfig` は trait 実装。いずれも `dead_code` にならない。**新設・移設した fn はすべて private のままにする**
- **`AppearanceConfig` の legacy `Option` 3 本**（`config.rs:341-342` の doc）: `None` でなければ `migrate_legacy_count_params` が黙って `visible_rows` へ昇格させる。`impl Default` を編集する差分がこの doc の直下に入るため、レビュー時に見落とさない
- **将来の退行の検知手段**: `config_parses_with_all_sections_omitted` と `empty_section_deserializes_to_default_*`（7 struct 分）。新しい必須フィールドを足すと空文字列 parse が落ちる

### 受容する残余

1. **`[paths]` を手で消した利用者の可視性が下がる。** 今日: parse 失敗 →`.bak` 退避 + トレイのバルーン通知（`SPEC.md`「13.1 設定データ」）+ `Config::default()` の seed で索引は動く。変更後: 正常 parse → `scan` が空 → **索引が空のまま無言で起動**する。母集団は「`[paths]` セクションを手で消した config.toml」だけで（アプリが書き出す config.toml は `skip_serializing` を持たない `scan` を必ず含む）、`[paths]` はあるが `scan` を書いていない利用者は**今日すでに同じ状態**である。`config_parses_with_all_sections_omitted` がこの挙動を pin するので、次に触る人が「バグだ」と誤認して seed 案へ倒すことは防げる。補うなら `Config::validate()` へ「`paths.scan` が空」の警告を足す手があるが、**セーフティネットの新設で合意が要る**（ルート `CLAUDE.md`「最重要ルール」2）ため、この PR に混ぜず別 issue とする
2. **`snotra-settings` のインポート**（`backup.rs:299`）が、今日はローカライズされたエラーで拒否している部分 TOML を受理し、`config.toml` へ上書き保存するようになる。インポートは元々「選んだファイルで全置換する」操作であり、欠損キーが既定へ落ちるのは `SPEC.md`「13.1 設定データ」の宣言どおりである（Phase 0 で SPEC 側に非衝突を明記する）
3. **値レベルの個別フォールバックは射程外である**（利用者裁定・2026-08-06）。本計画が塞ぐのは「キー・セクションの欠落」だけで、**型/variant の不一致**（`window_width = "600"` / `preset = "Solarised"`）と**型は合うが意味が不正な値**（`window_width = 50`——`Config::validate()` は本体のロード経路から呼ばれておらず `snotra-settings` の 2 か所（`app.rs:206` / `backup.rs:274`）からしか呼ばれない）は、今日どおり全体 parse 失敗 →`.bak` か、素通りのままである。**足さない理由: 手で書き損じた設定は設定アプリから設定し直せる。** 足すなら各フィールドを `toml::Value` 経由で個別に変換する `deserialize_with` ヘルパー・`.bak` の rename → copy 化・修復したキー名の通知（`SPEC.md`「13.1 設定データ」のバルーン規定の拡張）が同時に要り、ADR 1 本の規模になる。**本計画の `#[serde(default = "…")]` はその落とし先＝前提部品であり、将来足す場合も本計画のやり直しは生じない**

## テスト方針と検証コマンド

- Red → Green。**ただし新規テストは 2 群に分かれる**——契約の反転を測る (a) 群は Phase 2 の前に落ちなければならず、不変であることを測る (b) 群は前後どちらでも通らなければならない（Phase 1 参照）。「落ちないテストは契約を測れていない」は (a) 群にだけ当たる
- Phase 0 で `SPEC.md` へ足す 2 行は、Phase 3 の最後に**実装された姿と読み合わせて確認する**（`.claude/rules/spec.md`「as-built を記述する」——コードより先に書いた散文は計画の楽観をそのまま残しうる）
- コマンドの正本は `docs/build-commands.md`「変更後の検証チェックリスト（必須・スキップ不可）」。本タスクで該当するのは `cargo test -p snotra-core`（カテゴリ A）・`cargo clippy --workspace --all-targets -- -D warnings`（カテゴリ A）・`npm run governance:check`（カテゴリ F・`SPEC.md` / ADR / `CLAUDE.md` を触るため）
- `src-tauri` / `snotra-settings` / `snotra-egui-runtime` は触らないため、それらの `cargo test` はローカル任意（PR CI の rust-check が担保）

## persistence-check 結果

- **変更種別**: 「読み込み失敗ハンドリングの変更」＝**受理する入力集合の拡大のみ**。フィールドの追加・削除・型変更・シリアライザ切替・バイト形式の変更はいずれも無い
- **version バンプ**: **不要**。`config.toml` は version ヘッダを持たない TOML でキー単位の互換を取る形式であり、今日 parse できる入力はすべて同一の値へ parse される（`full_config_parse_is_unchanged` が証明する）
- **セマンティクス変更の有無**: 1 件——`[paths]` セクション欠落の意味が「破損 →`.bak` 退避 + `Config::default()`（seed 済み）」から「正常 parse + `scan` 空」へ変わる。**値の解釈が変わって既存データが誤読される類（#338 の座標系）ではない**が、利用者から見た挙動は変わるため「受容する残余 1」として明示し、テストで pin する
- **後方互換テストの向き**: `full_config_parse_is_unchanged` が「現行（＝旧）形式の fixture → 新コードで parse」の向きを取る。新形式の往復だけに寄らない
- **フィールド属性の false-green 対策**: フィールド級 `#[serde(default = …)]` を測るテストは**セクションを書き、対象キーだけを省き、既存キーを sentinel として置く**（`appearance_window_width_default_applies_when_key_missing` ほか 3 本）。セクションごと省くと親の `#[serde(default)]` が struct 丸ごと既定へ落とし、フィールド属性が一度も実行されない
- **デコード失敗時のデータ保全**: `load_from_dir_reporting`（`config.rs:928-978`）の 4 分岐は**変更しない**。真の構文エラー・非 UTF-8・一時的失敗の扱いは不変で、兄弟分岐の保全方針は揃ったまま
- **`apply_migrations()` の適用要否**: 新しい deserialize 経路は増やさない（`Config::from_toml_str` / `load_from_dir_reporting` の既存 2 経路のまま）
- **判定**: version 判定・後方互換・データ保全すべて満たしており、永続化変更は安全である

## plan-review 結果

- リスク: **高**（永続形式・設定キー・ガバナンス文書の変更）
- レビュー方式: 独立導出 1 体（Step 2b。計画書と research を読ませず、コードから独立に全数導出させた）
- エージェント数: 1 / 成果物: `workspace/plan-review-824-serde-defaults.md`

### 要対処（すべて主エージェントが根拠を再照合し、計画へ反映済み）

- **`CustomTheme` の 5 フィールドが射程から漏れていた** — `config.rs:421-427` を読んで確認。属性なし・既定関数は `config.rs:366-384` に既存。射程へ追加した
- **`Config.paths` を seed する案は #795 の乖離クラスを再び開く** — `config.rs:1673-1676` の doc を読んで確認。ユーザー裁定を仰ぎ、**seed しない案へ変更**した
- **`from_toml_str_fills_defaults` のコメントが偽になる** — `config.rs:3275-3276` を読んで確認。Phase 1 へ追加した
- **`SPEC.md` に線引きの 1 行が要る** — 配列要素を射程外とする決定は SPEC の字面より狭いので、コードだけ直すと文書と実装が食い違う。Phase 0 を新設した

### 軽微

- `localize_toml_error` の `missing field` 分岐は射程外の配列要素でなお到達するため生き残る（`backup.rs:214-218`）——触らない
- パーサ満足のためだけに `[paths]` 等を書いている既存テスト 20 数本は触らない（差分肥大の抑制）

### 未検証 → 解消

- レビューが未検証としていた「`PathsConfig` / `CustomTheme` の struct リテラル構築点の全数」は主エージェントが grep で解消した（それぞれ `config.rs:579` と `snotra-settings/src/tabs/visual.rs:95` の 1 か所ずつ・後者は 5 フィールド明示構築のため影響なし）
- `config_watcher` のリロード経路が `load_from_dir_reporting` を通るか（`src-tauri/src/config_watcher.rs:131` が `RecoveredFromCorrupt` を扱っていることから通る。挙動の向きは本体と同じで、追加の作業項目は生じない）

### 判断

- 実装着手: **可**（人間の承認後）

## 未確定（実装前に潰す）

（なし——射程・形・`paths` の扱い・非対称の記録先はすべて上表で決定済み）

## セルフレビュー

- リスク: 高
- plan-review: 独立導出 1 体
- エージェント数: 1
- 要対処: 4 件（`CustomTheme` の射程追加 / `Config.paths` の方式変更 / 既存テストのコメント 1 件 / `SPEC.md` の線引き）——すべて計画へ反映済み
- 未検証: なし

## 人間レビュー

- [x] 承認済み — 2026-08-06 / 問い: "`workspace/plan.md` をご覧いただき、**注釈を書き加える**か、**明示的にご承認**くださいませ。" および "(1) 計画どおり進めて follow-up 起票、(2) ②③ まで畳んで計画を作り直す、どちらにいたしましょう。" / 回答: "1だけにしよう。ミニマムで考えるなら、手で書き間違えたのなら設定アプリから設定しなおせばいい"
  - 注釈の反映: 値レベルの個別フォールバック（型不一致・意味的不正値）を**射程外**と確定し、follow-up issue は**起票しない**。射程外とした理由だけを ADR「後日の決定（#824 の 3）」へ残す（Phase 3・受容する残余 3）
  - この注釈は要件・対象ファイル/シンボル・インターフェース・不変条件・テスト期待値のいずれも変えない（射程を広げない確認であり、計画本体は無変更）ため、`/plan-review` の再実行はしない
