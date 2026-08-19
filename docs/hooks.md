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

**正本は `selectChecks` である。** 下は代表パスによる索引であり、全域の等価性ではない——ゆえに判断はコードに問う。**ただし写しが黙って腐ることはない**: `governance:check` の G-hook-fires が、代表パス列を `selectChecks` に食わせて検査 id 列と**順序込み**で一致すること、および**発行されうる id がどれかの行に現れる**ことを要求する（#863。同型のドリフトはルート `CLAUDE.md` の同じ表で一度起きており、その退去先がここである・#474〜#497）。

**書式が判定に効く**（崩すと赤になる）: 代表パス列はバッククォート括りの**実在する具体パス 1 件**（glob は書けない・実在も検査する）、**検査 id 列のバッククォートは検査 id だけ**（空集合は `（なし）` と綴る）、散文は補足列へ置く。表の走査は最初の空行までで、途中に表でない行があれば赤になる。**検査が 1 つも走らないパスの行を 1 本は置く**——それが無いと「沈黙は合格ではない」という主張だけが黙って消せる（id を持たない行は母集団照合に掛からないため）。

| 編集したファイル（代表パス） | 走る検査 id | 補足 |
|---|---|---|
| `snotra-core/src/lib.rs` | `fmt` `clippy` `core-test` | `.rs` は全域で fmt → clippy が走り、crate 配下ではその crate のテストが足される。**この順は報告の並びであって実行順の打ち切りではない**——検査ループは失敗しても break せず全部走る。fmt を先頭に置くのは証拠が先に目へ入るようにするため（#858） |
| `snotra-egui-runtime/src/lib.rs` | `fmt` `clippy` `egui-runtime-test` | |
| `snotra-settings/src/main.rs` | `fmt` `clippy` `settings-test` | |
| `src-tauri/src/main.rs` | `fmt` `clippy` `tauri-test` | 4 crate の外に置いた `.rs` は fmt → clippy だけになる（現在そのようなファイルは無い） |
| `src-tauri/tauri.conf.json` | `config-warn` | WARN（人間向け・Windows 互換の注意喚起）。`config.toml` も同じ経路だが、ランタイムのユーザー領域ファイルでリポジトリに実在しない |
| `src-tauri/Cargo.toml` | `cargo-check` | |
| `Cargo.toml` | `cargo-check` `hook-selftest` | **ルートだけは両方走る**——ワークスペース定義であると同時に「検査の定義を変えるファイル」でもあるから |
| `.claude/settings.json` | `hook-selftest` | `.claude/hooks/**` / `package.json` / `vitest.config.ts` も同じ |
| `.claude/lsp/snotra-rust-lsp/.lsp.json` | `hook-selftest` | `.claude/lsp/**` 全体。Claude Code の RA インスタンスへ渡す設定で、**設定が届かない・上書きされる壊れ方は沈黙する**（下節） |
| `rust-analyzer.toml` | `hook-selftest` | basename でアンカーするので crate 直下も拾う。同じカナリアの被検査対象（ratoml はクライアント設定より優先される） |
| `.githooks/pre-commit` | `githooks-selftest` | `.githooks/**` 全体 |
| `docs/hooks.md` | （なし） | 上記以外（`*.md`・`.claude/rules/**`・`.claude/skills/**`・`scripts/**` 等）は**検査が 1 つも走らない**——沈黙は「合格」ではない。`.md` には検査でない reminder が在るが（下記）、id を持たないのでこの列は空のままである |

**照合の外に残るものが 2 つある**（足の名指しと、なぜそこで止めたかは `docs/adr/ADR-hook-fires-table-check.md`）: 実在しないファイル（4 crate 外の `.rs`・`config.toml`）は代表パスにできないので補足列の散文だけが記述する。補足列そのものの意味整合も機構は見ない。

TS 型検査は #532 SU7 のフロント撤去で消滅した（`.ts` 編集は「検査はありません」の情報行のみ）。

**沈黙しうる経路は塞いである**（#471）。タイムアウト（検査ごと 300s）・出力溢れ・起動失敗・スクリプト内部エラーはいずれも必ず報告される（診断が予算を超えても、再現コマンドで全件を見られる）。**この閉塞を壊す変更を入れてはならない**——「沈黙 = 合格」という意味づけが成り立たなくなる。

