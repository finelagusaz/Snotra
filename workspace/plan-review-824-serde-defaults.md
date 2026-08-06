# #824 項目 3 独立導出レビュー — SPEC 13.1「欠損キーはデフォルト補完」に従っていない箇所

対象 issue: **#824**（項目 3 のみ。項目 1・2 は #877 で解決済み・issue 本文が「再検討しないこと」と明記）
調査日: 2026-08-06 / 調査対象コミット: `f00191b`（main・clean）
成果物パス: `C:\workspace\Snotra\workspace\plan-review-824-serde-defaults.md`

**独立導出の条件**: `workspace/plan.md` と `workspace/research.md` は開いていない。以下はコード・`SPEC.md`・`docs/adr/ADR-config-default-fallback-references.md`・`gh issue view 824` だけから導出した。

**読み手への注意**: §0 で立てた基準と §2 の処方は**一致させてある**（基準 (b) を宣言して処方から漏らす、という不整合を残していない）。一致していない箇所があればそれは私の誤りなので、逐語追認せず指摘してほしい。

---

## 0. 判定基準（この線引きは私の判断である）

`SPEC.md:665`（「13.1 設定データ」）の宣言は 1 行しかない。

> - 欠損キーはデフォルト補完

これを「`toml::from_str::<Config>` が `missing field` / `missing table` で失敗しない」と読む。失敗すれば `load_from_dir_reporting`（`snotra-core/src/config.rs:931-949`）が `.bak` 退避 + `RecoveredFromCorrupt` へ落ち、`SPEC.md:667` が「内容が壊れている場合」に割り当てた経路を、**壊れていない（構文的に正しい）TOML** が踏むことになる。SPEC 内部で矛盾するのはこの一点だけである。

### 「欠損キー」の射程 — 私の線引き

**含める**:
- **(a) トップレベルのセクション**（`Config` の struct フィールド）
- **(b) セクション内のスカラーキー** — 例外を設けない。**`HotkeyConfig.modifier` / `key` と `CustomTheme` の 5 色も含む**

**含めない**:
- **(c) ユーザーが書いた配列要素の必須フィールド** — `[[paths.scan]]` の `path` / `extensions`、`[[openers]]` の `target` / `tools`、`[[instant_commands]]` の `name`

(c) を外す根拠は 2 つ。

1. **補完先の値が既に名前を持っているか、無から発明することになるか。** (a)(b) の補完先はすべて既存の `impl Default` / `default_*()` が持っている値である（Alt+Q、600、Obsidian の 5 色）。一方 `ScanPath.path` を補完するなら `""` を発明することになり、それは値ではなく「書き損じ」である。
2. **`""` には既に報告経路がある。** `Config::validate()` は空 `path` を `ConfigError::ScanPathEmpty { index }` として拾う（`config.rs:1040-1044`）。`path` を `#[serde(default)]` にすると、書き損じが parse 段階の `missing field` から validate 段階の `ScanPathEmpty` へ移るだけで、**ユーザーの得は無く**、設定 GUI の import 経路（`snotra-settings/src/tabs/backup.rs:269-278`）でエラー種別が変わるだけである。

この線引きは SPEC 13.1 の字面より狭い。**したがって SPEC 側にも 1 行の追記を要求する**（§2-D）。AGENTS.md「開発ワークフロー」1 の「文書化された挙動を変えたら仕様変更」に当たるため、コードだけ直して終わりにはできない。

---

## 1. 【要対処】宣言に従っていない箇所（全数）

`toml::from_str::<Config>` の呼び出し点は 3 crate 横断の grep で **`config.rs:931`（load 経路）と `config.rs:1152`（`from_toml_str` → `backup.rs:299` の import 経路）の 2 つだけ**である（grep: `from_str::<Config>|toml::from_str` を `snotra-core/src snotra-settings/src src-tauri/src` に対して実行、テスト内呼び出しを除く）。したがって下の 5 件はどちらの経路にも同じように効く。

