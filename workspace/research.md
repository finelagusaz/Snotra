# Research — issue #438: i18n をテーブル駆動化し断片連結を廃止する

## issue の要約

`snotra-settings/src/i18n.rs`（1197行・169 メソッド※実測、issue 記載は概算180）が全翻訳を
「メソッド + 2アーム `match self.0`」で手書きしており、キー1件につき定型ボイラープレート
（`pub fn` 宣言 + `match` + 2アーム = 約6〜7行）が線形に積み上がる。加えて `err_hotkey_system_conflict`
等の一部エラーメッセージは「文の断片」（例: `" はシステムショートカットと競合します"`）を返し、
呼び出し側（`app.rs::config_error_message`）が値と断片を `format!` で連結して文を組み立てている。
この連結順序は日本語・英語で語順が偶然一致しているから成立しているだけの暗黙契約であり、
語順の異なる言語を将来追加すると破綻する。

対応方針（issue 記載）: キー→（言語→文字列）のテーブル駆動 + 完全文フォーマット
（プレースホルダ方式）へ移行し、断片連結を廃止する。設計思想は `ui/src/lib/i18n.ts`
（キー型 + 言語ごとテーブル + `{param}` プレースホルダ置換）に寄せる。

設計原則としては PR #443（`docs/development-principles.md` §「構造的設計原則と強制の階梯」）の
原則1「Derive, don't duplicate」（テーブル駆動化）と原則5「境界契約は1箇所で規約として定義する」
（断片連結という暗黙契約の解消）が対応する。

## 関連コード

### 変更が必要なファイル

| ファイル | 変更内容 |
|---|---|
| `snotra-settings/src/i18n.rs`（1197行） | `Tr` の169 `pub fn` を全廃し、`TrKey` enum + 言語別テーブル関数（`ja()`/`en()`）+ `Tr::t()`/`Tr::t_params()` に置換 |
| `snotra-settings/src/app.rs` | `config_error_message`（236-263行）を断片連結 `format!` から `tr.t()`/`tr.t_params()` 直接呼び出しへ書き換え。他の `tr.xxx()`/`self.tr.xxx()` 呼び出し（26箇所）を `tr.t(TrKey::Xxx)` へ移行。加えて `#[cfg(test)]` の kittest テスト内に `Tr(Language::En).method_name()` 形式（変数 `tr` を経由しない）の呼び出しが7箇所（793, 809, 815, 837, 864, 907, 911行）あり、`tr\.` の grep パターンでは検出できないため個別に列挙して移行する |
| `snotra-settings/src/hotkey_input.rs` | 2箇所の `tr.xxx()` 呼び出しを移行。**`egui::Key`（143行で `use egui::Key;` によりローカル束縛）との名前衝突を避けるため、新 enum は `Key` ではなく `TrKey` と命名する** |
| `snotra-settings/src/style.rs` | 2箇所の `tr.xxx()` 呼び出しを移行 |
| `snotra-settings/src/tabs/backup.rs` | 25箇所の呼び出し移行。`err_toml_missing_field(field)` は `tr.t_params(TrKey::ErrTomlMissingField, &[("field", field)])` へ |
| `snotra-settings/src/tabs/general.rs` | 22箇所 |
| `snotra-settings/src/tabs/index.rs` | 13箇所。うち1箇所（60行 `label_incl_folders` の断片連結）はテンプレート化対象（後述） |
| `snotra-settings/src/tabs/instant.rs` | 32箇所。`err_instant_unknown_modifier(modifier)` は `tr.t_params(TrKey::ErrInstantUnknownModifier, &[("name", name), ("modifier", modifier)])` へ統合（後述） |
| `snotra-settings/src/tabs/opener.rs` | 30箇所 |
| `snotra-settings/src/tabs/search.rs` | 27箇所 |
| `snotra-settings/src/tabs/visual.rs` | 16箇所（初版の本表から漏れていた。plan.md 本文の対象ファイル一覧には元々含まれていた） |
| `snotra-settings/CLAUDE.md` | i18n.rs の説明（「各メソッドが `match self.0` で `&'static str` を返す」）を新設計に同期 |
| `docs/architecture.md:136` | 「多言語対応（3層）」節の設定 GUI 行（「`Tr` 構造体の match ベース翻訳」）を新設計に同期。ui 側（134行）と対になる記述で DRY 対象 |

**call site 合計は209箇所・10ファイル**（`tr\.[a-z_]+\(` の単純 grep では202箇所・9ファイルしか
拾えず、`tabs/visual.rs` の16箇所と `app.rs` の `Tr(Language::En).method()` 形式7箇所を見落とす
——plan-review の独立検証で判明）。全てコンパイラ検証可能な機械的リネームだが、後者7箇所は
`#[cfg(test)]` 内のため `cargo build`（`--all-targets` 無し）では検出されない点に注意
（Phase 2 の検証コマンドで `cargo test -p snotra-settings` を必ず同じループ内で回す）。

