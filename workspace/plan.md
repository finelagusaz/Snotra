# 計画: issue #1499 — pre-bash の gh pr create 検出で、引用符・エスケープで語を割った綴りを検出する

調査: `workspace/research.md`（敵対的調査の所見と採否を含む）

## 目的と受け入れ条件

hook に止められたエージェントが `gh pr "create"` のような綴りへ書き換えて、PR 前ゲート（空 PR・plan.md の未チェック・ツールの要約）を素通りする事故を塞ぐ。

1. issue の表の 8 綴り（`gh pr create --fill` / `gh pr "create"` / `gh pr 'create'` / `gh "pr" create` / `"gh" pr create` / `gh pr cre""ate` / `gh pr cr\eate` / `git push -u origin HEAD; gh pr "create" --fill`）を、`decide()` が `gh pr create` として扱う（ゲートへ回す）。表に無い同類として、行継続（`gh pr \` + 改行 + `create`、PowerShell の `` ` `` + 改行）と、PowerShell のエスケープ（`` gh pr cr`eate ``）も扱う
2. 言及の扱いがテストに固定されている
   - 検出しない: `git commit -m "gh pr create を足す"`、`grep -n "gh pr create" CLAUDE.md`
   - 検出する（受容する過剰検出・fail-closed）: `node -e "console.log('gh pr create')"`、`"gh pr create"` を 1 語として引用した形
   - 検出しない（正規化後のほうが正しい）: `gh pr create'x'`（シェルでは `createx`）
3. 受容する未対応の形（`$'create'`・`$"create"`・`${X}create`・`c$()reate`）が、hook 冒頭の一覧とテストに固定されている
4. 正規化は `gh pr create` の判定だけに通る。`judgeCommandShape` は元の文字列で判定する（`--body-file "C:\Users\x\y.md"` を含む `gh pr create` は、今までどおり `\` パスの判定で止まる）
5. `main()` の verify-tree の import 判定と `decide()` の検出が、同じ関数を通る。`CWD_CHANGE` の位置比較と `hasSafeChain` も、検出と同じ正規化後の文字列で判定する（`echo "a" "b" "c" "d" "e" "f"; cd x; gh pr "create"` が cwd 変更で block される）

## 変更するファイル

- `.claude/hooks/pre-bash.mjs`
  - 新設: `export function ghPrCreateView(command)` — 行継続（`\` / `` ` `` + 改行）を連結し、残る `"` `'` `\` `` ` `` を取り除いた文字列を返す。`gh pr create` の判定だけが使う
  - `hasSafeChain(command)`: 引数は正規化後の文字列を受け取る前提にする（doc にそう書く。中身の正規表現は変えない）
  - `decide`: `judgeCommandShape(command, platform)` は元の文字列のまま。その後 `const view = ghPrCreateView(command)` を作り、`tokenStart(GH_PR_CREATE, view)`・`tokenStart(CWD_CHANGE, view)`・`hasSafeChain(view)` に渡す
  - `main`: import の判定を `tokenStart(GH_PR_CREATE, ghPrCreateView(command))` にする
  - 冒頭コメントの「受容する未対応リスク」: 引用・エスケープで語を割る形は検出することを書き、`$'…'`・`$"…"`・変数展開・置換で語を組み立てる形を受容の一覧へ足す。「`$(...)` の内側」は、`(` が区切りなので現状でも検出される（委譲先のコマンドで実測）ため、一覧から外す。バッククォートは、`` ` `` を取り除くので、先頭の `` `gh pr create` `` は検出されることに合わせて直す
  - `AT_CMD_POS` の注釈「引用の内側までは解釈しない」: 引用符は取り除くが、引用の内側の区切りは解釈しない、という今の意味に合わせて直す
- `.claude/hooks/pre-bash.test.mjs`
  - 新設: `describe("ghPrCreateView — 引用符・エスケープで語を割った綴り（#1499）")`
  - `decide — 分岐表` に、受け入れ条件 4・5 のケースを足す

`SPEC.md` は変えない（開発ツールの変更で、製品の挙動に触れない）。

## 実装の順序

1. 失敗するテストを先に書く（Red）: 受け入れ条件 1〜5 を `pre-bash.test.mjs` に書き、`ghPrCreateView` が無いことで落ちることを確かめる
2. `ghPrCreateView` を実装し、`decide` と `main` の配線を替える（Green）
3. 冒頭コメントと `AT_CMD_POS` の注釈を直す
4. 検証を回す

## 不変条件と異常系

- fail-closed の骨格（既定の exit 2、許可が確定した経路だけが 0）を変えない。`ghPrCreateView` は全域関数で、例外を投げない（文字列の置換だけ）。既存の「5 述語は全域関数である」の describe に倣い、空文字・制御文字を含む入力で投げないことをテストする
- `judgeCommandShape` に渡す文字列を変えない（変えると `\` パスと heredoc の判定が弱まる。調査で実測）
- 検出・`CWD_CHANGE`・`hasSafeChain`・`main` の import は、同じ `ghPrCreateView` の出力を見る（どれかが元の文字列を見ると、偽の理由での block か、cwd 判定の fail-open になる）
- 正規表現（`GH_PR_CREATE`・`FLAG` など）の形は変えない（バックトラックの性質を保つ）

## テスト方針と検証コマンド

- 単体: `ghPrCreateView` の出力、`GH_PR_CREATE` × `ghPrCreateView` の検出表、`decide` の分岐表（受け入れ条件 4・5）
- プロセス起動: 既存の「プロセス起動 — exit code の契約」に、`gh pr "create"` を stdin に渡したときに block（exit 2）になる 1 ケースを足す。`main` の import 判定の配線を、実プロセスで固定する。**block の理由の文言まで assert する**（「verify-tree.mjs を読み込めませんでした」で止まっていないこと。import 判定がずれても exit 2 にはなるので、exit code だけでは配線のずれが緑のまま残る）
- `CWD_CHANGE` のケースも、理由の文言（「作業ディレクトリを変更しています」）まで assert する（ずれた変異が別のゲートで block されても緑にならないように）
- 新しいテストは `ghPrCreateView` か `decide` を通して呼ぶ。既存の describe のように `GH_PR_CREATE.test(raw)` を直接呼ぶと、生の正規表現を測るだけになる
- 変異注入（`/implement`「3b. 委譲へ渡すもの」の委譲先が行う）: (a) `decide` の検出を元の文字列へ戻す (b) `main` の import 判定だけを元の文字列へ戻す (c) `CWD_CHANGE` の比較だけを元の文字列へ戻す (d) `judgeCommandShape` に正規化後の文字列を渡す。それぞれで新設のテストが assert で赤になることを確かめる
- 検証コマンドは `docs/build-commands.md`「変更後の検証チェックリスト」の該当カテゴリ（hook のテストと governance:check）に従う

## SPEC.md・関連文書の更新

- `SPEC.md`: 不要
- `docs/`: 不要（hook の判定の正本は `pre-bash.mjs` の冒頭コメントとテスト。ルート `CLAUDE.md`「フック」がそう定めている）

## 作業項目

### Phase 1 — Red

- [ ] `pre-bash.test.mjs` に受け入れ条件 1〜5 のテストを書き、落ちることを確かめる

### Phase 2 — Green

- [ ] `ghPrCreateView` を実装して export する
- [ ] `decide` の検出・`CWD_CHANGE`・`hasSafeChain` の入力を `ghPrCreateView(command)` にする（`judgeCommandShape` は元の文字列のまま）
- [ ] `main` の import 判定を `ghPrCreateView` 経由にする
- [ ] プロセス起動テストに `gh pr "create"` の block を足す

### Phase 3 — 注釈と検証

- [ ] 冒頭コメントの「受容する未対応リスク」と `AT_CMD_POS` の注釈を、実装に合わせて直す
- [ ] hook のテストと `npm run governance:check` を通す
- [ ] 範囲外の 2 件を PR 本文の「範囲外」に書く文面を用意する: (a) `usesNoVerify`・`pullWithoutFfOnly` の同じ形の素通り（`git "pull"`・`git commit --no-ver""ify`） (b) `hasSafeChain` が最初の一致の位置を使うため、引用の内側の言及が位置を前へずらす修正前からの素通り（`git commit -m "a && git push" && gh pr create` が allow になる。独立導出で実測）

## 未確定（実装前に潰す）

- [x] （裁定）引用符・エスケープで語を割った `gh pr create` を、検出するか、受容の一覧へ足すか — 2026-11-21 にユーザーへ問い、「検出で承認し実装」の回答を得た。計画は検出の形のまま。問うたときの材料: 推奨は検出。理由: #1493 で止められたエージェントが自然にたどり着いた事故の形で、`gh` はコマンド位置にある。反対の材料: `gitSegments` の注釈は、引用・行継続の解釈を「`sh -c` と同格の意図的迂回」として受容した先例である。検出すると、`node -e "console.log('gh pr create')"` のような言及が新たに検出される（fail-closed、ゲートが緑なら allow）。ユーザーに「検出する（推奨）／受容の一覧へ足す」で問う

## 承認

- [x] 承認済み — 2026-11-21 / 問い: "引用符で語を割った gh pr create を、検出する計画で承認なさいますか？" / 回答: "検出で承認し実装"

## plan-review 結果

- リスク: 高（hook の変更）
- レビュー方式: 独立導出 1 体（Step 2b。成果物 `workspace/plan-review-1499-derive.md`。workspace/ の計画は読んでいない。ライブの hook の拒否メッセージで plan.md の未チェック件数だけを目にした、と申告あり。網羅性の証拠として使える）
- エージェント数: 1

### 要対処
- プロセス起動テストと `CWD_CHANGE` のケースで、block の理由の文言まで assert する — 計画の修正（テスト方針）— exit 2 は import 判定がずれても出る（`readVerifyState` が tool=null で block する）ことを `pre-bash.mjs` の `readVerifyState` 冒頭で再照合した
- 新しいテストは `ghPrCreateView` / `decide` を通す — 計画の修正（テスト方針）— 既存の describe は `GH_PR_CREATE.test(raw)` を直接呼ぶ（`pre-bash.test.mjs` 121 行の describe）
- 範囲外の 2 件（`usesNoVerify` 系の同じ形・`hasSafeChain` の最初の一致による修正前からの素通り）を PR 本文へ — 計画の修正（作業項目）

### 軽微
- 導出は `hasSafeChain` の内側で正規化する形、計画は呼び出し側で `view` を渡す形。どちらも「同じ文字列の上で測る」不変条件を満たす。計画の形を採る（`hasSafeChain` は export されており、`decide` で 1 回だけ正規化して全判定へ同じ `view` を渡すほうが、ずれる箇所が減る）
- 導出は行継続の `\` + 改行を残す、計画は連結する。改行を残すと区切りに見えて `hasSafeChain` が変わるので、計画の形を採る
- 導出は PowerShell の `` ` `` を受容側に置く、計画は取り除く。hook の管轄が PowerShell を含む（`TARGET_TOOLS`）ので、計画の形を採る
- `"gh pr create"` を 1 語として引用する形が新たに検出される（block 方向）。受け入れ条件 2 の過剰検出のテストに 1 ケース足す

