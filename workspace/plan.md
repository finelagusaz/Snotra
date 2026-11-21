# plan — #1507 verify:tree remove が origin/main の祖先の委譲先ブランチを消せない

## 目的

委譲先のブランチを、`git branch -d` が拒んでも `origin/main` の祖先なら消す。`-d` の基準（打った作業ツリーの HEAD か upstream）は、ローカルの `main` の古さや本体が居るブランチに引きずられる。同じ判定を `scripts/clean-worktrees.mjs` も持つので、共通の関数にして両方を直す。

## 受け入れ条件

1. 本体の HEAD がブランチの先端を含まず（ローカルの `main` が古い状態）、ブランチが `refs/remotes/origin/main` の祖先のとき、`verify:tree remove --delegate-tree` がブランチを消し、exit 0 になる（テストで再現）
2. ブランチに `origin/main` にも HEAD にも無いコミットがあるとき、消さずに「origin/main に無いコミットがある」と報告し、exit 1 になる（テストで再現・理由の文言まで断言）
3. `refs/remotes/origin/main` が無く `-d` も拒むとき、消さずに 2 とは別の理由（判定できない）を報告し、exit 1 になる（exit 2 にしない。テストで再現・理由の文言まで断言）
4. `clean-worktrees`（`--force` 無し）も、1・2 と同じ状態の孤児ブランチを同じく扱う（テストで再現）
5. 既存のテスト（リモートの無いリポジトリで `-d` が通る経路）がそのまま通る
6. `npm test` と `npm run governance:check` が通る

## 変更するファイル

- `scripts/delete-merged-branch.mjs`（新設）: `deleteMergedBranch(cwd, branch)` を export する
- `scripts/delete-merged-branch.test.mjs`（新設）: 判定の理由ごとのテスト
- `scripts/verify-tree.mjs`: `cmdRemove` の `git(root, ["branch", "-d", branch])` を `deleteMergedBranch(root, branch)` へ替え、判定の理由を 1 行のコメントで添える
- `scripts/verify-tree.test.mjs`: `describe("verify-tree remove")` に受け入れ条件 1〜3 のテストを足す
- `scripts/clean-worktrees.mjs`: `--force` 無しの孤児ブランチ削除を `deleteMergedBranch` へ替える。冒頭のコメント（成果保全の最終防衛を `branch -d` に委ねる）、孤児の段のコメント（`-d` が拒否された残り）、失敗の警告文（「未マージ？」）を、判定の変化に合わせて直す
- `scripts/clean-worktrees.test.mjs`: 受け入れ条件 4 のテストを足す
- `.claude/skills/implement/SKILL.md`「Step 4 — 仕上げ」の worktree を片づける項: exit 1 の読み方に「ブランチを残した」場合を足す（理由が「origin/main に無いコミットがある」なら中身を確かめる。消してよければ `git branch -D`）

## 設計

`deleteMergedBranch(cwd, branch)` → `{ deleted: boolean, basis?: "-d" | "origin/main", reason?: string }`:

1. `git branch -d <branch>` を `cwd` で打つ。成功なら `{ deleted: true, basis: "-d" }`（今までの経路。リモートの無いリポジトリの既存テストはここを通る）
2. 失敗したら、`spawnSync` で `git merge-base --is-ancestor refs/heads/<branch> refs/remotes/origin/main` を打ち、終了コードで分ける
   - 0（祖先）: `git branch -D <branch>` で消す。`{ deleted: true, basis: "origin/main" }`
   - 1（祖先でない）: 消さない。`{ deleted: false, reason: "origin/main に無いコミットがある" }` に `-d` の stderr を添える
   - それ以外（128 など。`refs/remotes/origin/main` が無いなど）: 消さない。`{ deleted: false, reason: "origin/main の祖先か判定できない" }` に `-d` の stderr を添える
3. 1 と 128 を混ぜない（`execFileSync` の throw を catch して「非 0 は祖先でない」と読む実装にしない）

決めたこと:
- 基準は `refs/remotes/origin/main` の完全名で指す。短縮名の `origin/main` は、同名のローカルブランチがあるとそちらへ解決される（独立導出で実測）。`AGENTS.md`「検証の作法」が起点を `origin/main` と定めるので、`refs/remotes/origin/HEAD` からは導かない
- fetch はしない。`origin/main` が古ければ「祖先でない」側、つまりブランチを残す安全な側へ倒れる
- `-D` を使い、`update-ref -d` は使わない（後者は `branch.<b>.*` の設定を消さない）
- ブランチを残したときの終了コードは、verify-tree では今までどおり 1（ヘッダの「1 = 消せなかった（remove）」と合う）

