# 調査: #1298 /implement の委譲から A〜F の実行を外し、worktree の切り出しをスクリプトへ移す

## issue の要約

`/implement` Step 3b は検証 A〜F・レビュー・変異注入を worktree の 1 体へ束ねている（#1137）。#1137 以降 20 PR・48 巡の実測では、A〜F の exit≠0 は 0 件で、委譲の価値はレビューと変異注入から出ていた。初巡の worktree HEAD は 20/20 PR でアンカーと食い違い、防御は SKILL の指示文にしか無い。

issue の提案は次のとおり。
- sha を受けて `git worktree add --detach <sha>` で木を切り、A〜F を回し、カテゴリごとの exit code の要約を 1 ファイルへ書く道具を `scripts/` に置く
- 権威ある A〜F は、主エージェントがその道具を呼んで回す
- 委譲先はレビューと変異注入に絞り、道具で切った木を渡す
- 委譲先の出力先は worktree の外にする
- ルート `CLAUDE.md`「サブエージェント委譲と worktree」のうち不要になる条項を特定する

未確定として 2 点が挙がっている（変異注入の木／「報告が別の版へ向く」の保ち方）。比べ方は、マージ後の 10 PR で所見件数と High 採用件数が落ちないかを見ることである。

## 関連ファイル・シンボル（実在を grep で確認済み）

| 対象 | 所在 | 今回の関わり |
|---|---|---|
| `/implement` Step 3 本文 | `.claude/skills/implement/SKILL.md:80-125` | 書き換えの本体。`:82` は「A〜F もレビューも主の作業ツリーでは行わない」、`:84` は束ねる理由、`:98` は `isolation: "worktree"` で 1 体を起動、`:101` は HEAD 合わせ、`:102` は A〜F を委譲先へ渡す、`:113-` は 3c の報告後の手順 |
| `/implement` 出力 | `.claude/skills/implement/SKILL.md:138,146` | 「検証（カテゴリ A〜F 全件）」の欄。母集団の宣言が委譲に依存している |
| 却下の記録 | `docs/adr/ADR-implement-verification-delegated-to-worktree.md` | **「レビューだけ委譲し、検証は主エージェントが主ツリーで回す」を却下済み。** 理由は (a) 重複 (b) 検証とレビューが別の版へ向きうる（主ツリーは動く）。`CARGO_TARGET_DIR` の共有も却下（速い経路の warm さを優先）。パイプライン化も却下（報告の sha == HEAD を保つため） |
| A〜F の SSOT | `docs/build-commands.md:7-` | A は `.rs` 変更時で、crate ごとに条件つきの test 行がある。B は記述のみでコマンドが無い。C は「ウィンドウ生成／表示順・ホットキー・スラッシュコマンドに触れた」で、意味による判定。D は表示の見た目で、`smoke:manual` は人間専用、`check:colors` は GUI 自動。E は `.githooks/**`・`.claude/hooks/**`・`scripts/**` で `npm test`（`scripts/lib/**` の PS なら `test:powershell` も）。F はガバナンス文書で `governance:check` |
| コマンド実在の検査 | `scripts/governance/checks/G-build-commands.mjs` | `docs/build-commands.md` の `npm run X` が `package.json` に在るか、`cargo test -p` の crate が在るかを見る |
| worktree の掃除 | `scripts/clean-worktrees.mjs`・`package.json` `clean:worktrees` | 対象は `.claude/worktrees/` 配下で basename が `agent-` で始まるもの**だけ**。git 自身に列挙させる流儀 |
| `.gitignore:21` | Agent worktree の無視 | `.claude/worktrees/` を無視している |
| vitest の母集団 | `vitest.config.ts` | `scripts/**/*.test.mjs` を含む。新しいスクリプトのテストは自動で `npm test` に入る |
| 委譲の規範 | ルート `CLAUDE.md`「サブエージェント委譲と worktree」 | 「検査対象を変更しながら検査を走らせない」「成果物は呼び出し側が指定したパスへ」「長時間の委譲は中断を前提」など |
| 索引の写し | ルート `CLAUDE.md:65` | 「`/implement`「3b. 委譲へ渡すもの」が worktree で起動し、コミット済みの sha を対象にする（検証も同じ委譲が担う）」——今回の変更で偽になる |
| 計画側の写し | `.claude/skills/start-issue/SKILL.md:82` | 「変異注入は `/implement`「3b. 委譲へ渡すもの」の委譲先が行う」——変異注入を委譲に残すなら真のまま |
| ワークフロー | `AGENTS.md:49` | 「該当するカテゴリ A〜F をすべて実行」——誰が実行するかは書いていないので、中立のまま |
| レビュアの差分取得 | `.claude/agents/code-reviewer.md:11` | `git diff main...<sha>`。木の所在には依存しない |
| node_modules の解決 | `.claude/rules/safety-nets.md`（#1256 で圧縮）と圧縮前の文（`git show 190a2798`） | 本体の内側の worktree では Node／npm が親を遡って解決する（2026-08-26 実測）。junction を張ると `git worktree remove` が消し切れない（実測） |

