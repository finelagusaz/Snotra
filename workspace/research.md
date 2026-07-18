# research.md — issue #566: health-check Check 1 のテストファイル照合方針明確化

## issue の要約

`/health-check` Check 1 は各サブディレクトリ `CLAUDE.md` の「モジュール構成」と実ファイル一覧を照合する。
UI の対象 glob が `ui/src/**/*.{ts,tsx}` のため、文字どおりには `*.test.ts(x)` も母集団に入る。一方
`ui/CLAUDE.md`「モジュール構成」は production モジュールのみ列挙するため、production の記載漏れが無くても
テストファイルを「実ファイルあり・記載なし」と誤検出しうる（#555 サイクル末に顕在化）。

**決定事項（ユーザー合意済み・2026-07-18）**: Check 1 を **production モジュール整合チェック**と定義し、
テストファイルを個別列挙の対象外にする（issue 推奨の選択肢 1）。glob だけを真実源にせず、目的（何を索引すべきか）を明文化する。

## 関連コード / ファイル

- **唯一の変更対象**: `.claude/skills/health-check/SKILL.md`
  - Check 1 定義 = 行 16–31（対象定義の ui 行 = 行 23、手順 = 行 26–31）
  - 出力フォーマット = 行 131–153
- **変更なし（照合対象として現行が正しい）**: `ui/CLAUDE.md`
  - 現行「モジュール構成」節は既に production 限定。選択肢 1 では ui/CLAUDE.md は正しい状態なので触らない
- 参照のみ: `snotra-core/CLAUDE.md`・`src-tauri/CLAUDE.md`・`snotra-settings/CLAUDE.md`

## 実態（glob 実測）

| 母集団 | 件数 | 備考 |
|---|---|---|
| `ui/src/**/*.{ts,tsx}` 全体 | 46 | production + test |
| うち `*.test.{ts,tsx}` | **18** | すべて production と同居。issue の #555 時点「13」から **+5 増加** |
| production（test 除外） | 28 | `vite-env.d.ts` 含む。ui/CLAUDE.md「モジュール構成」の母集団 |
| テスト専用 helper / fixture / setup | **0** | `__tests__/`・`*fixture*`・`*setup*` いずれも不在 |

- **テスト件数が 13→18 と増加した事実**が、「現存ファイルを例外列挙する方式は恒常的ドリフト源」という issue の主張を実物で裏付ける（ハードコード禁止の根拠）。
- **Rust の対称性**: `snotra-core/src/**/*.rs`・`snotra-settings/src/**/*.rs`・`src-tauri/src/**/*.rs` を確認したが、独立したテストファイルは **0 個**。Rust はインライン `#[cfg(test)] mod tests` 規約ゆえ glob が別ファイルを拾わない。**この曖昧さは UI 固有**（`*.test.ts(x)` が production と同居する TypeScript だけの問題）。

## 既存パターン

- `ui/CLAUDE.md` には既に「テスト基盤」節があり、テストの構成・注意点を**方針として**記述している（個別ファイル列挙ではない）。テストの受け皿は既存でここ。
- #562（docgen migration・main 71df0ad にマージ済み）で「モジュール構成」節は **production のファイル名索引**と位置づけ直された（責務散文の正準は `//!`/TSDoc）。ゆえに Check 1 は「索引網羅性」の検査という性格が明確で、テスト（検証手段）を母集団に混ぜないのは圏の整合として自然。
- ガバナンス規約（`AGENTS.md`「検証の作法」）: 「全称表現は前提条件とセットで書く」「派生コピー同士の一致を完全性の証拠にしない」——Check 1 の報告は照合母集団と除外種別を証跡として添える必要がある。

## 技術的制約

- **`.claude/skills/` はエージェント設定** → 変更内容の合意が必要（チーム憲章）。**取得済み（選択肢 1）**。
- **現存 18 ファイルのハードコード禁止** → ファイル種別（`*.test.{ts,tsx}`）ベースの規則にする（issue 制約）。
- Win32 / IPC 依存なし（ドキュメント変更のみ・同期性の懸念なし）。
- **PostToolUse hook**: `.claude/skills/**/SKILL.md` は `selectChecks` に無い → 編集後の**沈黙は「未実行」であって合格ではない**。検証は手動 Check 1 実行が唯一の担保。

## 未解決の疑問

なし（方針確定・実態確認済み）。