### A-1. `Config.hotkey` — セクション欠落・キー欠落の**両方**で parse 失敗
- セクション: `snotra-core/src/config.rs:101` `pub hotkey: HotkeyConfig,`（属性なし）
- キー: `snotra-core/src/hotkey.rs:15-16` `pub modifier: String,` / `pub key: String,`（どちらも属性なし）
- `impl Default for HotkeyConfig` は存在し、既定リテラル `"Alt"` / `"Q"` は `hotkey.rs:25-26` の 1 か所だけ。

**(a) と (b) の両方に当たるので、処方も両方に要る**（§2-A）。struct 級の `#[serde(default)]` だけでは `[hotkey]` があって `key` が無い TOML が失敗し続ける。

### A-2. `Config.appearance` — セクションごと欠落で parse 失敗
`snotra-core/src/config.rs:104` `pub appearance: AppearanceConfig,`（属性なし）
`impl Default for AppearanceConfig` は存在する（`config.rs:336-353`）。

### A-3. `AppearanceConfig.window_width` — キー単体の欠落で parse 失敗（**#824 項目 3 が名指しした箇所**）
`snotra-core/src/config.rs:319` `pub window_width: u32,`（属性なし。同 struct の他 5 フィールドはすべて `#[serde(default…)]`）
既定リテラル `600` は `config.rs:346` の 1 か所だけ（`impl Default` 内）。`config.rs:337-339` の doc がこの非対称を「意図的に足していない」と明記しており、**その doc がこの変更で偽になる**。

### A-4. `Config.paths` — セクションごと欠落で parse 失敗。**設計判断が要るのはここだけである**
`snotra-core/src/config.rs:107` `pub paths: PathsConfig,`（属性なし）
`PathsConfig` の 2 フィールドはどちらも `#[serde(default)]` を持つ（`config.rs:486-489`）が、**`PathsConfig` 自身に `impl Default` も `#[derive(Default)]` も無い**（`config.rs:485-490`）。

ここには **2 つの「何も指定していない」の綴り**があり、値を食い違わせてはならない。

| 綴り | 今日 | 素朴に `impl Default { scan: default_scan_paths() }` を書いた場合 |
|---|---|---|
| `[paths]` **ごと欠落** | parse 失敗 → `.bak` + `Config::default()` = **scan あり** | `PathsConfig::default()` = **scan あり** |
| `[paths]` **あるが `scan` 無し** | フィールド級 default = **空** | フィールド級 default = **空** |

→ 後者の列で**同じ「未指定」が違う値になる**。これは `config.rs:1673-1682` の doc が名指しする乖離クラス（「セクションごと欠落 → `Section::default()`」と「キーだけ欠落 → `#[serde(default = "default_X")]`」が食い違うと同じ既定が TOML の書き方で変わる・#795）そのものであり、**#795 が潰したはずの穴を新設することになる**。

**採る案 (a): `PathsConfig` は `#[derive(Default)]`（= `scan: Vec::new()`）に留める。**
2 経路が一致し、受理集合を広げるだけの性質（§4-1）が無傷で保たれる。`Config::default_scan_paths()` は **`Config::default()` 専用のシード**（first-run と `RecoveredFromCorrupt`）という役割に純化し、その役割分離を `PathsConfig` の doc と `Config::default()` の側に書く。`Config::default()` は `paths: PathsConfig { scan: Self::default_scan_paths(), ..Default::default() }` とすれば `additional: Vec::new()` の手書きは消える（ADR 決定 2 の趣旨に沿う）。`PathsConfig::default()` が FS を叩く問題も発生しない（ADR が `LazyLock` を却下した理由に触れずに済む）。

**却下する案 (b): `scan` に `#[serde(default = "Config::default_scan_paths")]` を足して両経路を「scan あり」で揃える。**
2 経路は一致するが、**今日 `[paths]` を書いて `scan` を書いていない config の値が変わる**（空 → スタートメニュー + デスクトップ）。§4-1 の「今 parse できる TOML はすべて同じ値に parse され続ける」が偽になり、後方互換の判断が「片方向の拡大」から「既存挙動の変更」へ格上げされる。ADR が挙げたブロッカーが復活するので採らない。

**却下する案 (c): `Config.paths` に `#[serde(default = "…seeded PathsConfig…")]` を付け、`PathsConfig::default()` は空のままにする。**
`[paths]` 欠落だけを seed するので `Config::default()` との一致は取れるが、「`[paths]` を書いたか否か」で未指定 `scan` の値が変わる — (a) が避けた乖離クラスを、位置を変えて再導入するだけである。「`[paths]` と書くこと自体が積極的な意思表示だ」という擁護は可能だが、それは型にもコードにも現れず読者が再導出できない。

