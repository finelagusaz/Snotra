# 計画 — issue #1528 マージ後に本体のローカル main を進める

## 目的

`merge:pr` が exit 0 で終わったあと、本体のチェックアウトのローカル `main` を `origin/main` へ fast-forward する。スキルや hook を変える PR のマージ直後から、スラッシュコマンド・Skill ツール・hook が新しい版で動くようにする。

## 受け入れ条件

1. exit 0 の後に、本体が `main` に居て追跡ファイルに変更が無ければ、本体のローカル `main` がマージ後の `origin/main` まで進む
2. 本体が `main` 以外・detached HEAD・追跡ファイルに変更あり、のときは本体に触らず、理由と手で打つコマンドを出す
3. fast-forward できない（ローカルの `main` が分岐・先行）、fetch の失敗、のときも本体は変わらず、理由と手で打つコマンドを出す
4. どの場合も終了コードは変わらない。exit 1・2・3 では本体に書き込まない。exit 3 のうちマージ済みの経路では、手で打つコマンドを案内する
5. `/merge-pr` の exit 0 の行が、この後処理と、進められなかったときの次の一手を書いている
6. `npm test`・`npm run governance:check` が通る

## 変更するファイル

| ファイル | 対象 | 変更 |
|---|---|---|
| `scripts/merge-pr.mjs` | 新しい関数 `fastForwardMain(root, mergeCommit, exec = gitExec)` を export する | 本体の状態を判定して ff する。`{ ok, notes }` を返す（下の「判定の順序」）。`exec` の既定は `shell: false` で git を呼び、fetch に timeout（60 秒）を掛ける実装 |
| 同 | `realDeps` を export し、`syncMain: fastForwardMain` を足す | 既定の `exec` で直結する（引数順を 1 か所に閉じ、配線をテストで縛れるようにする） |
| 同 | `run` のマージの成否の確認 | `gh pr view --json state,mergedAt` を `state,mergedAt,mergeCommit` にし、`mergeCommit?.oid ?? null` を控える |
| 同 | `run` の exit 0 の後処理 | `deps.cleanup` の後に `deps.syncMain(root, mergeCommitOid)` を呼ぶ。`ok` なら `deps.out`、そうでなければ `deps.err`。例外は catch して終了コードを変えない |
| 同 | `run` の exit 3（findings あり）の出力 | 「exit 3 の案内」を添える |
| 同 | 冒頭のコメント「構造で消している落とし穴」 | exit 0 の後に本体の `main` を進めること（#1528）を 1 項足す |
| `scripts/merge-pr.test.mjs` | `fake()` | `keyOf` の `args.includes("state,mergedAt")` を新しい `--json` の値に合わせる（`args.some((a) => a.startsWith("state,mergedAt"))` など）。既定の "pr view:post" の fixture に `mergeCommit: { oid }` を足す。`syncMain` の差し替え（呼び出しの引数と順序を記録して `{ ok: true }`）を足す |
| 同 | `run` の describe を足す | exit 0 でだけ呼ぶ／片づけの後に呼ぶ／`(root, mergeCommit.oid)` で呼ぶ／`mergeCommit` が null なら null で呼ぶ／`ok: false` でも例外でも exit 0 のまま／exit 1・2・成否不明の 3 では呼ばず案内も出ない／findings ありの 3 では呼ばず案内が出る |
| 同 | `realDeps.syncMain === fastForwardMain` を確かめる 1 本 | 配線を外す・差し替える変異で落ちる |
| 同 | `fastForwardMain` の describe を足す | 一時リポジトリ（bare の origin + 本体 clone）で、下の各状態を実 git で確かめる |
| `docs/build-commands.md` | `merge:pr` の書き込みを述べる箇条（「`merge:pr` は書き込み（マージ）を伴う」） | 書き込みに、exit 0 の後の本体のローカル `main` の fast-forward を足す |
| `.claude/skills/merge-pr/SKILL.md` | 「3. 終了コードで次の一手が決まる」の exit 0 の行 | 本体の `main` を進めることと、「進められなかった」と出たら、出力の理由とコマンドに従って人が本体を直すことを足す（スキルの `allowed-tools` に git は無いので、人が打つ） |
| 同 | 「マージ後に要確認が出たとき（exit 3）」 | 「マージ済み」の判定のときは本体の `main` は進めていないこと、確かめたあと出力の案内に従って進めることを 1 項足す |

