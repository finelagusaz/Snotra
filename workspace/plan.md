# 計画 — issue #1523 pre-bash の検出に残る素通りを塞ぐ

## 目的

`.claude/hooks/pre-bash.mjs` で、引用符で語を割る・空白を含む引用値を前置する・引用の内側に `git push` を書く形の素通りを塞ぐ。今の検出を 1 件も減らさない。

## 受け入れ条件

1. 次がすべて検出される（今は素通りする）
   - `usesNoVerify`: `git commit --no-ver""ify`・`git "commit" -n`・`A="a b" git commit --no-verify`・`git -C "my dir" commit -n`
   - `pullWithoutFfOnly`: `git "pull"`・`"git" pull`・`A="a b" git pull`・`git -C "my dir" pull`・`git -c "a b=c" pull`
   - `ghPrCreateSites`: `GH_TOKEN="a b" gh pr create`・`gh --repo "a b" pr create`・`gh --repo="a b c" pr create`
   - `pullWithoutFfOnly`（追加）: `git --git-dir="a b c" pull`
   - `needsPyEncoding`（win32）: `A="x y" python -c "あ"`
2. `hasSafeChain` が false: `git commit -m "a && git push" && gh pr create`・`git push && echo "x && gh pr create" ; gh pr create`・`git commit -m "a && git push && gh pr create" ; gh pr create`・`echo "&& git push" && gh pr create`・`git push && gh pr create; gh pr create`・`git push && gh pr create || gh pr create`・`git push && gh pr create & gh pr create`。`decide` が静的 allow しない: `git push && gh pr create --fill; gh pr "create"`。true のまま: `git push -u origin HEAD && gh pr create --title "t" --body-file x`・`git commit -F f && git push -u origin HEAD && gh pr create --title "t b"`・`git push && gh pr create && gh pr create`。`git push "--dry-run" && gh pr create` などの引用で割った dry-run は、今と同じく `decide` で静的 allow にならない（回帰テスト）
3. 今の検出が消えない: `A=it's git pull`・`A=" git pull`・`A=it's git commit --no-verify`・`A=it's python -c 'あ'`（win32）・`A=it's gh pr create`・`A=a"b;c"d gh pr create`・`A="x;y"z gh pr create`・`A=x;gh pr create` が今と同じく検出される
4. 意図した過剰検出として、`git commit -m "--no-verify"`・`git log "--no-verify"`・`git commit -m "fix don't use -n"` が止まる。`git log --grep="--no-verify"`・`git commit -m "pull request"`・`echo "git pull"` は止まらない
5. 病的入力（引用値の大量の並び・対にならない引用符・大量のフラグ・`("; A=x'").repeat(n) + " gh x"` の形）で、各判定と `decide` が throw せず 500ms 未満で終わる（後者は今 90KB で 6.3 秒かかる既存の二乗。`ENV_PREFIX` の値が区切り文字を越えて食うことによる。独立導出で実測）
6. 既存のテストがすべて通る

## 変更するファイル

