# hook の責務を三層に分離し、main 保護を git/GitHub の機構へ移す

- 日付: 2026-07-09
- ステータス: 設計合意済み（Phase 1 実装中）
- 改訂: 実装中の実測により Phase 1 を **1a / 1b に分割**した。`.githooks/` は追跡ファイルであり、それを含まないツリー（マージ前の `main`）では Layer 1 が存在しない。ゆえに `block-main-commit` の削除をマージ前に行うと、マージまでの間 main 上のローカルガードが空になる。計画自身の不変条件「安全網を一瞬も空にしない」に従い、削除は **マージ後の Phase 1b** へ移した（§5・§7・§8・§9）
- 関連: #471（closed）, #473, #474, #475, #476, #477, #479 / `.claude/settings.json`, `.claude/hooks/post-edit.mjs`, `CLAUDE.md`, `AGENTS.md`
- 由来: hook 関連 issue 群の構造分析。個々の issue を潰すのではなく、共通の根を断つ

## 1. 背景と問題

hook 関連の open issue が 5 件あり、すべて #471 の子である。#471 自身が「根治」を掲げて PostToolUse を作り直したにもかかわらず、同じ根から枝が伸び続けている。

issue 群が共有する根の記述は「**hook が発火の判定に使う情報と、実際に検査する対象がずれている**」。これは正しいが、症状の記述である。

### 実測した抜け道

最重要ルール 1「`main` へ直接コミット・プッシュしない」を守る唯一の自動ガードが `block-main-commit`（`.claude/settings.json`）である。実際に測ると、守られていない。

| 経路 | ルール 1 は守られるか | 測定 |
|---|---|---|
| Bash tool + `git commit` + hook の cwd が main | ✅ 止まる | — |
| **PowerShell tool + `git commit`** | ❌ **素通り** | `matcher` が `"Bash"` のみ。この環境の primary shell は PowerShell |
| `git -C <tree> commit` | ❌ 素通り | 正規表現 `git\s+(commit\|merge\|rebase)` に当たらない |
| `git -c user.name=x commit` / `git --no-pager commit` | ❌ 素通り | 同上 |
| `git push origin HEAD:main` | ❌ 素通り | ガードの語彙は `commit\|merge\|rebase`。**push が無い** |
| `git pull origin main`（非 FF） | ❌ 素通り | 語彙外。かつ `settings.local.json` で自動承認済み |
| subagent / worktree | ❌ ずれる | `git branch --show-current` を hook の cwd で評価している |
| git ネイティブ hook | ❌ 不在 | `core.hooksPath` 未設定、`.git/hooks` に実体ファイル 0 本 |
| GitHub branch protection | ❌ 不在 | ruleset `default` が `enforcement: "disabled"` のまま |

**ルール 1 を守る機構は、実質的にひとつも存在しない。** PowerShell を選ぶだけで迂回できる。

### 誤爆も実測した

`grep` は payload 全体に当たるため、`tool_input.description` に「git commit」と書いただけで発火する。本設計の調査中、**`git` 操作を一切含まないコマンド**（文字列を `grep` に食わせる probe）が `block-main-commit` にブロックされた。

### 誤爆が文書に転移している

CLAUDE.md には、この誤爆を回避するための運用ルールが 2 つある。

1. **最重要ルール 2「`git` コマンドを `&&` でチェーンしない」** — 理由として「`git checkout <branch> && git rebase main` のような連鎖は `block-main-commit` を誤発火させた実績がある」と明記されている
2. **「main の fast-forward 同期は `git pull --ff-only` を使う」** — 理由は「`git merge --ff-only origin/main` はコミットを作らない FF でも `block-main-commit` に弾かれる」

つまり **コードのバグが、人間が守るべき文書ルールへ転移している**。しかも 2 の回避先である `git pull` はガードの語彙外であり、非 FF なら main にマージコミットを作れる。**誤検知の回避が、見逃しを生んでいる。**

### PostToolUse 側の問題も、同じ根から出ている

#471 は「沈黙は合格を意味する」という契約を導入し、スクリプト内部の沈黙経路（タイムアウト・出力溢れ・起動失敗・実行時例外）をすべて塞いだ。しかし列挙のスコープがスクリプト内に閉じていた。実測で確認した、外側に残る沈黙経路:

- `matcher` は `Edit|Write` のみ。Bash 経由のファイル変更（`git checkout` / `git pull` / rebase のコンフリクト解決 / `cargo fmt`）は無検査
- `config-warn` は `additionalContext` を作らない。`tauri.conf.json` を編集したエージェントには**一文字も届かない**（`systemMessage` は人間向け）
- `post-edit.mjs` 自身の構文エラーは `try { main() } catch` の外側で落ちる。エージェントには沈黙する
- `tsconfig.json` / `Cargo.toml` / `package.json` の編集は完全な沈黙。**tsconfig ドリフト検出カナリアを置いた当のファイルが、そのカナリアを起動しない**

## 2. 決定 — hook の目的を定義し、そこから責務を分ける

> **Claude Code の hook は、エージェントの「意図」と「認識」を扱う唯一の層である。**
>
> - **リポジトリの状態**は git が守る（どの経路でも発火する）
> - **成果物の正しさ**は CI が守る（走った証跡が残る）
> - **hook** は「エージェントが今から何をしようとしているか」と「エージェントが今何を知っているか」だけを扱う

現状の誤りはこの一文で説明できる。**hook にリポジトリ状態の保証を負わせた。** それは hook の視界にないものであり、穴を一つずつ塞いでも視界の外にあるものは見えない。

### 責務の三層

| 層 | 守るもの | 発火する経路 | 性質 |
|---|---|---|---|
| **Layer 0: GitHub ruleset** | main が **origin** で進むこと | すべて。どの端末・ツール・シェルからでも | **保証**。最終防衛線 |
| **Layer 1: `.githooks/`（git ネイティブ）** | main が **ローカル**で進むこと | ローカルの全 git 操作。PowerShell も `git -C` も worktree も subagent も | **早期停止**。best-effort |
| **Layer 2: Claude Code hook** | エージェントの**意図と認識** | ツール呼び出しのみ | (A2) 外部 API の不可逆呼び出し / (B) 編集直後の事実供給 |

### (A) は 2 つに割れる

これまで「禁止・保証」と一括りにしていたものは、性質の違う 2 種の混合だった。

| | 対象 | git 機構で守れるか | hook で守れるか |
|---|---|---|---|
| **(A1)** `block-main-commit` | リポジトリの状態（main が進む） | ✅ 守れる | ❌ 視界外の経路が多すぎる |
| **(A2)** PR 前 push チェック | 外部 API への不可逆呼び出し（`gh pr create` → 空 PR → merge 時の誤 close） | ❌ **守れない**（リポジトリを触らない。push しないので `pre-push` も鳴らない） | ✅ **hook にしか見えない** |

**(A2) は git にも CI にも原理的に観測できず、Claude Code の hook だけが見える領域である。** `gh pr merge --squash` の誤 close も `gh issue close` も同じ。ここが hook の固有価値であり、#473 が求めた「`pre-bash.mjs` を作り `tool_input.command` だけを構造的にパースせよ」という解は、**(A1) にではなく (A2) にこそ必要**だった。

### 非目標（YAGNI）

- **`block-main-commit` を強化しない。** `matcher` に PowerShell を足し `push` を語彙に加えても、守れるのは「エージェントがツール経由で叩いた操作」だけで、あなたの手元の端末や将来のツール追加には原理的に届かない
- **Layer 1 の不在を検知する仕組みを作らない。** `core.hooksPath` はローカル設定であり外れうるが、外れても Layer 0 が push を拒む。「安全網の不在を検知する安全網」という無限後退から降りる
- **`required_status_checks`（CI グリーン必須）は今回入れない。** 最終防衛線を立てるという本 Phase の目的から外れる。別判断とする
- **`post-edit.mjs` を触らない。** (B) の再設計は Phase 3

## 3. Layer 0 — GitHub ruleset

既存の休眠 ruleset（id `12941497`, name `default`, target `~DEFAULT_BRANCH`）を作り直さず、起こして 1 規則足す。repo は PUBLIC のため費用制約は無い。