### `ConfigError` enum（`snotra-core/src/error.rs:48-61`）

全12 variant。断片連結（暗黙の語順契約）を持つのは5 variant:

| variant | フィールド | 現在の組み立て（app.rs） | 断片連結か |
|---|---|---|---|
| `HotkeyModifierEmpty` | なし | `tr.err_hotkey_modifier_empty()` | いいえ（完全文） |
| `HotkeyKeyEmpty` | なし | `tr.err_hotkey_key_empty()` | いいえ |
| `HotkeySystemConflict` | `modifier, key` | `format!("{}+{}{}", modifier, key, tr.err_hotkey_system_conflict())` | **はい** |
| `VisibleRowsZero` | なし | `tr.err_visible_rows_zero()` | いいえ |
| `WindowWidthTooSmall` | `u32` | `format!("{}{}", w, tr.err_window_width_too_small())` | **はい** |
| `FuzzyCapRatioOutOfRange` | `value: f64` | `format!("{}{}", value, tr.err_fuzzy_cap_ratio_out_of_range())` | **はい** |
| `ScanPathEmpty` | `index: usize` | `format!("{}{}", index + 1, tr.err_scan_path_empty())` | **はい** |
| `InstantCommandPrefixEmpty` | なし | `tr.err_instant_prefix_empty()` | いいえ |
| `InstantCommandPrefixSlash` | なし | `tr.err_instant_prefix_slash()` | いいえ |
| `InstantCommandDuplicateName` | `name` | `format!("{}{}", name, tr.err_instant_duplicate_name())` | **はい** |
| `InstantCommandUnknownModifier` | `name, modifier` | `format!("{}: {}", name, tr.err_instant_unknown_modifier(modifier))` | 部分的（`name: ` の連結は残るが、`err_instant_unknown_modifier` 自体は既にプレースホルダ方式） |
| `MigemoMinCharsZero` | なし | `tr.err_migemo_min_chars_zero()` | いいえ |

`tabs/backup.rs` の `err_toml_missing_field(field)`（206行）は `ConfigError` 経由ではない独立の
TOML パースエラー処理だが、既に `format!("\"{}\" が必要です", field)` という完全文プレースホルダ形式
であり、断片連結ではない。テーブル移行のみ必要。

### `ConfigError` 以外で見つかった同型パターン（独立再導出で発見・本 issue のスコープに含める）

「同一パターン全コードパス検索」（AGENTS.md ワークフロー）に基づき、`err_*` 以外の箇所にも
同じ「値 + 言語依存の固定断片」形状が無いか確認した:

- **`tabs/index.rs:60`**: `format!("{} {}", scan.extensions.join(", "), tr.label_incl_folders())`
  — 拡張子リストの後ろに ja `"(フォルダ含む)"` / en `"(incl. folders)"` を外部で連結している。
  現状は日英とも「リスト→注記」の順で偶然一致しているだけであり、`ScanPathEmpty` 等と同型の
  断片連結。**本 issue のスコープに含め、完全文プレースホルダキーへ統合する**
  （例: ja `"{extensions} (フォルダ含む)"` / en `"{extensions} (incl. folders)"`）。

以下は独立再導出で発見されたが、**本 issue のスコープ外と判断**したもの（理由を明記）:

- **`app.rs`/`tabs/backup.rs` の `status_*` 系プレフィックス連結**
  （`format!("{}{}", tr.status_save_failed(), e)` 等、app.rs:209、backup.rs:234,238,250,256,263）:
  「ラベル: 動的内容」型の連結で、動的内容の大半が `std::io::Error`/TOML パーサ由来の**常に英語の
  生文字列**であり、断片が表す「文の一部」ではなく「見出し + ログ的詳細」の構造。ui 側
  `notice.launch.failed: "Launch failed{detail}"` と同種のパターンではあるが、対応する
  `err_*` 系（ConfigError由来、または `label_incl_folders` のように値を文中に埋め込む形）ほど
  語順崩壊のリスクが高くない。本 issue の「発見事項」が明示するのは `err_*` の断片であり、
  ここまで広げると変更ファイル・行数が大きく膨らむ。踏み込まず、テーブル駆動化の技術的基盤
  （`TrKey`/`t_params`）だけ用意し、この種の追加テンプレート化は別 issue に委ねる
- **`tabs/backup.rs:234`**: `format!("{}config dir not found", tr.status_export_failed())` の
  `"config dir not found"` が**未翻訳の英語ハードコード**（日本語 UI でもこの部分だけ英語になる
  既存バグ）。本 issue（テーブル駆動化）とは無関係の欠落翻訳バグであり、**このリファクタでは
  修正せず、別 issue として起票する**（挙動不変の原則にも反するため——この文字列を翻訳すると
  出力が変わり「バイト単位一致」の検証ができなくなる）