| ファイル | 対象 | 変更 |
|---|---|---|
| `.claude/hooks/pre-bash.mjs` | `ghPrCreateView` | `unquotedView` へ改名する（`gh pr create` 以外の判定も使うため）。中身は変えない |
| 同 | 新しい定数 `ENV_PREFIX_QUOTED`・`FLAG_QUOTED` | 値を**部品の連接**として読む版。部品は `"[^"]*"` / `'[^']*'` / 1 字（`ENV_PREFIX_QUOTED` は空白・引用符・区切り文字以外、`FLAG_QUOTED` は空白・引用符以外）で、部品どうしは先頭の文字で排他にする（分割が一意になり爆発しない）。`ENV_PREFIX_QUOTED` の値は部品の 0 個以上の連接（`A=a"b;c"d`・`A="x;y"z`・`A=x"a b"` を 1 つの代入として読む。選択肢 1 つとして読むと前 2 つの検出が消える。差分レビューで実測）。`FLAG_QUOTED` はフラグ名も値も部品の連接で、先頭は先読みで `-`・空白でないことを確かめる（`[^-\s]` で 1 字消費すると開きの引用符を食い、`git -C "a b" pull` を検出できない。フラグ名を連接にすると `--repo="a b c"`・`--git-dir="a b c"` も読める。独立導出と差分レビューで実測）。旧い `FLAG` は残す |
| 同 | `ENV_PREFIX` | 値（今の `\S*`）を、空白と区切り文字（`;` `&` `\|` 改行 `(` `)` `{` `}`）を含まない語にする。区切り文字を越えて食う既存の二乗を断つ。区切りの後ろは `AT_CMD_POS` が拾い直すので、検出は減らない（`A=x;gh pr create` などをテストで固定する） |
| 同 | `GH_PR_CREATE` に並ぶ `GH_PR_CREATE_QUOTED`、`GIT_AT_CMD` に並ぶ `GIT_AT_CMD_QUOTED`、`GIT_COMMIT`・`GIT_PULL` に並ぶ `GIT_COMMIT_QUOTED`・`GIT_PULL_QUOTED`、`PYTHON_AT_CMD` に並ぶ `PYTHON_AT_CMD_QUOTED` | 旧い版と引用値を読む版の**どちらかで当たれば検出**とする。片方だけに寄せない（引用値を読む版は対にならない引用符で外れ、旧い版は空白を含む引用値で外れる） |
| 同 | 新しい関数 `firstCommandAt(res, text)` | 正規表現の組の中で最も早い `tokenStart` を返す（どれも当たらなければ -1） |
| 同 | `ghPrCreateSites` | 見え方ごとに `firstCommandAt([GH_PR_CREATE, GH_PR_CREATE_QUOTED], text)` で位置を取る |
| 同 | `hasSafeChain` | 最初の gh（push より前に gh が無いことの確認）と、**最後の** gh（両方の正規表現の全一致のうち最も後ろ）の 2 つの位置を取る（`firstCommandAt` と、全一致の最後を返す `lastCommandAt`）。push の終端から**最後の** gh までの区切りがすべて `&&` でなければ false（最初の gh までしか見ないと `git push && gh pr create; gh pr create` を通す。差分レビューで実測）。さらに、最後の gh より前に、引用符・エスケープ文字（`"` `'` `\` `` ` ``）が 1 つでもあれば false。`decide` はこれを見え方ごとに呼ぶので、語割りの 2 つめの gh（`git push && gh pr create --fill; gh pr "create"`）は見え方の側で落ちる。引用の内側の `git push` や `gh pr create` を、チェーンの push や gh と取り違えない（最初の一致だけでは `git push && echo "x && gh pr create" ; gh pr create` を防げない）。保守的に、引用の解釈をせず静的 allow を外して動的判定へ回す。代償として `git push origin "feat/x" && gh pr create` のように push や前置に引用を書いた形も動的判定へ回る |
| 同 | `REMEDY` | 動的判定で止めたときの文言に、「引用符・エスケープを含むチェーンは静的に安全と判定しないので、その場合は `git push -u origin HEAD` を単独で打ってから PR を作る」を足す（既に `&&` で繋いだ人へ「`&&` で繋いでください」だけを返すと、復帰手順が当てはまらない） |
| 同 | `gitSegments` | `GIT_AT_CMD` と `GIT_AT_CMD_QUOTED` の両方の一致からセグメントを集める（開始位置で重複を除く） |
| 同 | `usesNoVerify`・`pullWithoutFfOnly` | 元の文字列と `unquotedView` の見え方の**両方**で `gitSegments` を取り、どちらかで当たれば検出。セグメントの判定は旧い版と引用値を読む版のどちらかで当たれば検出 |
| 同 | `needsPyEncoding` | `PYTHON_AT_CMD` と `PYTHON_AT_CMD_QUOTED` のどちらかで当たれば対象 |
| 同 | 冒頭のコメントと各 doc | 見え方を `gh pr create` 以外にも使うこと（冒頭の「`gh pr create` の判定だけは見え方の上でも探す」を直す）、旧い版と引用値を読む版を並べる理由（排他を保ったまま退行させない）、意図した過剰検出（受け入れ条件 4）、`hasSafeChain` の保守的な扱いを書く。受容する未対応リスクに、直した後も残る形（エスケープした空白 `a\ b`・引用内のエスケープした引用符 `"a\" b"`・cwd 判定が最初の gh としか比べないこと）を足す。`gitSegments` の doc は、引用内の区切りの過小検出（真のまま）と見え方の話を書き分ける |
| `.claude/hooks/pre-bash.test.mjs` | `ghPrCreateView` の import と describe 名 | `unquotedView` へ |
| 同 | 既存のコメント「正規化は gh pr create の判定だけに通す」（`decide` の describe 付近） | noVerify・pull にも見え方を通すので、偽にならない形へ直す |
| 同 | 新しい describe | 受け入れ条件 1〜5 をそれぞれ縛るテスト |

