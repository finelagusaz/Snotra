# 計画 — issue #1514 差分・範囲の起点とする版を 1 か所で宣言する

## 目的と受け入れ条件

目的: 差分・範囲の起点とする版を決める規範を 1 か所に置き、各所に写された理由の説明を正本への参照に置き換える。

受け入れ条件（issue #1514 から。範囲は未確定欄の裁定で絞る）:

- A1 基準の版の規範が 1 か所にあり、issue の表の各場所がそれに従うか、従わない理由（個別の issue に残す）が書かれている
- A2 「ローカルの `main` は古いことがある」の写しが、正本への参照になっている
- A3 `npm run governance:check` が通る

## 設計の決定

- D1 正本は `AGENTS.md`「検証の作法（全タスク共通）」の、検証コマンドが見る範囲の項（「書いた判定ロジック…」）を延長して置く。新しい節は作らない。書く中身:
  - 差分・範囲の起点は、`git fetch` したあとの `origin/main` にする。ローカルの `main` は、checkout している作業ツリーで pull しない限り進まないので古いことがあり、起点にすると他の PR の差分が母集団に混ざる
  - main から切ったブランチの差分・範囲は 3 点形（`origin/main...<rev>`）で取る。2 点形は merge-base を取らず、マージ後に自分の変更が消え、他の PR の変更が逆向きに出る。射程は「main から切ったブランチの差分・範囲」に限る（2 つの版を意図して直接比べる比較（`.claude/hooks/pre-bash.mjs` の 2 端点の照合・`scripts/race-boundaries.mjs` の作業ツリーと merge-base・`governance-docs.md` の `git show HEAD:<file>` と作業ファイル）は射程外。全称で書くとこれらが規範に反する。独立導出が指摘）
  - 既存の「`git diff main...HEAD` は作業ツリーを見ない」の例は `origin/main...HEAD` に揃える
- D2 射程は「差分・範囲の起点」に限る。「merged かの判定」（#1507 の `git branch -d`）と「委譲先が読む規範の版」（#1510）は条項に入れない。入れると、直すまで既存の道具が規範に反する状態を作るため（敵対的調査の争点 5）。#1504 の論点 3（振り返りの範囲の与え方）も入れない
- D3 写しの置き換え: 理由の説明を消し、`AGENTS.md`「検証の作法」を指す。コマンドそのもの（`origin/main...HEAD` など）は手順なので残す
  - `.claude/agents/code-reviewer.md` 起動時 1: 理由の文を参照に替える
  - `.claude/skills/implement/SKILL.md` 3c の軸の違うレビュー: 括弧の理由を参照に替える。3b の rules の照合（`git diff --name-only origin/main...<sha>`）に、主が委譲の前に `git fetch` しておくことを添える（委譲先の worktree は refs を本体と共有するので、主の fetch がそのまま効く）
  - `.claude/skills/retrospective/SKILL.md` Step 1: 括弧の理由（起点と 3 点形）を参照に替える
  - `.claude/skills/start-issue/SKILL.md` Step 2: 「差分の起点には `origin/main` を使う」に参照を添える（主張が手順の帰結なので文は残す）
  - `docs/build-commands.md` の `race:boundaries`: 括弧の理由を参照に替え、`git fetch` を済ませることを添える
  - `docs/check-skill-skeleton-design.md` の差分アンカーの例 `git diff main...HEAD` を `origin/main...HEAD` に揃える
- D4 `.claude/skills/deps-update/SKILL.md` Step 1 の「`git fetch` して `main` を最新にする」は、`git fetch` ではローカルの `main` が進まないので手順として不正確（独立導出が指摘）。「`git fetch` のあと `git merge --ff-only origin/main` で `main` を最新にする」に直す。本体のチェックアウトだけで走らせる運用（#1503 で合意）とその理由の文は変えない
- D5 #1507 には、敵対的調査の補足（委譲先がコミットを作れば squash 後は祖先にならないこと、harness が upstream を付けるかは未測定）をコメントで添える。#1510・#1504 は変えない

## 変更するファイル

| ファイル | 対象 |
|---|---|
| `AGENTS.md` | 「検証の作法（全タスク共通）」の検証コマンドの項 |
| `.claude/agents/code-reviewer.md` | 起動時 1 |
| `.claude/skills/implement/SKILL.md` | 「3b. 委譲へ渡すもの」の rules の照合の項／「3c. 報告を受けてから」の軸の違うレビューの項 |
| `.claude/skills/retrospective/SKILL.md` | Step 1 の括弧 |
| `.claude/skills/start-issue/SKILL.md` | Step 2 の段落 |
| `docs/build-commands.md` | `race:boundaries` の `--base` の項 |
| `docs/check-skill-skeleton-design.md` | 差分アンカーの例 |
| `.claude/skills/deps-update/SKILL.md` | Step 1 の main を最新にする手順 |

## 実装の順序

