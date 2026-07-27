# 独立導出 — #768

前提: issue #768 本文（`gh issue view 768`）と実コードのみを入力に導出した。`workspace/plan.md` / `workspace/research.md` / `plan-review/` の他ファイルは**開いていない**。

**ただし独立は完全ではない。** 作業初期の `grep -rn "AREA_BUDGET"`（`workspace/` を除外し忘れた 1 回）の出力に 3 行が混じった: `research.md:24`（`AREA_BUDGET` の所在＝590 行・由来コメント 544-589・`checkNormativeAreaBudget` 640 行）と `plan.md:26` / `plan.md:100`（いずれも `AREA_BUDGET.alwaysLoaded` を「実測 + 100 字」へ引き下げる旨）。以降は全 grep で `':!workspace/'` を付けた。**ゆえに下表の `AREA_BUDGET` 行（および「+100 字」という形）は、他者の計画との一致を独立に裏付ける証拠にはならない**——ただし内容自体は issue #768 の完了条件が逐語で要求しており（「`AREA_BUDGET.alwaysLoaded` を実測 + 100 字へ引き下げる（理由コメント付き）」）、`governance-check.mjs:590` と由来コメントは自分で読んで位置を確認した。他の行に汚染は無い。

判定述語は全件を代表入力で自分で実行して測った（下「列挙の証跡」）。

## 必要な変更集合（ファイル → シンボル/節）

