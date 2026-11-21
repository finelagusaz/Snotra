# #1507 独立導出（plan-review-derive1507）

要求（issue #1507 本文とコメント）とコードだけから導いた。`workspace/` の他のファイル（計画）は読んでいない・目にしていない。
git の挙動は使い捨てリポジトリ（scratchpad/derive1507）で実測した。

## (a) 変更が要るファイルとシンボル

| ファイル | シンボル / 位置 | 変更 |
|---|---|---|
| `scripts/verify-tree.mjs` | `cmdRemove` の `if (removeTree(root, target)) { if (branch) { git(root, ["branch", "-d", branch]) ... } }` | 判定を差し替える（(b)） |
| `scripts/clean-worktrees.mjs` | 孤児ブランチ掃除のループ `git(["branch", force ? "-D" : "-d", b])`（`--force` でない側） | 同じ判定を使う（issue コメントが名指し。cwd は呼び出し元で、`-d` は**呼び出し元の worktree の HEAD** と比べるので同じ欠陥を持つ） |
| 共有の置き場（新規 export） | 例: `verify-tree.mjs` から `deleteBranchIfMerged(cwd, branch)` を export し、`clean-worktrees.mjs` が import する。または `scripts/` 直下に小さな共有モジュール | 2 か所が同じ述語（「消してよいほど取り込まれた」）を持つ。片方だけ変わる将来は挙げにくい→共有が妥当（⚠️置き場は判断） |
| `scripts/verify-tree.test.mjs` | `describe("verify-tree remove")` の `--delegate-tree` 群の後 | (c) のテスト |
| `scripts/clean-worktrees.test.mjs` | `describe("clean-worktrees")` | (c) のテスト |
| `.claude/skills/implement/SKILL.md` | 「Step 4 — 仕上げ」の「worktree を片づける」項 | exit 1 の読み方に「ブランチを残した」場合を足す（(d)） |

触らないもの: `docs/adr/ADR-implement-verification-tool-tree.md`（凍結された歴史・`ADR-adr-frozen-history`。L25 の「命名が変われば…孤児掃除が拾う」は今も真）、`docs/build-commands.md` の `clean:worktrees` 行（「dirty はスキップ、--force で強制」は真のまま）、`AGENTS.md` の origin/main 起点の項（#1514 で「merged の判定」は射程外と裁定済み・コメント参照）。

いずれも `scripts/**` / `.claude/skills/**` でセーフティネット → PR 本文冒頭に「マージは合意してから」。

## (b) 判定ロジックの設計

実測した事実（scratchpad）:
- `git branch -d` は **実行した作業ツリーの HEAD** と、ブランチの upstream にだけ照らす。HEAD が古い `main`、upstream なし、ブランチが `origin/main` の先端 → exit 1「not fully merged」（再現した）。`remove` は `git(root, ...)` で本体の HEAD を使う——本体が別の feature ブランチに居ても同じ症状になる（issue の「ローカル main」は「本体の HEAD」の一例）
- `git merge-base --is-ancestor A B`: 祖先 → 0、祖先でない（固有コミット）→ 1、ref が解決できない → **128**（`fatal: Not a valid object name`）
- `origin/main` という名のローカルブランチがあると、短縮名 `origin/main` は `refname is ambiguous` の警告付きで**ローカルブランチ側へ解決された**。→ 完全名 `refs/remotes/origin/main` を使う
- `remote.origin` の設定があり、`git worktree add -b X <path> origin/main`（名前で開始点を指定）すると upstream が自動で付き、`-d` は「merged to refs/remotes/origin/main, but not yet merged to HEAD」の警告付きで成功する。remote の設定が無い（`update-ref` で作っただけの）リポジトリでは付かない

推奨する手順（1 本の関数に置く）:
1. まず `git branch -d <branch>`（既存挙動を保つ。HEAD か upstream に取り込み済みならここで終わる。既存テストは remote 無しのリポジトリでこの経路を通る）
2. 失敗したら `git merge-base --is-ancestor refs/heads/<branch> refs/remotes/origin/main` を `spawnSync` で打ち、status で 3 分岐する:
   - **0** → `git branch -D <branch>` で消す。出力に「origin/main の祖先なので消した」と理由を出す
   - **1** → 消さない。「origin/main に無いコミットがある」と報告（`git log --oneline refs/remotes/origin/main..<branch>` の件数か先頭を添えると判断しやすい）。`ok = false`
   - **それ以外（128 等）** → 消さない。「origin/main を解決できない／判定できない」と、手順 1 の `-d` の stderr を添えて報告。`ok = false`。**1 と混ぜない**（「固有コミットがある」と「判定できなかった」は直し方が違う——ファイル先頭の「2 を 1 と混ぜない」と同じ趣旨）