### 判定の順序（`fastForwardMain(root, mergeCommit, exec)`）

git の呼び出しは exit code を見て分岐する（rc ≠ 0 で例外を投げるヘルパは使わない。detached HEAD で `symbolic-ref` が rc=1 を返すのを理由として出すため）。

1. `git -C <root> symbolic-ref --quiet --short HEAD`。rc ≠ 0 なら「本体が detached HEAD である」、出力が `main` でなければ「本体が main に居ない（<ブランチ>）」で止める。`--ff-only` は detached HEAD や `origin/main` の祖先である別ブランチでも成功してしまうので、この判定は省けない（敵対的調査 F1 で実測）
2. `git -C <root> status --porcelain --untracked-files=no` が空でなければ止める: 「本体の追跡ファイルに変更がある」。重ならない変更なら `--ff-only` は成功するが、作業中の本体の HEAD を動かさないために止める（F2）
3. `git -C <root> fetch origin main`（timeout 60 秒）。失敗・timeout なら止める
4. マージ commit を確かめる。`mergeCommit` が null なら「マージ commit が取れなかった」、`git -C <root> merge-base --is-ancestor <mergeCommit> origin/main` が 1 なら「origin/main にまだマージが現れていない」、それ以外の rc なら「マージ commit を確かめられない（git の stderr）」で止める（F5。「既に最新」の誤報を防ぐ）
5. `HEAD` と `origin/main` が違い、`git -C <root> merge-base --is-ancestor origin/main HEAD` が 0 なら止める: 「本体の main が origin/main より先にある（未 push のコミット）」（`--ff-only` は先行していても `Already up to date.` で rc=0 を返す。計画レビューで実測）
6. `git -C <root> diff --no-renames --name-only -z --diff-filter=A HEAD origin/main` の各パスが本体に既にあれば止める: 「マージで追跡されるパスに、無視されたファイルがある（<パス>）」（`--ff-only` は無視されたファイルを警告なしで上書きする。F3 で実測。`--no-renames` が無いとリネームの移動先を取りこぼし、`-z` が無いと非 ASCII のパスが引用される。計画レビューで実測）
7. `git -C <root> merge --ff-only origin/main`。失敗なら止める（git の stderr を添える。未追跡ファイルを上書きする場合などは git が拒み、本体は変わらない）
8. 前後の `HEAD` が同じなら「既に最新」、違えば「<旧 sha>..<新 sha> へ進めた」

止めたときの notes には、理由を解消する操作を先に書き、そのあとに手で打つコマンドを添える。理由ごとに書き分ける（判定 1・2・5・6 で止めたときに `merge --ff-only` だけを案内すると、人が F1 の害を再現する）。パスは引用符で囲む。

- 判定 1: 本体の作業を片づけて `main` へ戻してから、`git -C "<root>" merge --ff-only origin/main`
- 判定 2: 本体の変更をコミットか退避してから、同上
- 判定 3・4: 時間を置いて `git -C "<root>" fetch origin main` からやり直す
- 判定 5: 先行しているコミットの扱いを決めてから（push するか捨てるか）、同上
- 判定 6: 名指したファイルを退避してから、同上
- 判定 7: git の stderr に従う

既知の穴（計画レビューの軽微。測っていない）: `assume-unchanged`・`skip-worktree` を付けたファイルの変更は判定 2 に見えない。マージ・cherry-pick の途中の本体では git が `--ff-only` を拒む見込みだが、測っていない。

### exit 3 の案内（F4）

「マージ済み。ただし要確認」で exit 3 を返す経路（事後検証の findings がある経路）では、本体に書き込まず、要確認の出力の末尾に「本体の main は進めていない。本体が main に居て追跡ファイルに変更が無いことを確かめてから、`git -C "<root>" merge --ff-only origin/main` を打つ」を添える。成否を確かめられない経路では何も足さない。

## 実装の順序

1. `fastForwardMain` とその一時リポジトリのテスト（Red: 関数を no-op で置いて落とす → Green）
2. `run` への配線と `fake()`・`run` のテスト（Red: 呼び出しを入れる前に落とす → Green）
3. 冒頭のコメントと `/merge-pr` の exit 0 の行
4. 検証

## 不変条件と異常系