**(a) が持ち込む残余は §4-9 に記録した**（`.bak` + バルーン通知による可視化が消える）。この残余は受容を推奨するが、判断は実装者・利用者に委ねる。

### A-5. `CustomTheme` の 5 フィールド — キー欠落で parse 失敗
`snotra-core/src/config.rs:421-427`（`background_color` / `input_background_color` / `text_color` / `selected_row_color` / `hint_text_color`、5 つとも属性なし）。
`VisualConfig.custom_theme` は `Option` + `#[serde(default, skip_serializing_if)]`（`config.rs:453-454`）なので**セクションごと無ければ問題ない**が、`[visual.custom_theme]` を書いて 1 色だけ書いた TOML は parse 失敗する。基準 (b) にそのまま当たる。

補完先は既に名前を持っている——`default_background_color()` 以下 5 本（`config.rs:366-384`）が `VisualConfig` の同名フィールドで使っている値そのものである。**属性へ流用するだけなので追加コストはほぼゼロで、基準 (b) を例外なく適用できる。**

---

## 2. 必要な変更（ファイルとシンボル）

### 2-A. `snotra-core/src/config.rs`

| 箇所 | 変更 |
|---|---|
| `config.rs:101` `Config.hotkey` | `#[serde(default)]` を追加（struct 級。`HotkeyConfig: Default` は既にある） |
| `config.rs:104` `Config.appearance` | `#[serde(default)]` を追加（`AppearanceConfig: Default` は既にある） |
| `config.rs:107` `Config.paths` | `#[serde(default)]` を追加 + `PathsConfig`（`config.rs:485-490`）へ **`#[derive(Default)]` を追加**（案 (a)。`impl` は書かない） |
| `config.rs:579-582` `Config::default()` の `paths:` リテラル | `PathsConfig { scan: Self::default_scan_paths(), ..Default::default() }` へ。**`scan` の seed はここに残す**（§1-A-4 の役割分離）。この非対称は必ず doc に書く——書かないと次の読者が「`PathsConfig::default()` へ寄せ忘れ」と誤読して案 (b) へ倒す |
| `config.rs:319` `window_width` | `#[serde(default = "default_window_width")]` を追加 |
| 新設 `fn default_window_width() -> u32 { 600 }` | `config.rs:189` 付近（`default_show_icons` の隣）。**リテラル `600` はここへ移し**、`impl Default for AppearanceConfig`（`config.rs:346`）を `window_width: default_window_width(),` へ書き換える。他の全フィールドが取っている形（`show_icons: default_show_icons()`）に揃うので、既定の SSOT が 1 か所である性質は保たれる。逆向き（`fn` が `AppearanceConfig::default().window_width` を読む）にしないこと — 属性から呼ばれる fn が struct 全体の `Default` を構築するのは無駄で、読者に依存の向きを 2 度読ませる |
| `config.rs:422-426` `CustomTheme` の 5 フィールド | それぞれ `#[serde(default = "default_background_color")]` … `#[serde(default = "default_hint_text_color")]` を追加（既存 fn をそのまま流用） |
| 新設 `impl Default for CustomTheme` | `config.rs:427` の直後。同じ 5 本の fn を呼ぶ。**`#[derive(Default)]` は使えない**（`String::default()` は `""` であって色ではない）。§3 の `empty_section_deserializes_to_default_custom_theme` を書けるようにするために要る。trait 実装なので `-D warnings` 下でも `dead_code` にならない |
| `config.rs:337-339` `impl Default for AppearanceConfig` の doc | **偽になる**。「serde の既定関数を持たない」「`[appearance]` に無い TOML は parse 失敗 → `.bak` 退避経路へ落ちる」「意図的に足していない」の 3 文をすべて書き換え。#824 の決着として「足した」と、リテラルが `default_window_width` へ移ったことを書く。issue 本文が指摘していた「項目 3 の現場だけ `#824` マーカーが無い」もここで解消する |
| `config.rs:1673-1682` の doc（`empty_section_deserializes_to_default_*` 群の説明） | **偽になる**。「`AppearanceConfig` / `HotkeyConfig` は必須フィールド（`window_width` / `modifier` / `key`）を持ち空文字列から parse できないためここでは対象にできない（`Config` 経由でのみ検証可能）」が、**3 つとも成立しなくなる**。段落を削り、代わりに §3 のテスト群を足す |
| `config.rs:1152` `from_toml_str` の doc | 「Parse a TOML string into a Config, filling missing keys with defaults.」— 変更**後**に初めて正確になる。書き換え不要だが「この doc が実装に追いついた」ことは PR 本文で触れる価値がある |

