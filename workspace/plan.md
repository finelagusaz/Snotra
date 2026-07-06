# Plan — issue #438: i18n をテーブル駆動化し断片連結を廃止する

> plan-review（3並列: call-site 網羅性検証 / ドキュメント・テスト同期検証 / 独立再導出）を実施済み。
> 検証で判明した誤り（call site 実数209件・10ファイル、テストコマンドのスコープ誤り、`Key` 命名の
> 衝突リスク）と、独立再導出でより issue の意図に忠実と判断された設計変更（言語別に独立した
> 網羅 match）を反映済みの版。差分点は各節に明記する。

## 設計方針

- `Tr(Language)` の169 `pub fn` メソッドを全廃し、以下に置換する:
  - `pub(crate) enum TrKey`（169+α バリアント、既存メソッド名を snake_case → PascalCase に機械変換した
    1:1対応。**`Key` ではなく `TrKey` と命名する**——`hotkey_input.rs:143` が `use egui::Key;` を
    ローカル束縛しており、`Key` だと同ファイル内で `egui::Key` と衝突・混同するリスクがあるため
    （plan-review で指摘）
  - **言語ごとに独立した2つの網羅 match 関数**:
    `fn ja(key: TrKey) -> &'static str` / `fn en(key: TrKey) -> &'static str`
    （**単一の `(ja, en)` タプルを返す1関数という初期案は不採用**。理由: `ui/src/lib/i18n.ts` が
    `JA_JP`/`EN_US` という言語ごとに独立したテーブルで構成されており、issue が名指しする設計目標と
    構造的に一致するのはこちらである。加えて、tuple 版は「第3言語追加時に既存169アーム全部を
    2要素→3要素タプルへ書き換える」必要があるのに対し、独立関数版は「`fn fr(key)` を丸ごと1つ
    追加するだけで既存 `ja()`/`en()` に一切触れない」——issue が名指しする発見事項
    「第 3 言語追加時は約180メソッド全部にアーム追加が必要」をより直接的に解消する。
    独立再導出でも同じ結論に至っている）
  - 各キー追加時は `ja()`/`en()` 両方にアームが要る（2箇所）。これは「1関数1言語」という構造の
    トレードオフだが、tuple 版でも実質的に同じ情報量（ja文+en文）を書く必要があり、行数増加は
    同程度。むしろ「新キーはこの2関数だけを見ればよい」という局所性の分かりやすさを優先する
  - `impl Tr { pub fn t(&self, key: TrKey) -> &'static str; pub fn t_params(&self, key: TrKey, params: &[(&str, &str)]) -> String }`
    — `t()` はプレースホルダ無しキー用、`t_params()` は `{param}` を `params` で順に置換した
    `String` を返す（`ui/src/lib/i18n.ts::t(key, params?)` と同じ設計思想。正規表現は使わず単純な
    `String::replace` の繰り返しでよい）
  - **ワイルドアーム禁止をコンパイル/lint 段に固定する**: `ja`/`en` 関数の直前に
    `#[deny(clippy::wildcard_enum_match_arm)]` を付与する。`_ => ...` を書けば `TrKey` 追加時の
    非網羅コンパイルエラーを回避できてしまうため、この抜け道を lint で塞ぐ（独立再導出の提案を採用。
    「強制の階梯」でコンパイル/lint 段に落とす対応そのもの）
- 呼び出し側は薄いラッパーメソッドを残さず、**全209 call site を `tr.t(TrKey::Xxx)` /
  `tr.t_params(TrKey::Xxx, &[...])` に直接書き換える**（advisor 相談・独立再導出とも一致）。
  理由: (1) issue が名指しする `ui/lib/i18n.ts` の設計そのものが call-site 直呼びである。
  (2) `TrKey` を `pub(crate)` のバリアント単位で直接参照させることで、将来キーが UI から
  参照されなくなった場合に `dead_code` lint が拾える可能性を残せる（薄いラッパー `pub fn` を
  挟むと死蔵検出が構造的に不可能になる）
