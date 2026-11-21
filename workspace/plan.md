# 計画 — issue #1496 start-issue Step 2 を worktree でも動く手順にする

## 目的と受け入れ条件

目的: `/start-issue` Step 2 の手順を、本体のチェックアウトでも harness の worktree でも同じ形で動き、最新の `origin/main` を起点にしたブランチができるものにする。

受け入れ条件（issue #1496 から）:

- A1 本体のチェックアウトと harness の worktree の両方で、Step 2 の手順がそのまま通り、最新の `origin/main` を起点にしたブランチができる
- A2 作業ツリーが clean であることの確認が残っている
- A3 `npm run governance:check` が通る

## 設計の決定

- D1 手順を次の 2 行にする。`git fetch origin` → `git switch --no-track -c <feat|fix|chore>/<short-name> origin/main`。`main` を checkout しないので、worktree でも止まらない
- D2（Q1）`--no-track` を入れる。リモート追跡ブランチを起点にすると、git の既定（`branch.autoSetupMerge`）で upstream が `origin/main` になる（#1493 で実測）。upstream が `origin/main` のまま `-u` なしで push すると、`.claude/hooks/pre-bash.mjs` が `gh pr create` を「未 push のコミットがある」と誤って止める（3b で実測。害の向きは停止）。upstream は Step 6 の `git push -u origin HEAD` で同名のリモートブランチに決まる（`origin/main` のままでも `-u` で張り替わる）。理由を手順の直後に 1 文で書く
- D3（Q2）ローカルの `main` は更新しない。`main` がどこかで checkout 中なら、本体からも worktree からも `git fetch origin main:main` は拒まれる（3b で実測）。ローカルの `main` を読む他の手順は、#1495 で `origin/main` を起点にした（3c）か、読み過ぎの向きにしか働かない（3b）
- D4（Q3）`/deps-update` の Step 1 は変えない（範囲外。user 起動専用で、散文にコマンドを書いていない）。PR 本文の「残したもの」に書く

## 変更するファイル

| ファイル | 対象 |
|---|---|
| `.claude/skills/start-issue/SKILL.md` | 「Step 2 — main 最新化 & ブランチ作成」の見出しとコードブロックと前後の文 |
| `.claude/agents/code-reviewer.md` | 起動時の手順 1 の `git diff main...<sha>`（実装中に追加。下の「実装中に判明した作業」） |
| `.claude/skills/retrospective/SKILL.md` | 「Step 1 — サイクルの全体像を把握」の `git log` / `git diff` の起点（同上） |
| `.claude/skills/implement/SKILL.md` | 「3b. 委譲へ渡すもの」の `git diff --name-only main...<sha>`（同上） |

## 実装の順序

1. Step 2 のコードブロックを D1 の 2 行にし、`--no-track` の理由を 1 文で添える
2. 見出しの「main 最新化」が手順と合わなくなるので、「最新の origin/main からブランチ作成」へ改め、旧見出しを `git grep` で数えて参照を直す

## 不変条件と異常系

- 作業ツリーに未保存の変更があれば止まる（既存の文を残す）
- `git switch -c` は同名のブランチが既にあると失敗する（git が止める。手順で扱わない）
- `git fetch` が失敗したら、古い `origin/main` から切ることになる。`git fetch` を `&&` で繋ぐか、失敗したら止まると書く

## テスト方針と検証コマンド

- 文書だけの変更。`npm run verify:tree '--' run --sha <sha> --categories E,F`
- `/implement` Step 3 の委譲: レビューと読み合わせ。変異注入は該当なし（`.claude/rules/safety-nets.md`「規範（ドキュメント・スキル・チェックリスト）を足すとき」）
- A1 の確かめ: 敵対的調査（3b）が一時リポジトリで、本体相当（`main` を checkout 中）と worktree 相当の両方で手順を打った結果を使う。あわせて、このセッションの #1494〜#1496 のブランチ作成が同じ手順で通った実績がある

## SPEC.md・関連文書の更新の要否

- `SPEC.md`: 不要
- `docs/`: 不要

## 作業項目

### Phase 1

- [x] Step 2 のコードブロックを `git fetch origin` と `git switch --no-track -c … origin/main` にし、`--no-track` の理由と、`git fetch` が失敗したら止まることを書く（D1・D2）
- [x] Step 2 の見出しを改め、旧見出しへの参照を `git grep` で数えて直す

### 実装中に判明した作業（委譲レビュー ラウンド 1 の指摘から）