### 2-B. `snotra-core/src/hotkey.rs`

| 箇所 | 変更 |
|---|---|
| `hotkey.rs:15-16` `modifier` / `key` | `#[serde(default = "default_hotkey_modifier")]` / `#[serde(default = "default_hotkey_key")]` を追加 |
| 新設 `fn default_hotkey_modifier() -> String { "Alt".to_string() }` / `fn default_hotkey_key() -> String { "Q".to_string() }` | `impl Default` の直前。private のまま（§4-5）。**リテラルはここへ移す** |
| `hotkey.rs:23-28` `impl Default for HotkeyConfig` の本体 | 上の 2 fn 経由へ（`config.rs` の `default_show_icons` パターンと同型） |
| `hotkey.rs:20-22` `impl Default for HotkeyConfig` の doc | **偽になる**。「`modifier` / `key` は必須フィールド（serde の既定関数を持たない）ため、`Config::default()` とロード時の不正ホットキー補正はどちらもこの実装を経由して読む」の前半が成立しなくなる。「既定ホットキーのリテラルはここ 1 か所だけである」は**書き換えれば真のまま保てる**——リテラルを 2 fn へ移し、`impl Default` と serde 属性の双方がそこを読む形にする |

### 2-C. 変更しないもの（意図的・基準 (c)）

- **`InstantAction`（`config.rs:83-97`）**: `#[serde(untagged)]` は素直に `#[serde(default)]` を付けられず、欠落時のエラーも `missing field` ではなく「どの variant にも一致しない」になる。`InstantCommand.name`（`config.rs:76`）と併せて (c)。
- `ScanPath.path` / `ScanPath.extensions`（`config.rs:478-479`）、`OpenerRule.target` / `OpenerRule.tools`（`snotra-core/src/opener.rs:20-21`）、`OpenerTool.name` / `OpenerTool.exe`（`opener.rs:12-13`）も (c)。

### 2-D. 偽になる散文（コード外）

- `docs/adr/ADR-config-default-fallback-references.md` — 「検討した代替案と却下理由」の 3 番目（`window_width` に `#[serde(default = "…")]` を足す: 却下）と、末尾「後日の決定（#824 の 1 と 2）」の最終行（「#824 の 3 は**未決のまま**である」）。ADR の本則どおり**却下理由の本文は書き換えず**、「後日の決定」節へ項目 3 の決定を追記する（#795 の判断としては今も正しいため。ADR 自身がこの作法を明記している）。
- ADR へ追記すべき中身: (i) 「受理する config 形式の変更」というブロッカーへの回答 = **受理集合を広げるだけの片方向変更**であり、今 parse できる TOML はすべて同じ値へ parse され続ける（§4-1）、(ii) **§1-A-4 の 3 案（(a) 採用 / (b)(c) 却下）と却下理由**——これは典型的な「否定の知識」であり、ADR に置かないと次に触る人が案 (b) を再発明する、(iii) 案 (a) の残余（§4-9）。

### 2-E. `SPEC.md`

- `SPEC.md:665`「欠損キーはデフォルト補完」に §0 の線引きを 1 行で足す。案: 「欠損キーはデフォルト補完（セクション・キーの欠落。ただし `[[paths.scan]]` / `[[openers]]` / `[[instant_commands]]` の各要素が持つ識別フィールドは補完対象外で、欠落は不正な設定として扱う）」。
- `SPEC.md:697`（13.3 インポート）「ホットキーは通常ロード用の自動修復を行う前の生値を検証する。不正値やシステムショートカット競合を既定値へ置換して成功扱いにはしない」との衝突を**明示的に否定して記録する**。素直に読めばこの文は「**不正値**」についてであり「不在」は 13.1 が覆う。したがって `[hotkey]` が無い TOML の import が黙って Alt+Q になるのは仕様どおりである — が、**仮定で済ませず一文で書く**（§4-3 に副作用として再掲）。
- SPEC を触るので `npm run governance:check`（AGENTS.md 条件別チェック表「ガバナンス文書を変更」）を PR 前に実行する。

