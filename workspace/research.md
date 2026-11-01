# 調査 — issue #1352

## issue の要約

別セッションの scratchpad の変異スクリプトが `spawnSync("npx", ["vitest", …], { shell: true, timeout: 300_000 })` で vitest を回していた。ある変異で判定が無限ループし、timeout の後も `npx` → `cmd` → vitest → ワーカーの木が孤児として残り、ワーカーが 1 コアを約 6.7 時間使い続けた。issue の提案は、再現で機序を確かめたうえで `.claude/rules/safety-nets.md`「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」へ 1 行足すこと。変異の道具をリポジトリへ常設することは範囲外。

## 再現（2026-11-01・Node v24.18.0・vitest 5.0.5・Windows 11）

足場: `C:/Users/Eoh/AppData/Local/Temp/claude/C--workspace-Snotra/1db319dc-6a4f-479a-8b00-499343a4843f/scratchpad/repro1352/`（`for (;;) {}` だけのテスト 1 本・`node_modules` は本体への junction・駆動は `drive.mjs`（`spawnSync`）と `drive2.mjs`（`spawn` + 途中のスナップショット））。

| 形 | 止め方 | 止めた後に残ったもの |
|---|---|---|
| `spawnSync("npx", ["vitest","run"], { shell: true, timeout: 15_000 })` | timeout（`ETIMEDOUT`・`SIGTERM`） | 直下の `cmd.exe` だけが消え、npx の node → `cmd /d /s /c vitest run` → vitest 本体 → ワーカーの 4 つが残った。ワーカーは 5 秒の観測で CPU 累計が 5 秒増えた（1 コアを使い切る） |
| `spawn` の同じ形 + `child.kill("SIGTERM")` | 同上を非同期で | 同じ 4 つが残り、**駆動スクリプトが終了した後も残った** |
| `spawn` の同じ形 + `taskkill /PID <child> /T /F` | 木ごと | 何も残らなかった |
| `spawn(process.execPath, ["node_modules/vitest/vitest.mjs","run"], { shell: false })` + `kill` | 直下だけ | 何も残らなかった（実行中はワーカーが CPU 12 秒を使っていた＝回っていた） |
| 上の形に `detached: true` | 直下だけ | 何も残らなかった |

結論: issue の推定「timeout が止めたのは直下のシェルだけ」は再現した。ただし「Windows では子を止めても孫は止まらない」は**全称としては偽**である。node → node の直接の鎖（vitest 本体 → ワーカー）では、親の vitest を止めるとワーカーも消えた。残るのは、鎖の途中に `cmd.exe`（`shell: true` と `npx.cmd` が挟む）が居るときである。

機序の仮説（未裁定・規範へは入れない）: libuv は Windows で、子を `KILL_ON_JOB_CLOSE` の job object へ入れるが `SILENT_BREAKAWAY_OK` も立てるので、孫は job から外れる。node が自分の直接の子だけを道連れにする、という観測とは整合する。ただし一次資料（`deps/uv/src/win/process.c`）は読んでいない。

再現していないこと: issue の変異（`GIT_AT_CMD` / `AT_CMD_POS` の書き換え）が無限ループを起こしたこと自体。合成した `for (;;)` で孤児の機序だけを再現した。規範は timeout の後始末について書き、その正規表現には触れない。

## 関連ファイル

- `.claude/rules/safety-nets.md` — 足す先。節「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」（4 行）。フロントマターの `paths` は `.claude/hooks/**`・`.githooks/**`・`.claude/settings.json`・`.claude/lsp/**`・`.github/workflows/**`・`.claude/rules/**`・`.claude/skills/**`・`scripts/**`
- `.claude/skills/implement/SKILL.md:111` — 「3b. 委譲へ渡すもの」の変異注入の行。すでに「timeout とモジュール解決の失敗は検知に数えない」と書いてある。後始末には触れていない
- 節名を逐語で引く箇所: 正本は `git grep 稼働中のガードを弱めない`（15 ファイル・B3）。見出しは変えないので影響なし

## 再利用できる既存パターン

- rules の条項の書式: `A のときは B する。C が起きたため（#NNN 実測）`（`AGENTS.md`「ドキュメント参照」）
- 同じ節の既存の行は、手順 1 文に #番号を添えた形が多い（4 本中 3 本・B4）