## 実装の順序

1. 受け入れ条件 1〜5 のテストを書き、今の実装で 1・2・4（止まる側）・5（引用値の病的入力）が落ち、3・4（止まらない側）が通ることを確かめる（Red）
2. 改名（`unquotedView`）。既存テストが通ることを確かめる
3. `*_QUOTED` の正規表現と `firstCommandAt`、各判定の OR、`hasSafeChain` の変更（Green）
4. doc とコメント
5. 検証

## 不変条件と異常系

- 今の検出を減らさない（旧い正規表現を残して OR にする。受け入れ条件 3 のテストで縛る）
- 各正規表現は単独で分割が一意になるよう組む（選択肢を先頭の文字で排他にする）。指数爆発は時間のテストで縛る
- 判定は全域関数のまま（throw しない）。病的入力のテスト（既存の `PATHOLOGICAL`）に引用値の形を足す
- 区切り文字は消さない（`unquotedView` の中身を変えない）
- `hasSafeChain` を偽にする変更は、静的 allow を外して動的判定へ回すだけで、block を直接増やさない

## テスト方針と検証コマンド

- 単体: `.claude/hooks/pre-bash.test.mjs`（`npx vitest run .claude/hooks/pre-bash.test.mjs`）
- 検証: `docs/build-commands.md` のカテゴリ E・F（`verify:tree` が回す）

## SPEC.md・関連文書の更新

- `SPEC.md`: 不要（開発ツールの変更）
- ルート `CLAUDE.md`「フック」: hook の判定の正本は `pre-bash.mjs` とそのテストと書いてあり、真のまま

## 新しく導入する用語

- なし（「見え方」は #1519 の既存の語。「引用値」は標準の語）

## 作業項目

### Phase 1 — テスト（Red）

- [x] 受け入れ条件 1〜5 のテストを書き、今の実装で落ちる・通るの内訳が上のとおりであることを確かめる

### Phase 2 — 実装（Green）

- [x] `ghPrCreateView` を `unquotedView` へ改名し、テストの import と describe 名を直す
- [x] `ENV_PREFIX_QUOTED`・`FLAG_QUOTED` と `*_QUOTED` の正規表現、`firstCommandAt` を足す
- [x] `ENV_PREFIX` の値から区切り文字を外す
- [x] `ghPrCreateSites`・`hasSafeChain`・`gitSegments`・`usesNoVerify`・`pullWithoutFfOnly`・`needsPyEncoding`・`REMEDY` を変更する
- [x] 冒頭のコメントと各 doc を直す

### Phase 3 — 検証

- [x] `npm test` と `npm run governance:check` が緑

## 消し込み（Step 3 のラウンド）