- **コンパイル時の対漏れ検出は維持される**（issue が許容する「テストによる代替ガード」への
  切り替えは不要）: `ja()`/`en()` はそれぞれ独立に `TrKey` に対する網羅 match であり、
  Rust コンパイラが両方に対して網羅性を強制する（`ui/src/lib/i18n.ts` の `Record<TranslationKey,string>`
  が `JA_JP`/`EN_US` それぞれに全キー必須を強制するのと同じ保証）。唯一の抜け道（ワイルドアーム）は
  上記の clippy lint で塞ぐ

## 変更ファイル一覧

### Phase 1 — テーブル基盤の構築 + 挙動ピン留めテスト（`snotra-settings/src/app.rs`, `i18n.rs`）

1. **`app.rs` に `config_error_message` のピン留めテストを先に追加**: `ConfigError` の
   **全12 variant × ja/en の計24アサーション**（`HotkeySystemConflict`/`WindowWidthTooSmall`/
   `FuzzyCapRatioOutOfRange`/`ScanPathEmpty`/`InstantCommandDuplicateName`/
   `InstantCommandUnknownModifier` は代表値で ja/en 両方、残り6 variant も ja/en 両方）について、
   **現状のコード**が返す文字列をそのまま assert する。目的は「リファクタ後も出力バイト列が
   変わらない」ことを保証するリグレッションガード（`config_error_message` は現状無テストで、
   今回最も壊れやすい箇所）
   - 実行して green を確認（現状のコードに対するテストなので既に通るはず）
2. **`i18n.rs` を全面書き換え**:
   - `enum TrKey`（`#[derive(Clone, Copy, Debug, PartialEq, Eq)]`、`pub(crate)`）を169+α バリアントで
     定義。元の1197行のセクションコメント（`// Window title` 等）をそのまま `TrKey` 定義の
     グルーピングに引き継ぐ
   - `#[deny(clippy::wildcard_enum_match_arm)] fn ja(key: TrKey) -> &'static str` と
     同 `fn en(key: TrKey) -> &'static str` を実装。既存167メソッドの文字列はそのまま
     対応するアームへ機械的に移す
   - 断片連結だった6キー + 部分連結1キー + 新規発見1キー（`ErrHotkeySystemConflict` /
     `ErrWindowWidthTooSmall` / `ErrFuzzyCapRatioOutOfRange` / `ErrScanPathEmpty` /
     `ErrInstantDuplicateName` / `ErrInstantUnknownModifier` / `ErrTomlMissingField` /
     `IndexScanExtensionsWithFolders`〔新設、後述〕）は `{value}` 等のプレースホルダを
     埋め込んだ完全文にする。**既存の出力文字列とバイト単位で一致させる**（語順・スペース・
     句読点を変えない。例: `ErrScanPathEmpty` は ja `"{n}のパスが空です"` / en `"{n} path is empty"`
     とし、現状の `"1のパスが空です"` / `"1 path is empty"` と同一になるようにする）
   - `IndexScanExtensionsWithFolders`（新設、`tabs/index.rs:60` の断片連結解消。独立再導出で
     発見・スコープに追加）: ja `"{extensions} (フォルダ含む)"` / en `"{extensions} (incl. folders)"`。
     呼び出し側は `tr.t_params(TrKey::IndexScanExtensionsWithFolders, &[("extensions", &scan.extensions.join(", "))])`
   - `impl Tr { pub fn t(&self, key: TrKey) -> &'static str; pub fn t_params(&self, key: TrKey, params: &[(&str, &str)]) -> String }`
     を実装。`t_params` は `t(key)` の結果に対し `params` を順に `{name}` → `value` で `replace`
   - （任意・低コストなら実施）`t_params` 内で `debug_assert!(!result.contains('{') || ...)` 等の
     軽量な未置換プレースホルダ検出を入れてもよい。**ただし `hint_path_placeholder` /
     `instant_description` / `hint_instant_command` / `hint_instant_program` の4キーは
     `{query}`/`{clip}`/`{date:...}`/`{uuid}`/`{path}` を恒久的にリテラル説明文として含むため
     `t()` のみで呼び、`t_params()` を経由させない**（この4キーは元々パラメータ化対象に
     含めていないため設計上抵触しないが、実装時に混同しないよう明記する）
   - 旧169 `pub fn` は削除

### Phase 2 — 呼び出し側の移行（コンパイルエラー駆動）

