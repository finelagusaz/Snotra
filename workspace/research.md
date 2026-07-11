# research — issue #509: Windows ランナーでも `npm test` を走らせる

## issue の要約

CI（`.github/workflows/ci.yml`）の `frontend-check` ジョブは **ubuntu-latest** で `npm test`（`vitest run`）を実行している。この `npm test` の対象（`vitest.config.ts` の `include`）は 3 群:

- `ui/src/**/*.test.{ts,tsx}` — フロントエンドのユニットテスト（jsdom / OS 非依存性が高い）
- `.claude/hooks/**/*.test.mjs` — PreToolUse / PostToolUse フックの selftest
- `.githooks/**/*.test.mjs` — main 保護 Layer 1（git hook）を実 git で実測する回帰テスト

後ろ 2 群は **実運用が Windows 上でのみ起きる安全網**（開発機は Windows + PowerShell）。しかし CI がこれらを走らせる OS は Linux だけで、Windows 固有の故障モードは現行 CI を緑のまま通過しうる。

**やること**: 既に windows-latest で走る `rust-check` ジョブに `npm test` を追加し、`docs/build-commands.md` の対応表を更新し、故障注入で一度実測する。

## 関連コード

| ファイル | 現状 | 変更 |
|---|---|---|
| `.github/workflows/ci.yml` | `frontend-check`（ubuntu）が `npm test`、`rust-check`（windows）は cargo のみ | `rust-check` に Node setup + `npm ci` + `npm test` を追加 |
| `docs/build-commands.md` L112 | 対応表の `npm test` 行が `frontend-check`（ubuntu）のみ | windows（rust-check）実行を追記 |
| `vitest.config.ts` | `include` は上記 3 群（変更しない・確認のみ） | 変更なし |
| `.githooks/githooks.test.mjs` | 使い捨て repo で実 git を撃つ（変更しない・確認のみ） | 変更なし |
| `package.json` | `test` = `vitest run`、`prepare` = `git config core.hooksPath .githooks` | 変更なし |

## 既存パターン

- `frontend-check`（ubuntu）が既に `actions/setup-node@v6`（node 22, `cache: npm`）→ `npm ci` → `npm test` を持つ。**issue の提案コードはこの既存ステップの写し**であり、新規パターンの導入はない。
- `rust-check` は既に windows-latest。Node ステップを足すだけでセットアップコスト最小（issue の推奨。独立ジョブは checkout/セットアップの重複コストが増える）。

## 技術的制約・確認事項（issue の「注意点」を実コードで裏取り）

1. **CI ランナーの git 設定に依存しないか** → **依存しない**。`.githooks/githooks.test.mjs` の `initRepo`（L48-58）は使い捨て repo ごとに `user.email` / `user.name` / `commit.gpgsign=false` を自前設定する。push テストは `initBare`（L60-64）でローカル bare repo を作り、remote 操作もランナーのグローバル設定に依存しない。**ローカル Windows で日常的に green（PostToolUse の githooks-selftest）= 使い捨て repo が自足している証拠**。
2. **`npm ci` の `prepare` 副作用** → CI checkout に `git config core.hooksPath .githooks` を設定するが**無害**。vitest の githooks テストは使い捨て repo の `core.hooksPath` を自前で向ける（`enableHooks`, L67-69）ため CI checkout の設定を参照しない。rust-check は checkout に対して commit/merge/push しないので、設定された hook が発火する経路もない。
3. **Windows と Linux は相補的**（設計文書 `docs/superpowers/specs/2026-07-09-...-design.md:307` + `.gitattributes` が明示）:
   - **ubuntu 側の検知能力**: 実行ビット（executable bit）・dash（POSIX sh）厳密性。
   - **windows 側の検知能力**: Git for Windows の shebang 経由 sh 起動という実行機構（`githooks.test.mjs:218` の `makeExecutable` が `win32` で chmod をスキップする通り、実行機構が OS で異なる）／パス区切り・`git config` 値のクォート（`githooks.test.mjs:16-17` の「PowerShell/msys の境界で \\ が壊れる」注記）。
   - **【当初の誤りを plan-review で訂正】CRLF は windows の検知領域ではない**。`.gitattributes:1-7`（実測）— CRLF で checkout されると **Linux の dash** が `PROTECTED_BRANCH=main` を `main\r` と読み、比較が永久に偽になり**静かに fail-open** する。**git-for-windows の sh は CR を落とすため Windows では再現しない**（＝CRLF 退行を捕らえるのは ubuntu 側）。かつ `.githooks/** text eol=lf`（`:7`）が両 OS の checkout で LF を強制し、commit 時にも CRLF→LF 正規化するため、CRLF を混入しても blob は LF で格納され CI に届かない。**CRLF を「windows 固有」と書いてはならない**。
   - `pre-bash.mjs` の `tool_name: "PowerShell"` 判定（#482）は `.claude/hooks` 側。ロジックテストは OS 非依存だが、実運用が Windows である以上 Windows でも回す妥当性はある。
   - ゆえに両 OS で走らせる価値がある（片方だけでは片側の故障モードを見逃す）。