呼び出し側の出力: 消したら今の行（`ブランチ削除: <b>`／`孤児ブランチ削除: <b>`）。`basis` が `origin/main` なら「（origin/main の祖先）」を添える。残したら `reason` と stderr を出す。

## 不変条件と異常系

- `origin/main` にも `-d` の基準にも無いコミットを持つブランチは消さない。`-D` を打つのは、祖先と確かめた後だけ
- 委譲先の worktree は、ブランチを消す前に消してある（今の順序を変えない）。祖先の確認から `-D` までの間にブランチを動かす者はいない（⚠ 論理上。測らない）
- `clean-worktrees --force` は今までどおり `-D` で全部消す
- 修正が効くのは、委譲先がコミットを作らなかったブランチである（`/implement` 3b の委譲は `align` で detach し、通常コミットを作らない）。squash マージでは、コミットを作ったブランチは祖先にならないので残る。これは正しい残し方（固有のコミットがある）

## テスト方針

偽の緑の罠（敵対的調査と独立導出の両方が指摘）:
- `verify-tree.test.mjs` の `initRepo` は本体を `feat/x` に置く。委譲先のブランチを本体の HEAD から作ると、修正前でも `-d` が通る。本体の HEAD が先端を含まない状態を作り、修正前のコードで受け入れ条件 1 のテストが落ちる（Red）ことを確かめる
- 始点に追跡ブランチを名前で渡すと upstream が付き、修正前でも `-d` が通る。始点には sha を渡し、`refs/remotes/origin/main` は `git update-ref` で置く
- 受け入れ条件 2 で `origin/main` を置き忘れると、128 の経路で残り、理由違いのまま緑になる。理由の文言まで断言する（`.claude/rules/safety-nets.md`「検出器のカバー範囲は、欠落のパターンごとに検算する」）

テスト:
- `delete-merged-branch.test.mjs`: (a) `-d` が通る → `basis: "-d"` (b) HEAD が古く `origin/main` の祖先 → `basis: "origin/main"` で消える (c) 固有のコミット → 残り、理由「origin/main に無いコミットがある」 (d) `origin/main` が無く `-d` も拒む → 残り、理由「判定できない」
- `verify-tree.test.mjs`: 受け入れ条件 1〜3（exit code と出力の理由）
- `clean-worktrees.test.mjs`: 受け入れ条件 4（祖先なら消える・固有のコミットなら残って exit 1）

変異注入（`/implement` の委譲先が行う）の候補: 祖先の判定を `status !== 1` に変える（128 を祖先として扱う）／1 と 128 の理由を混ぜる／`-d` の失敗後の判定を外す。

検証: `npm test`・`npm run governance:check`（`verify:tree` の E・F）。

## SPEC.md・関連文書

`SPEC.md` は変えない（開発ツールの挙動であり、製品の仕様ではない）。`docs/adr/ADR-implement-verification-tool-tree.md` は凍結された歴史なので触らない。`branch -d` の挙動に依る散文は、2 つのスクリプトのコメントと `/implement` Step 4 の exit 1 の読み方だけ（`git grep "branch -d\|fully merged\|未マージ\|孤児ブランチ"` と `prose:sweep`）。

## 作業項目

### フェーズ 1 — 共通の関数

- [x] `scripts/delete-merged-branch.mjs` にシグネチャだけ置き（常に `{ deleted: false }`）、`delete-merged-branch.test.mjs` の 4 ケースを書いて Red を確かめる
- [x] 設計どおりに実装して Green にする

### フェーズ 2 — 呼び出し側

- [x] `verify-tree.test.mjs` に受け入れ条件 1〜3 のテストを足し、今の `cmdRemove` で 1 が落ちることを確かめる
- [x] `cmdRemove` を `deleteMergedBranch` へ替えて Green にする
- [x] `clean-worktrees.test.mjs` に受け入れ条件 4 のテストを足し、今の実装で落ちることを確かめる
- [x] `clean-worktrees.mjs` を替え、コメントと警告文を直して Green にする
- [x] `/implement` Step 4 の exit 1 の読み方に、ブランチを残した場合を足す
- [x] `npm test` と `npm run governance:check` を通す

