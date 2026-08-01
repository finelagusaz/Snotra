# フックの実装契約と保守

このリポジトリの Claude Code フック（PreToolUse = `.claude/hooks/pre-bash.mjs`、PostToolUse = `.claude/hooks/post-edit.mjs`）を**改修**するときの実装契約・機構・保守規律。

- エージェントが日常操作でフックにどう**応答するか**・沈黙をどう**読むか**は、常時ロードの `CLAUDE.md`「フック」節が SSOT。本ファイルはそこから退去させた**一覧と内訳**（どのファイルに何が発火するか・沈黙しうる経路は何か）も併せ持つ——常時ロードに写しを置くとコードとの二重管理になり、実際にドリフトした（#474〜#497）。
- 設計哲学（検出は構造化信号で行い、fail-closed を既定値に埋める）は `docs/development-principles.md` §「構造的設計原則と強制の階梯」の項目 6・7 が SSOT。本ファイルはそのフック具体化＝運用 specifics を持つ。
- セーフティネットが**効いているか**の検証手順（フォールトインジェクション等）は `.claude/rules/safety-nets.md`（フック改修時に自動配送される）。

## PreToolUse（pre-bash.mjs）の実装契約

- **fail-closed 骨格** — `exit 2` だけがツールをブロックする（#482 実測）。`exit 0` は許可、それ以外の非ゼロ（Node が未捕捉例外で返す **1** を含む）は「非ブロッキングエラー」でコマンドはそのまま実行される。ゆえに**既定の `process.exitCode` を 2 に置き、許可が確定した経路だけが 0 を書く**。原理は development-principles §7。**判定不能はすべて block へ倒す** — payload 破損・`command` が非文字列・git 状態が読めない・鎖の途中で `cd`。この fail-closed の骨格を壊してはならない。
- **読むのは `tool_input.command` だけである**（#482）。`description` や payload 全体を grep してはならない（「言及」と「実行」を区別しない検出器は誤爆する）。原理は development-principles §6。
- **判定の起点はコマンド位置である。ただし全判定がそこに閉じるわけではない**（#768 で緩めた・**全称表現を実装より強く書かないため明記する**）。`gh pr create` / `git` の各判定は「コマンド位置に現れる呼び出し」を起点とし、`git` 系はさらに**次の区切りまでのセグメント**に閉じるので `grep -n "--no-verify" CLAUDE.md` では発火しない。一方 **heredoc 演算子・`\` パス・非 ASCII の 3 判定はコマンド全体を見る** — この 3 つの失敗様態（シェルが `\` を食う・cp932 が非 ASCII で落ちる）には「言及と実行を分ける構文的位置」が存在しないためである。§6 の一般則（構文的位置で判定単位を定義する）は正しいままで、ここはその**意図的な逸脱**であり、代償として引用の内側の言及でも発火する。過剰検出（`echo "&& gh pr create"` / `git commit -m "fix: C:\path"`）は fail-closed 方向ゆえ許容し、テストで意図として固定する。
- **見ないコマンド形がある**（#482・受容する性質）。`sh -c '...'` / `eval` / バッククォート / ラッパ経由（`timeout 5 gh pr create` / `xargs`）は「gh がコマンド位置に現れない」ため検出しない。これは事故モードではなく意図的迂回であり、`--no-verify` と同格に**人間専用**として扱う。検出を shell パーサ相当まで広げると payload 全体 grep の誤爆を作り直すことになる。
- **plan.md ゲート** — `gh pr create` 検出時、リポジトリルート（cwd から最近接 `.git` へ遡って導出）の `workspace/plan.md` に未チェックの `- [ ]`（`* [ ]` も数える）が残っていれば block する（#749: 計画に書いた作業の実行漏れを PR 前に捕捉する）。判定点は push 検査と同じコマンド位置検出であり、新しい発火点を作らない。fail-closed の倒し方: 存在するのに読めない → block、存在しない → 管轄外（計画なしタスク・他リポジトリを塞がない）、`.git` が見つからない → cwd を root とみなす（従来挙動）。`decide(payload, readGitState, readPlanState)` の注入でファイルシステム無しにテストできる。plan.md のコードブロック内の `- [ ]` への過剰検出は受容する（fail-closed 方向）。