1. `AGENTS.md` に正本を書く
2. 写しの 6 か所を参照へ置き換える
3. `git grep "ローカルの .main"` で、正本以外に理由の説明が残っていないことを確かめる
4. #1507 にコメントする

## 不変条件と異常系

- 各スキルのコマンド（`git fetch`・`origin/main...HEAD`・`git switch --no-track -c … origin/main`）は変えない
- `retrospective` の「`diff` は 3 点形にする」は手順の指示なので、理由だけを参照へ替え、指示は残す
- 正本の文は、作業ツリーの注意（2 点形・3 点形とも作業ツリーを見ない）を落とさない
- 置き換えで、理由の説明に同居していた起点以外の主張を落とさない（独立導出が列挙）: code-reviewer の「worktree の作業ツリーは空なので `git diff HEAD` は空を返す」／start-issue の「この手順はローカルの main を更新しない」と fetch 失敗で止まること／build-commands の「別の feature ブランチから切ったブランチでは、その分岐元を渡す」／implement 3c の「対象は作業ブランチ名で名指す」

## テスト方針と検証コマンド

- 文書だけの変更。`npm run verify:tree '--' run --sha <sha> --categories E,F`
- `git grep -n "ローカルの .main" -- . ':!docs/superpowers' ':!docs/adr' ':!docs/design' ':!workspace'` が `AGENTS.md` の 1 か所だけになる
- `/implement` Step 3 の委譲: レビューと、写しの置き換えで落ちた主張が無いかの照合。変異注入は 3b の判定に従う（G-references が読むパスと見出しの構造に触れるかを判定させる）。実地の確かめは、規範が述べる git の挙動（ローカルの main が古いと他の PR が混ざる・2 点形はマージ後に自分の変更が消える）を一時リポジトリで打つ

## SPEC.md・関連文書の更新の要否

- `SPEC.md`: 不要
- `docs/`: `build-commands.md`・`check-skill-skeleton-design.md`（変更するファイルに載せた）

## 作業項目

### Phase 1

- [x] `AGENTS.md`「検証の作法」の検証コマンドの項に、起点（fetch したあとの `origin/main`）と 3 点形の規範を足し、例を `origin/main...HEAD` に揃える
- [x] `code-reviewer.md` 起動時 1 の理由を参照に替える
- [x] `implement` 3b に委譲の前の `git fetch` を添え、3c の理由を参照に替える
- [x] `retrospective` Step 1 の理由を参照に替える
- [x] `start-issue` Step 2 に参照を添える
- [x] `build-commands.md` の `race:boundaries` の理由を参照に替え、fetch を添える
- [x] `check-skill-skeleton-design.md` の例を揃える
- [x] `deps-update` Step 1 の「main を最新にする」を `git merge --ff-only origin/main` の形に直す
- [x] 理由の説明が正本以外に残っていないことを `git grep` で確かめる
- [x] #1507 に敵対的調査の補足をコメントする（https://github.com/finelagusaz/Snotra/issues/1507#issuecomment-7059412256）
- [x] 検証ラウンド 1: sha abb7c961・verify:tree E,F green・指摘 3 件（挙動を変える 2 件を直した: log と diff の形の書き分け・code-reviewer の fetch の前提。言い回しの 1 件は書き直しに吸収）・変異注入 該当なし（検知器と、検知器が読む構造に触れない）・実地の確かめ 一時リポジトリで 3 つの git の挙動を確認（ローカル main が古いと混ざる・2 点形の diff は squash 後に自分の変更が消える・fetch だけでは main は進まず ff-only で進む）、.githooks は ff-only を止めない

## 未確定（実装前に潰す）

- [x] （裁定）範囲 — 規範の新設と写しの置き換えまでにし、#1507（merged の判定）・#1510（委譲先が読む規範の版）・#1504 の論点 3 は個別の issue に残してよいか。承認の問いに含めて決めた: 回答はこの範囲のまま

## 承認

- [x] 承認済み — 2026-11-21 / 問い: "この範囲で承認して、実装へ進めてよろしいでしょうか。それとも #1507 もこの PR に含めましょうか？" / 回答: "このままいきましょ"

## セルフレビュー

- リスク: 高（`AGENTS.md`・skills・agents を変更する）
- plan-review: 独立レビュー1体
- エージェント数: 1（ほかに 3b の敵対的調査 1 体）
- 要対処: 3 件を反映（3 点形の射程を「main から切ったブランチ」に限る／deps-update の fetch だけでは main が進まない手順を直す／置き換えで落ちうる起点以外の主張を不変条件へ）。導出と計画の写しの 5 か所は一致。#1504 の論点 3 は規範では解けないので個別に残す判断と一致
- 未検証: harness が委譲先ブランチに upstream を付けるか（#1507 側の論点）
- check スキル: なし
- 散文の全件列挙: 語 2 / 総件数 11 行 / ★未掲載 1（計画へ足した 0。`scripts/race-boundaries.mjs:251` は作業ツリーの注意で写しではない） / ☆行未掲載 0（計画へ足した 0）