## 再利用できる既存パターン

- `scripts/clean-worktrees.mjs`: `execFileSync("git", …)` と `git worktree list --porcelain` で列挙し、成果保全を git 自身の拒否に委ねる形
- `scripts/race-boundaries.mjs` と `.test.mjs`: `scripts/` の Node 道具にテストを添える形（`npm test` に自動で入る）
- `.githooks/githooks.test.mjs`: 使い捨て repo で git 操作を実測するテストの手本（worktree の作成も含む）
- `/start-issue` 3b・`/plan-review` Step 2: 出力先を呼び出し側が絶対パスで指定する規約

## 技術的制約

1. **カテゴリの該当判定は機械的に決まらない。** C と D は「変更が何をするか」で決まる（`.claude/rules/src-tauri.md`・#558）。A の `cargo test -p` の行は「どの crate を変更したか」で決まる。E と F はパスで決まる。ゆえに道具が自分で決めてよいのはパスで決まる部分だけで、C と D の該当は主が渡す引数になる
2. **コマンド文字列の SSOT は `docs/build-commands.md` である**（`/implement` SKILL `:102`・`AGENTS.md` 手順 6）。道具がコマンドを持つと 2 枚目の写しになる。対処は 2 案ある。(a) 道具が `docs/build-commands.md` のコードブロックを読む (b) 正本を道具へ移し、文書は道具を指す
3. **D は道具で完結しない。** `smoke:manual` は `Read-Host` を使うので人間専用。`check:colors` は GUI を要し、画面ロック中は走らない。C の smoke は先に `cargo build -p snotra` が要る（`cargo test` では本体が更新されない）
4. **worktree の置き場は本体の内側（`.claude/worktrees/`）にする。** node_modules を張らずに `npm test` が通る。ただし `clean-worktrees.mjs` が拾うのは `agent-*` だけである
5. **新しい worktree の `target/` は cold である。** 実測は `cargo build -p snotra` が 2m59s、`--release` が 5m38s、`cargo check --workspace` が 1m39s（#1179・#1185 の委譲報告。ADR の「6 分級」には出所が無い・敵対的調査 [3a]）。同じ木を巡をまたいで再利用し、`checkout --detach <新 sha>` すれば warm になる
6. **道具の A〜F と委譲先の変異注入を同じ木で同時に走らせると衝突する。** 変異は木を一時的に壊すので、A〜F の結果が変異後の木を測りうる。木を 2 本にする（cold ビルドが 2 回）か、直列にする（待ちが延びる）か、どちらかになる
7. **ADR が却下した形との距離。** 却下されたのは「主ツリーで回す」であって、「sha に固定した別の木で主が回す」ではない。主ツリーが動く問題は sha 固定で消える。残る論点は「2 つの報告が同じ sha を名乗るか」だけで、これは issue の未確定 2 と一致する。変更するなら、却下した形と今回の差を新しい ADR として残す必要がある（ADR は凍結された歴史で、既存の ADR を書き換えない）
8. **安全網の変更である。** `.claude/skills/**`・ルート `CLAUDE.md` は規範であり、`scripts/**` は CI が検査する安全網である（ルート `CLAUDE.md` 最重要ルール 2）。issue の承認と計画の人間承認で合意を取る
9. **「比べ方」（次の 10 PR）はこの PR の中では測れない。** 計画の作業項目に置けないので、PR 本文か別の issue へ送る
10. **長時間の実行。** cold で 6 分級なので、主は道具を `run_in_background` で回すことになる。完了通知を受けてから要約を読む形と、3c の「報告が届くまで作業ツリーに触らない」との関係を決める必要がある（主ツリーには触らないが、HEAD を先へ進めると「報告の sha == HEAD」が崩れる）

