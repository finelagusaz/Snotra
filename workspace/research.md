# 調査: #1334 docs/hooks.md を、テストとコードのコメントに載らない契約だけに絞る

## issue の要約

docs/hooks.md（17,139 字・太字 66・#1333 後）の条項を、置き場所の順（1 テスト・型 → 2 安いテスト → 3 該当箇所のコメント → 4 文書に残す → 5 消す）で振り分け、文書には 4 だけを残す。段 1 では振り分け表を出してユーザーの裁定を待つ。1 と判断したものは、複製への変異注入で機構が落ちることを確かめる。

## 関連ファイル・シンボル（実在確認済み）

- `docs/hooks.md` — 対象。見出しは 3 つ：「PreToolUse（pre-bash.mjs）の実装契約」「コマンドの形で判定する規範 5 件（#768）」「Claude Code の RA インスタンスと hook の分担」
- `.claude/hooks/pre-bash.mjs`（576 行）— 先頭コメント（1〜30 行目付近）、`segmentEnd`（91）、規範 5 件の節コメント（130〜135）、`usesHeredoc` の doc（150〜160）、`BACKSLASH_PATH` のコメント（176〜185）、`needsPyEncoding` の doc（197〜202）、`gitSegments` の doc（211〜218）、`pullWithoutFfOnly` の doc（245）、`SHAPE_REMEDY` の doc（250〜255）、`judgeCommandShape` の doc（279〜291）、`countUnchecked` 付近（386）、`readPlanState` の doc（405）、git 上限の doc（421〜422）、`judgeSummary` の doc（429〜431）、`verifiedSummaryState`（479）、`main()` の動的 import（533〜545）、`invokedDirectly` ブロック（563〜576）
- `.claude/hooks/pre-bash.test.mjs`（1157 行・221 テスト）
- `.claude/hooks/lsp-config.mjs`（262 行）— 先頭 doc（1〜16）、`MARKETPLACE` ほかの定数 doc、`RATOML_FORBIDDEN`（37〜40）、`WALK_EXCLUDE`（42〜43）、toolchain の `components` 判定（117〜145）
- `.claude/hooks/lsp-config.test.mjs`（31 テスト）
- `scripts/governance/checks/G-module-linkage.mjs`
- 外から hooks.md の見出しを正準形で指す生きた参照: ルート `CLAUDE.md`:19（「PreToolUse（pre-bash.mjs）の実装契約」）、`lsp-config.mjs`:8・121（「Claude Code の RA インスタンスと hook の分担」）。ほかは ADR 群と `docs/superpowers/specs/`（凍結された歴史）

## 振り分け表（段 1 の成果物）

凡例: 1 = テスト・型が守る／3 = 該当箇所のコメントが持つ（既にある or 移す）／4 = 文書に残す／5 = 消す。「既」は、コードのコメントに同じ内容がすでにあることを示す。

### PreToolUse の実装契約

| # | 条項 | 分類 | 根拠 |
|---|---|---|---|
| A1 | fail-closed 骨格（exit 2 だけが block・既定値 2・判定不能は block・async と `.catch`） | 1 + 3既 | 変異 M1（既定値 2→0）・M3（`.catch` を外す）でカナリアが赤。コメントは先頭と `invokedDirectly` ブロックに既にある |
| A2 | 読むのは `tool_input.command` だけ | 1 + 3既 | テスト「description に該当語があっても command が無害なら allow」「インライン grep が残っていない」。先頭コメント「設計」 |
| A3 | 判定単位 2 種と、その過剰検出の受容 | 1 + 3既 | テスト「引用内の `&&` の後ろは検出する（過剰検出を意図として固定）」「散文中の `C:\path` も検出する」「引数文字列の中の git は拾わない」。`gitSegments` の doc |
| A4 | 見ないコマンド形（`sh -c`・ラッパ・`gh.exe`・`gh api`） | 3既 | 先頭コメントにほぼ逐語で同じ列挙がある |
| A5 | plan.md ゲート（ルート導出・倒し方・注入・コードブロックの過剰検出） | 1 + 3既 | 「decide — plan.md ゲート」6 本・「readPlanState」4 本（最近接 `.git` を含む）。`countUnchecked`・`readPlanState` の doc |
| A6 | 要約ゲート（順序・骨子・マージ時の再照合・例外なし・動的 import・2 本 × 4 秒） | 1 + 3既 | 変異 M4 で 5 本赤。順序のテスト（「plan.md に未チェックが残るなら、そちらの理由で止める」）、`judgeSummary`・`readVerifyState`・`verifiedSummaryState` のテスト群。ADR への指し先は `judgeSummary` の doc、上限は 421 行目、動的 import は `main()` のコメントに既にある |

### コマンドの形で判定する規範 5 件

