# plan: issue #517 — worktree cleanup のスクリプト化

## 変更ファイル一覧

| ファイル | 変更内容 |
|---|---|
| `scripts/clean-worktrees.mjs` | 新規。`.claude/worktrees/agent-*` の worktree と対応する `worktree-agent-*` ブランチを掃除する（Node、依存なし） |
| `scripts/clean-worktrees.test.mjs` | 新規。使い捨て repo で clean 削除 / dirty スキップ / 対象外不干渉を実測する回帰テスト |
| `vitest.config.ts` | `test.include` に `scripts/**/*.test.mjs` を追加（現状 `ui/src`・`.claude/hooks`・`.githooks` のみ） |
| `package.json` | `"clean:worktrees": "node scripts/clean-worktrees.mjs"` を追加 |
| `AGENTS.md` :87 | 環境制約節 1項目目の手動手順を「`npm run clean:worktrees` で掃除する（dirty はスキップされる）」1行に置き換え。節構造・序数は不変（#518 との衝突を避け最小差分） |
| `.gitignore` :21 | コメント「変更ありの worktree は手動 cleanup」を新コマンド参照へ更新（規範の唯一の間接的写し） |
| `docs/build-commands.md` | 「Windows/macOS/Linux で実行可能」節に `npm run clean:worktrees` を追記（コマンド一覧の SSOT） |

言語選定: 当初 PowerShell（scripts/ の既存慣習）だったが、独立再導出の提案を採り Node .mjs へ変更。理由: (1) 受け入れ条件「誤って消さないこと」を手動手順でなく vitest 回帰テストで固定できる（`.githooks/githooks.test.mjs` の使い捨て repo パターンを再利用）(2) クロスプラットフォームで動く。

## スクリプト仕様

1. `git worktree list --porcelain` から `.claude/worktrees/agent-*` 配下の worktree を列挙（ファイルシステム glob ではなく git 自身に問う）
2. 各 worktree について:
   - `git -C <wt> status --porcelain` が非空（untracked 含む）または status 取得失敗 → **スキップして警告表示**。`--force` 指定時のみ `git worktree remove --force`
   - クリーン → `git worktree remove <path>`（clean なら `--force` 不要）
3. worktree 削除に成功したものだけ、対応ブランチ `worktree-agent-<id>` を `git branch -d` で削除（worktree が生きている間は checkout 中で消せないため順序固定）。未マージなら git が拒否 → 警告して残す。`--force` 時は `-D`
4. 最後に `git worktree prune`（ディレクトリだけ消えた迷子登録の回収）
5. 対象ゼロなら「対象なし」表示で exit 0。スキップ・失敗があれば一覧表示し非ゼロ exit

## 実装順序

1. テスト作成（Red）→ `scripts/clean-worktrees.mjs` 実装（Green）+ `vitest.config.ts` include 追加
2. `package.json` 配線
3. AGENTS.md / .gitignore / docs/build-commands.md の文書更新
4. コミット・PR

## 不変条件

- **成果保全**: 未コミット変更（untracked 含む）のある worktree・未マージのブランチは `--force` なしでは絶対に消えない。最終防衛は git 自身の拒否（`worktree remove` / `branch -d`）に委ね、自作述語のバグで保全が破れる経路を持たない。判定不能（status 失敗）は保守側＝スキップに倒す
- **対象限定**: `.claude/worktrees/` 配下かつブランチ `worktree-agent-*` のみ。メインツリー・他 worktree に触れない
- 異常系: git コマンド失敗は該当 worktree を中断して次へ。最後に失敗件数を非ゼロ exit code で返す

## テスト方針

`scripts/clean-worktrees.test.mjs`（vitest・使い捨て repo で実測）:

1. clean worktree → 削除され、ブランチも消える
2. dirty worktree（未コミット変更 / untracked）→ スキップされ、worktree・ブランチとも残る
3. `.claude/worktrees/agent-*` 以外の worktree → 触られない
4. 対象ゼロ → exit 0

hook 発火の予期: `vitest.config.ts` / `package.json` 編集は hook-selftest を発火する（既存カナリアは `prepare`/`test`/`typecheck` キーのみ検証のため green で通る見込み）。`scripts/*.mjs` は検査割り当てなし＝沈黙は合格ではない → だからこそ vitest include への追加が必須（include 漏れは「テストがあるのに走らない」静かな失敗になる）。

## SPEC.md 更新要否

不要（SPEC.md に worktree 記述は grep 0件。アプリ挙動変更なし）。

## セルフレビュー + plan-review 結果

- Step 2（成果物監査）: 要対処なし。軽微4件 → `.gitignore:21` コメント陳腐化・`docs/build-commands.md` 追記推奨・hook-selftest 発火の認識、をすべて計画へ反映済み
- Step 2b（独立再導出）差分:
  - 漏れ候補 → 採用: `.gitignore:21`（両者一致）、build-commands.md 追記、.mjs + vitest テスト化、vitest include 追加、ブランチ削除の順序依存（worktree 削除成功後に限定）
  - スコープ過剰候補: なし
  - 一致（完全性の証拠）: 規範の実体は AGENTS.md:87 の1箇所のみ・SPEC.md 更新不要・porcelain を真実源にする列挙・dirty スキップの保全設計、で独立に再一致
- セルフレビュー 8観点: 対称ペア=破棄専用で生成側不変 / 影響範囲=grep 済（上記）/ 境界条件=ゼロ件・dirty・未マージ・迷子登録 / リソース管理=git ネイティブ拒否に委譲 / 既存パターン=使い捨て repo テストパターン再利用 / YAGNI=対話プロンプト無し（-NonInteractive でハングするため）--force オプトインのみ / シンプル化=自作 dirty 述語を最終防衛にしない / 破壊不変条件=「--force なしで保全対象が消えない」をテスト2で固定