3. `execFileSync` の throw を catch して「非 0 = 祖先でない」と読む実装にしない（128 を 1 と同一視する）

基準にする ref: `refs/remotes/origin/main` を固定で。`origin/HEAD`（`git remote set-head` 次第で無いことがある）や `@{upstream}`（`-d` が既に見ている）は使わない。リポジトリの他の規範も `origin/main` を起点にしている。
fetch はしない: ツールがネットワークに触れず、`origin/main` が古ければ「祖先でない」側＝残す側へ倒れる（安全側）。報告文で「origin/main は fetch した時点の値」と分かればよい。

終了コード: ブランチを残したときは従来どおり 1（ヘッダの「1 = 消せなかった（remove）」に合う）。2 には上げない——worktree は既に消えており、`main()` の 2 は「走らなかった」の意味なので。

`-D` を使う理由: `update-ref -d refs/heads/<b> <sha>` は比較つき削除（実測: old 値が違えば拒む）で TOCTOU に強いが、`branch.<b>.*` の設定節を消さない。worktree を消した後はブランチがどこにも checkout されておらず、判定と削除の間に動かすのは並行プロセスだけ → `-D` で足りる（⚠️）。気にするなら判定前に sha を取り、`-D` の直前に `rev-parse` で一致を確かめる。

clean-worktrees.mjs への適用: `--force` でない側の `-d` を同じ関数に替える。`--force` 側（`-D`）は不変。失敗時の警告文「未マージ？ --force で消せます」は、判定できなかった（128）ときと固有コミットのときで文を分ける。

## (c) 足すべきテスト（と偽の緑の罠）

verify-tree.test.mjs（`remove --delegate-tree`）:
1. **受け入れ (1)**: 本体の HEAD を古い `main` に置き、委譲先ブランチが `refs/remotes/origin/main` の先端を指す → exit 0・worktree 消滅・`git branch --list worktree-agent-abc` が空
2. **受け入れ (2)**: 委譲先ブランチに `origin/main` に無いコミットがある → exit 1・ブランチが残る・出力に「固有コミット」の理由が出る
3. `refs/remotes/origin/main` が無いリポジトリで `-d` も失敗する → exit 1・ブランチが残る・出力が (2) と**別の理由**（判定できない）を示す。exit 2 ではない
4. （既存の 2 本「.delegate/ だけが残る…」「align で detach…」は remote 無しで `-d` 経路を通る回帰として残す）

clean-worktrees.test.mjs: 1 と 2 の同型（孤児ブランチが origin/main の祖先なら `--force` なしで消える／固有コミットなら残って exit 1）。既存の「未マージコミットのあるブランチは…残り、--force の再実行で…消える」はそのまま通るはず（origin/main 無し → 判定できず残す）。

偽の緑の罠:
- **本体の HEAD を動かし忘れる**: verify-tree の `initRepo` は本体を `feat/x` に置く。委譲先ブランチを `feat/x` の HEAD から作ると、修正前でも `-d` が通る。テスト 1 は「本体の HEAD から到達できず、origin/main からは到達できる」コミットにブランチを置き、**修正前のコードで落ちることを確かめる**（Red を見る）。手順例: `feat/x` でコミット X → `update-ref refs/remotes/origin/main X` → `worktree add -q <wt> -b worktree-agent-abc X` → 本体を `checkout -q main`（seed のまま）。`-d` は他のブランチ（`feat/x`）を見ないので、X が `feat/x` に在っても再現する
- **upstream が付いてしまう**: fixture で `git remote add origin …` を足し、`worktree add -b … origin/main`（名前で開始点を指定）とすると upstream が自動で付き、修正前でも `-d` が成功する（実測）。開始点は sha で渡すか、remote を足さず `update-ref` だけで `refs/remotes/origin/main` を作る
- **テスト 2 が別の理由で残る**: origin/main を作り忘れると、128 経路でブランチが残り、(2) が理由違いのまま緑になる。出力の理由の文言まで assert する。テスト 3 と組で、2 つの変異を当てて落ちることを確かめる: (i) 祖先の判定を `status === 0` から `status !== 1` に変える（128 を祖先扱い→消す）とテスト 3 のブランチ残存 assert が落ちる (ii) 「非 0 はすべて固有コミット」とする（128 と 1 を混ぜる）とテスト 3 の理由の文言 assert が落ちる
- **短縮名の曖昧さ**: 実装が `origin/main` の短縮名を使っても fixture では気づけない。必要ならローカルブランチ `origin/main`（古い sha）を置くテストで、完全名の使用を固定できる（優先度は低い）
- align で detach した worktree（実運用の形）でも 1 が通ること（ブランチ名を harness 命名から導く経路）を、1 の fixture で `align` を挟むか、既存の「align で detach」テストの変種で確かめる