---

## 3. 【要対処】今日の契約を符号化していて書き換えが要る既存テスト

**すべて `snotra-core/src/config.rs` 内の `mod tests`。実際に該当行を読んで確認した。**

| テスト名 | `file:line` | 何が壊れるか |
|---|---|---|
| `partial_toml_falls_back_to_default_via_unwrap_or_default` | `snotra-core/src/config.rs:2984`（`assert!(toml::from_str::<Config>(toml_str).is_err())` は **:2995**） | **中心アサーションが反転する。** `[hotkey]` だけの TOML が parse **成功**するようになる。テスト名・コメント（「Partial TOML missing required sections → toml::from_str fails.」:2985）ごと書き換え、「欠落セクションが既定で埋まる」ことを検証する形へ作り直す |
| `from_toml_str_fills_defaults` | `snotra-core/src/config.rs:3275`（偽になるコメントは **:3276**） | コメント「hotkey, appearance, paths are required; general, visual, search, openers, instant_commands have `#[serde(default)]`」が偽になる。アサーション自体は通り続ける（TOML が全セクションを書いているため）ので**コメントだけ**の修正で足りる。放置すると次の読者を誤らせる |
| `empty_section_deserializes_to_default_general` / `_search` / `_visual` | `snotra-core/src/config.rs:1684` / `1690` / `1696` | 3 本自体は無傷。ただし直上の doc（**:1673-1682**）が偽になる（§2-A）。**同じ形の `_appearance` / `_hotkey` / `_paths` / `_custom_theme` を追加するのが、この変更の最も強い検証である** — 「偽になる doc」と「足すテスト」が同じ場所を指す |
| `valid_toml_invalid_values_caught_by_validate` | `snotra-core/src/config.rs:2960` | 無傷（全セクションを書いており、検証対象は `validate()` の値検査）。**書き換え不要と確認した** |
| `invalid_toml_falls_back_to_default` | `snotra-core/src/config.rs:2947` | 無傷。`"{{{{not valid toml!!!!"` は**構文**エラーであってセクション欠落ではない。**書き換え不要と確認した** |

### `.bak` 退避テスト群 — 全 7 本が無傷であることを実測した

`load_from_dir_reporting` を呼ぶテストの seed をすべて読み、**セクション欠落で破損を作っているものは 1 本も無い**ことを確認した。したがって「変更後に `RecoveredFromCorrupt` → `Loaded` へ黙って反転し、退避経路を検証しなくなる」事故は起きない。

- `load_from_dir_parse_failure_backs_up_and_does_not_save`（`config.rs:3076`）— seed は `"{{{ not valid toml"` = 構文エラー
- `load_from_dir_missing_file_is_first_run_and_saves_default`（`config.rs:3103`）— ファイル不在
- `load_from_dir_valid_config_is_parsed`（`config.rs:3120`）— `Config::default()` を丸ごと書き出し
- `load_from_dir_repairs_and_saves_invalid_hotkey`（`config.rs:3136`）— 全セクション有・hotkey の**値**が不正
- `load_from_dir_invalid_utf8_is_backed_up`（`config.rs:3160`）— 非 UTF-8 バイト列
- `load_from_dir_transient_read_error_leaves_file_intact`（`config.rs:3188`）— `config.toml` をディレクトリにする
- `backup_invalid_missing_source_is_noop_no_panic`（`config.rs:3059`）— ファイル不在

### 触らないことを推奨するテスト（差分肥大の抑制）