- R1 `41f821b6`: verify:tree red（E の `race-boundaries.test.mjs` の 1 本が 7146ms で timeout。委譲との並走による CPU 競合とみる。単独の回し直しは修正の編集が先に入って exit 2 で走らず、次の sha で単独に回す）。委譲: High 2・Medium 4・Low 4・⚠️ 2。変異注入 22 本、生き残り 6（M6・M9 は等価、M19 は宣言済みの冗長、M8・M10・M15b はテストの穴）。実地: 複製の hook に stdin で流し、受け入れ条件 1 の 15 形が exit 2、/implement のレシピが静的 allow のままを確認。直した: High-1 `ENV_PREFIX` の値から `(){}` を外して `A=${X}y git pull` などの検出が消えた → `${…}`・`$(…)` を 1 つの展開として読む `ENV_CHAR`／High-2 造語「引用値」を「引用符で囲んだ値」へ／Medium-1 `FLAG_WORD` の引用の外の 1 字から区切りを外す（新しい FLAG で二乗が倍にならない。旧い FLAG の二乗は受容リスクへ）／Medium-2 noVerify の文言に `-F` と Grep ツールを足す／Medium-3 M8・M10 の 2 形のテスト／Medium-4 エスケープした区切りの 2 形のテスト／Low-1 理由まで確かめるテスト。捨てた: Low-2（テスト名の現在形）・Low-4（冒頭コメントの列挙）は挙動を変えない。残した: Low-3（M6・M9 は等価で構造は変えない）、⚠️-1 `git push && gh -x;gh pr create`（旧い FLAG の既存の穴。受容リスクに記録）、⚠️-2 needsPyEncoding に見え方を通さない（issue の対象外）、展開でない `(){}` で語が切れる前置（`A=x{y}z git …`）は受容リスク

## 範囲外（PR 本文の「残したもの」へ）

- `echo a || git push && gh pr create` が `hasSafeChain` で true になる（`echo` が成功すると push は走らず gh が走る）。引用と無関係の既存の穴（`false && git push && gh pr create` は gh も走らないので穴ではない。差分レビューで訂正）
- 引用符を使わない「実行されない文脈」の push（`echo $(false && git push) && gh pr create`・`echo ${x:- && git push } && gh pr create`・PowerShell の `<# && git push #>`・`git push --help && gh pr create`）。引用の判定では閉じない（差分レビューの ⚠️）
- cwd 判定が最初の gh としか比べない（`gh pr create --help; cd ../o; gh pr create`）。引用と無関係の根を持つ既存の穴
- エスケープした空白（`GH_TOKEN=a\ b gh pr create`）・引用内のエスケープした引用符（`"a\" b"`）・git の長オプションの省略形（`--no-verif`）・`-c core.hooksPath=`（未実測）

## 未確定（実装前に潰す）

- [x] 引用値の選択肢を足すと対にならない引用符で検出が消える — 旧い正規表現を残し、引用値を読む正規表現を別に足して OR にする（敵対的調査で退行と爆発を実測。`\S` の fallback は 22 個で 5119ms）
- [x] 見え方で判定すると新しく止まる形 — 受容してテストで固定する（fail-closed。冒頭コメントの方針どおり）
- [x] `hasSafeChain` の直し方 — push より前に引用符・エスケープ文字があれば静的に認めない（保守案）。奇偶で判定する案は shell パーサへ近づくので採らない。`git commit -m "x" && git push && gh pr create` の実例はリポジトリに無い
- [x] `ghPrCreateView` の名前 — `unquotedView` へ改名する（参照は `pre-bash.mjs` と `pre-bash.test.mjs` だけ。grep で確認）
- [x] 引用内の `gh pr create` の後ろに本物を置く形 — `hasSafeChain` の引用の判定範囲を「最後の gh より前」にする（独立導出で実測した素通り）
- [x] `ENV_PREFIX` の既存の二乗 — 値から区切り文字を外す（独立導出で 90KB 6.3 秒・`main` 合計約 34 秒と実測。hook の予算を超えると fail-open）。今回の変更で走査が倍になるので、同じ PR で断つ

