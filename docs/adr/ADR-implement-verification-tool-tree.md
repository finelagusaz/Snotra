# ADR-implement-verification-tool-tree: `/implement` の権威ある A〜F を道具の木へ移し、委譲先は harness の木に残す

## 文脈

`ADR-implement-verification-delegated-to-worktree` は、検証 A〜F・レビュー・変異注入を worktree の 1 体へ束ねた。#1137 以降 20 PR・48 巡の実測（#1298）では、A〜F の exit≠0 は 0 件で、委譲の価値はレビューと変異注入から出ていた。初巡の worktree HEAD は 20/20 PR でアンカーと一致せず、5 PR では前巡の未追跡の報告が checkout を止めた。防御はどちらも SKILL の指示文にしか無かった。

**0 件は道具化の根拠ではない**——C（smoke）が実際に走ったのは 20 PR 中 3 件だけで、主がコミット前に確認を回してから委ねる以上、誰が回しても 0 件は続く。根拠は HEAD 合わせと報告の衝突を構造へ上げられることにある。**費用は減らない**: 木が 2 本になり、cold ビルドは PR あたり 1 回増える（実測は `cargo build -p snotra` が 2m59s・`--release` が 5m38s・#1179 の委譲報告）。旧 ADR の却下理由のうち「重複」はこの形でも同じ量だけ残る。消えるのは「別の版へ向く」だけで、道具の要約と委譲先の `align` の出力がどちらも sha を名乗ることで消える。

## 検討した代替案と却下理由

計測は 2026-10-25、使い捨ての worktree と計測用のサブエージェントで行った（#1298 の `workspace/research.md` の M1〜M9）。

- **委譲先を isolation 無しで起動し、道具の木の絶対パスを渡す**（issue の当初案）: 却下。サブエージェントの Bash は**呼び出しごとに cwd が本体へ戻る**（実測。system prompt に "Agent threads always have their cwd reset between bash calls"）。`cd` を 1 回打ち忘れると、cargo も変異注入も本体の作業ツリーで走る。`isolation: "worktree"` が構造で保証していた「cwd が木」が、指示文へ下がる。
- **委譲先が `EnterWorktree(path)` で道具の木へ移る**: 却下。isolation 無しのサブエージェントからは「cwd がリポジトリのルート」として、`isolation: "worktree"` のサブエージェントからは「書き込みが自分の木に限られる」として、**どちらも拒否された**（実測）。
- **委譲先の出力先を木の外にする**（issue の当初案）: 却下。harness は、変更の無い木（gitignore 済みのファイルだけのものを含む）を委譲先の停止時に**木ごとブランチごと片づける**。`SendMessage` で再開した委譲先は、primary working directory が本体になっていた（実測）。過去の PR で木が巡をまたいで残ったのは、木の中に未追跡の報告があったからである。木の中の `.delegate/` に未追跡の報告を置くと、木は残り、再開後の cwd も木だった（実測）。Write ツールは木の外の本体パスをそもそも拒否する（実測）。
- **道具と委譲先が 1 本の木を共有する**（cold ビルドが 1 回で済む）: 却下。変異注入が木を一時的に壊している間に道具の A〜F が走ると、壊れた版を測る——旧 ADR の事故 2（主の変異注入がレビュアの観測を汚した）が形を変えて戻る。そもそも委譲先は他の木へ入れない（上の `EnterWorktree`）。
- **道具がコマンドの表を持つ**: 却下。`docs/build-commands.md` のフェンスと 2 枚目の写しになり、一致を見る検出器が要る。道具はフェンスを読み、読めたコマンドが 0 件なら exit 2 にする。フェンスの外の散文に書いた条件つきのコマンドは `--also` で足す。
- **該当カテゴリを道具が差分のパスから導く**: 却下。C と D は「変更が何をするか」で決まる（`.claude/rules/src-tauri.md`・#558）。E と F はパスで決まるように見えて意味判断が混じる（`.rs` のコメントの見出し参照で F に当たる等）が、数秒で終わるので常に回すことにした。A は CI と同じく全 crate のテストを回し、crate の導出を持たない。

## 受容した残余

- **委譲先の `align` を打つのは指示文である**。打てば HEAD 合わせと「巻き戻されていない変異」の検出は構造で行われるが、打たせることまでは強制できない。報告が sha を名乗ることで、事後に食い違いを見つける経路は残る。
- **Bash のリダイレクトは隔離された委譲先からも木の外へ書ける**（実測。Write ツールとの非対称）。`.delegate/` の規約は指示文で守らせる。
- **木を 2 本に分けても並走の競合は残る**。CPU 競合による timeout の赤は道具の再実行で、smoke の単一インスタンスは「C を要求する巡は直列」で避ける。どちらも手順である。
- **`remove --delegate-tree` は、委譲先のブランチ名を harness の命名（`worktree-<木の名前>`）から導く**。`align` が detach するので、`worktree list --porcelain` にはブランチが載らない。命名が変わればブランチが残るが、`clean:worktrees` の孤児掃除が拾う。
- **検知器を持たない修正が 1 つある**: `align` は `git check-ignore` が 0 と 1 以外を返したら exit 2 にする（fail-open を塞ぐ）が、その status を返させる fixture は git を壊さないと作れない。シェルの command not found は cmd.exe が 1 として返すので、道具は「赤」と区別できない（緑の側へは倒れない）。
- **junction 無しの木は本体の `node_modules` を解決する**。依存を変える PR では別の版を測りうるので、道具は sha の lock と `node_modules/.package-lock.json` の食い違いを警告するが、止めはしない。

---

status: Accepted
関連: #1298 ・#1137 ・`docs/adr/ADR-implement-verification-delegated-to-worktree.md` ・`/implement`「3b. 委譲へ渡すもの」
