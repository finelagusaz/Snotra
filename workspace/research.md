# research.md — インスタントコマンド修飾子パイプ (issue #403)

## issue の要約

変数 `{query}`/`{clip}` に修飾子パイプ `{name | mod | ...}` を追加。v1 修飾子: `lower`/`upper`/`trim`/`default:<text>`/`raw`。エンコード=シンク（種別）専任、`raw` が唯一の抑止。展開順序: 解決→修飾子(左→右)→シンク。不変条件: 修飾子出力は env 非展開・argv 不可分。不明修飾子は保存時拒否。後方互換: パイプなしは現状と byte-identical。SPEC §19.4/§19.8 が契約を先行記述済み（commit 1ab8f9e）。

## 関連コード

### snotra-core（中核）
- `snotra-core/src/instant.rs`:
  - `split_args`（:9）シェル風トークン化（再利用・変更なし）
  - `expand_vars`（:31）`.replace("{query}"..).replace("{clip}"..)` 素朴置換 ← **差し替え点**
  - `expand_exec_args`（:40）split→各トークン env_expand→expand_vars（exec 経路）
  - `expand_instant_command`（:56）URL 判定→query/clip をグローバル pre-encode（:60-61）→expand_vars（URL 経路）
  - `filter_instant_commands`（:70）前方一致（不変）
  - 既存テスト（:84-293）27 件
- `snotra-core/src/config.rs`:
  - `InstantCommand`（:49）/ `InstantAction`（:59 Url/Exec/Legacy）— **スキーマ不変**
  - `Config::validate()`（:1094-1165）instant 重複名チェック（:1150 近傍）← **バリデーション追加点**
- `snotra-core/src/error.rs`: `ConfigError`（:62-75）← **variant 追加点**

### snotra-settings
- `src/app.rs`: `save()`（:172-207）→ `validate()`（:183）→ エラー時 `config_error_message()`（:223-247）← **match arm 追加**（enum variant 追加でコンパイラ強制）
- `src/i18n.rs`: `err_*` パターン（`err_instant_duplicate_name` 同型）← エラー文字列追加
- `src/tabs/instant.rs`: プレビュー（:286 URL `expand_instant_command` / :327,:333 exec `expand_exec_args`）は core 関数経由＝**自動追従**（変更不要）

### 呼び出し元（コード変更なし・検証のみ）
- `src-tauri/src/commands/instant.rs`（:60,:68）`expand_instant_command`
- `src-tauri/src/commands/launch.rs`（:396）`expand_exec_args`

### UI / e2e（変更なし）
- `ui/src/stores/search.ts`: 変数展開はバックエンド委譲。`description || display` は生テンプレ表示（修飾子付きでも壊れない）
- `e2e/`: instant_commands 実行テストなし＝前提を壊さない

## 既存パターン

- exec の **split→env→置換** の順序が env 非展開・argv 不可分を担保（既存テスト `exec_args_external_input_is_not_env_expanded` / `exec_args_query_cannot_inject_extra_args`）
- バリデーションは core `Config::validate()` に集約、UI は `config_error_message` + i18n で表示（既存の instant prefix / duplicate name と同型）
- プレビューは core 関数を再利用（DRY）＝修飾子は core 更新で自動追従
- opener `build_launch_args` の `{path}` は**別概念**（修飾子対象外・`split_args` のみ共有）

## 技術的制約

- Win32 非依存（純ロジック crate）。SendInput / ウィンドウ系 API は不使用＝非同期性の懸念なし
- URL エンコードは `percent_encoding`（NON_ALPHANUMERIC）
- `snotra-core/CLAUDE.md`: TDD 必須、UI 表示文字列を持たない（エラーは型/フラグで伝える）
- `InstantAction` の serde 表現は不変＝旧オンディスク形式の deserialize テスト追加は不要（CLAUDE.md serde チェックリスト非該当）
- enum variant 追加で `config_error_message` の網羅 match がコンパイラ強制で随伴（＝改名検出器）

## 未解決の疑問

- `expand_vars`（素 replace）残置 or 削除（実装時に呼び出し元を grep で最終確認して決定）
- → ModifierError か ConfigError variant かは plan-review で **ConfigError::InstantCommandUnknownModifier に確定**