## Claude Code の RA インスタンスと hook の分担

**この分担の正本はここである**（`.lsp.json` は JSON でコメントを持てないため、`.claude/hooks/lsp-config.mjs` の `//!` 相当のコメントがこの見出しを指す）。

Claude Code が起動する rust-analyzer は **semantic navigation の道具**であり、**検証の権威ではない**。確定判定は PostToolUse hook の `fmt` / `clippy` / crate test が持ち、その判定材料に LSP の状態（診断の到着順・quiescence）を混ぜない——非同期な状態を混ぜると「沈黙 = 合格」が成り立たなくなる。

| 層 | 担うもの |
|---|---|
| rust-analyzer（Claude Code） | findReferences / definition / implementation / hover / workspace symbols |
| PostToolUse（`post-edit.mjs`） | `cargo fmt` / `cargo clippy -D warnings` / 編集した crate の `cargo test` |
| CI | 最終保証 |

設定は `.claude/lsp/`（リポジトリ所有の project-scope plugin）が運び、`.claude/settings.json` の `extraKnownMarketplaces` + `enabledPlugins` で配送する。**VS Code 側の rust-analyzer は巻き込まない**——`rust-analyzer.toml` は両クライアントが読むため、そこには書かない。

**診断（diagnostics）は抑制していない。抑制する理由が無かったからである**（#1085 で実測）。エージェントへ届くのは**構文エラー**で、正常な編集では 0 件だった。ゆえに `.lsp.json` の `diagnostics` キーも RA 側の `diagnostics.enable` も置かない。**測った変異の範囲・却下した 2 層・受容する残余は `docs/adr/ADR-ra-diagnostics-suppression.md`「決定を支える実測」が正本。**

分担にとって効くのは 1 点である: 未リンクの `.rs`（`mod` 宣言を書き忘れたファイル）は cargo の視界に無いので hook は沈黙するが、**その構文エラーは LSP が届ける**。`mod` 忘れそのものはどちらも報せず、それを見るのは `governance:check` の `G-module-linkage` である（機序と残余は同検査の注釈が正本・#1085）。

**壊れ方は 2 つに分かれ、片方だけが沈黙する。** ここが分担の要である。

| 壊れ方 | 現れ方 |
|---|---|
| **設定が届かない・上書きされる**（抑制キーの消失・ratoml による上書き・宣言箇所の取り違え） | **沈黙する**——rust-analyzer は設定が無ければ既定値で普通に起動するので、navigation は動いたまま `checkOnSave` だけが復活する |
| **plugin の load 自体が失敗する**（trust 未受諾・マニフェスト不正・パス解決失敗・名前の不一致） | 沈黙しない——公式 plugin を無効化してあるため `.rs` の LSP が上がらず、**navigation が消える**形で現れる（ただしエラー自体は debug log にしか出ない） |

公式の `claude plugin validate --strict` は `.lsp.json` を視界に入れない（JSON として壊しても抑制キーを消しても exit 0・2026-08-14 実測）。ゆえに上段（沈黙する側）は `.claude/hooks/lsp-config.mjs` のカナリアだけが機械的に捕まえる（発火は上の一覧、故障注入の実測は `lsp-config.test.mjs`）。**このカナリアは `rust-analyzer.toml` を、生成物ディレクトリ（`target` / `node_modules` / `dist` 等）を除くツリー全体から読む**——local 水準の設定は crate 直下の ratoml でも効くため、発火（basename アンカー）と判定の母集団を揃えてある。

**残余は 2 つあり、どちらもリポジトリの外に原因がある。**

