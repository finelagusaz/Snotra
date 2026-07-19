# plan: issue #586 既知の SSOT 不一致の解消

ブランチ: `fix/586-ssot-doc-drift`。文書・コメントのみの変更でコード挙動の変更はない。

## 変更ファイル一覧

| ファイル | 変更内容 |
|---|---|
| `docs/architecture.md` | (a) 52 行目: SSOT 指名を「各ファイルの `//!`（module doc）を正準とし、各サブディレクトリの `CLAUDE.md` はファイル索引 + 横断不変条件を持つ」に更新（#562 追従）。(b) 127 行目: バイト形式の行を削除し「バッチ形式の正準は `src-tauri/src/icon.rs` の `encode_batch_binary` rustdoc」への参照に置換 |
| `CONTRIBUTING.md` | 16 行目: 「スカッシュマージまたは通常マージする」→「スカッシュマージする（リポジトリ設定で squash のみ有効）」 |
| `.claude/rules/safety-nets.md` | (a) frontmatter `paths` に `.claude/rules/**` と `.claude/skills/**` を追加。(b) 本文 11 行目の守備範囲宣言を精密化: 配送されるのは hooks / githooks / settings.json / workflows / rules / skills であり、規範文書（ルート `CLAUDE.md` / `AGENTS.md` 等）の変更時は自動配送されない（手動参照）ことを明記 |
| `SPEC.md` | 92 行目: バイト形式の内訳（`[count:u32 LE]` + ...）を削除し、「バイナリバッチ形式（正準: `src-tauri/src/icon.rs` の `encode_batch_binary` rustdoc）」への参照に置換。ユーザー観測可能な事実（バイナリ IPC・Blob URL 化）は残す |
| `ui/src/lib/iconBatch.ts` | TSDoc 2 行目: 形式の再記述を「ワイヤ形式の正準は `src-tauri/src/icon.rs` の `encode_batch_binary` rustdoc。本デコーダはそれに一致し、往復は `iconBatch.test.ts` が検証する」への参照に置換 |
| `.claude/skills/health-check/SKILL.md` | **plan-review で検出（scout-docs と独立再導出が独立に一致）**: 42 行目の Check 2 説明文「モジュール構成の SSOT は各サブディレクトリの CLAUDE.md」が architecture.md:52 と同一の古い指名の写し。「正準は `//!`、CLAUDE.md は索引 + 横断不変条件」へ揃える。**スキル編集 = エージェント設定変更のため、実装着手前にユーザー合意を要する**（issue #586 本文には未記載の追加スコープ） |
| `AGENTS.md` | **plan-review（独立再導出）で検出**: 条件別チェック表 70 行目「安全網（hook・CI・`.githooks/`・`.claude/settings.json`・規範）…（対象を触ると自動配送）」が、規範文書まで自動配送されると読める第 3 の写し。paths 拡張後の実態（rules/skills は自動配送・規範文書は手動参照）に合わせ限定を追記 |

正準として残る形式記述は `src-tauri/src/icon.rs:108-115` の rustdoc のみ（エンコーダ = 形式の定義元）。`iconBatch.test.ts:28-56` は実バイト列を DataView で組み立てて検証しており、写しではなく検証手段（変更不要）。architecture.md 60/66/82/88 行の「→ モジュール構成は `<dir>/CLAUDE.md`」は索引参照として #562 後も正（変更不要）。

### safety-nets.md の自己包含について（意図的な設計判断）

`paths` に `.claude/rules/**` を足すと safety-nets.md 自身の編集で自分が配送される自己言及構造になる。既存 rules に前例はないが意図的に採る——同ファイルの内容（安全網変更時の運用手順・回避読者の故障注入）は rules ファイル自身の変更にもそのまま適用されるべきものであるため。

## 実装順序

単一フェーズ（各修正は独立・相互依存なし）。上表の順に編集し、最後にまとめて 1 コミット。

## 不変条件

