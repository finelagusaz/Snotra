# フックの実装契約と保守

このリポジトリの Claude Code フック（PreToolUse = `.claude/hooks/pre-bash.mjs`、PostToolUse = `.claude/hooks/post-edit.mjs`）を**改修**するときの実装契約・機構・保守規律。

- エージェントが日常操作でフックにどう**応答するか**・沈黙をどう**読むか**は、常時ロードの `CLAUDE.md`「フック」節が SSOT（本ファイルは改修者向け）。
- 設計哲学（検出は構造化信号で行い、fail-closed を既定値に埋める）は `docs/development-principles.md` §「構造的設計原則と強制の階梯」の項目 6・7 が SSOT。本ファイルはそのフック具体化＝運用 specifics を持つ。
- セーフティネットが**効いているか**の検証手順（フォールトインジェクション等）は `.claude/rules/safety-nets.md`（フック改修時に自動配送される）。

## PreToolUse（pre-bash.mjs）の実装契約

- **fail-closed 骨格** — `exit 2` だけがツールをブロックする（#482 実測）。`exit 0` は許可、それ以外の非ゼロ（Node が未捕捉例外で返す **1** を含む）は「非ブロッキングエラー」でコマンドはそのまま実行される。ゆえに**既定の `process.exitCode` を 2 に置き、許可が確定した経路だけが 0 を書く**。原理は development-principles §7。**判定不能はすべて block へ倒す** — payload 破損・`command` が非文字列・git 状態が読めない・鎖の途中で `cd`。この fail-closed の骨格を壊してはならない。
- **判定は `tool_input.command` のコマンド位置だけを見る**（#482）。`description` や payload 全体を grep してはならない（「言及」と「実行」を区別しない検出器は誤爆する）。原理は development-principles §6。判定単位は「コマンド位置に現れる呼び出し」であり、`grep "gh pr create" f` のように引用の内側にあるだけでは発火しない。過剰検出（`echo "&& gh pr create"`）は fail-closed 方向ゆえ許容する。
- **見ないコマンド形がある**（#482・受容する性質）。`sh -c '...'` / `eval` / バッククォート / ラッパ経由（`timeout 5 gh pr create` / `xargs`）は「gh がコマンド位置に現れない」ため検出しない。これは事故モードではなく意図的迂回であり、`--no-verify` と同格に**人間専用**として扱う。検出を shell パーサ相当まで広げると payload 全体 grep の誤爆を作り直すことになる。
- **plan.md ゲート** — `gh pr create` 検出時、リポジトリルート（cwd から最近接 `.git` へ遡って導出）の `workspace/plan.md` に未チェックの `- [ ]`（`* [ ]` も数える）が残っていれば block する（#749: 計画に書いた作業の実行漏れを PR 前に捕捉する）。判定点は push 検査と同じコマンド位置検出であり、新しい発火点を作らない。fail-closed の倒し方: 存在するのに読めない → block、存在しない → 管轄外（計画なしタスク・他リポジトリを塞がない）、`.git` が見つからない → cwd を root とみなす（従来挙動）。`decide(payload, readGitState, readPlanState)` の注入でファイルシステム無しにテストできる。plan.md のコードブロック内の `- [ ]` への過剰検出は受容する（fail-closed 方向）。

## PostToolUse（post-edit.mjs）の機構と保守

- **worktree でも「そのファイルが属するツリー」を検査する**。root は `file_path`（絶対パス）から最近接の `.git` を遡って導出するため、`CLAUDE_PROJECT_DIR` の意味論に依存しない。ただしスクリプト自身の所在は `settings.json` の `${CLAUDE_PROJECT_DIR:-.}` で解決し、相対 `file_path` を受け取った場合は cwd 基準で `resolve` する。
- **自己防護** — `.claude/settings.json` の編集は file watcher が即座に拾う（セッション再起動は不要・実測）。壊れたスクリプトを配線するとその瞬間から全検査が沈黙する。そのため `.claude/settings.json` と `.claude/hooks/**`、および検査の定義を変えるファイル（`package.json` / `vitest.config.ts` / ルートの `Cargo.toml`）の編集は `hook-selftest`（settings.json の JSON 検証 + `vitest run .claude/hooks`）を自動発火する。`.githooks/**`（main 保護の Layer 1）は同じ理由で `githooks-selftest` を発火する。
- **`selectChecks` に発火を足すときは、カナリアも対で足す**（#497）。カナリアの無いファイルに検査を実行しても vitest の起動しか証明しない（何も検証しない緑）。**守るのは沈黙する経路だけでよい** — 放っておいても実行時に明示的に失敗するものに見張りは不要（#500）。
- **`config.toml` の WARN 真陽性は事実上 `tauri.conf.json` のみ**（`config.toml` はランタイムのユーザー領域ファイルでリポジトリに実在しない）。その `src-tauri/tauri.conf.json` では WARN（人間向け・Windows 互換の注意喚起）のみが出る（旧 `csp-test` は #532 SU7 のフロント撤去で CSP ごと消滅）。