- 終了コードの意味を変えない（0 = マージ済みで事後検証が緑）。後処理の失敗・例外は `deps.err` に出すだけ
- exit 3 では本体に書き込まない（片づけ（#1359）と同じく、人が調べる材料を残す）。マージ済みの経路でだけ案内を出す
- 本体が `main` に居ないとき、追跡ファイルに変更があるときは、本体に書き込む操作（fetch を除く）をしない
- fetch は refs だけを書き、作業ツリーに触らない
- `shell: false` で git を呼ぶ

## テスト方針と検証コマンド

- `fastForwardMain` の一時リポジトリのテスト（bare の origin と本体の clone を作り、origin を 1 コミット先へ進めて本体を古くする）: (a) main・きれい・origin が先 → 進む (b) 既に最新 → 「既に最新」で HEAD 不変 (c) `origin/main` の祖先である別ブランチ → 止まり、そのブランチも main も不変 (d) detached → 止まる (e) 重ならない追跡ファイルの変更 → 止まり HEAD と作業ツリー不変 (f) ローカル main が分岐 → 判定 7 で ff が拒まれ HEAD 不変、git の stderr が notes に載る (g) origin が無い（fetch 失敗）→ 止まる (h1) `mergeCommit` が null (h2) `origin/main` に含まれない oid (h3) 不正な oid → それぞれ別の理由で止まる (i1) マージで新たに追跡されるパスに無視されたファイルがある (i2) リネームの移動先に無視されたファイルがある → どちらも止まり、そのファイルの内容が不変 (j) ローカル main が origin/main より先 → 「先にある」で止まり、「既に最新」と言わない (k) マージで追跡されるパスに未追跡ファイルがある（無視されていない）→ 判定 6 が存在を見て止め、本体不変（実装時に変更: 判定 6 は無視されたものに限らず、追跡していないファイルの存在を見るので、git の拒否より手前で止まる。理由の文言も「追跡していないファイルがある（無視されたものを含む）」にした）。どれも理由（notes の文言）まで assert し、止めたときの notes が理由ごとの操作を含むことも assert する（`.claude/rules/safety-nets.md`「テストも条件ごとに書く」）
- `run` のテスト: 変更表の `run` の describe のとおり。`realDeps.syncMain === fastForwardMain` の 1 本も置く
- 検証: `docs/build-commands.md` のカテゴリ E・F（`verify:tree` が回す）

## SPEC.md・関連文書の更新

- `SPEC.md`: 不要（開発ツールの変更で、製品の挙動を変えない）
- `.claude/hooks/pre-bash.mjs` 冒頭の「ゲートの修正が効くのは本体の作業ツリーがその変更を持ってから」は真のまま。触らない

## 作業項目

### Phase 1 — `fastForwardMain`

- [x] 一時リポジトリのテスト (a)〜(k) を書き、no-op の実装で落ちることを確かめる（14 本すべてアサーションで赤）
- [x] `fastForwardMain` を「判定の順序」のとおり実装して通す（理由ごとの notes を含む。135 本緑）

### Phase 2 — `run` への配線

- [ ] `fake()` の `keyOf` と既定の fixture（`mergeCommit`）を直し、`syncMain` の差し替えを足す。既存のテストが通ることを確かめる
- [ ] `run` のテスト（変更表の `run` の describe）と `realDeps.syncMain === fastForwardMain` の 1 本を書いて落とす
- [ ] `realDeps` の export と `syncMain`、`run` の呼び出し（`mergeCommit` の取得を含む）、exit 3（findings あり）の案内を足して通す

### Phase 3 — 文書

- [ ] `merge-pr.mjs` 冒頭のコメントに 1 項足す
- [ ] `/merge-pr` の exit 0 の行と「マージ後に要確認が出たとき（exit 3）」を直す
- [ ] `docs/build-commands.md` の `merge:pr` の書き込みの箇条を直す

### Phase 4 — 検証

- [ ] `npm test` と `npm run governance:check` が緑

## 未確定（実装前に潰す）

- [x] （裁定）どう直すか — 問い: 「merge:pr が main を進める／規範に一言書くだけ／merge:pr は案内だけ出す」。回答: 「merge:pr が main を進める (Recommended)」
- [x] 追跡ファイルに変更があるとき、`--ff-only` に任せるか事前に止めるか — 事前に止める。`--ff-only` は衝突しない変更なら進めてしまい、作業中の本体の HEAD を動かす。止めるほうが、本体で作業中の人に触らない
- [x] exit 3 で進めるか — 進めない。exit 3 は人が調べる場面である。ただしマージ済みの経路では旧版が動く害が残るので、手で打つコマンドを案内する（敵対的調査 F4）
- [x] 無視されたファイルの上書き — `--ff-only` は無視されたファイルを警告なしで上書きする（F3 実測）。マージで追跡されるパスが本体に既にあれば止める
- [x] `origin/main` にマージが現れたか — fetch 後に `merge-base --is-ancestor <mergeCommit> origin/main` で確かめる（F5）。`gh pr merge` 直後の反映の遅れは未測定なので、この確かめで吸収する