| # | 条項 | 分類 | 根拠 |
|---|---|---|---|
| B0 | 5 判定の表 | 5 | 本文自身が「判定の正本は `pre-bash.mjs`、下は読むための索引」と言っている。各述語の doc とテストが正本 |
| B1 | 拒否文言が規範の受け皿 | 1 + 3既 | `SHAPE_REMEDY` の doc がほぼ逐語。テスト「拒否文言は代わりの手段を含む」 |
| B2 | platform の注入（渡されないときは発火させない） | 1 + 3既 | 変異 M2 で 2 本赤（ソースカナリアとライブ e2e）。`judgeCommandShape` の doc。**ただし doc の「第 4 引数を渡さない」は #1299 以降偽**（今は options オブジェクト）→ 直す |
| B3 | 爆発半径（全域関数・線形・1812ms） | 1 + 3既 | 「5 述語は全域関数である」4 本・「候補が多数同居しても線形で終わる」。節コメント 130〜135 と `usesHeredoc` の doc |
| B4a | 引用内の区切りでセグメントが切れる | 1 + 3既 | テスト「引用内の `;` でセグメントが切れる形は検出しない」。`gitSegments` の doc |
| B4b | 行継続をまたぐ形 | 3（移す） | コードに記述が無い。`gitSegments` の doc へ 1 行足す |
| B4c | `segmentEnd` は `hasSafeChain` と共有（継続の扱いを変えるとゲートも動く） | 3（移す） | 84 行目は共有の事実だけ。帰結を 1 句足す |
| B4d | コマンドに現れない非 ASCII は見えない | 3既 | `needsPyEncoding` の doc |
| B4e | `\` は 3 形しか見ない（`.\` の相対形・接頭辞なし） | 3（移す） | `BACKSLASH_PATH` のコメントは `cd C:\` と `C:\a` だけ。相対形の受容を足す |
| B4f | pull はブランチを見ない | 3既 | `pullWithoutFfOnly` の doc と拒否文言 |
| B4g | `.githooks/_lib.sh` の迂回案内は矛盾しない | 5 | 先回りの否定。拒否文言 `noVerify` が「人間専用」を言っている |

### RA インスタンスと hook の分担

| # | 条項 | 分類 | 根拠 |
|---|---|---|---|
| C1 | 正本の宣言・RA は navigation の道具で検証の権威ではない | 4 | 判断であってコードに紐づかない。誤ると LSP の状態を検証の根拠にする（気づきにくい） |
| C2 | 層の表（RA／手動の検証／CI） | 5 | 検証の正本は `docs/build-commands.md`。C1 の 1 文で足りる |
| C3 | 設定の配送・VS Code を巻き込まない・`rust-analyzer.toml` に書かない | 1 + 3既 | `RATOML_FORBIDDEN` と `checkLspConfig` の判定・テスト、定数の doc |
| C4 | 診断は抑制しない | 5 | 正本は `ADR-ra-diagnostics-suppression.md`（本文自身がそう言っている） |
| C5 | 未リンクの `.rs` は LSP が構文エラーを届け、`mod` 忘れは `G-module-linkage` が見る | 1 | `G-module-linkage` とそのテスト |
| C6 | 壊れ方 3 分類の表と、バイナリ欠落の実例・守り手 | 3（移す） | `lsp-config.mjs` の先頭 doc が沈黙の説明を持ち、分類はここを指している。表を先頭 doc へ移し、指し先を外す |
| C7 | `validate --strict` は `.lsp.json` を見ない・カナリアはツリー全体を読む | 3既 | 先頭 doc 4〜5 行目、`WALK_EXCLUDE` |
| C8 | 残余: worktree は最初に登録したツリーの設定で動く | 4 | リポジトリの外が原因で、テストにできず、カナリアは緑のまま |
| C9 | 残余: `settings.local.json` の `enabledPlugins` で無効化できる | 4 | 同上 |

### 導入（hooks.md:1〜7）

| # | 条項 | 分類 | 根拠 |
|---|---|---|---|
| I0 | 導入文（対象は pre-bash.mjs 1 本・PostToolUse 撤去の注記・CLAUDE.md／development-principles／safety-nets への指し先） | 5 | 撤去でこの文書の案内そのものが要らなくなる。PostToolUse 撤去の経緯は `ADR-meta-harness-thinning` が持つ |

件数は数えず、削除台帳は行 ID（I0・A1〜A6・B0〜B4g・C1〜C9）ごとに 1 行ずつ立てる（plan-review の所見: 以前の件数行は表と合っていなかった）。

## 変異注入の実測（2026-11-01）

複製（scratchpad の `mut/`。`.claude/hooks`・`scripts`・設定を写し、`node_modules` は junction）の `pre-bash.mjs` へ 1 つずつ当て、`pre-bash.test.mjs` を回した。基準は 221 本すべて緑。

| 変異 | 結果 |
|---|---|
| M1 `invokedDirectly` の既定 exitCode 2 → 0 | 1 本赤（骨格カナリア） |
| M2 `main()` から `platform: process.platform` を外す | 2 本赤（ソースカナリア・ライブ e2e） |
| M3 `main().catch(...)` → `main()` | 1 本赤（骨格カナリア） |
| M4 `main()` から `readVerifyState` の配線を外す | 5 本赤（process 級 e2e） |

最初の試行は PowerShell の `Set-Content -NoNewline` が失敗して変異が当たらず、4 件とも緑と出た（計器の故障）。node で書き込み、書き込み後に原本と違うことを確かめる形にして測り直した。

## 再利用できる既存パターン

- #1329 の順序（正本へ移してから消す）と PR 本文の台帳
- ADR は凍結された歴史で、消えた見出しを指したままでよい（「PostToolUse（post-edit.mjs）の発火一覧」などは見出しが消えても `governance:check` が通っている）

## 技術的制約

- 生きた参照 2 件の見出し（「PreToolUse（pre-bash.mjs）の実装契約」「Claude Code の RA インスタンスと hook の分担」）は字面を残すか、参照側を同じ変更で直す
- `.mjs` のコメントを触るので、検証はカテゴリ E と F

## 敵対的調査の所見と採否（`workspace/adversarial-1334.txt`）

| 所見 | 採否 | 反映 |
|---|---|---|
| A3 の「heredoc・`\`・非 ASCII はコマンド全体を見る＝§6 からの意図的な逸脱」は `pre-bash.mjs` に無い（ADR-command-shape-norms-in-hook も「hooks.md に置いた」と言う） | 採る | A3 を「1 + 3既」→「3（移す）」へ。移す先は先頭コメント。あわせて「非 ASCII はコマンド全体」は不正確（`needsPyEncoding` はコマンド位置の python を要する）なので、移すときに直す |
| A3: git 判定の「コマンド位置」要件は、アンカーを緩めても 221 本緑 | 採る | 2（安いテスト）: `usesNoVerify("echo git commit --no-verify") === false` を足す |
| A6: diff の 3 点形・`workspace` の接頭辞判定の緩め・git 上限 4→40 秒が全緑 | 採る | 2: 接頭辞（`workspaceX/` は外）と上限の予算（`VERIFY_GIT_TIMEOUT_MS` × 2 + readGitState の 2 × 10 秒 < 30 秒）のテストを足す。3 点形はコメント（3既）に留める |
| B1: 拒否文言の「代わりの手段」は 5 件中 3 件しか守られていない | 採る | 2: 5 件それぞれの代替手段の語を assert するテストを足す |
| C3: `WALK_EXCLUDE` の要素を外しても全緑 | 採る | C3 を「3既」へ落とす（コメントは定数の doc にある）。テストは足さない（生成物の除外は性能の話で、誤ると誤爆ではなく遅くなる） |
| C4: `.lsp.json` に `diagnostics` を置かない、に検出器が無い | 採る | 正本は ADR-ra-diagnostics-suppression。`lsp-config.mjs` の先頭 doc に ADR への 1 行の指し先を足す（3） |
| C5 の前半（LSP が構文エラーを届ける）は ADR にしか無い | 採る | C1 の根拠として扱う。C5 は後半（`mod` 忘れ）だけが 1 |
| B3 の FLAG 退行の検知は assert でなくタイムアウト頼み | ⚠️ 採らない | 落ちることには変わりない。今回の範囲（文書を絞る）の外 |
| 生きた参照は 2 箇所ではない: `AGENTS.md`:23、`development-principles.md`:78・89・93、`G-stale-identifiers.mjs`:96 とその fixture | 採る | 触る範囲に加える。`AGENTS.md` は規範ルートなので、変更は裁定を経る |
| research の複製 `mut` は C3/C5 を測れない（lsp・crate が無く 39 本赤） | 採る | M1〜M4 は `pre-bash.test.mjs` だけなので有効。C 系は git archive した `mut2`（586 本緑）の測定を採る |
| ADR は消えた見出しを指しても通る | 壊せず | `headingRefDocs` が `docs/adr/` を走査元から外す（lib.mjs:586〜602） |
| 「第 4 引数を渡さない」は偽 | 壊せず | 加えて「経路は 1 本だけ」も不正確（options 無しの呼び出しと、platform を入れない呼び出しの 2 形） |

## 未解決の疑問

- 振り分けどおりにすると、hooks.md に残るのは C1・C8・C9 と導入文だけになる。**hooks.md を残すか、RA の 3 項目も `lsp-config.mjs` の先頭 doc へ移して hooks.md を撤去するか**（ユーザー裁定）
- ルート `CLAUDE.md`:19 の指し先: 見出しを 1 行の案内（「正本は `pre-bash.mjs` の先頭コメントとテスト」）として残すか、`CLAUDE.md` の指し先を `pre-bash.mjs` へ直すか