3. **`app.rs::config_error_message`** を断片連結の `format!` から `tr.t()`/`tr.t_params()` 直呼びへ
   書き換え（research.md で列挙した12 variant 全てのマッピングに従う）
4. **`tabs/index.rs:60`** の `label_incl_folders` 断片連結を `IndexScanExtensionsWithFolders` の
   `t_params` 呼び出しへ書き換え
5. **残り207 call site の機械的リネーム**（`app.rs` の他26箇所 + kittest テスト内7箇所、
   `hotkey_input.rs`, `style.rs`, `tabs/{backup,general,index,instant,opener,search,visual}.rs`）:
   `tr.method_name()` → `tr.t(TrKey::MethodName)`、パラメータありは
   `tr.t_params(TrKey::MethodName, &[...])`。Phase 1 で旧メソッドを削除済みのため、
   `cargo check -p snotra-core -p snotra -p snotra-settings` の `no method named 'xxx' found`
   エラーが漏れの検出器になる（compile-fail を改名検出器として使う）。
   **ただし `#[cfg(test)]` 内の7箇所（`app.rs` kittest テスト、`Tr(Language::En).method()` 形式）は
   通常の `cargo check`/`cargo build` では検出されない**（`#[cfg(test)]` はテストビルドでのみ
   コンパイルされる）。このため Phase 2 の反復では **`cargo check --all-targets` または
   `cargo test -p snotra-settings` を通常のチェックと同じループ内で回し**、テストコード内の
   漏れも同じフェーズで検出する（最終検証まで先送りしない）
6. Phase 1 のピン留めテストを再実行し、**バイト単位で出力が不変**であることを確認（Green）

### Phase 3 — ドキュメント同期

7. **`snotra-settings/CLAUDE.md`**「モジュール構成」節の `i18n.rs` の説明を更新:
   「各メソッド（`tr.tab_general()` 等）が `match self.0` で `&'static str` を返す」→
   「`TrKey` enum + 言語別テーブル関数（`ja()`/`en()`）+ `Tr::t(key)`/`Tr::t_params(key, params)`
   （`{param}` プレースホルダ置換）でテーブル駆動。新キー追加時は `TrKey` に variant を足すだけで
   `ja()`/`en()` が非網羅コンパイルエラーになり網羅を強制する」に書き換える
8. **`docs/architecture.md:136`**「多言語対応（3層）」節の設定 GUI 行を同期:
   「`Tr` 構造体の match ベース翻訳」→ ui 側（134行）と対になる表現
   （例:「`Tr` 構造体 + `TrKey` enum の言語別テーブル駆動翻訳（`t(key)`/`t_params(key, params)` +
   `{param}` プレースホルダ置換）」）

### スコープ外と判断し、このリファクタでは対処しないもの（plan-review の独立再導出で発見）

research.md の該当節に詳細を記載。理由も含めて要約:

- **`status_*` 系のプレフィックス連結**（`app.rs`/`tabs/backup.rs` 各所）: 動的内容の大半が
  常に英語の生エラー文字列であり、`err_*` 系ほど語順崩壊のリスクが高くない。踏み込むと
  変更範囲が大きく膨らむため、本 issue の技術的基盤だけ用意し別 issue に委ねる
- **`tabs/backup.rs:234` の英語ハードコード `"config dir not found"`**: テーブル駆動化とは
  無関係の欠落翻訳バグ。挙動不変の原則（バイト単位一致）に反するためこの PR では修正せず、
  **別 issue として起票する**（実装完了後に対応）
- **`tabs/backup.rs::localize_toml_error` の `tr.0 == Language::En` 直接参照**: 英語 UI では
  TOML パーサの生メッセージ（常に英語）をそのまま返す意図的な early-return であり、
  「語順が変わる断片」ではない。無理にテーブル化すると不自然な行が生まれるため据え置く
- **`config_error_message` を `Tr::config_error_message` として i18n.rs 側へ移設する案**
  （独立再導出の提案）: 検討したが不採用。`i18n.rs` は `Language`/`TrKey` のみに依存する
  汎用翻訳ユーティリティであり、`snotra_core::config::ConfigError`（バリデーション固有の型）への
  依存を持ち込むと責務が混ざる。断片連結（暗黙の語順契約）はキー側を完全文プレースホルダ化する
  ことで既に解消されるため、`config_error_message` を app.rs の自由関数のまま「ConfigError
  variant → (TrKey, params) の対応表」に簡素化するだけで principle 5 は満たせる