- [x] 「本体が `main` を掴んでいる」を「checkout している」に直す（訳語）
- [x] ローカルの `main` を起点にした差分の 3 か所（`.claude/agents/code-reviewer.md`・`/retrospective` Step 1・`/implement` 3b）を `origin/main` 起点にし、Step 2 にその旨を書く。計画 D3 は「読み過ぎの向きにしか働かない」として範囲外にしていたが、消した `git pull --ff-only` はローカルの `main` を進める唯一の手順で、この変更で `main` はさらに古くなりやすくなる。code-reviewer と retrospective では、他の PR の変更が所見やサイクルの範囲に混ざる（読み過ぎではなく誤りの向き。委譲先が使い捨てのリポジトリで実測）。自分の変更が悪化させる害なので範囲に入れた

### 検証の記録

- ラウンド 1: アンカー `48b7401e` / ツール green（E・F）/ 指摘 High 1（訳語「掴む」）・Medium 1（ローカルの `main` を進める手順が消え、`main` 起点の差分の害が増える）→ 2 件とも直した / 実地の確かめ: 委譲先が使い捨てのリポジトリで、本体相当と worktree 相当の両方で新しい 2 行を打ち、最新の `origin/main` から upstream なしで切れ、`push -u` で同名が upstream になることを確かめた / 変異注入 該当なし
- ラウンド 2: アンカー `7cd7bd88` / ツール green（E・F）/ ラウンド 1 の 2 件は解消 / 新規 High 1（`/retrospective` の `git diff origin/main..HEAD` は 2 点形で、マージの後に自分の変更が消え他の PR が逆向きに出る。委譲先が使い捨てのリポジトリで実測）・Low 2（以前からの問題: 複数 PR にまたがるサイクルでは Step 1 の範囲が空になる／書き方: Step 2 の括弧の数え上げ・code-reviewer の括弧の連続）→ High と書き方の 2 点を直した。以前からの問題は範囲外として PR 本文の「残したもの」へ / 変異注入 該当なし
- ラウンド 3: アンカー `354d9212` / ツール green（E・F）/ ラウンド 2 の指摘は解消（retrospective の 3 点形を S1〜S4 で打ち直し、途中で main を merge したブランチでも正しいことを実測）/ 新規 Low 1（code-reviewer の括弧の掛かり先）→ 直した / 変異注入 該当なし

## 未確定（実装前に潰す）

- [x] 敵対的調査（3b）の所見を反映する — `workspace/research.md`「敵対的調査（3b）の反映」に採否を書き、D2・D3 の理由を実測に合わせて直した

## 承認

- [x] 自己承認 — 2026-11-21 / 仕様変更: なし — `SPEC.md` を変更ファイルに含まない（開発プロセスの文書だけ） / 戻せない変更: なし — 永続形式・識別子/キー形式に触れない / 人の裁定: なし

## plan-review 結果

- リスク: 高（skills を変更する）
- レビュー方式: 独立導出1体（Step 2b。issue に書かれていない人間の裁定は計画の前提に無い）
- エージェント数: 1（成果物 `workspace/plan-review-1496-derive.md`。計画を見ていないと申告。使い捨てのリポジトリで git の挙動を E1〜E7 として測った）

### 要対処

- なし（変更対象・`--no-track`・ローカルの `main` を更新しない・`git fetch` の失敗で止まる、はいずれも計画と一致）

### 軽微

- ローカルの `main` を起点にした差分が他にもある（`.claude/agents/code-reviewer.md`・`/implement` 3b・`/retrospective`・`docs/build-commands.md` の `race:boundaries --base` の例）— 範囲外。読み過ぎの向きにしか働かない。PR 本文の「残したもの」に書く
- `/deps-update` の Step 1 も同じ型 — 範囲外（D4）
- `git fetch` の失敗で止まることは、`&&` ではなく文で書く（#1324 の複合形の扱い）— 実装で反映する
- clean の確認を `git status --porcelain` で書くと、Step 2 の前に `workspace/` へ書いたものに当たる — Step 2 は Step 3 の `workspace/` より前なので当たらない。手段は既存の文のまま変えない

### 未検証

- `git fetch origin` と `git fetch origin main` の違い — 前者を使う（全リモート追跡ブランチを更新するだけで、結論に依存しない）

### 判断

- 実装着手: 可

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体
- エージェント数: 1（3b の敵対的調査を含めると 2）
- 要対処: 0 件
- 未検証: なし（`git fetch` の refspec の違いは結論に依存しない）
- check スキル: なし
- 散文の全件列挙: 語 3 / 総件数 11 行 / ★未掲載 2（計画へ足した 0。hook のテストの入力例と PERFORMANCE.md の別の `git checkout`） / ☆行未掲載 0
- 試行（#1300・高リスクのみ・独立レビュー 1 回ごとに 1 行）: 方式 Step 2b / 設計変更 0 / 主の見落とし 0