`window_width = 600` / `[paths]` / `[hotkey]` を**パーサを満足させるためだけに**書いているテストが多数ある（`config.rs:1261, 1288, 1323, 1369, 1402, 1432, 1491, 1515, 1546, 1577, 1626, 1652, 1776, 1800, 1822, 1858, 2005, 2031, 2492, 2517, 2700, 3248` ほか）。変更後は省略できるようになるが、**省略して回ると差分が数百行に膨らみ、レビューの信号対雑音比が壊れる**。1 本も触らないことを推奨する。

### 追加すべきテスト（新規）

1. **`empty_section_deserializes_to_default_appearance` / `_hotkey` / `_paths` / `_custom_theme`** — `toml::from_str::<T>("")` が `T::default()` と一致すること。既存 3 本と同じ形。**この 4 本が §0 の基準 (a)(b) を機械的に符号化する。**
2. **`config_parses_with_all_sections_omitted`** — TOML 空文字列 `""` が `Config` として parse 成功すること。**`Config::default()` との全体一致を assert してはならない** — 案 (a) の下では `paths.scan` だけが意図的に食い違う（parse は空、`Config::default()` は seed 済み）。正しい形は
   - 各セクションが対応する `Default` と一致すること（`hotkey` / `appearance` / `general` / `visual` / `search`）
   - `paths` が `PathsConfig::default()` と一致し、`scan` が**空**であること
   - そのアサーションに「`default_scan_paths()` は `Config::default()`（first-run / 破損復旧）専用の seed であり、パース経路の既定ではない」というコメントを付ける
   → **この 1 本が §4-9 の残余をコード上で pin する**。`default_language()` が OS ロケール依存なのでリテラル比較にはしないこと。
3. **`appearance_window_width_default_applies_when_key_missing`** — `[appearance]` はあるが `window_width` が無い TOML で `600` になること。§1-A-3 の直接の回帰テスト（1 の struct 級 default では属性を外しても通ってしまうため別建てが要る — `config.rs:1277` の `visual_field_defaults_apply_when_section_present`（その理由を書いた doc は `config.rs:1268-1276`）が同じ理由で別建てされている先例）。同型で `hotkey_key_default_applies_when_key_missing`（`[hotkey]` に `modifier` だけ）も置く。
4. **`load_from_dir_missing_section_is_loaded_not_recovered`** — `[hotkey]` だけの config.toml を置き、`LoadOutcome::Loaded` になり `.bak` が作られないこと。**SPEC 13.1 と 13.3 の境界（構文エラーだけが「破損」）を符号化する。** 現行の `.bak` テスト群の対の位置に置く。

---

## 4. 【要対処 / 軽微】壊れうるもの・見落としやすい副作用

### 4-1. 【要対処】後方互換の向きは片方向で綺麗（バージョンバンプ不要）— PR に明記
案 (a) を採る限り、この変更は受理集合を**広げる**だけで、**今 parse できる TOML はすべて同じ値に parse され続ける**（`#[serde(default)]` は既存キーの解釈を変えない）。`config.toml` にはバージョンヘッダが無く、`snotra-core/CLAUDE.md`「データ永続化の注意」が要求する「旧形式の凍結バイト列 → 新コードで deserialize」テストの新設もバージョンバンプも不要である。**ADR が挙げた「受理する config 形式の変更＝後方互換の判断が要る」というブロッカーには、この一文が答えになる。** 変わるのは *失敗していた* TOML の扱いだけである。
**案 (b) を採ると、この一文は偽になる**（`[paths]` 有 + `scan` 無しの既存 config の値が変わる）。案を変えるなら §4-1 も書き換えること。

### 4-2. 【要対処】書き戻しは起きない ＝ 欠落は欠落のまま残る
`load_from_dir_reporting`（`config.rs:933-937`）は `apply_migrations()` が `true` を返したときだけ `save_to_dir` する。セクション・キーの欠落自体は `changed` に寄与しない（migration は欠落を知らない）ので、**欠落したままの config.toml はディスク上で欠落したまま残る**。設定 GUI が保存すれば `toml::to_string_pretty(self)` が全キーを書き出してそこで materialize される。今日は「欠落 → `.bak` へ改名 → 次の save で全キー付きの新 config.toml」だったので、**ユーザーから見た `.bak` ファイルの発生が止まる**。これは意図した改善であり、`SPEC.md:669` のバルーン通知が「壊れた設定からの復旧」でのみ出る条件は変わらない（構文エラー・非 UTF-8 は依然として出る）。