4. **ジョブ名 `rust-check` は改名しない**。GitHub の required status check 名の可能性があり、改名は保護設定を静かに壊しうる。npm test 追加は**ステップ追加のみ**で吸収し、ステップ名/コメントで意図を明示する。
5. **`/health-check` Check 10** が対応表 ↔ workflow のドリフトを検知する（`.claude/skills/health-check/SKILL.md:116-129`）。「workflow で実行されているが表に無い / 表の workflow 名がずれている」を Warning にするため、npm test が rust-check でも走ることを表へ反映する必要がある。

## 故障注入の設計上の論点（受け入れ条件 3 個目）

受け入れ条件: 「`.githooks/pre-commit` を意図的に壊す（または win 固有分岐を壊す）ドラフト PR を作り、windows 側の `npm test` が red になることを確認してから revert」。

論点 A — **ライブのガードを弱めてはならない**（#504: 条件節を免除と読み、ライブ `pre-commit` を無害化して main 保護が数十秒消えた）。`AGENTS.md` Step 3「故障注入では、稼働中のガードを弱めない——複製に変異を当てる」。
- 破壊コミットを作る間、ローカル作業ツリーの `.githooks/pre-commit`（＝ライブのガード実体）を壊さない工夫が要る。`core.hooksPath` は `.git/config` 共有 & 相対（`.githooks`）で**操作対象ツリーのトップ基準**で解決されるため、**別 worktree（throwaway ブランチ）で破壊すれば main worktree のガードは無傷**。CI（使い捨て checkout）が「複製」に当たるので、複製に変異を当てる原則を満たす。

論点 B — **ubuntu が先に赤くなる**。`frontend-check`（ubuntu）は既に `npm test`（`.githooks` 含む）を走らせるため、**汎用的な破壊は ubuntu 側も赤にする**。しかし受け入れ条件 3 の字義は「windows 側 npm test が red」だけであり、**汎用破壊（例: `pre-commit` の main ブロックを無効化）で十分**満たせる。故障注入が検証するのは「新設した windows ステップが hook テストを実際に走らせ、失敗時に赤で報告する（＝no-op でない・作業ディレクトリや npm 導入を誤配線していない）」ことであり、これは汎用破壊で確認できる。**「windows 固有の破壊」で相補性を実証する案は取り下げる**: CRLF は論点 3 のとおり windows 固有ではなく（むしろ ubuntu の領域）、`.gitattributes` の LF 固定で commit に消える。クリーンな windows 一意破壊は構成しにくく、criterion にも不要。相補性は文書（対応表の注記）で担保する。

論点 C — **`--no-verify` は人間専用**。破壊コミット作成で hook が邪魔になっても Claude は `--no-verify` を使わない。破壊は feature ブランチ上で行い、feature ブランチの commit は pre-commit を通る（誤爆しない設計）ため、通常は迂回不要。

## 未解決の疑問

- なし（要求は一意。実装判断のみ）。故障注入の具体手順は plan で確定する。
