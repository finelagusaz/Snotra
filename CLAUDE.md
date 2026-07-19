# CLAUDE.md

このリポジトリで Claude Code が作業するときの運用ガイド。

- 共通開発プロセス（ワークフロー・事前チェック）は `AGENTS.md`（次行で自動読込）
- モジュール固有の不変条件は各サブディレクトリの `CLAUDE.md`（`snotra-core/` / `src-tauri/` / `ui/` / `snotra-settings/`）
- 本ファイルの各ルールは「**太字 = 守る指示**、後続 = 理由・過去の事故」の形式。迷ったら太字部分に従えば安全

@AGENTS.md

## 最重要ルール（常に適用）

作業種別を問わず適用される3つ。詳細は各セクションを参照。

1. **`main` へ直接コミット・プッシュしない** — 必ず feature ブランチ（`feat/<機能名>` / `fix/<バグ名>` / `chore/<作業名>`）を作成してからコミットする
2. **bash の HEREDOC（`<<EOF`）を使わない** — 複数行テキストは一時ファイルか PowerShell here-string（→「シェル環境」）
3. **エージェント設定（スキル・フック・rules）の変更は合意してから** — Claude が単独で判断しない（→「チーム憲章」）

## MCP ツール

- **Tauri v2 / SolidJS / Rust クレートの最新 API 調査には context7 MCP を使う**（設定済み）

## シェル環境（Windows / PowerShell）

このリポジトリは Windows + PowerShell 環境で運用されている。Bash 系の慣習をそのまま持ち込むと、過去のセッションで複数回踏んだ摩擦を再発させる。