### 4-3. 【要対処】設定 GUI の import 経路（SPEC 13.3）— §2-E で SPEC 側に明記が要る
`snotra-settings/src/tabs/backup.rs:299` → `Config::from_toml_str` → `prepare_import_config`（`backup.rs:269-278`）。`[hotkey]` が無いバックアップファイルが、今日は「parse エラー」で弾かれるが、変更後は**黙って Alt+Q になって import 成功する**（`validate_hotkey()` は Alt+Q を通す）。SPEC 13.3 の「不正値やシステムショートカット競合を既定値へ置換して成功扱いにはしない」と字面で近いが、この文は**不正値**についてであり**不在**は 13.1 が覆う、と読むのが整合的である。**仮定で済ませず SPEC / PR 本文に一文で記録すること。**

### 4-4. 【軽微】`localize_toml_error` の `missing field` 分岐は生き残る
`snotra-settings/src/tabs/backup.rs:214-218` の `desc.contains("missing field")` 分岐と `TrKey::ErrTomlMissingField`（`snotra-settings/src/i18n.rs:186, 400, 616`）。§0 の基準 (c)（配列要素の識別フィールドは補完しない）を採る限り、`[[paths.scan]]` の `path` 欠落などで**今後も到達する**ので分岐は生きたままである。もし (c) まで既定化する判断に変えるなら、この分岐は到達不能になる（**コンパイルは通る** — `TrKey::ErrTomlMissingField` は i18n の match 表 2 か所で構築されるので `dead_code` は出ない。つまり **`-D warnings` はこの後退を検出しない**）。線引きを広げるなら、この分岐の扱いを明示的に決めること。

### 4-5. 【軽微】`-D warnings` 下の `dead_code`
新設する `default_window_width` / `default_hotkey_modifier` / `default_hotkey_key` はいずれも serde 属性の文字列から呼ばれるので `dead_code` にならない。`impl Default for CustomTheme` と `#[derive(Default)] for PathsConfig` は trait 実装なので同様。**ADR 却下 1（`default_*()` を `pub` にすると lib crate では `dead_code` が出なくなる＝到達性の検出器を失う）に抵触しないよう、新設・移設した fn はすべて private のままにすること。**

### 4-6. 【軽微】`fallback_invalid_hotkey` との相互作用
`[hotkey]` 欠落 → `HotkeyConfig::default()` = Alt+Q は `validate_hotkey()` を通る有効値なので、`apply_migrations()` の `fallback_invalid_hotkey` は何もしない（＝ `changed` が立たない）。既存テスト `load_from_dir_repairs_and_saves_invalid_hotkey`（`config.rs:3136`）は全セクション有の**値**不正を測っているので影響なし。

### 4-7. 【軽微】`AppearanceConfig` の legacy `Option` 3 本
`config.rs:341-342` の doc が「legacy な `Option` 3 本は **`None` でなければならない** — `Some(v)` にすると `migrate_legacy_count_params` が黙って `visible_rows` へ昇格させる」と警告している。`window_width` を触るだけなので抵触しないが、**`impl Default for AppearanceConfig` を編集する差分がこの doc の直下に入る**ため、レビュー時に見落とさないこと。`PathsConfig` 側に同種の罠は無い（`additional` は `Vec::new()` で、`migrate_additional_to_scan` は空なら即 return する・`config.rs:711-714`）。

### 4-8. 【軽微】設定 GUI 側の `Config` 構築経路
`snotra-settings/src/app.rs:206` の `config.validate()` は import 以外の保存経路。今回の変更は deserialize だけに触れるので影響なし。**ただし `Config { … }` / `PathsConfig { … }` の struct リテラル構築点の全数は grep していない**ため §5-1 に未検証として再掲する。

### 4-9. 【要対処・受容を推奨する残余】`[paths]` 欠落ユーザーの可視性が下がる
案 (a) の代償。**今日**: `[paths]` が無い config.toml → parse 失敗 → `.bak` 退避 + トレイのバルーン通知（`SPEC.md:669`）+ `Config::default()` の seed で**索引は動く**。**変更後**: 正常 parse → `scan` が空 → **索引が空のまま無言で起動**（結果が 1 件も出ないが理由の手がかりは出ない）。

