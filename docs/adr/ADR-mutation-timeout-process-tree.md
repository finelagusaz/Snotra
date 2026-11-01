# ADR-mutation-timeout-process-tree: 変異の timeout は、ルートプロセスが生きているうちにプロセスツリーごと止め、控えた子孫の組で照合する

## 文脈

#1352: 別セッションの変異スクリプトが `spawnSync("npx", ["vitest", …], { shell: true, timeout })` で vitest を回していた。無限ループする変異で timeout が発火したが、止まったのは直下の `cmd.exe` だけだった。npx → `cmd` → vitest → ワーカーの木は孤児として残り、1 コアを使い続けた。変異スクリプトは各セッションが scratchpad に書くので、規範は `.claude/rules/safety-nets.md` に置いた。採った手順はその条項が持つ。

## 検討した代替案と却下理由

- **timeout の後に、戻り値の pid へ `taskkill /T /F` を打つ（issue の当初案）**: 却下。`spawnSync` が返る時点で根の `cmd.exe` は既に死んでいる。`ERROR: The process "<pid>" not found.` で空振りし、4 つの孤児が残った。根が消えると子孫も辿れない（2026-11-01 実測）。
- **`shell: false` で `process.execPath` と `vitest.mjs` を直接起動することを主にする**: 却下。この形では、vitest 本体を止めるとワーカーも消え、孤児は 0 だった。しかし `node>node>cmd>spin` の鎖では spin が残った。安全かどうかは、鎖に `cmd.exe` が挟まらないという起動先の実装に依存する。変異スクリプトの側からは保証できない。
- **孤児の確認を、変異を当てた複製のパスで絞る**: 却下。ワーカーの `CommandLine` は junction の先の本体（`C:/workspace/Snotra/node_modules/vitest/dist/workers/forks.js`）を指しており、複製のパスを含まなかった。CPU を食う当のワーカーを取りこぼす。
- **孤児の確認を「開始後に作られ、`CommandLine` に `vitest` を含む」で絞る**: 却下。並走する正当な vitest（道具のカテゴリ E、並行セッション）を孤児と見誤る。Node から PowerShell を呼んで確かめると、クエリの文字列が `CommandLine` に載り、確かめる側の自分自身が一致した。
- **`ParentProcessId` が現存しないことで孤児を判定する**: 却下。Windows は親を付け替えず、PID は再利用される。
- **控えた後に生まれた子孫や、辿るときの PID の再利用まで照合で塞ぐ**（控えを 2 回取る・子の `CreationDate` ≥ 親）: 採らなかった。どちらも未測定である。控えてから止めるまでの窓は短く、`taskkill /T` は生きている親の鎖を止める。手順を重くする費用に見合わない。

機序（libuv の job object が孫を外す）は未裁定であり、条項に入れていない。

## 帰結

- `.claude/rules/safety-nets.md`「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」に 1 項目を足した。`.claude/skills/implement/SKILL.md` の変異注入の行は、その節名を指す。
- 実測の前提は Windows 11・Node v24.18.0・vitest 5.0.5 である。Node の版で `spawnSync` の timeout の挙動が変われば、却下の 1 つ目を測り直す。

---

status: Accepted
関連: #1352