## 技術的制約

- Windows の Node 24 は `.cmd` を `shell: false` で起動できない（`npx` は `npx.cmd`）。`shell: false` にするには `process.execPath` と `node_modules/vitest/vitest.mjs` を直接渡す必要がある
- セーフティネット（rules）の変更なので、PR 本文の冒頭に「マージは合意してから」と書く（ルート `CLAUDE.md`）
- 見出しは変えない。`npm run governance:check` は回す

## 配送の経路

この rule が配送されるのは、`paths` に当たるファイルを読み書きしたときである。scratchpad のスクリプト自体は当たらない。ただし変異の対象（今回は `.claude/hooks/pre-bash.mjs`）を読めば当たる。変異を選ぶには、対象を読むことになる。`/implement` 3b の委譲先が worktree で変異を当てる場合も、対象がセーフティネットのファイルなら当たる。

当たらないのは、セーフティネット以外（Rust のコード等）への変異である。ただし、そこでの駆動は `cargo test` である。`npx` の `cmd.exe` が挟まる形にはならない（Rust 側の木が残るかは測っていない）。

## 敵対的調査の所見と採否（`workspace/adversarial-1352.txt`）

壊せた項目:

- **B1 採る（主が再測で確認）**: `spawnSync` の timeout の後に、戻り値の pid へ `taskkill /PID <pid> /T /F` を打つと、`ERROR: The process "13868" not found.` で空振りし、4 つの孤児が残った（`drive4.mjs`）。timeout がすでに根の `cmd.exe` を殺しているからである。issue の案「timeout で止めたあとに木ごと止める」は、`spawnSync` では実行できない。条文は「根が生きているうちに木ごと止める」へ変える。`spawn` と自前のタイマーを使い、`kill` の代わりに `taskkill /T /F` を打つ。確認の手順は「子孫 0」だけでは足りない。根が消えると子孫は辿れないので、「親が死んだ孤児が残っていない」まで確かめる
- **B2 採る（所見のみ）**: `node>node>cmd>spin` でも spin が残った。害を決めるのは、鎖のどこかに `cmd.exe` が挟まることである。機序の仮説（job object）は未裁定のまま、規範へ入れない
- **B3 採る**: 節名を逐語で引く箇所は 3 つではなく、`git grep` の結果として 15 ファイルある。見出しを変えないので影響なしという結論は保たれる。列挙はやめ、`git grep 稼働中のガードを弱めない` を正本にする
- **B4 採る**: 節の 3 本目の行には #番号が無い。書式の主張を弱める
- **B5 採る**: 配送は、リポジトリ内の原本を読んだときに当たる。複製だけを読み書きした場合は当たらない。委譲先に届くかは、リポジトリの内側からは決まらない。そのため経路 B（`/implement` SKILL.md:111 に正本を指す一言）を計画へ入れる。この行は、主が委譲先へ渡す本文の一部なので、配送とは独立に届く

壊せなかった項目: 命題 1（孤児の 4 つが駆動の終了後も残る）、計器の疑い（ワーカーの CommandLine は `*vitest*` に当たる。CreationDate で全 node/cmd を列挙しても direct 形は 0）、命題 2 の中核（`spawnSync(shell:false, timeout)` でも残存 0）、命題 5（`npx`→ENOENT、`.cmd`→EINVAL。`node npx-cli.js` でも npx が内部で `cmd` を挟む）、SKILL.md:111・節の 4 本・`AT_CMD_POS`/`GIT_AT_CMD` の実在。

⚠️ 未測定: 駆動側が node 以外の場合、本物の変異（破滅的バックトラック）での再現、CPU 競合下。いずれも条文の手順（根が生きているうちに木ごと止め、孤児 0 を確かめる）を変えない。

## 未解決の疑問

- 経路 B: `/implement` SKILL.md:111 にも、正本（safety-nets.md）を指す一言を置くか。置けば、変異の対象がセーフティネットでない委譲にも届く。ただし、写しが 1 つ増える
- 規範に「`shell: false` + 直接起動」を代替として書くか。実測では、木が残らなかった。しかし、それはワーカーが node の直接の子であるという vitest の実装に依存する。そのため、後始末の手順（木ごと止めて、子孫ゼロを確かめる）を主にする