母集団は「`[paths]` セクションを手で消した config.toml」だけである——アプリが書き出す config.toml は必ず `[paths]` と `scan` を含む（`PathsConfig.scan` は `skip_serializing` を持たないため `to_string_pretty` が常に書く）。したがって実害の確率は低いと見て**受容を推奨する**が、以下は実装者・利用者の判断に委ねる。

- 受容する場合: §3 の新規テスト 2 がこの挙動を明示的に pin するので、次に触る人が「バグだ」と誤認して案 (b) へ倒すことは防げる。
- 補いたい場合の最小手: `Config::validate()` に「`paths.scan` が空」の警告を足す。ただし**これはセーフティネットの新設であり、`.claude/rules/safety-nets.md` と合意が要る**（`CLAUDE.md`「最重要ルール」2）。この PR に混ぜず別 issue にすることを推す。

---

## 5. 【未検証】読んでいない・測っていないもの

以下は「要確認」として明示する。**この項目群についての私の記述は根拠が弱い。**

1. **`snotra-settings` / `src-tauri` が `Config` / `PathsConfig` / `CustomTheme` を struct リテラルで構築する箇所の全数**。`from_toml_str` と `validate()` の呼び出し点しか grep していない。`#[derive(Default)] for PathsConfig` と `impl Default for CustomTheme` を新設するとき、既存リテラルを `..Default::default()` へ寄せるべきかの判断には `PathsConfig {` / `CustomTheme {` の全数 grep が要る（未実施）。
2. **`config_watcher` のリロード経路が `load_from_dir_reporting` を通るか**。`SPEC.md:669` が「起動時・実行中リロードのいずれでも」バルーン通知と言っているので通るはずだが、`src-tauri` 側のリロード実装は読んでいない。§4-2・§4-9 がリロード経路にも同じく効くかは未確認。
3. **`CustomTheme` の部分入力が実際に parse 失敗することの実測**。struct 定義（`config.rs:421-427`、5 フィールドとも属性なし）からの推論であって、`toml` に食わせてはいない。§3 の新規テスト `_custom_theme` がこれを測る。
4. **`Language` enum に `Default` derive が無いこと**の帰結。`default_language()` が OS ロケールを読むので derive できない（ADR 却下 5 が同旨）が、`GeneralConfig` 経由でしか使われないため今回の射程外と判断した。判断の根拠はコードではなく ADR の記述である。

（旧「`[appearance]` が present-but-empty のときの振る舞い」は未検証から外した——`window_width` に属性が付けば `AppearanceConfig` の全 6 フィールドが既定を持つので、§3 の `empty_section_deserializes_to_default_appearance` が定義上これを測る。）

---

## 6. まとめ（実装順の推奨）

1. `PathsConfig` に `#[derive(Default)]`（案 (a)）+ `Config::default()` を `..Default::default()` 経由へ。**`scan` の seed をそこに残す非対称を doc に書く**（§1-A-4・最初にやる。ここを誤ると #795 の穴が復活する）
2. `default_window_width()` を新設し `impl Default for AppearanceConfig` をそこ経由へ
3. `hotkey.rs` に `default_hotkey_modifier()` / `default_hotkey_key()` を新設し `impl Default` をそこ経由へ + フィールド属性
4. `CustomTheme` の 5 フィールドへ属性 + `impl Default for CustomTheme`
5. `Config` の 3 フィールド（`hotkey` / `appearance` / `paths`）+ `window_width` に `#[serde(default…)]` を追加
6. doc 4 か所（`config.rs:337-339`, `config.rs:1673-1682`, `hotkey.rs:20-22`, ADR「後日の決定」）を修正
7. 既存テスト 2 本（`config.rs:2984` の反転、`config.rs:3276` のコメント）を修正
8. 新規テスト 4 群（§3）を追加。**とくに `config_parses_with_all_sections_omitted`（§4-9 の残余を pin する）**
9. `SPEC.md:665` に線引きを追記 + 13.3 との非衝突を記録 → `npm run governance:check`
10. `/persistence-check`（ADR が「この判断は `/persistence-check` の領分」と名指ししている）を実行してから PR