### コマンドの形で判定する規範 5 件（#768）

ルート `CLAUDE.md` の常時ロード面に置いていた 5 件を `judgeCommandShape` の判定へ吸収した（#593 の階梯「規範を機構へ吸収する」）。**判定は `pre-bash.mjs` が SSOT** であり、下は読むための索引である。

| 判定 | 発火する形 | platform |
|---|---|---|
| `usesHeredoc` | bash の heredoc 演算子（`<<EOF` / `<<-'EOF'`）。`<<<` とシフト演算子は除く | win32 のみ |
| `usesBackslashPath` | `C:\` / `$env:X\` / `%X%\` の 3 形。ドライブレターは語頭かつ `\` の後に 2 字以上を要求する（`rg "version:\s+"` を巻き込まないため。代償で `cd C:\` は見ない） | win32 のみ |
| `needsPyEncoding` | コマンド位置の `python` かつ非 ASCII を含み、`PYTHONIOENCODING=` / `PYTHONUTF8=` / `-X utf8` のいずれも無い | win32 のみ |
| `usesNoVerify` | `git` セグメントの `--no-verify`（commit セグメントの短縮 `-n` / `-nm` も同義。`git push -n` は `--dry-run` なので無傷） | 非依存 |
| `pullWithoutFfOnly` | `--ff-only` を持たない `git pull` | 非依存 |

- **拒否文言が規範の受け皿である。** 5 件それぞれが「何が起きるか」と「代わりに何をするか」を持つ（`SHAPE_REMEDY`）。常時ロードから降ろす設計はこの文言がその場で教えることに賭けているので、**ここが痩せると規範は機構へ移らずに消える**。
- **platform は第 4 位置引数で値として注入する。** `process.platform` は失敗しないので `readGitState` のような `{ ok: false }` を持つ reader 形にはしない。**渡されないときは Windows 専用判定を発火させない** — これは「判定不能」ではなく「規範の射程外」であり、block へ倒すと非 Windows で false block になる。この状態への到達経路は「呼び出し側の渡し忘れ」1 本だけで、ソースカナリアと process 級 e2e（`npm test` は ubuntu と windows の双方で走る）が `main()` の配線を固定する。却下した代替（options オブジェクト化・`undefined` を Windows へ倒す案）は `docs/adr/ADR-command-shape-norms-in-hook.md`。
- **爆発半径が (1)(2) と違う。** この 5 判定は**全 Bash/PowerShell コマンド**で走る（`gh pr create` 系は検出後のみ）。ゆえに**全域関数でなければならない** — throw すれば `main()` の catch が exit 2 を書いてセッションの全コマンドが止まり、hang すれば hook の timeout まで全コマンドが待つ。`usesHeredoc` は全候補を走査するが、終端行の索引を 1 パスで作ることで線形に保つ（候補ごとに全文走査する素朴形は候補 2 万件で 1812ms・実測）。
- **フック間契約**: `post-edit.mjs` が会話へ出す再現コマンド（`repro`）は、`\` 判定が通す形でなければならない。Windows では `resolveBin` が `path.join` で `\` 区切りの絶対パスを作るため、`repro` だけを `/` に正規化している（実行に使う `cmd`/`args` は正規化しない）。**片方の hook が指示するコマンドをもう片方が拒む状態**は、規範を機構へ移した設計の信頼を直に壊す——`pre-bash.test.mjs` の相互契約カナリアが `decide` を直に呼んで固定する。
- **受容する未対応リスク**（いずれも fail-closed の設計方針の下で意図的に残す。**「検出されないなら使ってよい」ではない** — 検出されない形も規範に反するなら人間専用の意図的迂回であり、上の `sh -c` 項と同格に扱う）:
  - **区切りの走査はシェルの構文を理解しない**ので、区切り文字が構文の内側にあるとセグメントが早く切れて見落とす: 引用内（`git commit -m "a;b" --no-verify`）と**行継続をまたぐ形**（`git commit \` + 改行 + `--no-verify`。PowerShell のバッククォート継続も同型）。引用・継続を解釈する分割は shell パーサ相当になり、payload 全体 grep の誤爆を作り直す。`segmentEnd` は `hasSafeChain` と共有されているため、継続の扱いを変えると `gh pr create` ゲートの意味も動く。
  - **コマンド文字列に現れない非 ASCII は見えない**（`python foo.py` でスクリプト側が出す形）。コマンドの形からは判定材料が無い。
  - **`\` 判定は上表の 3 形しか見ない**ので、`.\scripts\x.ps1` のような相対形も接頭辞を持たない `docs\hooks.md` も検出されない（`.\` は PowerShell では動くため誤爆の代償が大きく、形を絞ったことの代償でもある）——**検出しないだけで、規範としてはコマンドに書くパスの区切りを `/` にする**。
  - **`pull` はブランチを見ない**ので `git pull --rebase` も feature ブランチでの pull も止まる（過剰検出）。ブランチ判定は `readGitState` の責務を広げるため採らない。
  - **`.githooks/_lib.sh` は拒否メッセージで `--no-verify` による迂回を案内する。** 明示的に「人間専用。エージェントは使用禁止」と書いてあるため矛盾ではない（この hook が拒むのはエージェントの実行であり、人間の判断を妨げない）。

## PostToolUse（post-edit.mjs）の発火一覧

**正本は `selectChecks` である。** 下は現在の割り当てを読むための索引であり、判断はコードに問う。

| 編集したファイル（ツリー相対） | 走る検査 |
|---|---|
| `*.rs` | fmt → clippy（各 Rust crate 配下ではその crate のテストも）。fmt が先なのは 0.7s でビルドを要さないため（#858） |
| `tauri.conf.json` / `config.toml` | WARN（人間向け・Windows 互換の注意喚起） |
| `Cargo.toml` | cargo check |
| `.claude/settings.json` / `.claude/hooks/**` / `package.json` / `vitest.config.ts` / ルートの `Cargo.toml` | hook-selftest |
| `.githooks/**` | githooks-selftest |
| 上記以外（`*.md`・`.claude/rules/**`・`.claude/skills/**`・`scripts/**` 等） | **何も走らない**——沈黙は「合格」ではない |

TS 型検査は #532 SU7 のフロント撤去で消滅した（`.ts` 編集は「検査はありません」の情報行のみ）。

**沈黙しうる経路は塞いである**（#471）。タイムアウト（検査ごと 300s）・出力溢れ・起動失敗・スクリプト内部エラーはいずれも必ず報告される（診断が予算を超えても、再現コマンドで全件を見られる）。**この閉塞を壊す変更を入れてはならない**——「沈黙 = 合格」という意味づけが成り立たなくなる。

## PostToolUse（post-edit.mjs）の機構と保守

- **worktree でも「そのファイルが属するツリー」を検査する**。root は `file_path`（絶対パス）から最近接の `.git` を遡って導出するため、`CLAUDE_PROJECT_DIR` の意味論に依存しない。ただしスクリプト自身の所在は `settings.json` の `${CLAUDE_PROJECT_DIR:-.}` で解決し、相対 `file_path` を受け取った場合は cwd 基準で `resolve` する。
- **自己防護** — `.claude/settings.json` の編集は file watcher が即座に拾う（セッション再起動は不要・実測）。壊れたスクリプトを配線するとその瞬間から全検査が沈黙する。そのため `.claude/settings.json` と `.claude/hooks/**`、および検査の定義を変えるファイル（`package.json` / `vitest.config.ts` / ルートの `Cargo.toml`）の編集は `hook-selftest`（settings.json の JSON 検証 + `vitest run .claude/hooks`）を自動発火する。`.githooks/**`（main 保護の Layer 1）は同じ理由で `githooks-selftest` を発火する。
- **`selectChecks` に発火を足すときは、カナリアも対で足す**（#497）。カナリアの無いファイルに検査を実行しても vitest の起動しか証明しない（何も検証しない緑）。**守るのは沈黙する経路だけでよい** — 放っておいても実行時に明示的に失敗するものに見張りは不要（#500）。
- **`config.toml` の WARN 真陽性は事実上 `tauri.conf.json` のみ**（`config.toml` はランタイムのユーザー領域ファイルでリポジトリに実在しない）。その `src-tauri/tauri.conf.json` では WARN（人間向け・Windows 互換の注意喚起）のみが出る（旧 `csp-test` は #532 SU7 のフロント撤去で CSP ごと消滅）。