| 項目 | 現在 | 変更後 |
|---|---|---|
| `enforcement` | `disabled` | **`active`** |
| `deletion` | 定義済み（不活性） | 有効 — main の削除を拒否 |
| `non_fast_forward` | 定義済み（不活性） | 有効 — main への force-push を拒否 |
| `pull_request` | なし | **追加**（`required_approving_review_count: 0`） |
| `bypass_actors` | なし | **なし のまま** |

`pull_request` 規則が「main への直接 push 禁止」の実体である。PR 経由の merge は通るため `gh pr merge --squash` は従来どおり動作し、承認者は不要。`bypass_actors` を置かないため、リポジトリ所有者にも適用される。

**エスケープハッチ**: `enforcement` を `disabled` へ戻す。API または Web UI から、意図的に、監査ログを残して行う。`--no-verify` のような「うっかり通る」経路ではないことが要件である。

## 4. Layer 1 — `.githooks/`

リポジトリ管理下に置き、`core.hooksPath` で有効化する。

```
.githooks/
  _lib.sh            共通の判定とメッセージ
  pre-commit         main 上での commit を拒否
  pre-merge-commit   main 上での merge commit を拒否（非 FF の git pull を含む）
  pre-rebase         main を rebase 対象にする操作を拒否
  pre-push           refs/heads/main を宛先とする push を拒否
```

4 本あるのは、元の grep が担っていた語彙（`commit|merge|rebase`）に **push** を足した結果である。git は操作ごとに別の hook を呼ぶため 1 本にはまとまらない。判定とメッセージは `_lib.sh` に集約する。

```sh
# .githooks/pre-commit（as-built）
#!/bin/sh
. "$(dirname "$0")/_lib.sh"
# detached HEAD（rebase 中など）は current_branch_ref() が空を返す＝判定不能なので通す
[ "$(current_branch_ref)" = "$PROTECTED_REF" ] && die "main への直接コミットは禁止です。"
exit 0
```

**`git symbolic-ref --short` を使ってはならない。** `main` という名の tag が存在すると、曖昧性回避のため `refs/heads/main` が `heads/main` に縮み、`main` との比較が偽になって**静かに素通りする**（最終レビューで実測）。削除予定の `block-main-commit` は `git branch --show-current` を使っておりこの曖昧性に免疫があったため、短縮名で比較すると**置き換え対象からの後退**になる。完全 ref（`refs/heads/main`）で比較する。

```sh
# .githooks/pre-push — stdin: <local ref> <local sha> <remote ref> <remote sha>
#!/bin/sh
. "$(dirname "$0")/_lib.sh"
while read -r _l _ls remote_ref _rs; do
  [ "$remote_ref" = "refs/heads/$PROTECTED_BRANCH" ] &&
    die "main への直接 push は禁止です（$remote_ref）。"
done
exit 0
```

`pre-push` は source ではなく **destination の ref** を見る。ゆえに `git push origin HEAD:main` も `git push origin :main`（削除）も宛先で捉える。これが「リポジトリの状態を守る」語彙である。

### なぜ漏れないのか

git が hook を呼ぶとき、cwd は**コミットされるツリーのトップ**である。したがって `git -C /other/tree commit` はそのツリーの `.githooks/pre-commit` を、そのツリーのブランチで評価する。「cwd と実際のコミット先がずれる」というバグが構造的に発生しない。worktree も同じ理由で守られる。

実測（`.githooks/githooks.test.mjs`）:

- 相対 `core.hooksPath` は **working tree のトップ**を基準に解決される。プロセスの cwd ではない。ゆえに `ui/` や `src-tauri/` の中から `git commit` しても hook は発火する
- linked worktree では **worktree 自身の `.githooks`** が解決される（main ツリー側の hook を無害化してもなお拒否されることで反証済み）

### この層が存在しないツリーがある（実測・受容する性質）

`.githooks/` は**追跡ファイル**である。したがってそれを含まないコミットを checkout すると、`core.hooksPath` は存在しないディレクトリを指し、**git は「hook 無し」として操作を通す（fail-open）**。

該当するのは、`.githooks/` 導入前のコミット・古いタグ、そして**この変更がマージされるまでの `main` そのもの**である。

