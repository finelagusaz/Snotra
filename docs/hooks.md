# フックの実装契約と保守

このリポジトリの Claude Code フック（PreToolUse = `.claude/hooks/pre-bash.mjs` の 1 本だけ）を**改修**するときの実装契約・機構・保守規律。編集後の自動検証（PostToolUse）は 2026-09-29 に撤去した——検証は手動（`docs/build-commands.md`「変更後の検証チェックリスト」）と CI が持つ。

- エージェントが日常操作でフックにどう**応答するか**・沈黙をどう**読むか**は、常時ロードの `CLAUDE.md`「フック」節が SSOT。本ファイルはそこから退去させた**一覧と内訳**も併せ持つ。
- 設計哲学（検出は構造化信号で行い、fail-closed を既定値に埋める）は `docs/development-principles.md`「構造的設計原則と強制の階梯」が SSOT。本ファイルはそのフック具体化＝運用 specifics を持つ。
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
- **受容する未対応リスク**（いずれも fail-closed の設計方針の下で意図的に残す。**「検出されないなら使ってよい」ではない** — 検出されない形も規範に反するなら人間専用の意図的迂回であり、上の `sh -c` 項と同格に扱う）:
  - **区切りの走査はシェルの構文を理解しない**ので、区切り文字が構文の内側にあるとセグメントが早く切れて見落とす: 引用内（`git commit -m "a;b" --no-verify`）と**行継続をまたぐ形**（`git commit \` + 改行 + `--no-verify`。PowerShell のバッククォート継続も同型）。引用・継続を解釈する分割は shell パーサ相当になり、payload 全体 grep の誤爆を作り直す。`segmentEnd` は `hasSafeChain` と共有されているため、継続の扱いを変えると `gh pr create` ゲートの意味も動く。
  - **コマンド文字列に現れない非 ASCII は見えない**（`python foo.py` でスクリプト側が出す形）。コマンドの形からは判定材料が無い。
  - **`\` 判定は上表の 3 形しか見ない**ので、`.\scripts\x.ps1` のような相対形も接頭辞を持たない `docs\hooks.md` も検出されない（`.\` は PowerShell では動くため誤爆の代償が大きく、形を絞ったことの代償でもある）——**検出しないだけで、規範としてはコマンドに書くパスの区切りを `/` にする**。
  - **`pull` はブランチを見ない**ので `git pull --rebase` も feature ブランチでの pull も止まる（過剰検出）。ブランチ判定は `readGitState` の責務を広げるため採らない。
  - **`.githooks/_lib.sh` は拒否メッセージで `--no-verify` による迂回を案内する。** 明示的に「人間専用。エージェントは使用禁止」と書いてあるため矛盾ではない（この hook が拒むのはエージェントの実行であり、人間の判断を妨げない）。

## Claude Code の RA インスタンスと hook の分担

**この分担の正本はここである**（`.lsp.json` は JSON でコメントを持てないため、`.claude/hooks/lsp-config.mjs` の `//!` 相当のコメントがこの見出しを指す）。

Claude Code が起動する rust-analyzer は **semantic navigation の道具**であり、**検証の権威ではない**。確定判定は `fmt` / `clippy` / crate test（手動実行と CI）が持ち、その判定材料に LSP の状態（診断の到着順・quiescence）を混ぜない。

| 層 | 担うもの |
|---|---|
| rust-analyzer（Claude Code） | findReferences / definition / implementation / hover / workspace symbols |
| 手動の検証（`docs/build-commands.md` カテゴリ A） | `cargo fmt` / `cargo clippy -D warnings` / 編集した crate の `cargo test` |
| CI | 最終保証 |

設定は `.claude/lsp/`（リポジトリ所有の project-scope plugin）が運び、`.claude/settings.json` の `extraKnownMarketplaces` + `enabledPlugins` で配送する。**VS Code 側の rust-analyzer は巻き込まない**——`rust-analyzer.toml` は両クライアントが読むため、そこには書かない。