## 未解決の疑問

- Q1: 道具が回すカテゴリは誰が決めるか。主が引数で渡すか、道具が差分のパスから A・E・F を導き、C・D だけ主が足すか
- Q2: コマンドの正本を文書に残すか、道具へ移すか（制約 2）
- Q3: 木を 1 本にするか 2 本にするか（制約 6）。2 本なら cold ビルドの費用、1 本なら待ちの延び
- Q4: 委譲先を `isolation: "worktree"` なしで起動し、道具の木の絶対パスを渡す形で、Agent ツールの制約に触れないか（サブエージェントが別ディレクトリの木で Bash / Read を使えるか、hook の cwd 前提）
- Q5: 道具が作る worktree の名前と掃除。`agent-*` に揃えて `clean:worktrees` に拾わせるか、別の接頭辞にして `clean-worktrees.mjs` を広げるか
- Q6: ルート `CLAUDE.md` のどの条項が不要になるか。少なくとも `:65` の索引文は偽になる
- Q7: 「A〜F の exit≠0 が 0 件」は「委譲で A〜F を回す価値が 0」を意味するか。主の事前確認が A〜F の全部を覆っていないなら（`cargo doc`・`governance:check`・smoke など）、それらを権威ある一式として回す役は道具に移るだけで、消えるわけではない

## 主エージェントによる追加の実測（2026-10-25・敵対的調査と並行）

使い捨ての worktree（`probe-1298`・`probe2-1298`）と計測用のサブエージェント 3 体（haiku）で測った。木とファイルはすべて撤去済みで、`git worktree list` は本体だけを返す。

| # | 計測 | 結果 |
|---|---|---|
| M1 | 主セッションの Bash で `cd <木>` → 次の呼び出しで `pwd` | **cwd は残る**。セッションの primary working directory まで木へ移った |
| M2 | 木の中で `cargo metadata` | `target_directory` = `<木>/target`。`node_modules` は無いが `require.resolve("vitest/package.json")` は本体の `node_modules` へ解決する |
| M3 | 主セッションの cwd が木の中にある状態で `git worktree remove` | 登録とファイルは消えるが、ディレクトリ本体の削除が Permission denied になった。cwd を外へ出してから `rmdir` すると消えた |
| M4 | isolation 無しのサブエージェントで `cd <木> && pwd` → 別の呼び出しで `pwd` | **cwd は呼び出しごとに本体（`/c/workspace/Snotra`）へ戻る**。system prompt に逐語で "Agent threads always have their cwd reset between bash calls" とある |
| M5 | isolation 無しのサブエージェントから `EnterWorktree(path=<既存の木>)` | **拒否**:「the current working directory … is the repository root, not an isolated worktree」 |
| M6 | `isolation: "worktree"` のサブエージェントから `EnterWorktree(path=<別の既存の木>)` | **拒否**:「would not give this agent write access there … its writes are limited to that folder」 |
| M7 | `isolation: "worktree"` のサブエージェントが木の外へ書く | Write ツールは本体のパスを**拒否**、session の scratchpad は許可。**Bash のリダイレクトは本体のパスへも通った**（非対称・保証ではない） |
| M8 | `isolation: "worktree"` のサブエージェントが gitignore 済みのファイル（`target/reports/x`）だけを作って停止 | 停止後、**木とブランチ `worktree-agent-*` が消えた**（harness の「変更が無ければ自動で片づける」）。M6 の木も同様に消えた。そのあと `SendMessage` で再開した委譲先は、primary working directory が本体になっていた |
| M9 | `isolation: "worktree"` のサブエージェントが `.delegate/probe.txt`（未追跡・ignore されない）を作って停止 → `SendMessage` で再開して `pwd` | **木とブランチは残った。** 再開後の cwd も toplevel も自分の木（`agent-a555…`）だった。そのあと `.delegate/` を消し、`git worktree remove`（`--force` なし）と `git branch -d` を打つと、どちらも成功した（`--delegate-tree` の予行）。再開できる状態のエージェントが木を握っていても、削除は止まらなかった |

M8 からの帰結は 2 つある（M9 が 1 つ目の機序を実測で確かめた）。
- #1155・#1179 で木が巡をまたいで残ったのは、**未追跡の報告ファイルが木を「変更あり」にしていたから**だと読める
- issue の「委譲先の出力先を worktree の外にする」をそのまま入れると、木は毎巡の終わりに片づけられる。**2 巡目の委譲先は本体の作業ツリーで動く**（変異注入が本体を壊す）