### ラウンドの記録

- [x] R1 `a0d5ac57`: verify:tree green（E・F）／指摘: 挙動を変える 3（Step 4「判定できない」の打ち直しが exit 2 で働かない・fetch を先に挟む・clean-worktrees で `-D` の throw がループを止める）を直した。挙動を変えない 2（短縮名の `origin/main`・テストのコメントの精度）は捨てた／変異 M1〜M5 生き残り 0、M6（始点を追跡ブランチ名）も assert が basis を断言するので赤／実地: 一時リポジトリ（bare origin + clone）で Step 4 の手順を打ち、「判定できない」の打ち直しだけが働かなかった
- [x] R2 `402f47e0`: verify:tree green（E・F）／R1 の 1〜3 は同じ道具で解消を確認（3 は `-D` を失敗させる変異で、修正前はスタックトレースで集計が出ず、修正後は「掃討 1 / 失敗 1」で exit 1）／挙動を変える 1（catch の経路を断言するテストが無い）を直した: ref のロックファイルで `-D` を失敗させるテストを足した。捨てた: 既定ブランチが main でないリポジトリ（このリポジトリに当たらない）・`--force` なしの clean:worktrees の巻き添え（消すのはクリーンな worktree と origin/main の祖先だけで、中身を失わない）・Step 4 の文言の精度
- [x] R3 `c3f4b739`: verify:tree green（E・F）／足したテストへの変異: try/catch を外す・ロックファイルを消す・catch で失敗に数えない、は assert で検知。catch の後に break する変異が生き残った（孤児は名前順に処理され、失敗する側が最後だった）→ 孤児の名前を z-free に替え、順序に依ることをコメントで添えた（委譲先が同じ改名で break の変異が赤になることを測った）

## 未確定（実装前に潰す）

- [x] 基準を `origin/main` に固定するか、`refs/remotes/origin/HEAD` から導くか — 固定する。`refs/remotes/origin/main` の完全名で指す（設計の「決めたこと」）
- [x] 共通化の置き場 — 新しいモジュール `scripts/delete-merged-branch.mjs`。`scripts/lib/` は PowerShell のモジュール置き場で、`scripts/lib/**` を触ると E の `test:powershell` の条件に当たる。`verify-tree.mjs` から export すると、`clean-worktrees.mjs` が verify-tree の依存（`governance/lib.mjs`）まで読み込む
- [x] `-d` と祖先の判定の順序 — `-d` を先にする（独立導出の案を採る）。既存の経路を変えずに済み、消える集合は祖先の判定を先にする案と同じである
- [x] harness が委譲先のブランチに upstream を付けるか — 判定が upstream を見ないので、答えに依らず効く。測らない
- [x] ブランチを残したときの終了コード — 1 のまま（ヘッダの定義と合い、`/implement` Step 4 は exit 1 で出力を読む）

## 承認

- [x] 自己承認 — 2026-11-21 / 仕様変更: なし — `SPEC.md` を変更ファイルに含まない（開発ツールの挙動） / 戻せない変更: なし — 永続形式・識別子/キー形式に触れない / 人の裁定: なし

## セルフレビュー

- リスク: 高（`scripts/` と skills を変える）
- plan-review: 独立レビュー1体（Step 2b の独立導出。計画を目にしていないと成果物に記載）
- エージェント数: 1（3b の敵対的調査を含めると 2）
- 要対処: 4 件を反映 — 失敗の理由を 1 と 128 で分けて断言する／`origin/main` が無く `-d` も拒むケースのテスト／`/implement` Step 4 の exit 1 の読み方／`clean-worktrees.mjs` の警告文。判断の不一致は順序（`-d` を先にする案を採った）
- 未検証: harness が委譲先のブランチに upstream を付けるか（判定に影響しない）
- check スキル: なし（新しい関数の導入は `/dry-check` の対象だが、既存に同等の判定は無い — `git grep "is-ancestor" -- scripts .claude/hooks .githooks` で確かめる）
- 散文の全件列挙: 語 4 / 総件数 15 行 / ★未掲載 1（計画へ足した 0。`package.json` の `clean:worktrees` の行は呼び出しを変えないので対象外） / ☆行未掲載 0