| ファイル | 追加・変更するシンボル / 節 | 理由 |
|---|---|---|
| `.claude/hooks/pre-bash.mjs` | 冒頭の設計コメント（1-24 行）の書き換え | 現在の全文が「`gh pr create` ガード」という単一目的宣言である（1-2 行「`gh pr create` が (1) 空 PR を…(2) plan.md…」）。判定が 7 本になるので、目的宣言・「受容する未対応リスク」の射程・`--no-verify` を人間専用と呼ぶ 19 行（**自分が `--no-verify` を機構で塞ぐ以上、この比喩の参照先が変わる**）を書き直す |
| 同 | `HEREDOC`（新規 export const） | `(?:^|\s)<<-?\s*(['"]?)([A-Za-z_]\w*)\1(?:$|[\s;&|)])`。**コマンド位置ではなくリダイレクト演算子位置**で判定する。`<<<`（herestring）・`1 << 2`・`"<<EOF"`（引用内の言及）を落とすことを実測済み |
| 同 | `WIN_ABS_PATH`（新規 export const） | `(?:^|[\s"'=(,])([A-Za-z]:\\)`。ドライブレター 1 文字に限定して `HKLM:\SOFTWARE`（PSDrive・`\` が必須）を落とす |
| 同 | `PYTHON_CMD` + `PYTHONIOENCODING` 有無の判定 | コマンド位置の `python` / `python3` / `py`。既存 `ENV_PREFIX` を再利用。**ただし PowerShell 側の設定構文は `$env:PYTHONIOENCODING=...;` で `ENV_PREFIX` の形と違う**（下「同概念・別名」） |
| 同 | `GIT_NO_VERIFY`（新規 export const） | `git` + `FLAG` + `(commit|push|merge|rebase|am|cherry-pick|revert)` の引数域に `--no-verify`。サブコマンド allow-list が必須（無しにすると `git grep -n -- "--no-verify"` を誤爆する。実測） |
| 同 | `GIT_COMMIT_SHORT_N`（新規 export const） | `git commit -n` / `-nm` は `--no-verify` の短形。**`git push -n` は `--dry-run` であり別概念**（既存 `DRY_RUN` が `-n` をそう解釈している） |
| 同 | `GIT_PULL_NO_FF`（新規 export const `GIT_PULL` + 既存に無い `FF_ONLY`） | `git pull` セグメントに `--ff-only` が無いことを見る。既存 `GIT_PUSH` を流用してはならない（`-C` の扱いが逆・下記） |
| 同 | `FLAG` の再利用 | `git -C . commit --no-verify` を取るには `(?:-\S+\s+)*` では足りず既存 `FLAG`（スペース区切りのフラグ値を読み飛ばす）が要る。実測で確認 |
| 同 | `isWindows(platform)`（新規 export） | `platform` を真偽へ落とす単一通過点。`decide` 内に `platform === "win32"` を 3 回書くと、判定不能時の倒し方が 3 か所に散る |
| 同 | `decide(payload, readGitState, readPlanState, platform)` — **第 4 引数の追加と分岐表の構造変更** | 新判定は `gh pr create` に依存しないため、現行の `if (ghAt < 0) return ALLOW`（121 行）**より前**に置く必要がある。すなわち「管轄外」の早期 return の位置が変わる = fail-closed 骨格の中心への手入れ |
| 同 | `REMEDY` → 復帰手順の定数群（`REMEDY_PUSH` / `REMEDY_HEREDOC` / `REMEDY_WIN_PATH` / `REMEDY_PY` / `REMEDY_NO_VERIFY` / `REMEDY_PULL`） | 現行 `REMEDY` は push 専用の module 定数（69 行）。改名は `scripts/governance-check.mjs:566` の由来コメント（`pre-bash.mjs` の REMEDY を名指し）にも波及する |
| 同 | `main()` — `decide(payload, …, process.platform)` | `decide` の中で `process.platform` を読んではならない（注入で外部状態を読む契約・`docs/hooks.md`）。**唯一の実 platform 読み取り点はここ** |
| `.claude/hooks/pre-bash.test.mjs` | 既存 `decide(` **22 箇所**の呼び出しに第 4 引数を足す（または後述のヘルパで包む） | 第 4 引数を省略可能にすると「省略 = Windows 扱い」が沈黙で成立する。ヘルパ `const dec = (p, g, pl = "win32", plan = NO_PLAN) => decide(p, g, plan, pl)` 方式が現実的 |
| 同 | 新 describe: `decide — HEREDOC ゲート` / `— パス区切り` / `— PYTHONIOENCODING` / `— --no-verify` / `— git pull --ff-only` | 各々に緑・赤の両方向。赤側は「拒否メッセージに復帰手順が含まれる」ことも assert する（issue の完了条件） |
| 同 | 新 describe: `platform ゲート — 注入で両 OS を固定` | `platform: "win32"` で block・`"darwin"` / `"linux"` で allow を同一入力で対にする。**`process.platform` を参照するテストを書かない**（CI は ubuntu と windows の両方で走る・`ci.yml:39,116`） |
| 同 | 新 describe: `platform 不明時は Windows 側へ倒す` | `undefined` / `""` / `"unknown"` を渡して block を固定（判定不能 → 厳しい側。下「設計判断」） |
| 同 | 新 describe: **allow corpus（日常コマンドが素通りすること）** | これが最重要の新テストである。現行の block 経路はすべて `gh pr create` に閉じているが、変更後は 5 述語のバグが**任意の Bash を止める**。`npm test` / `cargo clippy --workspace --all-targets --message-format short -- -D warnings` / `git status` / `gh pr view 768 --json state` / `rg '\d+' src/` / `find . -name '*.tmp' -exec rm {} \;` / `node scripts/governance-check.mjs` / `git commit -F <tmpfile>` を **両 platform で** allow に固定する。母集団は下の corpus sweep（123/128 clean）から採る |
| 同 | 既存 `fail-closed の骨格カナリア` describe への 1 件追加 | `main()` が `process.platform` を `decide` へ渡していることをソース述語で固定する。**platform 注入の喪失は 3 判定を沈黙で無効化する経路**であり、振る舞いテストでは赤くならない（既定の `exitCode = 2` を守る既存カナリアと同型の理由） |
| `.claude/hooks/post-edit.mjs` | `buildCommand` の `vitestSpec`（257 行）と 344 行 `repro: \`${spec.repro}  (cwd: ${root})\`` の `\` → `/` 正規化 | **実測で発覚した衝突**: PostToolUse が会話へ出す再現コマンド 5 本すべてが新 `WIN_ABS_PATH` に一致する。ただし内訳を区別すべきで、**そのまま貼って実行できる形での衝突は 2 件**——`vitestSpec` 由来の `node C:\workspace\Snotra\node_modules\vitest\vitest.mjs run .claude/hooks` と `… run .githooks`（hook-selftest / githooks-selftest 失敗時に出る＝**本 PR で必ず踏む経路**）。残る 3 件（cargo 系）は末尾注記 `  (cwd: C:\workspace\Snotra)` だけで一致しており、注記はシェルへ貼る部分ではない。**2 件が実害・3 件は注記由来**。両サイト（257・344）を直すのが正しいが、行の重みはこの内訳で述べる。片方の hook が「これを実行して再現せよ」と言い、もう片方が拒否する状態になり、**規範の語（HEREDOC / `\` / no-verify）で grep しても到達しない消費者である** |
| `.claude/hooks/post-edit.test.mjs` | 新カナリア: 再現コマンド文字列に `\` が現れない | 上の是正が将来の編集で沈黙して戻るのを防ぐ。hook 間の不変条件（一方の出力が他方に拒否されない）を機構で固定する |
| `CLAUDE.md` | 「シェル環境（Windows / PowerShell）」節を**丸ごと削除**（12-19 行。見出し・表ヘッダ・区切り・3 行） | 3 行すべてが対象規範なので節が空になる。G11 の母集団（`headingRefDocs`）内にこの見出しへの正準形参照は無い（唯一の参照は `docs/superpowers/plans/2026-07-24-su6.5-flip-hardening.md:20` で、`docs/superpowers/` は除外・`governance-check.mjs:803`）ため削除して緑 |
| 同 | 「Git/GitHub 運用」の 2 bullet（24-25 行）を削除。**見出しは残す** | `CONTRIBUTING.md:16` と `docs/adr/0002-*.md:15,27,33` が `CLAUDE.md`「Git/GitHub 運用」を正準形で指しており、見出しを消すと G11 が 4 件落ちる |
| 同 | 「フック」表の PreToolUse 行（46 行）の見出し文言と発火条件 | 行名が「PR 作成前 push チェック（PreToolUse）」のままでは、7 判定を持つ hook の説明として偽になる。**ただし判定の列挙を書き写してはならない**（面積が戻る・`docs/hooks.md` が一覧の正本）。「拒否メッセージが復帰手順を含むので従えばよい」という 1 句に圧縮する |
| `scripts/governance-check.mjs` | `AREA_BUDGET.alwaysLoaded`（590 行）を **13274 + 追記分 + 100** へ引き下げ | 実測: 現在 always 14036 / 基準 14058。CLAUDE.md は 7611 字で、上記削除は **-762 字** → 削除のみなら 13274。フック表への追記を 60 字とすると実測 13334・**基準 13434** が妥当 |
| 同 | 544-589 行の由来コメントへ日付入り項目を追加 | ADR-0005「引き上げ/引き下げの記録は定数コメントに残す」。**引き下げでも理由コメントが要る**（直近 4 件の引き下げがすべてそう記録されている） |
| 同 | 566 行「`pre-bash.mjs` の REMEDY と三重だった」 | `REMEDY` を改名するなら、この由来コメントの名指しが腐る。**シンボル名での間接参照であり `REMEDY` を grep しなければ到達しない** |
| `docs/hooks.md` | 「PreToolUse（pre-bash.mjs）の実装契約」節に 7 判定の一覧・platform ゲートの倒し方・受容する偽陰性を追記 | 常時ロード面から降ろした内容の着地先（非課税面）。`AGENTS.md:45` が指す `docs/hooks.md`「PostToolUse（post-edit.mjs）の発火一覧」と対になる PreToolUse 側の一覧が現状無いので、**節名を変えずに**中身を足す（変えると G11 が `AGENTS.md:45` を落とす） |
| 同 | 「判定は `tool_input.command` のコマンド位置だけを見る」（12 行）の文言修正 | **7 判定のうち 4 つはコマンド位置ではない**（heredoc = リダイレクト演算子位置、`\` パス = 引数位置、`--no-verify` = フラグ位置）。この全称文が偽になる。`AGENTS.md`「検証の作法」の「全称表現は前提条件とセットで書く」に直接当たる |
| `docs/development-principles.md` | §6 の #482 の記述（81 行）に「判定単位は構文的位置」の一般化を 1 句 | 現行は「コマンド位置に現れる呼び出し」と単数形で書いており、リダイレクト演算子位置・フラグ位置という**別の構文位置**が加わる事実と齟齬する。任意（機構は変わらない）だが、この文が規範として引かれている |
| `docs/adr/` に追記または新規 | 却下した設計（下記 5 件）の否定の知識 | `AGENTS.md`「ドキュメント参照」が「否定の知識が生じた決定のみ ADR」と定める。本 issue は却下が 5 件生じる（env override・広い `\` 検出・branch 参照・グローバル block・`core.hooksPath` 同時対応）。**新規 ADR-0009 より、コード内コメントで足りるものを分ける**判断が要る |
| （変更しない） | `.claude/settings.json` | `matcher: "Bash|PowerShell"` は不変。**「新しい発火点を作らない」が真なのはこの層のみ**（issue 本文の主張の射程） |
| （変更しない） | `.claude/rules/safety-nets.md` | rules 面は 7956/8056（余裕 100 字）。詳細をここへ移すと ADR-0001 の二面独立が可視化する目的そのものの**面替え**になる。着地先は非課税の `docs/hooks.md` |
| （変更しない） | `.githooks/_lib.sh:20` | 「意図的な操作なら --no-verify で迂回できます（人間専用。エージェントは使用禁止）」は**人間の端末向け**。PreToolUse hook は Claude のツール呼び出ししか見ないので矛盾しない。ただしレビューでこの整合を明示的に確認すべき箇所である |

## 設計判断（自分の結論と理由）

### platform の注入の形

**結論: `decide` の第 4 位置引数 `platform`（文字列・既定値なし）+ export した `isWindows(platform)` ヘルパ。`decide` 内で `process.platform` を読まない。`main()` の受け渡しをソースカナリアで固定する。**

- **オブジェクト 1 個へ束ねる案（`decide(payload, deps)`）は却下**: 既存 22 箇所の呼び出しと `docs/hooks.md:14` の `decide(payload, readGitState, readPlanState)` の逐語記述を全面改稿することになり、変更の中心（判定 5 本）から離れた差分が支配する。第 4 引数は既存 2 引数と同じ「外部状態は注入」の形をそのまま延長する。
- **env 変数（`PRE_BASH_PLATFORM` 等）での上書きは却下**: それは**文書化されたバイパスの新設**である。ガードを無効化できる env が 1 つ在れば、それが最も安いすり抜け経路になる（`--no-verify` を機構で塞ぐ本 issue の趣旨と正面から矛盾する）。プロセス越えのテストが要るときは `it.runIf(process.platform === "win32")` で範囲を狭める。
- **`process.platform` を `decide` 内で直接読む案は却下**: `docs/hooks.md`「fail-closed 骨格」の「外部状態は注入で読む」を破り、両 OS 分をテストで固定できなくなる（issue の完了条件が「注入で両 OS 分を固定する」と明示）。

### 判定不能・platform 不明時の倒し方

**結論: `isWindows(platform) = !(platform === "darwin" || platform === "linux")`。すなわち既知の非 Windows だけを allow-list し、`undefined` / `""` / 未知の値は Windows 扱い＝3 判定を適用する。全 Bash の block へは倒さない。**

- 理由 1: `platform` が不明になる経路は実行時には存在しない（`process.platform` は常に文字列）。到達するのは**配線ミスのときだけ**である。そのとき静かに 3 判定が消えるのが最悪であり、`.claude/rules/safety-nets.md`「これまで無意味だった状態に意味を与える変更は、その状態に到達する全経路を列挙する」がまさにこの形を禁じている。
- 理由 2: それでも「全コマンド block」へは倒さない。判定不能の作用範囲を、当の 3 判定が見ているパターン（heredoc・`C:\`・python）に限れば、fail-closed の通貨を払いつつ復旧不能な状態（全 Bash 停止）を作らない。issue「危険」節が言う全 Bash ブロックは避けられる方が良い。
- 理由 3: それでも**沈黙は残る**（配線が消えても誰も気づかない可能性）。だからソースカナリア 1 件で `main()` の受け渡しを縛る。既定 `exitCode = 2` を縛る既存カナリアと同じ理由・同じ場所に置く。
- 既存の判定不能（payload 破損・`command` 非文字列・git 読めない・鎖に `cd`）はいずれも block のままにする。**新判定は「判定不能」の状態を新設しない**（純粋な文字列述語で外部 I/O をしない）。これは意図した性質で、**hook は全 Bash 呼び出しで走るため、5 判定が git spawn を増やしてはならない**（現行は `gh pr create` 検出時のみ spawn する）。

### 各判定の検出漏れ / 誤検出のどちらへ倒すか

既存の doctrine は「過剰検出は許容、過小検出は不可」（`pre-bash.mjs:43`）だが、**それは対象が `gh pr create` という稀なコマンドだったから成り立っていた**。5 判定は任意のコマンドに当たるので、判定ごとに倒す向きを変える。

| 判定 | 倒す向き | 理由 |
|---|---|---|
| HEREDOC | 誤検出側（過剰検出）へ。ただし「引用内の言及」は落とす | 事故そのものが静かに間違った結果を生む（後述の実測）。`git grep "<<EOF"` を止めない程度に絞れることを実測した（14/14 一致）。`sh -c 'cat <<EOF'` の見逃しは既存の「ラッパ経由は見ない」と同格に受容 |
| パス区切り `\` | **検出漏れ側へ倒す（例外）** | ここだけ逆にする。`\` は正規表現・`find -exec \;`・`sed` に日常的に現れ、広い検出は**作業を止める誤検出を量産する**。ドライブレター絶対パス（`C:\...`）＝実際の事故の形だけを取り、相対 `src-tauri\src\main.rs` は受容する偽陰性として `docs/hooks.md` に明記する。**この非対称を明文化しないと、後の誰かが「過小検出は不可」を根拠に広げて作業を止める** |
| PYTHONIOENCODING | 誤検出側へ（python 起動時は常に要求） | 出力が非 ASCII かは hook から見えない。「出力を予測して付ける」は書き手の記憶段の規則なので、**常に要求する**方が階梯が上。追跡ファイル内に `python` の言及は **0 件**（`git grep -ln python` が 0）なので誤検出の実害はほぼ無い |
| `--no-verify` | 誤検出側へ、ただし git サブコマンド allow-list で絞る | allow-list 無しでは `git grep -n -- "--no-verify"`（規範文書を検索する行為そのもの）を止める。#482 が根治した「バグを説明する行為を妨げる」誤爆の再来になる。実測で allow-list ありなら落ちる |
| `git commit -n` | 誤検出側へ（`-nm` のような短形クラスタも取る） | `-n` を無視すると `--no-verify` を塞いだ隣に**そのままの抜け道**が残る。`git push -n` を巻き込まないため commit 限定 |
| `git pull --ff-only` | 誤検出側へ（**branch を見ない**） | `readGitState` を拡張して「main の時だけ」にする案は却下: (a) 稀なコマンドのために git spawn と**新しい判定不能分岐**を増やす、(b) その分岐は fail-closed で block へ倒すので結局 block が増える、(c) feature ブランチでも `--ff-only` は正しく、このリポジトリの rebase は `git fetch` + `git rebase` 経路である。`git fetch` + `git merge` は塞がないので袋小路にはならない |

### 復帰手順メッセージ同士の衝突（設計制約）

**heredoc の復帰手順と `\` パス判定は同時に設計しないと矛盾する。** heredoc の remedy は「一時ファイルへ書いて `git commit -F <tmpfile>`」だが、この環境の scratchpad パスは system prompt から `C:\Users\Eoh\AppData\Local\Temp\...` の形で与えられる。remedy に素直に従うと次の試行が `WIN_ABS_PATH` で止まる。**remedy 文言に `/` 区切りで書くことを明記する**（例: 「`C:/Users/.../msg.txt` のように `/` 区切りで」）。これは文言の綾ではなく、2 判定の間の不変条件である。

### issue 本文と食い違う 2 点（自分の結論）

1. **「判定点は既存のコマンド位置検出と同じである（新しい発火点を作らない）」は部分的にしか真ではない。** 真なのは `.claude/settings.json` の `matcher` 層だけ。判定位置は 3 種に増える（コマンド位置＝python / `git pull` / `git ...`、リダイレクト演算子位置＝heredoc、引数・フラグ位置＝`\` パス / `--no-verify`）。**「同じ判定点」と見積もると、`docs/hooks.md:12` の全称文の是正と `decide` の早期 return 位置の移動を落とす。**
2. **「macOS で完結する 2 件 / Windows が要る 3 件」の切れ目は着手機の都合である。** 本作業は Windows 機（`win32`・実測環境）で行うので、5 件すべてをライブで 1 パス実測できる。issue が想定する段階化は不要で、PR 本文には 5 件のフォールトインジェクション結果を載せられる。逆に `"darwin"` / `"linux"` 側の分岐は**注入テストだけが唯一の実行経路**である（この機ではランタイムに一度も通らない）ことを明記すべき。

## 取りこぼしやすいと判断した箇所（間接参照・コンパイラを持たない機構）

### 同名・別概念（同じ語・同じトークンが違う概念を担う）

- **`-n` は 2 つの概念である。** `git commit -n` = `--no-verify`（塞ぐ対象）、`git push -n` = `--dry-run`（既存 `pre-bash.mjs:64` の `DRY_RUN` がそう解釈し、`hasSafeChain` が「送信しないので安全な鎖ではない」判定に使っている）。既存 `DRY_RUN` を no-verify 判定に流用すると、意味が反転したまま緑になる。実測で `git push -n` が commit 判定に当たらないことを確認済み。
- **`-C` の扱いが判定ごとに逆である。** `GIT_PUSH`（`pre-bash.mjs:59-61`）は `git -C <tree> push` を**意図的に一致させない**（別ツリーへの push は安全な鎖ではない）。`--no-verify` 判定は逆に `git -C <tree> commit --no-verify` を**一致させなければならない**（別ツリーでも hook 迂回は迂回）。同じ `FLAG` 定数を使いながら期待が逆——`GIT_PUSH` を再利用すると過小検出になる。
- **「コマンド位置」が 2 つの意味で使われる。** (a) hook の発火点（`settings.json` の `matcher`）、(b) コマンド文字列内で*プログラム名*が現れる位置（`AT_CMD_POS`）。issue 本文の「新しい発火点を作らない」は (a) の話、「判定点は既存のコマンド位置検出と同じ」は (b) の話で、後者は偽。
- **「セグメント」も 2 つ。** `hasSafeChain`（`pre-bash.mjs:90-92`）は「push セグメントの終端 = 次の区切り文字」を局所計算する。新判定で `command.split(/[;&|\n\r]+/)` のような汎用セグメント分割を導入すると、**heredoc 本文中の改行や `|` が区切りとして数えられ**、`hasSafeChain` の定義と非互換な第 2 の「セグメント」ができる。新判定は `[^;&|\n\r]*?` の局所形で書き、分割器を作らない方が安全。
- **「platform」も 2 つ。** ランタイムの `process.platform` と、注入される判定用の値。前者を読む場所を `main()` の 1 点に限るのが不変条件。

### 同概念・別名の間接参照（当の語を grep しても到達しない）

- **`.claude/hooks/post-edit.mjs` が会話へ出す再現コマンド 5 本全部が新 `WIN_ABS_PATH` に一致する**（実測・下記 sweep）。`post-edit.mjs:257` の `path.join(...)`（`node C:\workspace\Snotra\node_modules\vitest\vitest.mjs run .claude/hooks`）と `post-edit.mjs:344` の `(cwd: ${root})`。HEREDOC / `\` / `--no-verify` のどの語も `post-edit.mjs` には現れないので、**規範の語で grep しても永遠に到達しない**。片方の hook の指示が他方に拒否される状態は false green では済まず、実運用で毎回衝突する。
- **`scripts/governance-check.mjs:566` が `pre-bash.mjs` の `REMEDY` をシンボル名で名指している**（面積削減の由来コメント）。`REMEDY` を分割・改名するとこの記述が腐る。`--no-verify` でも `HEREDOC` でも grep には出てこない。
- **`docs/hooks.md:12` の全称文**「判定は `tool_input.command` の**コマンド位置だけ**を見る」。この文は規範として `.claude/rules/safety-nets.md` 経由で hook 改修者に配送される。4 判定が別の構文位置を見る以上、実装より強い主張になる（`AGENTS.md`「全称表現は前提条件とセットで書く。書けないなら書かない」）。
- **`docs/hooks.md:13` の「`--no-verify` と同格に人間専用として扱う」**。`pre-bash.mjs:19` にも同文がある。**`--no-verify` を機構で塞いだ後、この比喩は「hook が塞ぐもの」を「hook が塞がないものの喩え」として使い続けることになる**。二重の意味で腐るので両方直す。
- **`CONTRIBUTING.md:16` / `docs/adr/0002-*.md:15,27,33` → `CLAUDE.md`「Git/GitHub 運用」**（G11 正準形・4 件）。bullet を 2 本消しても見出しが残れば緑。逆に**節ごと消すと 4 件落ちる**。
- **`docs/superpowers/plans/2026-07-24-su6.5-flip-hardening.md:20` → ルート `CLAUDE.md`「シェル環境」**。正準形だが `headingRefDocs`（`governance-check.mjs:801-805`）が `docs/superpowers/` を除外するため**検査されない**＝節を消しても緑。`.claude/rules/governance-docs.md`「既に消滅した節の名前を正準形で書かない」に照らすと本来は腐った参照だが、履歴資料なので放置が妥当。**ただし「G11 が緑だから参照は全部生きている」と読まないこと**が要点。
- **`docs/superpowers/plans/**` に HEREDOC 禁止・`--no-verify` 禁止を書き写した計画書が 12 件以上ある**（`git grep -n HEREDOC` で 15 行）。非規範化済み（#589）なので変更不要だが、**規範の削除後もこれらが「まだ規範が在る」ように読める**。触らない判断とその理由を計画に書いておくべき箇所。
- **harness 側の tool description との衝突（機構外・変更不能）。** Bash ツールの説明文は "for multi-line strings use a heredoc" と**推奨**しており、PowerShell ツールの説明文は here-string を推奨している。**常時ロード規範は harness の tool description に負ける**——本セッションで私自身が CLAUDE.md を読んだ直後に heredoc を使った（下記実測）。これは「規範 → 機構」の階梯を 1 段上げる根拠として PR 本文に書く価値がある一次証拠であり、同時に**heredoc block が高頻度で発火する**予測でもある（remedy 文言の質が実運用コストを決める）。

### コンパイラを持たない機構への影響

- **`.github/workflows/ci.yml:39,116`（`npm test` が ubuntu と windows の両方で走る）**。platform を注入したテストは両 OS で決定的に走るが、**プロセス越えの `runHook` テスト（9 箇所）は実 platform を使う**。heredoc / `\` / python の e2e を `runHook` で書くと ubuntu で赤くなる。`it.runIf` で絞るか、`decide` レベルに留める。
- **`docs/build-commands.md:160`「`skip-ci` を貼ってよいのは…」** — `.claude/hooks/**` は貼ってはならない側に既に明記済み（変更不要だが、この PR で `skip-ci` を貼れないことの根拠）。
- **`scripts/governance-check.test.mjs:433`** が `AREA_BUDGET.alwaysLoaded + 1` を使う（定数参照なので引き下げに追随する）。**実測値のハードコードは無い**ので追加変更は不要。これは確認して「変更なし」と言える形。
- **`.claude/rules/safety-nets.md`「効いていることはフォールトインジェクションで一度は実測する」**。5 判定それぞれについて「意図的に違反コマンドを打って block される」を実測する。これは**ガードの行使**であり「稼働中のガードを弱めない」規則の対象外（同ファイルが明示）。ただし**allow 方向の実測も要る**（誤検出で日常作業が止まらないこと）——これは corpus sweep で代替できる。
- **`/norm-review` の起動条件**（`.claude/rules/safety-nets.md`「規範の場合」）。本 PR は規範を**減らす**方向だが `.claude/rules/**` を触らないなら自動配送されない。`CLAUDE.md` は規範文書ゆえ手動参照が要る（`AGENTS.md` 条件別チェック表の「セーフティネットを新設/変更」行）。
- **out-of-scope の隣接分類（実装せず、上げるべき論点）**: `--no-verify` を塞ぐと隣に `git -c core.hooksPath=/dev/null commit` と `git config --unset core.hooksPath` が残る（`docs/superpowers/specs/2026-07-09-hook-responsibility-layers-design.md:220` が `pre-rebase` の迂回手段として明記している）。正規表現 1 本で塞げること・`.claude/rules/safety-nets.md`「分類を 1 つ足したときは隣接分類への到達経路も再列挙する」が要求することは実測済み（`core\.hooksPath` で 2/2 検出）。**ただし issue のスコープは 5 件であり、`CLAUDE.md`「最重要ルール」2（エージェント設定の変更は合意してから）に照らして、実装ではなくユーザーへ提起する。**

## 列挙の証跡（どのツールに何件問うたか）

- `gh issue view 768` — 一次の本文を読んだ（要約を経由していない）。
- `git ls-files | wc -l` → **288 件**。`git ls-files .claude/` → 27 件。
- `npm run governance:check` → 緑。**証跡: 対象文書 42 件 / rules 7 件 / skills 13 件 / 常時ロード 14036/14058 字・rules 7956/8056 字 / 見出し参照 117 件を 54 文書から照合**（余裕は常時ロード 22 字・rules 100 字）。
- `npm test` → **248 passed / 5 files**。`npx vitest run .claude/hooks` → **153 passed / 2 files**（変更前のベースライン）。
- `grep -o "decide(" .claude/hooks/pre-bash.test.mjs | wc -l` → **22**（第 4 引数を足す呼び出し点の数）。`runHook(` → **9**（実 platform を使う経路）。`it(`/`it.each(` → 72。
- `git grep` による参照の数え上げ（`workspace/` 除外）: `HEREDOC|here-string` → 21 行（うち規範は `CLAUDE.md:16` の 1 行、`scripts/run-codex.sh:5` は**ファイル内容**でコマンド文字列ではない）/ `PYTHONIOENCODING` → 1 行（`CLAUDE.md:18` のみ）/ `no-verify` → 15 行（規範は `CLAUDE.md:24`、実装参照は `pre-bash.mjs:19`・`docs/hooks.md:13`・`.githooks/_lib.sh:20`）/ `ff-only|非 FF` → 28 行（規範は `CLAUDE.md:25`）。
- `git grep -ln python` → **0 件**（追跡ファイルに python の使用も言及も無い＝PYTHONIOENCODING 判定の誤検出実害の見積り）。
- `git grep "「シェル環境" ` → 4 行、うち `headingRefDocs` の母集団内は **0 件**（残り 3 件は `docs/superpowers/`）。`「Git/GitHub 運用」` → 母集団内 **4 件**（`CONTRIBUTING.md:16`・ADR-0002 の 3 行）。
- **述語の代表入力実測（自作スクリプト・scratchpad）**: 45 → 49 ケースまで拡張し **49/49 一致**（HEREDOC 14 / `\` パス 10 / python 6 / `--no-verify` 6 / `commit -n` 5 / `git pull` 8）。誤検出側の負例には `git grep -n "<<EOF"`・`cat <<< "x"`・`1 << 2`・`HKLM:\SOFTWARE`・`grep -E "\d+"`・`find … -exec rm {} \;`・`git push -n`・`git grep -n -- "--no-verify"` を入れた。
- **実コマンド corpus sweep**: `.claude/skills/**/*.md`（全 13 skill）+ `docs/build-commands.md` + `docs/hooks.md` + `CONTRIBUTING.md` + `CLAUDE.md` + `AGENTS.md` + `.claude/rules/**` + `.claude/agents/code-reviewer.md` の **27 ファイル**からコマンド候補 **123 件**を抽出し、`post-edit.mjs` が出す再現コマンド **5 件**を加えた **128 件**に 6 述語を当てた。→ **ヒット 6 件**。内訳: `PULL_NO_FF` 1 件（`CLAUDE.md:25` の規範文中の `git pull` = 削除予定の当の行）、**`WIN_PATH` 5 件（post-edit.mjs の再現コマンド全数・うち実行形は 2 件）**。**それ以外の 122 件は 0 ヒット**＝既存の手順書のコマンドは 1 件も止まらない。
  - **この母集団が測っているのは「文書に書かれたコマンド」だけである。** 実セッションのコマンド列で `C:\` が最も多く現れるのは、harness が system prompt に注入する scratchpad ディレクトリ（`C:\Users\Eoh\AppData\Local\Temp\claude\…\scratchpad`）であり、**これはリポジトリのどのファイルにも現れない**＝sweep の視界の外にある。ゆえに `WIN_ABS_PATH` の実発火率は文書由来ではなくこのパス由来で決まる。sweep は「既存手順書を壊さない」ことの証拠であって「発火が稀である」ことの証拠ではない——**だから heredoc の remedy を `/` 区切りで書くことは文言の綾ではなく 2 判定間の不変条件である**（上「復帰手順メッセージ同士の衝突」）。
- **既存テストのコマンドリテラルへの当て込み**（22 呼び出し点が signature 変更後も緑のままかの予測）: `pre-bash.test.mjs` から文字列/テンプレートリテラルを抽出し `${…}` を展開値で埋めた **54 件**（`echo hi` / `ls -la` / `git push … && gh pr create` 系 / `git push --dry-run` / `git push -n` / `git push --no-thin origin HEAD` / `git -C /x push` / `timeout 5 gh pr create` / `nohup …` / `sh -c '…'` / `GH_TOKEN=x gh pr create` / `cd ../other && …` / `Set-Location ../other && …` 等）に、win32 前提で 6 述語を当てた → **ヒット 0 件 / 54 件**。既存の allow ケースが新判定で block へ転じないことを実測で確認した（`git push -n` が `COMMIT_SHORT_N` に当たらないことを含む）。
- 面積の実測（`countChars` と同じ数え方＝コードポイント・CR 除去で自作計測）: `CLAUDE.md` **7611 字** / `AGENTS.md` **5591 字** / skill description 合計 **834 字**（= 14036 − 7611 − 5591、逆算で母集団を検算）。削除対象 10 行で **-762 字** → 削除のみで **13274**、フック表へ 60 字追記なら実測 **13334**・提案基準 **13434**。
- **heredoc 事故の一次実測（意図せず再現）**: 本作業中、私は `Bash` ツールで `cat > probe.mjs <<'ZZEOF'` を使って検査スクリプトを書いた（**CLAUDE.md を読んだ直後の違反**）。クォート付き heredoc なのに `\\s` が `\s` へ、`\\|` が `|` へと**バックスラッシュが 1 段落ちて**ファイルが壊れ、正規表現 7 本が「不一致」を返した（21/28）。同じ内容を `Write` ツールで書き直すと **49/49 一致**。**エラーではなく間違った結論が出た**——これがこの規範を機構へ上げる価値の最も直接的な証拠であり、同時に「判定不能へ倒す」設計の必要性の実例である。
