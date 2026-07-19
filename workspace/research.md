# research: issue #586 既知の SSOT 不一致の解消

## issue の要約

設計書 `docs/superpowers/specs/2026-07-19-doc-governance-design.md`（PR #590）で実地検証した 4 件の SSOT 不一致を解消する。いずれも「変更が正準を動かしたのに写しが追従しなかった」構造。

## 関連コード・文書（実在確認済み）

1. **`docs/architecture.md:52`** — 「ファイル単位のモジュール構成と各モジュールの責務は、各サブディレクトリの `CLAUDE.md` を SSOT とする」。#562 以降の実態（`snotra-core/CLAUDE.md:10`・`src-tauri/CLAUDE.md` モジュール構成節）は「`//!` を正準、CLAUDE.md は索引 + 横断不変条件」であり、古い指名
2. **`CONTRIBUTING.md:16`** — 「レビュー承認後にスカッシュマージまたは通常マージする」。リポジトリ設定は squash-only（`allow_merge_commit=false` / `allow_rebase_merge=false`、#488。ルート `CLAUDE.md` Git/GitHub 運用節に実測記録）
3. **`.claude/rules/safety-nets.md`** — 本文 11 行目は配送対象を「PreToolUse/PostToolUse hook・`.githooks/`・CI workflow・**スキル/ドキュメントの規約**」と宣言し、21 行目の節「安全網が『規範』の場合」は規範向けの故障注入手順を持つ。しかし frontmatter `paths` は `.claude/hooks/**` / `.githooks/**` / `.claude/settings.json` / `.github/workflows/**` のみで、rules・skills の編集では配送されない
4. **アイコンバッチ形式の写し** — grep（`status:u8|png_len|count:u32`）で数え上げた結果、issue 記載の 3 箇所ではなく **4 箇所**:
   - `src-tauri/src/icon.rs:108-115` — `encode_batch_binary` の rustdoc（エンコーダ側・条件を正確に記述）
   - `ui/src/lib/iconBatch.ts:2` — デコーダ側 TSDoc（条件を正確に記述）
   - `docs/architecture.md:127` — 条件を正確に記述
   - `SPEC.md:92` — 「`[count:u32 LE]` + 各アイコン `[status:u8][png_len:u32 LE][png_bytes]`」。**status=0 のとき `png_len` 以降が無いという条件が欠落**（曖昧さの実体）

## 既存パターン

- SSOT 一本化の様式は #562 が確立済み: 正準（`//!`）+ 索引/参照（CLAUDE.md）。責務分担表（設計書 §1）に従う
  - `docs/architecture.md` の役割: 「関数名・バイト形式・現在の状態式を持たない」
  - `SPEC.md` の役割: ユーザー観測可能な契約。実装名・式を書かず、コード/テストを参照
- `.claude/rules/` の `paths` glob 意味論: bare 名はルート直下のみ・階層横断は `**/` 明示必須（メモリ `reference_claude_rules_paths_matcher` で確認済みの実測）。`.claude/rules/**` / `.claude/skills/**` の形なら配下全体に届く

## 技術的制約

- Win32 / IPC 境界には触れない（文書とコメントのみの変更。コード挙動の変更なし）
- `.claude/rules/safety-nets.md` の変更は**エージェント設定の変更**に当たる——ルート CLAUDE.md「最重要ルール」3 により合意が必要。ただし本 issue は設計書（PR #590 で合意済み）が「`paths` 拡張か本文縮小かを issue 内で判定する」ことまで含めて合意している
- `ui/src/lib/iconBatch.ts` の編集は PostToolUse hook の typecheck を発火する（コメントのみの変更なので沈黙 = 合格が期待値）
- `.claude/rules/safety-nets.md` は `selectChecks` の割り当て対象外（`.md`）——沈黙は「何も走らなかった」であり、frontmatter YAML の破損は機械検査に掛からない。編集後に YAML として読めることを目視確認する

## 判定（issue 内判定を委ねられた点）

**safety-nets.md は「`paths` 拡張 + 本文の宣言の精密化」の複合で解消する**:

- `paths` に `.claude/rules/**` と `.claude/skills/**` を追加する。本文の規範向け節（読者を演じる故障注入・#488/#489）は rules/skills を編集する局面でこそ効く内容であり、宣言に配送を合わせるのが正しい方向。rules/skills の編集は低頻度・高リスク（エージェント設定変更で合意必須）のため配送ノイズは小さい
- ただし「ドキュメントの規約」（ルート `CLAUDE.md` / `AGENTS.md` 等）まで `paths` に足すと、頻繁な文書編集のたびに配送されノイズが大きい。本文側の守備範囲宣言を「規範文書の変更時は自動配送されない（手動参照）」と精密化し、両者の主張を一致させる

## 未解決の疑問

なし（4 件とも修正方針が確定）。