- **worktree は自分の設定ではなく、最初に登録したツリーの設定で動く**（2026-08-14 実測）。`known_marketplaces.json` はマシン全体で marketplace 名をキーに持ち、その installLocation が**最初に登録したツリーの絶対パスを指し続ける**。ゆえに worktree で `.claude/lsp/` を編集しても、そのセッションには効かない。**カナリアはそのツリーのファイルを読むので緑のまま**で、この乖離は検知できない。
  - 実測の形: worktree 側の `.lsp.json` のサーバ名だけを変えて起動したところ、登録されたのは**メインツリー側の名前**だった。宣言パスは両方に存在しており、**パスの不在は条件ではない**。
  - 一方、`.claude/lsp/` を持たない古い枝から作った worktree は**公式 plugin へ素直に落ちる**（project 設定がツリーごとに読まれるため）。LSP サーバはどのツリーでも常にちょうど 1 つで、二重に付くことはない。
- `.claude/settings.local.json`（gitignore 済み。実在検査は ignore 対象を免除するので参照してよい・#1088）は project より**優先順位が高い**ため、そこへ `enabledPlugins` を書けば plugin を無効化できる。カナリアは `.claude/settings.json` しか読まず、`selectChecks` もそのファイルに検査を割り当てていない。**リポジトリからは守れない**（現在そのキーは書かれていない）。

## 検査ではない reminder（発火一覧に現れない）

`selectChecks` が返す検査 id とは別に、`main()` が `warnings` へ直接積む reminder が在る。**exit code を動かさず、id を持たないので上の表にも現れない**（表の母集団照合は `checks.push("<id>")` の呼び出しだけを見る）。

| 発火条件 | 出るもの |
|---|---|
| `.rs` を **Write** した | モジュール索引の更新 reminder（#629/#630） |
| `.md` を編集し、**依存を持つ節の本文が変わった** | その節に依存する参照の一覧（#1140。判定は `scripts/governance/dependents.mjs` を subprocess で呼ぶ） |

**この reminder の不在は「依存が無い」を意味しない。** 純追記（行が足されただけ）では出ず、判定スクリプトが無いツリーでも出ない。**鳴ったときにだけ意味がある**——沈黙は検査のときと同じく「何も走らなかった」側である。

**判定を hook へ静的 import してはならない。** import 文は `try { main() } catch` の**外**で走るため、解決に失敗すると JSON エンベロープを出さずにプロセスごと落ちる——この hook は全 `Edit|Write` で発火するので、`.rs` の fmt / clippy / test まで含めて**全編集が沈黙する**。相対 import が下記の非対称（スクリプトの所在は `${CLAUDE_PROJECT_DIR}` 基準）に巻き込まれる問題も同時に避けられるので、subprocess で呼ぶ。

## PostToolUse（post-edit.mjs）の機構と保守

- **worktree でも「そのファイルが属するツリー」を検査する**。root は `file_path`（絶対パス）から最近接の `.git` を遡って導出するため、`${CLAUDE_PROJECT_DIR}` の意味論に依存しない。ただしスクリプト自身の所在は `settings.json` の `${CLAUDE_PROJECT_DIR:-.}` で解決し、相対 `file_path` を受け取った場合は cwd 基準で `resolve` する。
- **自己防護** — `.claude/settings.json` の編集は file watcher が即座に拾う（セッション再起動は不要・実測）。壊れたスクリプトを配線するとその瞬間から全検査が沈黙する。そのため `.claude/settings.json` と `.claude/hooks/**`、および検査の定義を変えるファイル（`package.json` / `vitest.config.ts` / ルートの `Cargo.toml`）の編集は `hook-selftest`（settings.json の JSON 検証 + `vitest run .claude/hooks`）を自動発火する。`.githooks/**`（main 保護の Layer 1）は同じ理由で `githooks-selftest` を発火する。
- **`selectChecks` に発火を足すときは、カナリアも対で足す**（#497）。カナリアの無いファイルに検査を実行しても vitest の起動しか証明しない（何も検証しない緑）。**守るのは沈黙する経路だけでよい** — 放っておいても実行時に明示的に失敗するものに見張りは不要（#500）。
- **`config.toml` の WARN 真陽性は事実上 `tauri.conf.json` のみ**（`config.toml` はランタイムのユーザー領域ファイルでリポジトリに実在しない）。その `src-tauri/tauri.conf.json` では WARN（人間向け・Windows 互換の注意喚起）のみが出る（旧 `csp-test` は #532 SU7 のフロント撤去で CSP ごと消滅）。