- **コード挙動を変えない**: 変更は `.md` と TSDoc コメントのみ。`iconBatch.ts` はコメント行以外に触れない
- **情報を消失させない**: バッチ形式の「status=0 のとき長さフィールドが無い」という条件は、正準（icon.rs rustdoc）に既に正確に存在することを確認済み（research.md）。写し側の削除で条件の記述が世界から消えることはない
- **safety-nets.md の frontmatter は編集後も有効な YAML**: `.md` は selectChecks 対象外で破損が沈黙するため、編集後に frontmatter の構文を目視確認する
- **`paths` glob は `**/` 明示の意味論に従う**: `.claude/rules/**` / `.claude/skills/**` は既存エントリ（`.claude/hooks/**` 等）と同形式であり、配下全体に届く

## テスト方針

- 自動テストの追加なし（検査対象となる機構が存在しない文書変更。governance:check は #587 のスコープ）
- 検証:
  - `ui/src/lib/iconBatch.ts` 編集後、PostToolUse hook の typecheck が沈黙すること（コメントのみの変更の確認）
  - `grep -rn "count:u32\|status:u8\|png_len" --include="*.md"` で、バイト形式が `.md` から消えていること（docs/superpowers/ の歴史資料は対象外）
  - `grep -rn "または通常マージ"` が 0 件になること
  - `grep -rn "SSOT は各サブディレクトリ"` 相当の古い指名が architecture.md / health-check SKILL.md から消えていること
  - `git diff` で `iconBatch.ts` の差分がコメント行のみであること
  - safety-nets.md の frontmatter を読み直し、YAML リストの構文が保たれていること（`.md` は検査割当なし＝破損が沈黙するため目視必須）

## SPEC.md 更新要否

更新する（本計画に含む）。ただし挙動変更ではなく記述の一本化——ユーザー観測可能な契約（アイコンがバイナリ IPC で届く事実）は不変で、内部バイトレイアウトの記述位置だけが正準へ移る。

## スコープ外

- governance:check による機械検査（#587）
- rules 全般のルーター化（#588）
- 他の文書ドリフトの捜索（本 issue は設計時に実地検証した 4 件 + plan-review が検出した間接参照 2 件に限る。網羅的検出は #587 の検査が担う）
- `docs/superpowers/` 配下の言及（日付付き設計履歴＝当時の記録。遡及編集しない。非規範化宣言は #589）

## セルフレビュー

### plan-review 結果（Step 5a）

- **要対処 1 件（反映済み）**: `.claude/skills/health-check/SKILL.md:42` の古い SSOT 指名。scout-docs と独立再導出が**独立に一致**して検出 → 変更ファイル一覧へ追加
- **軽微 1 件（反映済み）**: safety-nets.md の自己包含は前例なき新規パターン → 意図的判断として計画に明記
- **独立導出との差分**: 漏れ（導出 ∖ plan）= health-check SKILL.md:42・AGENTS.md:70 の 2 件（いずれも反映済み）。スコープ過剰（plan ∖ 導出）= なし。一致 = バイト形式の写し 4 箇所（全数 grep で両者一致）・CONTRIBUTING の写し 1 箇所のみ・safety-nets は「paths 拡張 + 本文精密化」の複合案で判断も一致。完全性の能動的証拠として記録

### 5b の 3 観点

1. **境界条件**: 文書変更のため実行時境界はない。「写しの数え上げ」の境界（docs/superpowers/ の歴史資料・テストによる再導出・同表層別概念 `launch_count: u32`）は独立再導出が分類済みで、事後検算 grep がテスト方針に含まれる
2. **シンプル化**: 新規の状態・機構の導入なし。全変更が「削除 + 正準参照への置換」の単一パターンで、これ以上単純化できない
3. **破壊不変条件 + 検知手段**: (a) safety-nets.md frontmatter の YAML 破損 → rules 配送が沈黙停止する。検知手段が存在しない（#587 前）ため編集後の目視確認をテスト方針に明記。(b) iconBatch.ts のコメント以外への誤編集 → typecheck hook + `git diff` 目視で検知。(c) 正準（icon.rs rustdoc）の誤削除 → 事後検算 grep で「正準が残っていること」も確認する

### 要合意事項（実装着手前）

`.claude/skills/health-check/SKILL.md:42` の修正はスキル編集＝エージェント設定変更（ルート CLAUDE.md 最重要ルール 3）。issue #586 本文に無い追加スコープのため、実装着手（/implement）の指示をもって合意とみなすか、明示的な承認を得る。