## 敵対的調査（`workspace/adversarial-1298.txt`）の採否

| 所見 | 判定 | 採否と理由 |
|---|---|---|
| [1] junction 無しの `npm test` は通る（25 files・612 tests） | 壊せなかった | **採る。** M2 と一致する。⚠️ 依存を変える PR では、本体の古い `node_modules` を解決しうる。道具はこの場合に警告を出す（`package.json`／`package-lock.json` が sha と本体で違うとき）。測定は計画の未確定へ回さない（警告で足りる） |
| [2] ADR の却下文は「主ツリーで回す」で、今回案には文面上当たらない。ただし (a) 重複は残る | 一部壊せた | **採る。** 権威ある A〜F の回数は「主 + 委譲」から「主 + 道具」へ変わるだけで減らない。木が 2 本になれば cold ビルドの費用は**増える**。issue の利点は費用ではなく、HEAD 合わせの構造化と委譲の焦点化にあると書き直す（新しい ADR へ） |
| [3a] 「6 分級」に出所が無い | 壊せた | **採る。** 実測は `cargo build -p snotra` が 2m59s、`--release` が 5m38s、`cargo check --workspace` が 1m39s（#1179・#1185 の報告）。research の制約 5 を差し替える |
| [3b] 木は SendMessage をまたいで残る（#1155 は 5 巡、#1179 は 4 巡） | 壊せた | **採る。** ただし機序は M8 で裁定した。残ったのは未追跡の報告があったからである。採るのは「残った」という所見で、「harness が残す」とは読まない |
| [3c] 巡 2 以降の checkout が、前巡の未追跡報告で abort した（#1155） | 壊せた | **採る。** 報告の置き場は「木の中で、どのコミットにも現れないパス」にする。こうすれば M8 の自動片づけを防ぎつつ checkout とも衝突しない |
| [3d] CPU 競合による偽の赤が少なくとも 2 件（#1154・#1179、どちらも委譲先自身の並走） | 壊せた | **採る。** 道具を委譲先と並走させるなら、道具の赤は委譲の報告が届いたあとに単独で 1 回だけ再実行してから確定する |
| [3e] smoke は木を分けても並走できない（`SnotraSmoke.psm1:543` が既存のプロセスで throw） | 壊せた | **採る**（コードで確かめた）。C を要求する巡は、道具を先に完走させてから委譲を起動する（直列） |
| [4] A の crate 行はパスで決まる。E と F にも意味判断が混じる。E と F は 8 秒と 0.68 秒で安い | 一部壊せた | **採る。** 道具は E と F を常に回す。A は CI と同じく全 crate のテストを回す（`docs/build-commands.md:26`）。C の該当は主が渡す。制約 1 を書き直す |
| [5] 漏れ: `code-reviewer.md:11-12` の worktree 前提、`scripts/race-boundaries.mjs:15` の 3a 見出し参照、`allowed-tools` に node と pwsh が無い | 壊せた | **採る**（3 点とも grep で確かめた）。ただし委譲先は `isolation: "worktree"` のまま残すので（M4〜M6）、`code-reviewer.md:11-12` は真のまま。道具は `npm run` で呼ぶ（`Bash(npm *)` で足りる）。3a と 3b の見出しは改名しない |
| [6] isolation 無しの委譲先は cwd を取り違える | 壊せた（⚠️ だったものを M4 で確定） | **採る。** 機序は M4 で実測した。issue の「委譲先は木を作らない（道具で切った木を渡す）」は成り立たない（M4〜M6）。委譲先は harness の木に残り、HEAD 合わせだけを道具のサブコマンドで行う |
| [6'] 道具が出す木のパスは `/` 区切りにする（pre-bash の `usesBackslashPath`） | 壊せた | **採る。** |
| [B] 「A〜F の 0 件」のうち C が走ったのは 3 PR だけで、D は全件が実行不能 | 壊せた（解釈） | **採る。** 0 件は道具化の根拠にも委譲維持の根拠にもならない。道具化の根拠は HEAD 不一致 20/20 と未追跡報告の衝突 5 PR に絞る |
| 復元の罠: `review.md` 116 本は同一の古いファイル | 壊せた | **採る。** issue の「所見 247」は `verify-*.txt` 由来で、影響は無い（issue の数字は変えない） |