## (d) 修正で偽になる散文・コメント

- `scripts/clean-worktrees.mjs` ヘッダ「成果保全の最終防衛は git 自身の拒否（`worktree remove` / `branch -d`）に委ねる」——ブランチは自前の祖先判定＋`-D` で消すようになるので偽。「ブランチは origin/main の祖先かを判定してから消す」へ書き換える
- `scripts/clean-worktrees.mjs` 孤児掃除の上のコメント「未マージゆえに -d が拒否された過去実行の残り（squash マージのみの本リポジトリでは worktree 削除後にブランチだけ残るのが標準経路）も同じ規則で掃討される」——「-d が拒否」の部分は判定の変更に合わせて言い直しが要る。squash でコミットを持つブランチは今後も残る（祖先にならない）ので「標準経路」の主張自体は真
- `scripts/clean-worktrees.mjs` の警告「孤児ブランチ削除に失敗（未マージ？ --force で消せます）」——理由を分けるなら更新
- `scripts/verify-tree.mjs` `cmdRemove` の `branch -d` の周辺にコメントが無い。新しい判定の理由（`-d` は本体の HEAD に照らす・#1507）を 1 行置く。ヘッダの終了コード行は真のまま
- `.claude/skills/implement/SKILL.md`「Step 4 — 仕上げ」: exit 1 の読み方が「.delegate/ 以外の変更」「ロック」「ディレクトリだけ残った」の 3 つで、**ブランチを残した場合が無い**（修正前から欠けていたが、修正後は「固有コミットがあるので残した」が正規の報告になる）。中身を確かめてから `git branch -D` するか残すか、を足す
- `docs/adr/ADR-implement-verification-tool-tree.md` L25 は凍結（直さない）。真のまま

## (e) 確信の持てない所見（⚠️）

- ⚠️ harness が `worktree-agent-*` に upstream を付けるかは未測定（現在このリポジトリに `worktree-agent-*` ブランチが無く、`branch.worktree-agent*` の設定も無い）。実測で「開始点を `origin/main` の名前で渡すと upstream が付き `-d` が通る」ので、症状が出た以上 harness は sha か HEAD から作っている（＝upstream なし）と推測するが、裏取りしていない
- ⚠️ squash マージで委譲先がコミットを作ったブランチは `origin/main` の祖先にならない。この修正が効くのは「委譲先がコミットを作らなかった」場合（`/implement` 3b の通常形）だけ。clean-worktrees のコメントが「標準経路」と呼ぶ squash 残りは引き続き `--force` 頼み。issue の受け入れ条件はこれを要求していないが、報告文で「squash 済みの可能性」を示すかは判断
- ⚠️ 共有関数の置き場。`clean-worktrees.mjs` は export を持たず import 時に走るトップレベルスクリプトなので、テストからは子プロセスでしか呼べない。`verify-tree.mjs` から export して `clean-worktrees.mjs` が import する形は依存の向きが逆に見える（掃除ツールが検証ツールに依存）。小さな共有モジュールの新設も候補（新設 `.mjs` に `governance:check` の照合があるかは未確認）
- ⚠️ ブランチを残したときの exit を 1 に据える案は、「固有コミットがある＝正しい失敗」と「消せなかった」を同じ 1 にする。`/implement` Step 4 で exit 1 が出るたびに出力を読む運用なら問題ないが、0 にして報告だけにする案もありうる（issue は「残して報告」とだけ書く）
- ⚠️ 判定と `-D` の間の TOCTOU は実害がほぼ無いと見たが、比較つき削除（`update-ref -d <ref> <old>`）にするなら `branch.<b>.*` の設定掃除を別に要する
- ⚠️ `clean-worktrees.mjs` は `git` を `process.cwd()` で打つ。linked worktree から打つと `-d` はその worktree の HEAD に照らす。共有関数は cwd を引数に取り、`refs/remotes/origin/main` は worktree 間で共通なので結果は変わらない（はず。未測定）
