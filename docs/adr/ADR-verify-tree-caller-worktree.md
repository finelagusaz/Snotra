# ADR-verify-tree-caller-worktree: verify:tree と PR 前ゲートの置き場を本体の下に、照合の相手を呼び出し元に分ける

## 文脈

`scripts/verify-tree.mjs` と PR 前ゲート（`.claude/hooks/pre-bash.mjs` の `readVerifyState`）は、主のセッションが本体の作業ツリーに居ることを前提にしていた。Claude Desktop のセッションは harness の linked worktree（`.claude/worktrees/<name>`）で動くことがある。その場合、worktree からの `run` は「本体の作業ツリーから打つこと」で止まり、本体からの `run` は「sha が本体の HEAD でない」で止まった（#1429 で実測）。#1429 は、本体のブランチを一時的に切り替えて回避した。本体は並行するセッションと共有なので、この回避は他のセッションの足場を動かす。

採った形の全文は、`scripts/verify-tree.mjs` の `mainRoot` / `callerContext` / `BUILD_BEFORE` と、`readVerifyState` の doc が持つ。この ADR は却下した案を残す。

## 検討した代替案と却下理由

- **linked worktree からの実行を拒み続け、本体のブランチを切り替えて回す**（#1429 の回避）: 却下。本体は他のセッションと共有である。切り替えている間に他のセッションが本体で打ったコマンドは、別のブランチを相手にする。戻し忘れれば、それが残る。
- **置き場も呼び出し元の worktree の下へ移す**（`show-toplevel` をそのまま使う）: 却下。ツールの worktree は harness の worktree の中にネストされ、harness が worktree を片づけるときに道連れになる。マージのツール（`scripts/merge-pr.mjs`）は PR の head ブランチ名から要約の置き場を導くので、どの worktree で回したかを知らないと要約に届かない。置き場は、どこから打っても同じ値になる本体のルートに置く。
- **`test:powershell` を、本体でビルドした本体（`CARGO_TARGET_DIR=<本体>/target`）で回す**（#1429 の回避）: 却下。本体のブランチを sha へ切り替えない以上、本体の `target/` にあるのは別の版の本体である。検査は成功したまま、sha でない版を測る（#835・#1179 と同じ型）。`CARGO_TARGET_DIR` の共有は、`ADR-implement-verification-delegated-to-worktree` が fingerprint の往復を理由に既に却下している。
- **`run-pester.ps1` 自身が、既定の本体を使うときに `cargo build -p snotra` を打つ**: 見送り。古い本体を測る危険を発生源で塞げるが、CI と人が手で打つ経路の挙動も変える。CI の rust-check はすでに別のステップでビルドしており、効くのはツールの worktree だけである。変える範囲をツールの計画に閉じた。

## 受容した残余

- **`BUILD_BEFORE` は `docs/build-commands.md` の C 節の 1 行の写しである。** 字面が違うと重複を見分けられず 2 回建てるので、C のフェンスに同じ字面があることをテストが固定する。実バイナリを起動する npm script を `--also` で足す経路が増えたら、`BUILD_BEFORE` へも足す必要があり、それを見る検査は無い。
- **委譲先が `align` の前に `run` を打つと、構造では止まらない。** 以前は linked worktree の拒否で止まっていた。委譲先はブランチ（`worktree-<名前>`）の上に居るので、そのブランチ名でツールの worktree を作る。委譲先へ渡す手順に `run` は無く、作られても `clean:worktrees` が `agent-` 接頭辞で拾う。
- **PR 前ゲートの修正は、hook が読むファイルがこのブランチの版になるまで効かない。** settings の hook コマンドは `CLAUDE_PROJECT_DIR` の `pre-bash.mjs` を呼ぶ。harness の worktree のセッションでそれがどちらを指すかは、この変更では確かめていない。

---

status: Accepted
関連: #1436 ・#1429 ・#1298 ・`docs/adr/ADR-implement-verification-tool-tree.md` ・`docs/adr/ADR-implement-verification-delegated-to-worktree.md`