### SPEC.md 更新要否

**不要**（plan-review で SPEC.md 全文を i18n/翻訳/エラーメッセージ関連キーワードで grep 済み、
`Tr`/`i18n.rs` の実装詳細への言及なしを確認）。挙動（表示文字列・UI 動作）を一切変えない
リファクタであり、`SPEC.md` に記載されたユーザー可視の仕様・フロー・IPC 契約に変更はない。

### e2e への影響

**無し**（plan-review で確認済み）。`e2e/tauri.slash.e2e.ts` は `snotra-settings`
（egui ネイティブウィンドウ）に一切触れておらず、同ファイルのコメントにも
「snotra-settings は egui ネイティブウィンドウのため WebDriver から不可視」と明記されている。

## 実装順序（フェーズ依存関係）

Phase 1（ピン留めテスト → テーブル基盤）→ Phase 2（呼び出し側移行、コンパイラ駆動。
`cargo check`/`cargo test` 双方を毎回のループに含める）→ Phase 3（ドキュメント同期）。
各 Phase 完了時点で `cargo check -p snotra-core -p snotra -p snotra-settings` が通ることを
確認してからコミットする。

## 不変条件

- **出力文字列の完全な不変**: 本リファクタは構造変更のみが目的であり、ユーザーに見える
  日本語・英語の文言は一切変更しない（`err_scan_path_empty` 系のスペース有無を含め、既存の
  微妙な表記もそのまま踏襲する。`tabs/backup.rs:234` の英語ハードコードのような改善したい
  箇所があっても本 issue のスコープでは変更せず、別途 issue 化する）
- **`ja()`/`en()` の網羅性**: `TrKey` に新バリアントを追加したら、`ja()`/`en()` の両方が
  非網羅になりコンパイルエラーになる（`#[deny(clippy::wildcard_enum_match_arm)]` により
  ワイルドアームでの回避も防ぐ）。この不変条件は Rust の網羅 match + clippy lint が構造的に
  保証するため、追加のテストは不要
- **`t_params()` のプレースホルダ未置換時のフォールバック無し**: `ui/src/lib/i18n.ts::t()` と
  同様、`params` に対応しないプレースホルダはそのまま文字列に残る（no-op）。呼び出し側が必ず
  必要な全パラメータを渡す責務を負う（既存の `format!` ベースでも同様の暗黙契約だったため、
  新たなリスクではない）
- **`i18n` モジュール・`TrKey` の可視性は `pub(crate)` のまま変えない**（外部クレートからの
  参照が無いことを調査済み。加えて `snotra-settings` は `[lib]` ターゲットを持たない `[[bin]]`
  のみのクレートであり、構造的にも外部参照は不可能）

## テスト方針

- **追加**: `app.rs` に `config_error_message` のピン留めテスト（`ConfigError` 全12 variant、
  ja/en 計24アサーション）。egui 非依存の純粋関数テストであり `snotra-settings/CLAUDE.md` の
  「例外1: 純粋な非 egui ヘルパー」に該当するため方針に反しない
- **既存**: `tabs/backup.rs` の `localize_toml_error` 系テスト（`tr_ja()`/`tr_en()` ヘルパー使用）は
  無改修で green を維持することを確認する（`err_toml_missing_field` 等の出力不変の裏付け）。
  `app.rs` の kittest テスト5件（`Tr(Language::En).method()` を使う箇所）は呼び出し形式を
  `Tr(Language::En).t(TrKey::Xxx)` に置換するのみでアサーション内容は不変
- **検証コマンド**（`docs/build-commands.md` カテゴリA「`.rs` ファイルを変更した場合」に
  正確に一致させる。plan-review で当初案のスコープ誤りを指摘されたため修正済み）:
  - `cargo check -p snotra-core -p snotra -p snotra-settings`（必須・無条件。変更クレートに
    関わらず全crate対象——`snotra-settings` のみに絞っていた当初案は誤り）
  - `cargo clippy -p snotra-core -p snotra -p snotra-settings --all-targets -- -D warnings`
    （必須・無条件。`--all-targets` によりテストコードも lint 対象になり、`#[cfg(test)]` 内の
    漏れも拾える）
  - `cargo test -p snotra-settings`（必須・`snotra-settings` 変更のため。`cargo test -p snotra-core`
    は `snotra-core` 非変更のため対象外）