### 未検証
- 実際の過剰検出の頻度（過去の transcript の集計は分類器に拒まれた）
- PowerShell ツールが hook に渡す文字列が逐語かどうか

### 判断
- 実装着手: 可（裁定は「検出」で解消）

## セルフレビュー

- リスク: 高
- plan-review: 独立導出 1 体
- エージェント数: 2（3b の 1 体・Step 2b の 1 体）
- 要対処: 3 件を反映（上の「plan-review 結果」）。3b からは 3 件を反映 — 正規化を `gh pr create` の判定だけに限る／`CWD_CHANGE` と `main` を同じ正規化に通す／新たな過剰検出と `$'…'` 系の受容をテストと一覧に固定する
- 未検証: 上の「plan-review 結果」の未検証 2 件
- check スキル: なし（新しい関数 `ghPrCreateView` は `/dry-check` の対象だが、同等の前処理が hook と `scripts/` に無いことを grep で確かめた。対称ペア・UI モード・永続形式には当たらない）
- 散文の全件列挙: 語 5（GH_PR_CREATE・hasSafeChain・引用の内側・受容する未対応・意図的迂回）/ 総件数 51 行 / ★未掲載 1（計画へ足した 0。`scripts/governance/lib.mjs` 88 行は Markdown のフェンスの話で無関係）/ ☆行未掲載 0（計画へ足した 0）。`gitSegments` の doc（`pre-bash.mjs`「引用・継続の解釈は shell パーサ相当」）は git のセグメント判定の話で、変更後も真なので触らない
