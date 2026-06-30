# plan — ホットキーバリデーション Rust/TS 乖離の解消 (#409)

## 種別判定（AGENTS.md Step 0）

**(c) 孤児コード削除 + SPEC 訂正**（ユーザー判断）。バグでも仕様変更でもなく、**死蔵コードの除去 + doc を as-built に戻す**整合作業。Rust の検証挙動（`Config::validate` / `hotkey_input.rs` / `parse_vk`）は一切変えない。SPEC §7.4 が記述する「保存時バリデーション」の挙動は不変——変えるのは「フロントも同じリストでガード」という**実装と乖離した記述**のみ。

## 変更ファイル一覧

| ファイル | 変更 | 理由 |
|---|---|---|
| `ui/src/lib/hotkeyValidation.ts` | **削除** | 孤児コード（live import ゼロ・git grep で確定）。`FORBIDDEN_MAIN_KEYS` は egui キャプチャ不能 + `parse_vk`→reg 失敗と冗長 |
| `ui/src/lib/hotkeyValidation.test.ts` | **削除** | 削除対象ファイルのみを exercise する test |
| `SPEC.md` (343) | フロントガード節を削除し実態に訂正 | 「フロントも同じリストでガード」が二重に不正確（リスト不一致 + 死蔵） |
| `ui/CLAUDE.md` (35) | モジュール構成から `hotkeyValidation.ts` エントリ削除 | AGENTS.md「モジュール構成ドキュメント同期」（.ts 削除時） |

## 実装内容（逐語）

### 1. ファイル削除（git rm）
- `ui/src/lib/hotkeyValidation.ts`
- `ui/src/lib/hotkeyValidation.test.ts`

### 2. SPEC.md line 343 置換
- 旧: `- snotra-settings のキャプチャ UI でも即時拒否し、フロントエンド（\`hotkeyValidation.ts\`）でも同じリストでガードする`
- 新: `- snotra-settings のキャプチャ UI（\`hotkey_input.rs\`）でも \`is_system_shortcut\` で即時拒否する（保存時の \`Config::validate()\` がバックストップ）`

### 3. ui/CLAUDE.md line 35 削除
- 削除: `- \`hotkeyValidation.ts\`: ホットキーの有効性チェック（\`isHotkeyInvalid\`・\`formatHotkeyLabel\`）。Win キー・禁止キー・修飾キーなしをガード`

## 実装順序

単一フェーズ（相互依存なし）。削除 2 + 編集 2。

## 不変条件

1. **Rust 検証挙動は不変**: `Config::validate` / `is_system_shortcut` / `hotkey_input.rs` / `parse_vk` に触れない。ホットキー保存・登録の挙動は現状維持。
2. **削除の安全性**: `git grep hotkeyValidation` の live import = ゼロ（exhaustive）。typecheck（カテゴリ B）が build backstop。
3. **SPEC §7.4 の他記述は不変**: 表・除外理由・正規化・Win+* ワイルドカードは正確なので触らない。セクション番号も不動。
4. **doc 同期の完全性**: TS ファイル削除に伴い、それを参照する SPEC.md:343 と ui/CLAUDE.md:35 を**同時に**更新（参照の宙吊り＝リンク切れを残さない）。

## テスト方針

- **削除されるテスト**: `hotkeyValidation.test.ts`（17 ケース）。これらが守っていた不変条件は「孤児バリデータの内部正しさ」であり、live パスが無いため**孤立した不変条件ではない**（証明対象の機能自体が存在しない）。新規補完テストは不要。
- **検証コマンド**: `docs/build-commands.md` カテゴリ B（`.ts` 削除）= typecheck + frontend test。削除でビルド/型が壊れないこと、残存テストが緑であることを確認。Rust 変更なしのためカテゴリ A は非該当。

## SPEC.md 更新要否

更新する（line 343 のみ）。挙動変更なし＝記述の as-built 訂正。docs/architecture.md 等への波及なし（横断パターンに該当せず）。

## セルフレビュー（start-issue Step 5b）

1. **対称コードパス**: 該当なし（削除 + doc。show/hide 等の対称ペア無関係）。
2. **影響範囲の網羅性**: `git grep`（全 tracked 横断）で `hotkeyValidation` 参照を exhaustive に列挙＝SPEC.md:343 + ui/CLAUDE.md:35 の 2 箇所のみ。両方を更新対象に含めた。`e2e/`・`docs/` 参照ゼロも確認。
3. **境界条件**: 削除対象に動的 import/barrel/path alias 無し（git grep が import 文を捕捉）。typecheck が最終 backstop。
4. **リソース管理**: 該当なし。
5. **既存パターンとの整合**: 既存 validate の設計哲学（競合キーは拒否・解釈不能キーは reg 失敗委譲）に合わせ、`FORBIDDEN_MAIN_KEYS` を backend に**昇格させない**＝非対称を作らない。
6. **YAGNI 違反**: なし。むしろ過剰防御（死蔵リスト）の除去。
7. **シンプル化**: 状態・インターフェース導入ゼロ。コード行数は純減。
8. **破壊不変条件**: Rust 検証経路を触らないため、ホットキー登録の「戻ってこない」系リスクはゼロ。typecheck で frontend build 健全性を検知。

### check スキル判定（plan-review Step 5a）

- `/plan-review`: 影響範囲は `git grep` で exhaustive に確定済み（import 文を全 tracked から捕捉）+ typecheck が deletion の build backstop。**完全性が要件のタスクではなく局所的削除**のため、多エージェント fan-out は不均衡（plan-review Step 2b「局所的な計画では省略可」該当）。独立確認の価値がある「本当に死蔵か」は git grep の exhaustive 性が既に担保。→ 1 体の Explore で死蔵 + 影響漏れを独立再確認するに留める。
- `/symmetric-check` `/race-check` `/cache-check` `/state-check`: 非該当（コードパス追加・async・cache・state いずれも無し。純削除 + doc）。