### Layer 1 が見ていない操作もある（実測・受容する性質）

git は操作ごとに別の hook を呼ぶ。以下では `pre-commit` が**呼ばれない**——最終レビューが実測し、いずれも main が無警告で進んだ。

`git cherry-pick` / `git revert` / `git am` / `git branch -f main <sha>` / `git update-ref refs/heads/main <sha>`

（`git commit --amend` と `git merge --squash` 後の `commit` は正しく拒否される。）

旧 `block-main-commit` の正規表現 `git\s+(commit|merge|rebase)` もこれらを捕まえていないため**後退ではない**。ただし「拒否する」と書いてはならない。全経路を捕捉するには `reference-transaction` hook が要るが、`git fetch` / `pull --ff-only` による main 更新でも発火するため FF 判定が必要であり、本 spec のスコープ外（follow-up issue）。

### 帰結

これは三層設計を壊さない。Layer 1 は best-effort であり、**保証は Layer 0（ruleset）が担う**。ローカルの main が汚れても push は拒否され、`git reset --soft` で回復できる。

ただし帰結が 2 つある:

1. **ドキュメントは「`.githooks` が main を守る」と無条件に書いてはならない。** 守るのは `.githooks` を含むツリーにおいてである
2. **`block-main-commit` の削除は、この変更が main にマージされた後に行う。** マージ前に削除すると、マージまでの間 main 上のローカルガードが空になる（→ §7 Phase 1b）

### bootstrap

`core.hooksPath` はローカル設定（`.git/config`）でリポジトリに乗らない。`package.json` に追加する。

```json
"scripts": { "prepare": "git config core.hooksPath .githooks" }
```

npm の `prepare` は `npm install` / `npm ci` の後に走る。worktree は `.git/config` を共有するため、一度で全 worktree に効く。

### エスケープハッチ

`--no-verify` が `pre-commit` / `pre-merge-commit` / `pre-push` を迂回する。**人間専用**であり、エージェントには harness の system prompt と CLAUDE.md が禁じる。`pre-rebase` は `--no-verify` を受け付けないため、迂回するなら `git config --unset core.hooksPath` を一時的に行う。

## 5. Layer 2 — `block-main-commit` の削除（Phase 1b）

`block-main-commit` を `.claude/settings.json` から**削除**する。Layer 0/1 が守る以上、残す価値は「より早く止まる」ことだけであり、その対価は漏れ・誤爆・「守られている」という誤った信念である。

**ただし実行は Phase 1b（Phase 1a のマージ後）**。理由は §4「この層が存在しないツリーがある」を参照。マージ前に削除すると、`.githooks/` を持たない `main` 上でローカルガードが空になる。

PR 前 push チェックは触らない（Phase 2）。`post-edit.mjs` は振る舞いを触らない（Phase 3）。

### 削除に伴い CLAUDE.md から消えるもの

git ネイティブ hook は実際に実行される瞬間に、実際のツリーで判定する。ゆえに以下 2 ルールの存在理由が消滅する。

- 最重要ルール 2「`git` コマンドを `&&` でチェーンしない」
- 「main の fast-forward 同期は `git pull --ff-only` を使う」

代わりに記載するもの:

- main 保護は `.githooks/` + GitHub ruleset が担うこと（hook ではない）
- `.githooks/` を含まないツリーでは Layer 1 が存在せず、保証は Layer 0 が担うこと（§4）
- `--no-verify` は人間専用であり、エージェントは使用してはならないこと
- `npm install` が `core.hooksPath` を設定すること（bootstrap の所在）

**Phase 1a では追記のみ**（新しい層と caveat）。**削除は Phase 1b**。

**設計の良し悪しを測る指標のひとつは、ドキュメントが減るかどうかである。** 本変更は最終的に CLAUDE.md から 2 ルールを削り、hook を 1 本削り、#473 の半分を消滅させる。

## 6. 検証（故障注入）

AGENTS.md の要求「安全網が『効いている』ことは、故障注入で一度は実測する」に従う。#471 は、この規律が無かったために「hook の出力が一度もエージェントに届いていなかった」ことを見逃した。