## plan-review 結果

- リスク: 高（hook を変更する）
- レビュー方式: 独立導出1体（issue に無い人間の裁定は前提に無いので Step 2b。結果は `workspace/plan-review-1523-derivation.md`。委譲先は workspace/ の計画を目にしていないと記録）
- エージェント数: 1

### 要対処
- 引用内の `gh pr create` の後ろに本物を置く形が静的 allow になる — 計画の修正（`hasSafeChain` の判定範囲を最後の gh より前に。受け入れ条件 2 に 4 形）— 実測の入力を受け入れ条件へ写した
- `ENV_PREFIX` の値が区切りを越えて食う既存の二乗（90KB 6.3 秒）— 計画の修正（値から区切り文字を外す。受け入れ条件 5）
- `FLAG` の先頭の 1 字が開きの引用符を食う — 計画の修正（`FLAG_QUOTED` の値の先頭の形）
- 安全なチェーンを狭めたときの `REMEDY` の不一致 — 計画の修正（`REMEDY` の文言）
- 引用で割った dry-run が `decide` の `every` に守られている — 回帰テストを受け入れ条件 2 へ
- 偽になる散文（冒頭コメントの「`gh pr create` の判定だけは」、`gitSegments` の doc の書き分け、受容する未対応リスク）— 計画の変更表へ

### 軽微
- `GIT_PUSH` は独自の照合で、直さなくても fail-closed — 変更しない
- エスケープした空白・引用内のエスケープした引用符・長オプションの省略形 — 範囲外へ記録

### 計画の差分のレビュー（同じ委譲先・独立ではない）
- 要対処 2 件を反映: 最後の gh までの区切りを見る（`lastCommandAt`・受け入れ条件 2 に 4 形）／`ENV_PREFIX_QUOTED` の値を部品の連接にする（受け入れ条件 3 に 3 形）
- 反映: テストのコメントの偽、範囲外の記述の訂正（`false && …` は穴ではない）、`FLAG_QUOTED` のフラグ名も連接にする（`--repo="a b c"`）、実行されない文脈の push を範囲外へ
- 覆ったと確認: 語割り・空白を含む引用値・`FLAG_QUOTED` の値の先頭・引用内の push/gh の 4 形・既存の二乗（4900ms → 1ms）・受け入れ条件 3 と 4

### 判断
- 実装着手: 可（自己承認）。差分のレビューの後の修正は、レビューが試作で確かめた形をそのまま採ったもの

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体（独立導出）
- エージェント数: 1（ほかに 3b の敵対的調査 1 体）
- 要対処: 6 件。すべて計画へ反映
- 未検証: なし（計画の差分は同じ委譲先が追加レビュー済み）
- 散文の全件列挙: 語 4 / 総件数 61 行 / ★未掲載 4（計画へ足した 0: どれも「見え方」を描画の見た目の意味で使う別の語で、本変更で偽にならない — `scripts/lib/SnotraInputMetrics.psm1`・`src-tauri/src/egui_shell/font_stack.rs`・`layout.rs`・`view.rs`） / ☆行未掲載 0
- check スキル: なし（関数の改名 `ghPrCreateView` → `unquotedView` は参照が 2 ファイルで grep で列挙済み。JS で LSP の findReferences は使っていない。`/dry-check` は、新しい `firstCommandAt` が `tokenStart` の繰り返しを束ねるもので、同等の手書きは無い）

## 承認

- [x] 自己承認 — 2026-11-22 / 仕様変更: なし — `SPEC.md` を変更ファイルに含まない（開発ツールの hook の検出を、今の規範どおりに効かせる修正） / 戻せない変更: なし — 永続形式・識別子/キー形式に触れない / 人の裁定: なし
