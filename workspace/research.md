# research: issue #587 governance:check の導入（CI 独立 job + npm script）

## issue の要約

Markdown・rules・skills は編集時検査の対象外（#497 受容の残余）で、PR CI 緑でもガバナンス文書は未検査。決定的に照合できる 6 項目を `scripts/` の機械検査へ移し、CI 独立 job + `npm run governance:check` で強制する。hook は触らない（設計書 §2・合意済み）。

## 関連コード・文書（実在確認済み）

- **`scripts/`**: 既存の Node スクリプトは `clean-worktrees.mjs` + `clean-worktrees.test.mjs`（vitest）。`vitest.config.ts` の include に `scripts/**/*.test.mjs` が既に入っており、テストを置けば `npm test`（CI の frontend-check=ubuntu と rust-check=windows の両方）で自動実行される
- **`package.json` scripts**: `clean:worktrees` が `node scripts/clean-worktrees.mjs` の前例。`governance:check` → `node scripts/governance-check.mjs` が同形
- **`.github/workflows/ci.yml`**: `frontend-check`（ubuntu）/ `rust-check`（windows）の 2 job。`skip-ci` ラベルの if ガードと `concurrency` が共通様式。governance job は checkout + setup-node のみで走れる（スクリプトは依存ゼロで書く＝`npm ci` 不要）
- **`/health-check` SKILL.md の Check 1〜10**: 機械化対象の対応は次のとおり
  - Check 1（CLAUDE.md モジュール索引 ↔ 実ファイル）→ issue 検査 (d)
  - Check 3（AGENTS.md 参照実在）・Check 6（development-principles.md 参照実在）→ (a) の部分集合
  - Check 4（SPEC 番号連続性）→ (f)
  - Check 5（build-commands ↔ package.json/Cargo members）・Check 10（build-commands ↔ workflows 対応表）→ (e)。ただし Check 5 の「hook の buildCommand との内容照合」（フラグの意味論判定）と「コマンド直書き grep」は意味判断を含む
  - Check 8（rules paths glob の有効性）→ (b)
  - Check 9（スキル表 ↔ ディレクトリ）→ (c)
  - Check 2（architecture.md へのモジュール表再導入検知）は行頭パターン判定で機械化可能だが issue の 6 項目に含まれない
  - Check 7（メモリ整合）はリポジトリ外・harness 依存で機械化不能（スキルに残す）
- **`.claude/rules/*.md` の paths**: 8 ファイル。glob 形式は `"dir/**"` / `"dir/**/*.ext"` / bare 名（`AGENTS.md` / `CLAUDE.md` / `SPEC.md`）/ 単一パス / `{ts,tsx}` ブレース。意味論はメモリ実測（bare 名はルート直下のみ・階層横断は `**/` 明示・#520 系）
- **ルート `CLAUDE.md` フック節**: 「#497 の残余（.md の沈黙は未検査）」の記述があり、CI が過半を引き取った後は弱める（設計書 §2 で合意済み）
- **`docs/build-commands.md`**: コマンド SSOT。カテゴリ表 + 「CI/CD メモ」対応表。governance:check 自身の行を両方へ追加する（自己参照になるが、検査対象は「表のコマンドが package.json / workflow に実在するか」なので自己整合的に検査される）

## 既存パターン

- **fail-closed / loud の規律**（safety-nets.md・自動配送で受領済み）: 「hook が自動実行する」「CI が担保する」は書かれた時点の期待——**故意に壊して検知されることを 1 度実測する**。稼働中のガードは弱めず、複製に変異を当てる（スクリプトはフィクスチャで故障注入テスト、CI 配線は PR ブランチ上で一時的に赤を実測してから直す）
- **検査の入力集合を具体対象で検算する**: 守りたい対象 1 件が実際に入力へ現れること、判定対象外が混じらないことを両方向で示す（テストに含める）
- `.claude/hooks/post-edit.mjs` + テストの構造（純関数 `selectChecks` を切り出しテスト可能にする）が、スクリプト設計の手本
- Check 1 の実装注意（SKILL.md 34 行目）: 列挙は Glob 系で行い `git ls-files` の pathspec `**` の取りこぼしに注意。ui はテストファイル（`*.test.{ts,tsx}`、SSOT は vitest.config.ts）を母集団から除外

## 技術的制約

- **依存ゼロで書く**: `scripts/governance-check.mjs` は Node 組込み（fs/path）のみ。glob は自前の変換（`**`→`.*` 等）で足りる——検査の主張は「パターンが実在ファイルに 1 件以上マッチ」であり、harness の配送意味論の再現ではない（過剰一致は fail-closed 方向でない点に注意: マッチ 0 件の検知が目的なので、緩い変換は偽陰性を作る。ドキュメント化された意味論に忠実に変換し、テストで bare 名/`**`/ブレースの代表例を固定する）
- **バッククォート参照の抽出は述語を絞る**: URL・glob（`*` `{` を含む）・プレースホルダ（`<>`）・存在検査に不適な形（`~/` 等リポジトリ外）を除外し、パス様（`/` を含む or 既知のルート直下ファイル名）だけを実在検査する。偽陽性が出た場合に備え、行内 `governance:ignore` 等の免除注記は**設けない**（免除機構は沈黙経路になる。誤検出はパターン修正で対処）
- **Windows パス**: スクリプト内は `/` 区切りで統一（CLAUDE.md シェル環境規則と同方向）
- **CI job 追加は safety-nets.md の合意手順に従う**（エージェント設定変更。#587 の器は設計書で合意済み、workflow の具体配線はこの PR のレビューが合意点）
- **`.claude/skills/health-check/SKILL.md` の変更**もスキル編集＝要合意（issue 本文に「痩せさせる」と明記済み・起票時に合意済みだが、最終差分はユーザーレビュー対象）

## 未解決の疑問

- Check 5 の「hook buildCommand との内容照合」「コマンド直書き grep」をスクリプトに含めるか → **含めない**（フラグの意味論・「参照リンクは許容」の判定は意味判断。スキル側に残す）。plan で明記
- Check 2（architecture.md モジュール表再導入）を含めるか → **含める**（行頭 `| \`xxx.rs\` |` の決定的パターンで判定可能・実装コスト極小・スキルから 1 Check 減らせる）。issue の 6 項目に加える追加スコープとして plan に明記