V5 / V6 は、本 spec §1 で測った実在の抜け道をそのまま逆向きに撃つ回帰テストである。

| # | 故障注入 | 期待 | 何を証明するか |
|---|---|---|---|
| V1 | 使い捨てブランチから `git push origin tmp:main` | 拒否 | Layer 0 が立った。main は動かない |
| V2 | `gh pr merge --squash` | **通る** | `pull_request` 規則が既存の運用を壊さない |
| V3 | main への force-push | 拒否 | `non_fast_forward` |
| V4 | main 上で commit | 拒否 | `pre-commit` |
| V5 | **PowerShell ツールから** main 上で commit | 拒否 | 前回素通りした経路が塞がった |
| V6 | feature ブランチの cwd から `git -C <main ツリー> commit` | 拒否 | cwd 判定バグの構造的消滅 |
| V7 | main で非 FF の `git pull` | 拒否 | `pre-merge-commit` |
| V8 | `git push origin HEAD:main` | 拒否 | `pre-push`（destination で判定） |
| V9 | feature での通常 commit / main で `git pull --ff-only` / `git merge --ff-only origin/main` | **すべて通る** | **誤爆しない**。CLAUDE.md の 2 ルールを消せる根拠 |
| V10 | worktree 内で commit | 拒否 | 相対 `core.hooksPath` の解決が worktree で成立する |

**V10 を独立させた理由**: 相対 `core.hooksPath` が「working tree のトップを基準に解決される」というのは git のドキュメントから読んだ期待であって、Windows + worktree での測定結果ではない。ここで転けたら絶対パス方式へ切り替える。

**V9 の重要性**: 通ることの確認が、通らないことの確認と同じだけ重要である。V9 が失敗すれば「誤爆しないから doc ルールを消せる」という前提が崩れ、§5 の削除は取り消しになる。

## 7. 実施順序 — 安全網を一瞬も空にしない

### Phase 1a（本 PR）

1. ruleset を `active` にし `pull_request` を追加 → **V1 で実測**
2. `.githooks/` 4 本 + `_lib.sh` + `.gitattributes` + `package.json` の `prepare` → **V4〜V10 で実測**
3. CLAUDE.md と `docs/build-commands.md` に**新しい層を追記**する（既存ルールは消さない）
4. マージ。この瞬間、`main` のツリーに `.githooks/` が入り、Layer 1 が main を守り始める

### Phase 1b（マージ後の別 PR）

5. `block-main-commit` を `settings.json` から削除
6. CLAUDE.md の最重要ルール 2 と `--ff-only` 運用ルールを削除し、置き換え文言へ差し替える

削除を Phase 1b に置くのは、`.githooks/` が **マージされるまで `main` のツリーに存在しない**ためである（§4）。Phase 1a の途中で削除すると、漏れはあれど存在する唯一のローカルガードが、新しいガードの届かないツリー（`main`）で失われる。

この分割は実装中の実測（Task 7 の V5）で判明した。当初の計画は「実測が緑になってから削除」としており、`.githooks/` の存在するツリーでしか緑を測っていなかった。

## 8. 受け入れ条件

### Phase 1a（本 PR）