| やらないこと | 代わりにやること | 理由（過去の事故） |
|---|---|---|
| bash の HEREDOC（`<<EOF` / `<<'EOF'`） | 一時ファイルに書き出して `git commit -F <tmpfile>`、または PowerShell here-string `@'...'@`（閉じ `'@` は必ず行頭） | here-string の引用境界が壊れ、終端マーカーがコミットメッセージ本文に漏れる事故が起きている |
| 文字列中のパスに `\` 区切り | `/` で統一する | PowerShell でも Git/Node/Cargo は `/` を受け付ける。`\` はエスケープが必要になり壊れやすい |
| `/tmp` への書き込み | `$env:TEMP` 配下に置くか Write ツールで作る | Windows の Bash ツールに `/tmp` は無く、`cat > /tmp/...` は `FileNotFoundError` で失敗する |
| Python で非 ASCII をそのまま標準出力 | `PYTHONIOENCODING=utf-8` を付ける | cp932 コンソールで `—`・日本語などを print すると `UnicodeEncodeError` で落ちる（JSON/ログ整形で多用） |

## Git/GitHub 運用

- **main 保護の実体は `.githooks/` と GitHub ruleset である** — `.githooks/{pre-commit,pre-merge-commit,pre-rebase,pre-push}` が `git commit` / 非 FF の `git merge` / `git rebase` / `git push` を拒否する（`.githooks/githooks.test.mjs` の回帰テストで実測。`git -C <別ツリー>`・linked worktree・`ui/` 等のサブディレクトリからの起動を含む）。GitHub ruleset `default` は main への直接 push を拒否する（実測）。force-push と削除は `non_fast_forward` / `deletion` 規則が `active`（設定の read-back のみ。実地の試行は未実施）。git は hook を「操作されるツリーのトップ」を cwd として呼び、相対 `core.hooksPath` もそこを基準に解決する（実測）。bootstrap は `npm install`（`prepare` が `core.hooksPath` を設定する）
- **`.githooks/` を含まないツリーでは Layer 1 は存在しない** — hook は追跡ファイルなので、`.githooks/` が無いコミットを checkout すると git は「hook 無し」として操作を通す（fail-open）。古いタグや導入前のコミットが該当する。**ローカルの取りこぼしは push の時点で GitHub ruleset が捕捉する**（直接 push の拒否は実測済み）。`.githooks/` は「手前で親切に止める」best-effort な層であり、その不在を検知する仕組みは意図的に置いていない
- **Layer 1 が見ていない操作がある** — git は `cherry-pick` / `revert` / `am` / `branch -f` / `update-ref` で `pre-commit` を呼ばない。main 上でこれらを実行すると **hook は何も出力せず main が進む**（実測）。`commit --amend` と `merge --squash` 後の `commit` は拒否される。取りこぼしは push の時点で GitHub ruleset が捕捉する
- **`--no-verify` は人間専用** — `.githooks/` を迂回する。Claude は使用してはならない。迂回しても main への直接 push は GitHub ruleset が拒む（実測）
- **`gh pr create` は `git push` と `&&` で繋いでよい** — PR 前 push チェック hook（`.claude/hooks/pre-bash.mjs`）は、鎖の中で `git push` が `&&` で先行していれば通す（`&&` が前段の成功を保証するため）。区切りが `;` / `||` / 改行の場合は push 失敗時に PR が作られうるので拒否する。`git -C <別ツリー> push` も安全な鎖とは見なさない（#482 で実測）
- **main の同期は `git pull --ff-only` を使う** — 非 FF の `git pull` は main にマージコミットを作るため `.githooks/pre-merge-commit` が拒否する。FF ならマージコミットが生じず hook は呼ばれない
- **マージで閉じる issue を決めるのは PR 本文である。`gh pr merge --body-file` では止まらない**（#488 実測） — auto-close の経路は 2 本あり、制御点が違う。
  - **PR 本文の closing keyword** → GitHub が「リンク」として計算し、PR ページと `gh pr view <PR> --json closingIssuesReferences` に現れる。**マージした瞬間に閉じる。`gh pr merge` の `--subject` / `--body-file` はこの close を抑止できない**（足すことはできる。→ 手順 3）。実測: PR #491 を `--body-file` 付きでマージ → squash commit 本文に `Closes` は無いのに #489 が 1 秒後に close、close イベントの `commit_id` は `null`。PR テンプレート（`.github/pull_request_template.md`）に `Closes` 行があり作成時に番号を埋めるため、**いま自分が書いた覚えが無くても残っている**
  - **squash commit 本文の closing keyword** → main に載った時点で閉じる。既定本文はリポジトリ設定が決め、**本リポジトリは `squash_merge_commit_title=PR_TITLE` / `squash_merge_commit_message=PR_BODY`**（#488 で `COMMIT_OR_PR_TITLE` / `COMMIT_MESSAGES` から変更。**ブランチのコミット本文は squash 本文へ流入しない**）。ただし `--body-file` に closing keyword を書けば、**`closingIssuesReferences` に現れないまま**閉じる
  - **hook はどちらも見ていない**（→「フック」の (A2) の非対称）。**これは規範であって機構ではない** — 手順を飛ばせば止めるものは無い。**だからこそ下の手順が唯一の防御である**
  - **マージ方式を変えても逃げられない**: 本リポジトリは squash のみ有効（`allow_merge_commit` / `allow_rebase_merge` はいずれも `false`。`--merge` / `--rebase` は GitHub が拒否する）

  手順（squash マージでは常にこの順。`<PR>` は PR 番号、`<issue>` は issue 番号）:
  1. **マージ直前に** `gh pr view <PR> --json closingIssuesReferences` を**必ず**見る。これが GitHub の計算した「いま閉じる issue」である
  2. 一覧に閉じたくない issue があれば **PR 本文を編集して手順 1 を実行し直す**（`gh pr edit <PR> --body-file <tmp>`）。**一覧から消えるまで繰り返す。** どの行のどの語が効いたかを推測しない — 認識されるのは `close/closes/closed` `fix/fixes/fixed` `resolve/resolves/resolved` の 9 形で大文字小文字を問わず、表やチェックリストの中の行も、1 行に同居する複数の参照も効く。**編集を終えてよいと決めるのは一覧であって、自分のキーワード走査ではない**。マージ時の `--subject` / `--body-file` では止められない
  3. `--subject` / `--body-file` は squash commit のメッセージを整えるためだけに使う。**closing keyword を書いてはならない**（散文の "partially fixes #N" も効く）— 書くと手順 1 の一覧に現れないまま閉じる。省けば squash 本文は **PR 説明文そのもの**になる（表・チェックリスト込みで冗長）
  4. マージ後に**必ず**、次の 3 つを確認する。**`closingIssuesReferences` を数えるだけでは足りない** — それは PR 本文からその瞬間に再計算される値であって、閉じた事実そのものではない:
     - 取り直した `gh pr view <PR> --json closingIssuesReferences` の全件が意図どおり閉じたか
     - **残すと決めた issue が今も `OPEN` か**（`gh issue view <issue> --json state`）。正しく動いていればそれらは上の一覧に現れない。**ゆえに一覧を数えるだけでは、守りたい当の issue を一度も見ないことになる**
     - `gh issue list --state closed --search "closed:>=<mergedAt>"`（`mergedAt` は `gh pr view <PR> --json mergedAt`）。どちらの一覧にも属さない「知らないうちに閉じた issue」を拾う、唯一の接地した観測点
     誤って閉じていたら `gh issue reopen <issue>`（close イベントは履歴に残り、close を契機に動く下流は巻き戻らない。**reopen は回復であって、事前確認を省く免罪符ではない**）

  **手順 1 の一覧が「閉じる issue のすべて」になるのは、手順 3 を守り、かつ確認からマージまで PR 本文が変わらなかったときだけである。** 本文を凍結する機構は無く、`gh pr merge --auto` は確認とマージを分単位で引き離すため**使わない**。塞げない残余（多主体による本文の書き換え）は設計文書 §2 に記した。

  設定の組み合わせは GitHub が制限する（実測・422。有効な組は設計文書 §2）。**元に戻す**なら `gh api -X PATCH repos/:owner/:repo -f squash_merge_commit_title=COMMIT_OR_PR_TITLE -f squash_merge_commit_message=COMMIT_MESSAGES`

## フック（.claude/settings.json）

エージェントの操作には以下のフックが介入する。**どちらのフックも、発火（`matcher`）は `.claude/settings.json` が、判定は各スクリプトが SSOT である** — PreToolUse は `.claude/hooks/pre-bash.mjs` の `decide`、PostToolUse は `.claude/hooks/post-edit.mjs` の `selectChecks`。**main 保護の実体はここではない** — リポジトリの状態は hook の視界の外にあるため、`.githooks/` と GitHub ruleset が担う（→「Git/GitHub 運用」）。

| フック | 発火条件 | 正しい対応 |
|---|---|---|
| PR 作成前 push チェック（PreToolUse） | `Bash` / `PowerShell` の `tool_input.command` の**コマンド位置**に `gh pr create` があり、かつ安全と確認できないとき（空 PR / `Closes` 誤 close 防止）。`&&` で `git push` が先行するなら通る | `git push -u origin HEAD` してから PR を作る（または `&&` で繋ぐ） |
| 編集後の自動検証（PostToolUse） | `tool_input.file_path` が属するツリーからの相対パスで判定。`*.rs` → clippy（`snotra-core` / `src-tauri` / `snotra-settings` 配下ではその crate のテストも）、`ui/src/**` / `e2e/**` の `*.{ts,tsx,mts,cts}`（テスト含む）とルートの `vite.config.ts` / `vitest.config.ts` / `playwright.tauri.config.ts` / `tsconfig.json` → typecheck、`tauri.conf.json` / `config.toml` → WARN（`src-tauri/tauri.conf.json` はさらに CSP 契約テスト）、`Cargo.toml` → cargo check、`.claude/settings.json` / `.claude/hooks/**` / `tsconfig.json` / `package.json` / `vitest.config.ts` / ルートの `Cargo.toml` → hook-selftest、`.githooks/**` → githooks-selftest | **検査が割り当てられているファイルでは、沈黙は合格を意味する**（割り当ての SSOT は `selectChecks`）。失敗時のみ `exit code` と再現コマンドと診断が会話に届く。手動での再実行は不要 |

- **PreToolUse は `exit 2` だけがブロックする**（#482 実測）。`exit 0` は許可、それ以外の非ゼロ（Node が未捕捉例外で返す **1** を含む）は「非ブロッキングエラー」でコマンドはそのまま実行される。ゆえに `pre-bash.mjs` は**既定の `process.exitCode` を 2 に置き、許可が確定した経路だけが 0 を書く**。判定不能（payload 破損・`command` が非文字列・git 状態が読めない・鎖の途中で `cd`）はすべて block へ倒す。この fail-closed の骨格を壊してはならない
- **PreToolUse の判定は `tool_input.command` だけを見る**（#482）。`description` や payload 全体を grep してはならない。判定単位は「コマンド位置に現れる呼び出し」であり、`grep "gh pr create" f` のように引用の内側にあるだけでは発火しない。過剰検出（`echo "&& gh pr create"`）は fail-closed 方向ゆえ許容する
- **hook が見ないコマンド形がある**（#482・受容する性質）。`sh -c '...'` / `eval` / バッククォート / ラッパ経由（`timeout 5 gh pr create` / `xargs`）は「gh がコマンド位置に現れない」ため検出しない。これは事故モードではなく意図的迂回であり、`--no-verify` と同格に**人間専用**として扱う。検出を shell パーサ相当まで広げると payload 全体 grep の誤爆を作り直すことになる
- **(A2)「外部 API の不可逆呼び出し」のうち hook が守るのは `gh pr create` だけである**（#488 実測・**意図的な非対称**。「対称にせよ」という指摘への答えはここにある）。`gh pr merge --squash` / `gh issue close` に hook を足さない理由は 3 つ:
  - **誤りの定義が観測できない。** `gh pr create` の誤り（空 PR）は**リポジトリの状態**であり、hook は git に問える。`merge` / `close` の誤りは**人の意図**で、真実源がどこにも無い。閉じる集合は計算できても、それが誤りかは判定できない。ゆえに `deny`（exit 2）は原理的に書けず `ask` 止まりになる
  - **`ask` は fail-closed 骨格と両立しない。** `permissionDecision` を返すには stdout の JSON と **exit 0** が要る。`pre-bash.mjs` は「既定 exit 2、許可が確定した経路だけが 0」で成り立っており、JSON が壊れた瞬間に fail-open へ倒れる（パース失敗時の挙動は未文書＝未測定）
  - **視界が実運用の経路を覆わない。** hook が見るのはエージェントのツール呼び出しだけで、GitHub Web UI の "Squash and merge" とユーザー端末の `gh pr merge` は盲である（実際そちらでもマージされている）

  代わりに **Layer 0 で断った** — `squash_merge_commit_message` を `PR_BODY` にし、**ブランチのコミット本文が squash 本文へ流入する経路**を、あらゆるマージ実行経路（Web UI を含む）から消した（→「Git/GitHub 運用」）。既定では閉じる集合が `closingIssuesReferences` に一本化され、PR ページに表示される。ただし**すべての close 経路を消したわけではない** — `--body-file` に closing keyword を明示的に書けば依然閉じる。そこは機構ではなく手順 3 が覆う残余である。`gh issue close` は権限プロンプトが issue 番号をそのまま提示するため、hook を足しても情報が増えない。**設定の read-back を監視する検知器も置かない** — GitHub ruleset と同格に扱う（「安全網の不在を検知する安全網」という無限後退から降りる）
- **検出は exit code、出力は証拠**（#471）。検査が成功した hook は何も出力しない。失敗したときだけ `--- <検査>: 失敗 (exit N) ---` と再現コマンドが会話に現れる。診断が予算（`head`/`tail` 数行〜数十行）を超えても、再現コマンドで全件を見られるので取りこぼしは無い
- **検査が走ったなら、沈黙を「合格」と読める。沈黙しうる経路をすべて塞いだから**。タイムアウト（検査ごと 300s で自ら打ち切る）・出力溢れ・起動失敗・スクリプト内部エラーは、いずれも必ず報告される。この契約を壊す変更を `.claude/hooks/` に入れてはならない。**ただしこれは「検査が走った」ことを前提とする主張である**（→ 次項）
- **`selectChecks` に載っていないファイルの沈黙は「何も走らなかった」であり、合格ではない**（#497）。`*.md` 全般・`SPEC.md`・`scripts/` 配下の非 TS ファイル・`.github/workflows/`・`Cargo.lock`・`docs/build-commands.md` 自身がこれに当たる（`scripts/*.ts` は「include 対象外」の一行が出るため沈黙しない）。**エージェントはこの 2 種類の沈黙を区別できない** — 決定的な項目（参照実在・索引・スキル表・SPEC 番号・rules glob・コマンド写像）は PR CI の `governance-check` job（`skip-ci` 非対象・#587）が捕捉する。編集時の即時性は無く、governance:check の検査対象外の記述（責務の妥当性等の意味的整合）は依然**受容する残余**である
- **肯定的報告（走った検査に名乗らせる）を採らなかった理由**（#497）。「沈黙 = 合格」を剥がせば上の残余は消えるが、全編集で出力が増える（`.md` 1 行の編集でも「検査なし」と報告する）。入力集合の拡張で「検査の定義を変えるファイル」の穴は塞がり、残るのは検査を持ちようがないファイルだけになった。ゆえに前提条件の明文化で足りると判断した。**これは機構ではなく規範である** — 読者が前提条件を忘れれば false green は再発する
- **`selectChecks` に発火を足すときは、カナリアも対で足す**（#497）。`hook-selftest` は「そのファイルを読んで前提と照合するテスト」が `.claude/hooks/**` の中に在って初めて意味を持つ。カナリアの無いファイルに検査を撃っても、vitest が起動することしか証明しない（何も検証しない緑）。`tsconfig.json` / `vitest.config.ts` / `package.json` / ルートの `Cargo.toml` はそれぞれ対応するカナリアを持つ。**守るのは沈黙する経路だけでよい** — 例えばパッケージ名の改名は `cargo test -p snotra` が "package did not match" で loud に落ちるため、カナリアは要らない（#500）
- **`.ts`/`.tsx` を編集したのに何も出ない場合は 2 通りある**: 型検査が通った、または `tsconfig.json` の `include` 対象外（`ui/src`・`e2e`・ルート config 3 ファイル以外の `.ts`。例: `scripts/` 配下）。後者では `[post-edit] ... は tsconfig の include 対象外です` という一行が出る
- **hook は worktree でも「そのファイルが属するツリー」を検査する**。root は `file_path`（絶対パス）から最近接の `.git` を遡って導出するため、`CLAUDE_PROJECT_DIR` の意味論に依存しない。ただしスクリプト自身の所在は `settings.json` の `${CLAUDE_PROJECT_DIR:-.}` で解決し、相対 `file_path` を受け取った場合は cwd 基準で `resolve` する
- **`.claude/settings.json` の編集は file watcher が即座に拾う**（セッション再起動は不要・実測）。壊れたスクリプトを配線するとその瞬間から全検査が沈黙する。そのため `.claude/settings.json` と `.claude/hooks/**`、および検査の定義を変えるファイル（`tsconfig.json` / `package.json` / `vitest.config.ts` / ルートの `Cargo.toml`）の編集は `hook-selftest`（settings.json の JSON 検証 + `vitest run .claude/hooks`）を自動発火する。`.githooks/**`（main 保護の Layer 1）は同じ理由で `githooks-selftest` を発火する
- `config.toml` はリポジトリに実在しない（ランタイムのユーザー領域ファイル）ため、WARN の真陽性は事実上 `tauri.conf.json` のみ。その `src-tauri/tauri.conf.json` では WARN（人間向け・Windows 互換の注意喚起）に加えて `csp-test`（機械検査・CSP 契約）が併走する — 役割が違うため両方残している

## チーム憲章

Claude とユーザーが一緒に作業するときの関係性の原則。

- **意図は明確に、指示は短くていい** — 「なぜそうしたいか」が共有されていれば、具体的な手順は Claude が判断する。意図のない指示より、意図のある短い一言の方がよい結果になる
- **複雑さや意図不明さに気づいたら声を上げる** — 設計が複雑すぎる、コードの意図が読めない、と感じたらどちらが先でも「あ」の一言でいい。指摘のタイミングに遠慮はいらない
- **「やりすぎでは」を歓迎する** — 提案・実装・ドキュメントのどれに対しても、削る・簡略化するという方向の指摘を双方が行う
- **記録への信頼で動く** — 記憶ではなく AGENTS.md・CLAUDE.md・スキル・RETROSPECTIVE.md への記録がチームの連続性を作る。気づきはその場で記録する
- **エージェント設定の更新は相談してから** — スキル・フック・rules など、エージェントの行動やワークフローを制約する設定の変更は Claude が単独で判断せず、合意してから行う

## サブエージェント委譲と worktree

委譲という局面に入ったら効く規則。AGENTS.md の条件別チェック表からもここへ誘導される。

- **並列エージェント委譲はファイル境界で衝突を予測してから行う** — 同一ファイルに触りうるタスクは直列化するかマージ順を決める。境界は「実装中に踏み込みうる隣接ファイル」まで含めて見積もる。リベース解決はコンテキストを保持した実装エージェント本人に依頼するのが最短（#439, #435）
- **委譲はコンテキストを継承しない** — メインエージェントの system prompt にしか無い事実（メモリ領域の絶対パス等）はサブエージェントのプロンプトへ明示的に渡す——渡し忘れると見えないものを「無いもの」として報告する。**`allowed-tools` はインライン実行のスキルを拘束しない**（実測）——frontmatter に `Agent` が無いことを根拠に「このスキルは委譲しない」と推論してはならない。また**委譲した検査が対象を読む時刻は制御できない＝検査対象を変更しながら検査を走らせない**（#489）
- **長時間の委譲タスクは中断を前提に設計する** — セッションリミット・API エラーで途中終了しうる。大きなタスクは Phase 分割し「各 Phase の検証 green 後にコミット」を指示に含める（#431）
- Agent `isolation: "worktree"` は使用可（worktree は `.claude/worktrees/agent-<id>/` に作られ、`.gitignore` 済み）。残った worktree は `npm run clean:worktrees` で掃除する（→ `docs/build-commands.md`）

## コミュニケーション原則

### 着手の判断

- **タスクが真に曖昧でない限り、分析・計画より実行にバイアスをかける**
- **ユーザーが具体的な計画や修正指示を既に提示している場合、プランモードへの遷移・事前の全体探索を禁止する** — 読むファイルは直接関係する最小限（1〜2ファイル）に絞り、最初の Edit/Write から着手する
- **コミット・PR 作成を指示された場合、確認やプランモードなしに即実行する** — コミットは必ず feature ブランチで行う（→「最重要ルール」1）
- **不明点がある場合は、1つの焦点を絞った質問をしてから実装に移る**

### 実装・レビュー時

- **計画書・設計書を提示された実装は、内容を忠実に実装する** — 計画書の要素を省略・統合・削除するのは明示的に指示された場合のみ行う
- **意図的なリファクタリングの結果を元に戻さない** — `/simplify` などのコードレビュー系スキルを実行するとき、意図的な分割・名前変更・責務分離は維持する。「重複に見えるが意図的に分けた」構造は、ユーザーに確認してから変更する

### 調査・助言の依頼

- **分析・調査・助言を求められたら、調査結果のみを報告する** — 明示的に指示されない限り、実装計画やコード変更に踏み込まない

## 利用できるスキル

| スキル               | 使うとき                                                                | 呼び出し例                                                                     |
|----------------------|-------------------------------------------------------------------------|--------------------------------------------------------------------------------|
| `/plan-review`       | 計画（plan.md）完了後: サブエージェントで影響範囲・不変条件・スコープを並列検証（横断変更では独立導出+差分も） | `/plan-review`                                                            |
| `/symmetric-check`   | コードパス変更・バグ発見時に対称ペアの適用漏れを確認                    | `/symmetric-check result-clicked: added emitSelectionUpdate`                   |
| `/dry-check`         | 関数を新規定義・変更したとき、手書き重複が残っていないか確認            | `/dry-check show_main_and_emit: show() + set_focus() + emit(window-shown)`     |
| `/race-check`        | async 関数を新規追加・変更したとき、各 await 地点の状態競合リスクを検証 | `/race-check executeInstantCommandSelected: await api.executeInstantCommand()` |
| `/cache-check`       | キャッシュロジックの追加・変更時に述語の単調性と状態遷移の安全性を検証  | `/cache-check search_with_options: use_incremental 判定`                       |
| `/persistence-check` | シリアライズ・on-disk 形式（index.bin/config.toml/history/window.bin）の変更時に version バンプ要否・旧形式の後方互換テスト・デコード失敗時のデータ保全を検証 | `/persistence-check IndexCache: Cow 統合`                       |
| `/state-check`       | UI モード・ガード条件の追加・変更時に直交性・リセット経路・SPEC §8.6 整合を検証 | `/state-check InstantCommandMode 追加`                                    |
| `/health-check`      | 定期・サイクル完了後に governance:check の実行 + 意味的整合（hook フラグ照合・メモリ）を検証（報告のみ・修正しない） | `/health-check`                                                           |
| `/retrospective`     | サイクル終了後に教訓の抽出・残タスクの振り分け・RETROSPECTIVE.md 上書きを実施 | `/retrospective`                                                               |
| `/start-issue`       | GitHub issue から作業を開始（実装前段階のブランチ作成・調査・計画まで）  | `/start-issue 123`                                                        |
| `/implement`         | コード変更を伴うタスクの実装（調査からコミットまでのフルサイクル）      | `/implement キーボードショートカットの追加`                                     |
| `/deps-update`       | cargo/npm の依存を一括更新し PR 作成・CI 確認まで（マージは手動）       | `/deps-update` または `/deps-update npm`                                       |

サブエージェント: `code-reviewer`（`.claude/agents/`）— 実装後・コミット前の3フェーズレビュー（実装検証 / 計画判断・SPEC.md 同期 / パフォーマンス）。`/implement` Step 5b が自動で起動する。
