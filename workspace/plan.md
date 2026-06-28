# 実装計画: インスタントコマンド変数の修飾子パイプ (issue #403)

SPEC §19.4 / §19.8 が契約を定義済み（commit 1ab8f9e, branch `feat/instant-modifier-pipe`）。本計画はその実装追従。

## 1. 要件（受け入れ条件）

- 文法 `{name | mod | ...}`、name ∈ {query, clip}。`|` `:` 周りの空白は任意。
- v1 修飾子: `lower` / `upper` / `trim` / `default:<text>` / `raw`。
- **エンコード = シンク専任**: URL 種別は `raw` なければ percent-encode、exec 種別は encode なし。`urlencode` 修飾子は提供しない。
- 展開順序: 変数解決 → 修飾子(左→右) → シンク処理。
- 不変条件継承: 修飾子出力は env 非展開・argv 不可分。
- 不明修飾子名は**設定保存時に拒否**（実行時へ到達させない）。
- 後方互換: パイプなし `{query}`/`{clip}` は現状と byte-identical。
- 受け入れテスト（具体値）:
  - `{query | trim}` URL: `"  Foo Bar "` → `Foo%20Bar`
  - `{query | lower | raw}` URL: `Docs/API` → `docs/API`（encode せず）
  - `-s {query | trim}` exec: `"  report  "` → `["-s","report"]`
  - `{query | default:.}` exec 空入力 → `["."]`
  - 不明修飾子 → 設定保存エラー
  - 既存テスト（`url_query_is_encoded` 等）全て緑のまま

## 2. 影響範囲

### 触る

- **`snotra-core/src/instant.rs`**（中核。現状 `expand_vars:31` が `.replace()` の素朴置換＝差し替え点）:
  - 新規 `enum Modifier { Lower, Upper, Trim, Default(String), Raw }`
  - 新規 `parse_placeholder(inner: &str) -> Option<ParsedPlaceholder>`: `{...}` 内側を解析。name ∈ {query, clip} 以外は `None`（リテラル扱い）。`|` 分割・空白 trim・`default:` 引数（最初の `:` 以降）。**runtime 適用と保存検証で共有する単一パーサ**（DRY）
  - 新規 `expand_template(template, query, clipboard, encode: bool) -> String`: `{...}` を走査→`parse_placeholder`→変数解決→修飾子チェーン→`encode && !raw` のとき percent-encode。**URL/exec 両シンクが共有する単一 walker**。未認識 `{...}`・parse 失敗はリテラル emit（total・panic しない）
  - 新規 `collect_unknown_modifiers(template) -> Vec<String>`: 保存バリデーション用。`parse_placeholder` を再利用して未知修飾子名を収集（`config.rs::validate` が使用）
  - 改修 `expand_instant_command`（:56）→ `is_url` 判定後 `expand_template(.., is_url)` へ委譲。現状のグローバル pre-encode（:60-61）を廃し per-placeholder encode + `raw` 抑止へ
  - 改修 `expand_exec_args`（:40）→ 各トークンを `expand_template(&env_expand(tok), .., false)` で展開。`env_expand(tok)` → placeholder 解決の順は維持（env 非展開・argv 不可分を保存）
  - **決定点**: `expand_vars`（:31）はシンク経路で不要になる。素 replace ユーティリティとして残置 or 削除を決め、CLAUDE.md と整合させる
- **`snotra-core/src/error.rs`**: `ConfigError` に variant 追加 `InstantCommandUnknownModifier { name: String, modifier: String }`
- **`snotra-core/src/config.rs`**: `Config::validate()`（:1094-1165、instant 重複名チェック :1150 近傍）に、各コマンドの url/args テンプレへ `collect_unknown_modifiers` を適用し未知名ごとに `ConfigError::InstantCommandUnknownModifier` を push。テスト追加
- **`snotra-settings/src/app.rs`**: `config_error_message()`（:223-247）の `match` に新 variant の arm を追加（**コンパイラ強制**: 非網羅だとビルド不能）。`save()`（:172-207）は `validate()` 経由で自動的に保存ブロック＝変更不要
- **`snotra-settings/src/i18n.rs`**: 不明修飾子エラー文字列を `err_*` パターン（`err_instant_duplicate_name` と同型）で ja/en 追加
- **`snotra-core/CLAUDE.md`**: instant.rs 公開関数リストを同期（新規関数・`expand_*` の説明更新・`expand_vars` の扱い）
- **`docs/architecture.md`**（:141）: instant.rs 記述を修飾子パイプ + 「エンコードはシンク責務・`raw` 抑止」に同期
- **テスト**: `snotra-core/src/instant.rs` `#[cfg(test)]` に修飾子テスト群 + `config.rs` に validate テスト

プレビュー自動追従（**変更不要**を確認のみ）:
- `snotra-settings/src/tabs/instant.rs`（:286 URL / :327,:333 exec）はプレビューが core 関数経由＝修飾子自動反映

### 触らない（根拠）

- `src-tauri/src/commands/launch.rs:396`・`instant.rs:60,68`: **core 関数シグネチャを非破壊**に保てば呼び出し側は無改修（引数 `(args, query, clipboard, env_expand)` / `(command, query, clipboard)` 不変）。`cargo build`（src-tauri 含む）で確認
- `ui/`（SolidJS）: 変数展開はバックエンド責務。フロントは query を送るだけ。`display` 副テキストはテンプレートをそのまま表示（修飾子付きでも可）。`e2e/` を grep しインスタントコマンド変数前提が無いか確認
- `snotra-core/src/config.rs` の `InstantCommand`/`InstantAction`（:49/:59）: 修飾子はテンプレート文字列内の構文＝**スキーマ不変・新フィールド不要・マイグレーション不要**