- [x] **V1 実測**: server が main への直接 push を拒否（`GH013 / Changes must be made through a pull request`）。main は動いていない
- [ ] **V3 は実行しない**: harness が main への force-push を拒否した。迂回しない。`pull_request` 規則は main へのあらゆる直接 ref 更新を拒むため V1 に包含される。`non_fast_forward` が `active` であることは read-back で確認済みだが、**実地の force-push・削除は一度も試行していない**
- [x] **V4 実測**: Bash ツールからの commit は旧 `block-main-commit` が exit 2 で止める
- [x] **V5 実測**: `.githooks` を置いた main で、PowerShell ツールからの commit が `exit 1 / BLOCKED` で拒否される（前回素通りした経路が塞がった）
- [x] **V6・V7・V10 自動化**: `.githooks/githooks.test.mjs` の 14 テスト（`git -C`・非 FF マージ・rebase・push・worktree・サブディレクトリ）
- [ ] **V8 は実行しない**: harness が main への push を拒否。迂回しない。`pre-push` の 3 テスト（client）と V1（server）で二重に測定済み
- [x] **V9 実測**: `git pull --ff-only` も `git merge --ff-only origin/main` も通る＝誤爆しない
- [x] `docs/build-commands.md` にカテゴリ E と bootstrap が記載されている
- [x] `.gitattributes` が `.githooks/**` を `eol=lf` に固定している（`_lib.sh` も含む＝CRLF による静かな guard 無効化を防ぐ）
- [x] CLAUDE.md に新しい層（`.githooks/` + ruleset）と §4 の caveat が**追記**されている（既存ルールは削除しない）。主張の確度は測定の等級に合わせる（実測 / read-back のみ / 視界外、を書き分ける）
- [x] `.claude/settings.json` と `.claude/hooks/post-edit.mjs` に変更が無い
- [x] `_lib.sh` は完全 ref（`refs/heads/main`）で比較する。`--short` は ref 曖昧性で静かに fail-open する
- [x] `AGENTS.md` と `.claude/skills/implement/SKILL.md` の「カテゴリ A〜D」が「A〜E」になっている
- [ ] **V2**: `gh pr merge --squash` が動く（マージ時に確認）
- [ ] CI グリーン（ubuntu の `npm test` が実行ビットと dash 互換性の検知器になる）

### Phase 1b（マージ後）

- [ ] `block-main-commit` 削除後も、`.githooks/` を含む `main` で commit が拒否される
- [ ] Bash ツールから `git merge --ff-only origin/main` が通る（誤爆の消滅）
- [ ] CLAUDE.md から最重要ルール 2 と `--ff-only` 運用ルールが消え、置き換え文言が入っている
- [ ] `post-edit.mjs` のコメントから `block-main-commit` の名前が消える（振る舞いは不変）

## 9. issue の処遇

| issue | 処遇 |
|---|---|
| **#473** | Phase 1a では触らない（`block-main-commit` はまだ生きている）。**Phase 1b でその半分が消滅**する。残る半分＝ PR 前 push チェック側（payload 全体 grep・PowerShell 素通り）は Phase 2 の issue として書き換える |
| #474 / #475 / #476 / #477 / #479 | すべて (B) 側。肯定的報告への転換（Phase 3）に依存。今回は触らない |
| **新規起票 ①** | 「Phase 1b: `block-main-commit` を削除し CLAUDE.md の 2 ルールを消す」— 本 PR のマージ後に実施 |
| **新規起票 ②** | 「PreToolUse の `matcher: "Bash"` は PowerShell tool に一致しない」— Phase 2 に含める |
| **新規起票 ③** | 「`reference-transaction` hook で `cherry-pick` / `revert` / `am` / `branch -f` / `update-ref` を捕捉する」— §4 の視界外経路。`fetch` / `pull --ff-only` による main 更新でも発火するため FF 判定が要る |
| **新規起票 ④** | 「`selectChecks` に `.githooks/**` を追加する」— `.githooks/` は今日から安全網であり、`.claude/hooks/**` と同じ理由（安全網そのものを編集したら安全網が生きているか確かめる）が適用されるが対象外。Phase 3 と同時が自然 |
| **新規起票 ⑤** | 「`prepare` の `git config` は `.git` 無し環境で `npm ci` ごと落とす」— 現状すべての `npm ci` は `actions/checkout` の後なので到達不能。Docker のレイヤキャッシュ build を足すと踏む。`\|\| exit 0` は「Layer 1 は best-effort、不在は検知しない」と整合する |

## 10. この先（本 spec の範囲外）

- **Phase 2 — (A2)**: `pre-bash.mjs` を作り、`tool_input.command` だけを見て、`matcher` を `Bash|PowerShell` に広げ、判定不能なら fail-closed に倒す。git にも CI にも見えない、hook 固有の領域
- **Phase 3 — (B)**: 「沈黙 = 合格」を剥がし、走った検査に名乗らせる。沈黙は「何も走らなかった」を意味するようになり、#471 が塞ごうとした沈黙経路（matcher 外の編集・parse error・`config-warn` の envelope 分岐・`tsconfig.json` の無反応）が、塞がなくても無害になる