## plan-review 結果

- リスク: 高（`scripts/`・skills を変更する）
- レビュー方式: 計画準拠レビュー1体（issue に無い人間の裁定が前提にあるため Step 2。結果は `workspace/plan-review-merge-ff.md`）
- エージェント数: 1

### 要対処
- 判定 6（旧 5）がリネームの移動先を取りこぼす — 計画の修正（`--no-renames -z`、テスト (i2)）— レビューの一時リポジトリでの実測
- ローカル main が先行していると「既に最新」と誤報する — 計画の修正（判定 5、テスト (j)）— `--ff-only` は先行でも rc=0（git の既知の挙動）
- 案内のコマンドが理由によっては本体を壊す — 計画の修正（理由ごとの操作を先に書く。exit 3 の案内に前置き）
- `fake()` の `keyOf` が `--json` の変更で壊れる・配線の assert が無い — 計画の修正（`keyOf`・fixture・`(root, oid)` の assert）— `scripts/merge-pr.test.mjs` の `keyOf` が `args.includes("state,mergedAt")` で完全一致を見ることを確認
- シグネチャの食い違いと `realDeps` の配線を縛るテストの欠落 — 計画の修正（`fastForwardMain(root, mergeCommit, exec = gitExec)`、`realDeps.syncMain === fastForwardMain`）
- `/merge-pr` の exit 3 の節に復帰手順が無い — 計画の修正（変更表に 1 行）

### 軽微
- detached の `symbolic-ref` は rc=1 — 判定 1 に反映（rc で分岐）
- 判定 4 の理由を null・1・128 で分ける — 反映（テスト (h1)〜(h3)）
- fetch の timeout — 反映（60 秒）
- 未追跡ファイルの衝突 — 反映（テスト (k)）
- `assume-unchanged`・マージ途中の本体 — 既知の穴として計画に記録（測っていない）
- スキルの `allowed-tools` に git が無い — 案内のコマンドは人が打つと SKILL に書く
- `<root>` の空白 — 案内で引用符を付ける

### 未検証
- CI の Windows runner で一時リポジトリのテストが回るか — `verify-tree.test.mjs` の先例に倣う。PR の CI で確かめる
- 変異注入（`syncMain` の呼び出しを外す・`mergeCommit` を渡さない・判定 1/6 を外す）— `/implement` 3b の委譲先が行う

### 判断
- 実装着手: 人間の承認待ち。要対処の修正はレビューの対処案を採ったもので、テストの期待値と不変条件が変わった。独立レビューの再実行はしていない

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体
- エージェント数: 1（ほかに 3b の敵対的調査 1 体）
- 要対処: 6 件。すべて計画へ反映
- 未検証: CI の Windows runner での一時リポジトリのテスト（PR の CI で確かめる）
- check スキル: なし（対称ペア・状態・永続形式に当たらない。関数の新設は `fastForwardMain` だが呼び出し元は `realDeps` の 1 か所で、`/dry-check` の対象の重複は `/deps-update` の手順（散文）だけ）
- 散文の全件列挙: 語 4 / 総件数 59 行 / ★未掲載 12（計画へ足した 1: `docs/build-commands.md` の `merge:pr` の書き込みの箇条。残る 11 は `AGENTS.md`・`/start-issue`・`/deps-update`・`scripts/delete-merged-branch.mjs` の「ローカルの `main` は fetch だけでは進まない」、ルート `CLAUDE.md` のスキル表と運用、`.github/workflows/ci.yml`・`package.json` の名前の出現、`/implement` の worktree の片づけ、`RETROSPECTIVE.md`・`scripts/verify-tree.mjs` の片づけで、どれも本変更で偽にならない） / ☆行未掲載 0

## 承認

- [x] 承認済み — 2026-11-22 / 問い: "workspace/plan.md の計画を承認なさいますか？" / 回答: "承認して実装"