## 3. 対称パス確認

- URL 種別 / exec 種別の2経路に修飾子適用を**両方**入れる（片方漏れ防止）
- プレビュー（settings）と実行（src-tauri）は同一 core 関数を共有 → 対称性は構造的に保証（DRY）
- 保存バリデーション: url フィールドと args フィールドの**両方**を検証

## 4. 不変条件

- **env 非展開**: `expand_exec_args` は `env_expand(token)` → placeholder 解決の順を維持。修飾子出力（外部値由来）は env_expand の後に注入＝展開されない。`{query | upper}` が `%FOO%` を展開しないテストを追加
- **argv 不可分**: 修飾子適用は `split_args` の後、トークン内 in-place。`default:<text>` の空白挿入もトークン内に留まる。テスト追加
- **後方互換**: パイプなし `{query}`/`{clip}` は byte-identical。既存テスト全緑で担保
- **raw はシンクのみ作用**: content を変えず encode を抑止するだけ。exec では no-op

## 5. テスト（Red → Green）

- parse: 各修飾子・チェーン・空白許容・unknown→Err・`default:about:blank`（2個目 `:` リテラル）・未認識 name はリテラル
- URL sink: `trim`→encode / `lower|raw`→no-encode / 既存 encode テスト緑
- exec sink: `trim` / `default` 空判定 / env 非展開 / argv 不可分
- `validate_template`: 既知修飾子 OK、不明 Err

## 6. 検証

- `.rs` 編集 → clippy + core テスト（PostToolUse フック自動）
- 手動: `cargo test -p snotra-core` / `cargo build -p snotra-settings`（呼び出し側 compile-fail を改名検出器に）/ `cargo build`（src-tauri 含め非破壊確認）

## 7. plan-review で解決済み / 残論点

- ✅ 挙動等価性: グローバル pre-encode → per-placeholder encode は「query/clip 値のみ encode・テンプレリテラルは生」で同一（Agent 確認、複数出現・混在も等価）
- ✅ バリデーション差し込み点: core `Config::validate()`（config.rs）に集約。settings 側は `config_error_message` の match arm のみ（旧計画は「settings save ハンドラ」と誤配置）
- ✅ エラー型置き場: `ConfigError::InstantCommandUnknownModifier`（error.rs）。`instant.rs` 独自 ModifierError は不要
- ✅ 不変条件: env 非展開（`exec_args_external_input_is_not_env_expanded`）・argv 不可分（`exec_args_query_cannot_inject_extra_args`）は既存テストで担保、修飾子版を追加
- 残: `expand_vars` 残置/削除の決定（実装時）
- 残: opener 系 `build_launch_args` の `{path}` は**別概念＝修飾子対象外**（巻き込まない）を実装時に堅持

## 8. 実装順序（フェーズ）

- **Phase 1 — core パーサ + walker**: `instant.rs` に `Modifier` / `parse_placeholder` / `expand_template` / `collect_unknown_modifiers` を TDD（Red→Green）で追加。`expand_instant_command` / `expand_exec_args` を委譲に書き換え。**既存 27 テスト緑を維持**。`cargo test -p snotra-core`
- **Phase 2 — core バリデーション**: `error.rs` variant 追加 → `config.rs::validate()` に url/args の `collect_unknown_modifiers` 検査 + テスト。`cargo build -p snotra-core`
- **Phase 3 — settings**: `app.rs::config_error_message` の match arm + `i18n.rs` 文字列。`cargo build -p snotra-settings`（arm 漏れをコンパイラが検出＝mid-verify）
- **Phase 4 — docs 同期**: `snotra-core/CLAUDE.md`（公開関数リスト）+ `docs/architecture.md:141`。SPEC は先行済み＝as-built 突合のみ（§19 子セクション番号整合を確認）

## セルフレビュー（Step 5）

### 5a — check スキル
- **`/plan-review`**: 本セッションで実施済み（core / settings・i18n / 独立導出の3体並列）。独立導出が盲点（error.rs variant・app.rs match arm・validate の配置・docs/architecture.md）を検出 → §2 に反映済み。同一計画につき再実行は省略（結果は #403 コメントにも記録）
- **`/symmetric-check`**: URL シンク / exec シンクの対称ペアは**単一 walker `expand_template` で構造的に統一**＝対称性を担保。保存検証も url/args 両フィールド
- **`/state-check` `/cache-check` `/race-check`**: 非該当（UI モード・状態遷移・キャッシュ・async いずれも変更なし）

### 5b — チェックリスト
1. **対称パス**: ✅（5a。単一 walker で URL/exec を統一）
2. **影響範囲網羅**: ✅ 呼び出し元 grep 済み（src-tauri ×2・settings ×2）、独立導出と差分照合
3. **境界条件**: ✅ 空 query（default）・複数出現・未認識 `{...}`・`default:about:blank`（2個目 `:` リテラル）・env 値空白
4. **リソース管理**: 該当なし（新規プロセス・listener・状態フラグを導入しない）
5. **既存パターン整合**: ✅ validate + config_error_message + i18n の既存パターンに乗る。新規パターン導入なし
6. **YAGNI**: ✅ v1 修飾子5種に限定。`urlencode` 修飾子・カウンタ・selection 等は範囲外
7. **シンプル化**: ✅ 単一 walker + 単一パーサ（適用/検証共有でDRY）。`raw` は値変換でなくシンクへ渡すフラグ。新たな `AtomicBool`/`Mutex`/子プロセス無し
8. **破壊不変条件**: env 非展開・argv 不可分・後方互換 byte-identical。**検知手段** = 既存 27 テスト緑のまま + 修飾子版テスト（env 非展開・argv 不可分）追加。`cargo build`（src-tauri 含む）で呼び出し元非破壊を確認