- **`tabs/backup.rs::localize_toml_error`**: `tr.0 == Language::En` という `Tr` の内部フィールド
  直接参照がある（179-183行）。英語 UI では TOML パーサの生メッセージをそのまま返し、日本語 UI
  でのみ 1 行サマリを前置する非対称処理で、これ自体は「言語ごとに語順が変わる断片」ではなく
  意図的な early-return（生メッセージは常に英語なので翻訳不要）。`Tr::t()`/`t_params()` を
  経由しない点は美観上の非一貫だが、機能的リスクは無いため**据え置く**（無理にテーブル化すると
  「英語列だけ存在しない」不自然な行が table に生まれる）

### `i18n.rs` のメソッド内訳（実測）

- `pub fn` 総数: **169**（issue 記載の「約180」より少ないが、想定範囲内の概算差）
- パラメータなし・`&'static str` 返し: **167**
- パラメータあり・`String` 返し: **2**（`err_instant_unknown_modifier(modifier)`, `err_toml_missing_field(field)`）
- 169メソッド全てが実際に呼び出されている（未使用0件）— 現時点でテーブル化前でも死蔵キーは無い

## 既存パターン（再利用できるもの）

- **`ui/src/lib/i18n.ts`**: `TranslationKey` 型（文字列リテラルユニオン）+ `JA_JP`/`EN_US` の
  `Record<TranslationKey, string>` 2テーブル + `t(key, params?)` で `{param}` 置換。issue が
  名指しで参照する設計目標。ただし Rust では文字列リテラルキーの代わりに `enum Key` を使うことで
  タイポを型で防げる（TS は `Record<K,V>` の網羅性チェックで同等の安全性を確保しているが、Rust
  enum は同じ安全性を「1つの match（1キー1アーム、ja/en 同居）」で得られ、かつ TS の2テーブル
  分離よりもさらに強い——1アームが tuple 型なので、そのアームを書く時点で ja/en 両方が
  揃っていないとコンパイルが通らない）
- **`ui/src/lib/i18n.test.ts`**: 「EN テーブルが JA の全キーを持つ」ことを確認するテスト。Rust 側は
  enum + 単一 match の設計なら構造的に不要（コンパイラが両言語同時に強制する）
- **`tabs/backup.rs` の `localize_toml_error` テスト（271-331行）**: `tr_ja()`/`tr_en()` ヘルパーで
  `Tr(Language::Ja/En)` を直接構築し、`err_toml_missing_field` 等の出力文字列を assert している。
  テーブル移行後も出力文字列を変えなければこのテストは無改修で green のまま — 挙動保存の
  リグレッションガードとして機能する
- **`config_error_message`（app.rs:236-263）自体には現状テストが無い** — 今回の断片連結解消が
  最も壊れやすい箇所であるにもかかわらず未検証。ピン留めテストの追加が必要（後述）

## 技術的制約

- **`i18n` モジュールは `pub(crate)`**（`main.rs:6`）。`src-tauri`・`snotra-core`・他クレートからの
  参照は0件（Explore agent 調査で確認済み）。変更の影響範囲は `snotra-settings` クレート内に
  完全に閉じる
- **新規依存クレートは不要**: `strum`/`phf`/`once_cell`/`lazy_static` は `snotra-settings/Cargo.toml`
  に直接依存として存在せず（`Cargo.lock` 上の推移的依存のみ）、追加するには `Cargo.toml` 編集が
  要る。プレーンな `enum` + `match` で要件を満たせるため、新規依存は追加しない（YAGNI）
- **Win32 API 依存なし**: 本 issue は純粋な文字列テーブルのリファクタであり、`ime.rs`/`hotkey.rs`
  系の非同期 Win32 API の同期性検証は不要
- **egui UI コードではない**: `i18n.rs` の `Tr`/`Key`/`raw()`/`config_error_message` は egui 非依存の
  純粋関数・データであり、`snotra-settings/CLAUDE.md` の「ユニットテストは書かない方針」の
  「例外1: 純粋な非 egui ヘルパー」に該当する。テスト追加は方針に反しない
- **`Language` enum**（`snotra_core::config::Language`）は現在 `Ja`/`En` の2バリアントのみ。第3言語は
  未実装（issue の将来課題であり本 issue のスコープ外）

## 未解決の疑問

なし。issue の記述（テーブル駆動 + 完全文プレースホルダ + ui/i18n.ts 準拠）と既存コードの
構造から実装方針が一意に導ける。advisor との事前相談で「call site を `tr.t(Key::X)` へ全面移行する
（薄いラッパーメソッドで温存しない）」の方針を確認済み（ui 側設計との整合、および将来の
未使用キー検出を dead_code lint に委ねられる利点のため）。