**診断（diagnostics）は抑制していない。抑制する理由が無かったからである**（#1085 で実測）。エージェントへ届くのは**構文エラー**で、正常な編集では 0 件だった。ゆえに `.lsp.json` の `diagnostics` キーも RA 側の `diagnostics.enable` も置かない。**測った変異の範囲・却下した 2 層・受容する残余は `docs/adr/ADR-ra-diagnostics-suppression.md`「決定を支える実測」が正本。**

分担にとって効くのは 1 点である: 未リンクの `.rs`（`mod` 宣言を書き忘れたファイル）は cargo の視界に無いので cargo は沈黙するが、**その構文エラーは LSP が届ける**。`mod` 忘れそのものはどちらも報せず、それを見るのは `governance:check` の `G-module-linkage` である（機序と残余は同検査の注釈が正本・#1085）。

**壊れ方は 2 つに分かれ、片方だけが沈黙する。** ここが分担の要である。

| 壊れ方 | 現れ方 |
|---|---|
| **設定が届かない・上書きされる**（抑制キーの消失・ratoml による上書き・宣言箇所の取り違え） | **沈黙する**——rust-analyzer は設定が無ければ既定値で普通に起動するので、navigation は動いたまま `checkOnSave` だけが復活する |
| **rust-analyzer のバイナリが無い**（toolchain の入れ替え・pin の変更で component が落ちる） | **沈黙する**——cargo / clippy / test は緑のまま、`LSP` ツールだけが `crashed with exit code 1` を返す（#1239 実測。2026-08-24 に新しい toolchain が実体化したときは 11 時間気づかれなかった・#1177。診断は `checkOnSave: false` で切ってあるので plugin の使用カウンタの 0 も報せない）。守り手は `rust-toolchain.toml` の `components` の宣言で、`lsp-config.mjs` が見るのは**宣言が在ること**まで——実際に入っているかは射程の外（runner には入っていないので環境の実測は置けない） |
| **plugin の load 自体が失敗する**（trust 未受諾・マニフェスト不正・パス解決失敗・名前の不一致） | 沈黙しない——公式 plugin を無効化してあるため `.rs` の LSP が上がらず、**navigation が消える**形で現れる（ただしエラー自体は debug log にしか出ない） |

公式の `claude plugin validate --strict` は `.lsp.json` を視界に入れない（JSON として壊しても抑制キーを消しても exit 0・2026-08-14 実測）。ゆえに上段（沈黙する側）は `.claude/hooks/lsp-config.mjs` のカナリアだけが機械的に捕まえる（発火は上の一覧、故障注入の実測は `lsp-config.test.mjs`）。**このカナリアは `rust-analyzer.toml` を、生成物ディレクトリ（`target` / `node_modules` / `dist` 等）を除くツリー全体から読む**——local 水準の設定は crate 直下の ratoml でも効くため、発火（basename アンカー）と判定の母集団を揃えてある。

**残余は 2 つあり、どちらもリポジトリの外に原因がある。**

- **worktree は自分の設定ではなく、最初に登録したツリーの設定で動く**（2026-08-14 実測）。`known_marketplaces.json` はマシン全体で marketplace 名をキーに持ち、その installLocation が**最初に登録したツリーの絶対パスを指し続ける**。ゆえに worktree で `.claude/lsp/` を編集しても、そのセッションには効かない。**カナリアはそのツリーのファイルを読むので緑のまま**で、この乖離は検知できない。
  - 実測の形: worktree 側の `.lsp.json` のサーバ名だけを変えて起動したところ、登録されたのは**メインツリー側の名前**だった。宣言パスは両方に存在しており、**パスの不在は条件ではない**。
  - 一方、`.claude/lsp/` を持たない古い枝から作った worktree は**公式 plugin へ素直に落ちる**（project 設定がツリーごとに読まれるため）。LSP サーバはどのツリーでも常にちょうど 1 つで、二重に付くことはない。
- `.claude/settings.local.json`（gitignore 済み。実在検査は ignore 対象を免除するので参照してよい・#1088）は project より**優先順位が高い**ため、そこへ `enabledPlugins` を書けば plugin を無効化できる。カナリアは `.claude/settings.json` しか読まない。**リポジトリからは守れない**（現在そのキーは書かれていない）。