## セルフレビュー

1. **対称コードパス**: 該当なし（`show`/`hide` のような対称ペアを持つ変更ではない、純粋な
   文字列テーブルのリファクタ）
2. **影響範囲の網羅性**: plan-review の独立検証により当初の「202箇所・9ファイル」が
   「209箇所・10ファイル」（`tabs/visual.rs` 16箇所 + `app.rs` kittest テスト内7箇所）の
   誤りであったことが判明し、本版で修正済み。`i18n` モジュールの外部参照は `pub(crate)` +
   `[lib]` ターゲット不在により構造的に存在し得ないことを2系統の独立調査（Explore agent の
   影響範囲調査 + plan-review の call-site 検証）で確認済み
3. **境界条件**: 断片連結7キー（新規発見の `IndexScanExtensionsWithFolders` 含む）のプレースホルダ
   位置・スペース有無を現状の `format!` 呼び出しから逐語的に書き起こし、バイト単位一致を
   明記した（research.md 参照）。`FuzzyCapRatioOutOfRange` の `f64` → `to_string()` が
   `format!("{}", value)` と同一の `Display` 実装を使うため出力が変わらないことも確認済み
4. **リソース管理**: 該当なし（ファイルハンドル・スレッド・リスナー等のリソースを生成しない）
5. **既存パターンとの整合**: `ui/src/lib/i18n.ts` の `JA_JP`/`EN_US` 独立テーブル + `t(key, params?)`
   設計に合わせた（当初の単一タプルmatch案から、より忠実な設計へ plan-review で修正）
6. **YAGNI 違反**: 新規依存クレート（`phf`/`strum`等）を追加しない。第3言語対応の枠組み
   （汎用 N 言語対応の trait 抽象化）は本 issue のスコープ外であり導入しない。独立再導出で
   発見された `status_*` 系プレフィックス連結・英語ハードコードバグは、本 issue の直接スコープ
   （`err_*` 系の断片連結解消）を超えるため意図的に対象外とした（理由を本文に明記）
7. **シンプル化の挑戦**: 薄いラッパーメソッド案（202/209 call site を変えずに済む案）と
   `config_error_message` の `Tr` 移設案の両方を検討したが、前者は advisor 相談で不採用
   （issue の設計意図・死蔵キー検出の可能性を優先）、後者はセルフレビューで不採用
   （i18n.rs の責務が ConfigError という別ドメインへ漏れ出すため）。新たな状態
   （`AtomicBool`・`Mutex`等）は導入しない、純粋データ変換のみ
8. **破壊不変条件の明示**: 本変更が「壊れたら即アウト」とみなす不変条件は「表示文字列の不変」の
   1点のみ。検知手段はピン留めテスト（`config_error_message`、24アサーション）+ 既存
   `backup.rs` テスト + 実装後の目視スモーク（設定 GUI を起動し General/Search/Index/Visual/
   Opener/InstantCommand/Backup 全タブと日英切替、バリデーションエラー表示を確認）。Win32
   フックやホットキー等の「戻ってこない」系リスクは本変更に存在しない

## plan-review 結果（統合サマリ）

| 検証エージェント | 結果 |
|---|---|
| call-site 網羅性検証 | 要対処 → 反映済み（209箇所・10ファイルに修正、kittest 7箇所を明記、Phase 2 の検出コマンドに `cargo test`/`--all-targets` を追加） |
| ドキュメント・テスト同期検証 | 要対処1件 → 反映済み（テストコマンドのスコープを全3crateに修正）。軽微な懸念1件 → 反映済み（`Key`→`TrKey` に改名） |
| 独立再導出 | 設計面で採用（言語別独立 match・`#[deny(clippy::wildcard_enum_match_arm)]`・`label_incl_folders` のスコープ追加）。一部不採用（`config_error_message` の Tr 移設、`status_*` 系の全面テンプレート化、`localize_toml_error` の `tr.0` 直接参照修正）とその理由を明記 |

総評: 3エージェントの指摘を反映し、完全性の確度は高いと判断。実装着手可。
