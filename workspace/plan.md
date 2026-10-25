# 計画: #1298 /implement の委譲から A〜F の実行を外し、worktree の切り出しをスクリプトへ移す

根拠は `workspace/research.md`（M1〜M8 の実測と、敵対的調査の採否）。

## 目的

- 権威ある A〜F は、主エージェントが道具を呼んで、sha に固定した道具の木で回す。HEAD 合わせは道具の構造になる
- 委譲先の仕事はレビューと変異注入に絞る
- 委譲先の HEAD 合わせと「前巡の未追跡報告で checkout が止まる」を、指示文から道具へ移す

## issue の案から変える点（実測による）

| issue の案 | 変更 | 根拠 |
|---|---|---|
| 委譲先は木を作らず、道具で切った木を渡す | **委譲先は `isolation: "worktree"`（harness の木）に残す。** HEAD 合わせだけを道具の `align` で行う | isolation 無しのサブエージェントは cwd が呼び出しごとに本体へ戻る（M4）。EnterWorktree は、isolation 無しからも（M5）、isolation 付きからも（M6）拒否される。道具の木は委譲先に渡せない |
| 委譲先の出力先を worktree の外にする | **出力は木の中の、どのコミットにも現れないパス（`.delegate/`）に置く** | 木の外へ書くと木が「変更なし」になり、停止時に harness が片づける。再開した委譲先は本体で動く（M8）。Write ツールは外を拒否する（M7）。`.delegate/` はどのコミットにも無いので、checkout と衝突しない |
| 未確定: 変異注入も道具の木で回すか | **委譲先が自分の木で回す**（既定案どおり） | 道具の木へは委譲先が入れない（M6） |
| 未確定: 両方の報告が sha を名乗ることで保てるか | **保てる。** 道具の要約と `align` の出力がどちらも sha を機械可読で出す。照合の機構化は #1299 が持つ | ADR の却下理由 (b)「別の版へ向く」は sha 固定で消える（research 制約 7） |

**費用の向きは issue の想定と逆になる。** 権威ある A〜F の回数は「主 + 委譲」から「主 + 道具」へ変わるだけで減らない。木が 2 本になるので、cold ビルドは PR あたり 1 回増える（敵対的調査 [2]）。この変更で得るのは費用の削減ではなく、次の 3 点である。
- HEAD 不一致（20/20 PR）の手当てを構造で消す
- 未追跡報告の衝突（5 PR）を構造で消す
- 委譲の焦点をレビューと変異注入に絞る

## 受け入れ条件

1. `npm run verify:tree -- run --sha <sha> --categories <list> [--also <npm-script>]... [--out <abs>]` は次のとおりに動く
   - **本体の作業ツリーの `git status --porcelain` が空で、`sha == 本体の HEAD` であることを確かめる。** 違えば exit 2。コミットし忘れた変更が検証されないまま緑で返る形（ADR の受容した残余・#922）を、3a の指示文から構造へ上げる
   - 道具の木 `.claude/worktrees/agent-verify-<branch-slug>` を作るか再利用し、`HEAD == sha` を確かめる。接頭辞を `agent-` にするのは、`clean:worktrees` がそのまま拾えるようにするため（`clean-worktrees.mjs:33-35`。木は detached なので、ブランチ削除の対象にもならない）
   - `docs/build-commands.md` の該当カテゴリのコードブロックを回す。`--also` は、散文に条件が書かれた単発のコマンドを npm script の名前で足す口で、`package.json` に無い名前なら exit 2 にする（E の「`scripts/lib/**` の PowerShell を触ったら `npm run test:powershell`」・`docs/build-commands.md:161`）
   - 要約 JSON を `--out` へ書く。既定は `.claude/worktrees/agent-verify-<branch-slug>.summary.json` で、gitignore 済みなので本体の porcelain を汚さない（次の巡の 3a と衝突しない）
   - 終了コードは、全部緑なら 0、どれかが赤なら 1、環境の失敗なら 2
2. 環境の失敗は「緑」と混ざらない。exit 2 にするのは次の場合である
   - 要求されたカテゴリのコマンドが 0 件しか取れない
   - 木の作成か checkout が失敗する
   - 再利用する木に追跡ファイルの変更が残っている
   - 木を作ったあと `HEAD != sha` になっている
3. 道具は E と F を常に回す。A は要求されたとき、ブロックの全行（全 crate のテスト）を回す。C は要求されたとき回す。D は回さず、要約に `handoff` と書く
4. `node C:/workspace/Snotra/scripts/verify-tree.mjs align --sha <sha>`（**本体の絶対パスで呼ぶ**）は、委譲先の cwd の木で次を行う。`npm run` で呼ばないのは、委譲先の木が harness によってセッション開始時点のコミットで切られるため（ADR「初回運用で測ったこと」）。そのコミットには `verify-tree.mjs` も `verify:tree` も無いことがあり（この PR 自身の `/implement` で必ず起きる）、`npm run` は missing script で落ちて checkout に至らない。スクリプトは本体のものを使い、操作するのは cwd の木である。パスはリポジトリのルートを解決した値を主が渡す（委譲はコンテキストを継承しない）
   - 「本体ではなく linked worktree であること」と「追跡ファイルの変更が無いこと」を確かめ、`checkout --detach sha` して `HEAD == sha` を確かめる
   - 違えば exit 2 にする
   - 前巡の変異が巻き戻されずに残っていれば、ここで止まる
5. `align` は `.delegate/` を、`sha` に存在せず、かつ gitignore されていないパスとして確かめる。満たさなければ exit 2 にする（自動片づけを防ぐ仕組みが成り立つ前提）
6. `npm run verify:tree -- remove [--delegate-tree <path>]` の動作
   - 現在のブランチの道具の木を `git worktree remove`（`--force` なし）で消す。木が dirty なら消さずに exit 1 にする。削除のあとディレクトリが残っていれば（呼び出し元の cwd が木の中にあった場合・M3）exit 1 で報告する
   - `--delegate-tree` を渡すと、委譲先の木も消す。手順は次のとおり
     - `.delegate/` 以外の変更が無いことを確かめる
     - `.delegate/` を消してから `git worktree remove` する（`--force` なし）
     - ブランチ `worktree-agent-*` を `git branch -d` で消す
   - 委譲先の木は `.delegate/` によって意図的に dirty に保たれるので、`clean:worktrees` は `--force` なしでは拾わない。このため破棄の経路を対で置く（`/symmetric-check` 候補 3）
7. 要約 JSON は `sha`・`worktree`・`categories_requested`・`categories_run`・各コマンドの `exit`・開始と終了の時刻を持つ（#1299 が `sha` を照合する）。パスは `/` 区切りで出す（pre-bash の `usesBackslashPath`）
8. sha の `package-lock.json` と、本体の `node_modules/.package-lock.json`（npm が書く、実際に入っている依存の状態）の `packages` が食い違うときは、要約に警告を書く。junction 無しの木は本体の `node_modules` を解決するので、依存を変える PR では別の版を測りうる（敵対的調査 [1] ⚠️）。**比べる相手は本体の `package-lock.json` ではない。** 受け入れ条件 1 が `sha == 本体の HEAD` を確かめるので、両者は構造的に同一になり、警告が一度も出ない（advisor の指摘）
9. 道具の木は `agent-verify-*` という名前なので、`npm run clean:worktrees` が変更なしで取りこぼしを拾う（作られて消されない木を残さない）
10. `/implement` Step 3 は次を持つ
    - 上の分担
    - 並走の規則。C を要求する巡は直列にする。それ以外は並走し、道具の赤は、委譲の報告が届いたあとに単独で 1 回再実行してから確定する
    - 報告の記録。委譲先の `.delegate/` の報告と道具の要約を主が `workspace/` へ写し、次のコミットに含める。issue の「比べ方」が測る経路を、今日の `verify-*.txt` と同じ形で残すため
    - 3c の「報告が届くまで作業ツリーに触らない」の理由の差し替え。今の理由は「委譲先が実ツリーでビルドする」で、変更後は偽になる（`SKILL.md:115`）。新しい理由は「報告の sha == HEAD を保つため」（ADR がパイプライン化を却下した理由と同じ）
    - 委譲先への指示に 1 行を足す。「変異注入の赤は、理由が検査の assert であることを出力で確かめる。timeout とモジュール解決の失敗は検知に数えない」。道具との並走で CPU 競合が増えるため（敵対的調査 [3d]。既存の規範 `.claude/rules/safety-nets.md` の「赤の理由を毎回出力で確かめる」に、timeout を足す形）
    - Step 4 に救済経路を書く。`remove --delegate-tree` が exit 1 で止まったら（再開可能な委譲先がディレクトリを握っている場合・M3）、`npm run clean:worktrees -- --force` で消す
    - 「緑が何も見ていない」の観点の置き場。主は道具の要約の `categories_run` とコマンドの一覧を、変更が触れたコードパスと突き合わせる。委譲先は変異注入でテストの検出力を測る（今日もこの観点の一次証拠は変異注入であり、道具の要約は委譲先へ必須の入力としない。直列の巡では要約のパスを渡す）

## 変更ファイル一覧と対象シンボル

| ファイル | 変更 |
|---|---|
| `scripts/verify-tree.mjs`（新規） | サブコマンド `run` / `align` / `remove`。`parseCategoryCommands(markdown)` は、`scripts/governance/lib.mjs` の `sectionOf`（`:379`。釣り合わないフェンスで fail-closed）で `### <X>.` の節を切り出し、その中の最初の ` ```bash ` ブロックの各行から、コメントを除いたコマンドを返す。`git()` は `clean-worktrees.mjs` と同じ `execFileSync` の形。外部の依存は持たない |
| `scripts/verify-tree.test.mjs`（新規） | 使い捨て repo で `run` / `align` / `remove` を実測する（`.githooks/githooks.test.mjs` の流儀）。`parseCategoryCommands` は実際の `docs/build-commands.md` に当て、A・C・E・F が空でないことを確かめる |
| `package.json` | `"verify:tree": "node scripts/verify-tree.mjs"` |
| `docs/build-commands.md` | C のブロックの先頭に `cargo build -p snotra` を足す（本文 `:51` の前提をブロックへ移す。道具はブロックしか読まない）。「変更後の検証チェックリスト」の冒頭に「各カテゴリの最初の bash ブロックは `scripts/verify-tree.mjs` が読む」と 1 文書く |
| `.claude/skills/implement/SKILL.md` | Step 3 の導入（`:82`・`:84`・`:86`）、3b の箇条（`:101` と `:102` は道具へ移す。`align` の呼び出し、出力先 `.delegate/`、並走の規則を足す）、3c（道具の赤の再実行、両方の報告が揃うまで HEAD を進めない）、Step 4（`verify:tree -- remove --delegate-tree <align が出したパス>`）、出力の検証欄（根拠は道具の要約）。**見出し 3a / 3b / 3c は改名しない**（`scripts/race-boundaries.mjs:15`・ルート `CLAUDE.md:65`・`/start-issue` SKILL `:82` が参照している） |
| ルート `CLAUDE.md:65` | 「検証も同じ委譲が担う」を、道具が A〜F を担い、委譲はレビューと変異注入を担う、へ直す |
| `docs/adr/ADR-implement-verification-tool-tree.md`（新規） | 却下した代替案と理由。(1) isolation 無しの委譲先に道具の木を渡す（M4・M5） (2) EnterWorktree で道具の木へ移す（M6） (3) 出力を木の外へ出す（M7・M8） (4) 道具と委譲先が 1 本の木を共有する（変異と A〜F の衝突、ADR 事故 2 の再現） (5) コマンドを道具の中へ写す（SSOT の 2 枚目） (6) カテゴリを道具がパスから導く（C・D は意味判断） |

触らないもの:
- `.claude/agents/code-reviewer.md`。委譲先は isolation 付きのままなので、`:11-12` は真のまま
- `AGENTS.md:49`。誰が実行するかを書いておらず中立
- `/start-issue` SKILL `:82`。変異注入は委譲先に残るので真のまま
- 既存の ADR（凍結）

## 実装順序

1. 変異を先に書く（下の「テスト方針」）→ `verify-tree.test.mjs` を Red で置く（`verify-tree.mjs` はサブコマンドが exit 0 を返すだけの空実装）
2. `parseCategoryCommands` → `run` → `align` → `remove` を Green にする
3. `docs/build-commands.md` の C ブロックと冒頭文、`package.json`
4. `/implement` SKILL、ルート `CLAUDE.md:65`、ADR
5. 自分の branch に当てて試す: `npm run verify:tree -- run --sha <HEAD> --categories E,F --out <scratchpad>/s.json` が exit 0 を返し、要約の sha が HEAD と一致すること。続けて `remove`

## 不変条件と異常系

- **「対象が無いから緑」を作らない。** 取れたコマンドが 0 件、木の作成の失敗、`HEAD != sha` は、どれも exit 2 にする。緑（0）は「要求されたカテゴリのコマンドが 1 件以上走り、すべて exit 0」のときだけである
- **道具は本体の作業ツリーに触らない。** すべての git 操作とコマンドを `cwd: <木>` で走らせる（`execFileSync` の `cwd`。M4 の cwd リセットの影響を受けない）。`--out` への書き込みだけは例外
- **`run` は、コマンドの抽出と引数の検証を、木の作成より前に行う。** 回す物が無いのに cold の木を残さないため（`/symmetric-check` 候補 1）
- **木のライフサイクル**: 作るのは `run`、再利用するのも `run`（追跡ファイルの変更が無いときだけ `checkout --detach`）、消すのは `remove`（`/implement` Step 4）と `clean:worktrees`（取りこぼし。`agent-` 接頭辞で変更なしに拾う）。`remove` は cwd が木の外にあることを前提にする（M3）。dirty な木を消さない保護は git 自身の拒否に委ねる（`clean-worktrees.mjs` の流儀）
- **委譲先の木を生かし続ける仕組み**: `.delegate/` の未追跡ファイルが木を「変更あり」に保つ（M8）。`align` が「`.delegate/` は sha に無く、ignore されていない」を確かめる
- **junction を張らない**（`.claude/rules/safety-nets.md`。張ると `worktree remove` が消し切れない）

## テスト方針と検証コマンド

先に書く変異（`.claude/rules/safety-nets.md`「新設する検出器は、設計を確定する前に『注入する変異』を先に書く」）。どれもテストが**assert で**赤くなること（コンパイル・解決エラーではない）を、委譲先の変異注入で測る。
- V1: `parseCategoryCommands` が常に `[]` を返す → 実文書テストが赤、`run` が exit 2
- V2: `run` の `HEAD == sha` の照合を外し、わざと別の sha に checkout する → 赤
- V3: `align` の「追跡ファイルの変更が無い」照合を外す → 変更を残した木で exit 0 になるので、テストが赤
- V4: `align` の `.delegate/` の ignore 照合を外し、fixture の `.gitignore` に `.delegate/` を足す → 赤
- V5: `run` の「本体の porcelain が空で `sha == 本体の HEAD`」の照合を外す → 未コミットの変更がある fixture で exit 0 になるので、テストが赤
- V6: `remove --delegate-tree` の「`.delegate/` 以外の変更が無い」照合を外す → 追跡ファイルを変えた木が消されるので、テストが赤

検証（`docs/build-commands.md`）: E（`npm test`）と F（`npm run governance:check`）。A〜D は非該当（`.rs` なし、ウィンドウ・ホットキー・表示に触れない）。実装順序 6 の自己適用も行う。

## SPEC.md・関連文書の更新要否

- `SPEC.md`: 不要（製品の挙動ではなく開発プロセスの変更）
- 関連文書: 上の変更ファイル一覧の文書類。ルート `CLAUDE.md`「サブエージェント委譲と worktree」の条項の特定は Phase 4 の作業項目にする（削除は含めない）

## 作業項目

### Phase 1 — 道具の核（Red → Green）
- [ ] `scripts/verify-tree.mjs` を空実装で置き、`scripts/verify-tree.test.mjs` が V1〜V6 の性質で Red になることを確かめる
- [ ] `parseCategoryCommands` を実装し、実文書テストを Green にする
- [ ] `run` を実装する（木の作成と再利用、HEAD の照合、E と F の常時実行、D の handoff、要約 JSON、exit 0/1/2、依存の警告）
- [ ] `align` を実装する（linked worktree の確認、追跡ファイルの変更の確認、`.delegate/` の確認、checkout、HEAD の照合）
- [ ] `remove` を実装する（`--delegate-tree` と、削除後の不在確認を含む）
- [ ] `package.json` に `verify:tree` を足す

### Phase 2 — 周辺の文書
- [ ] `docs/build-commands.md` の C ブロックの先頭へ `cargo build -p snotra` を足し、チェックリストの冒頭に道具が読む旨を 1 文書く

### Phase 3 — 規範
- [ ] `/implement` SKILL の Step 3 と Step 4、出力欄を書き換える（見出しは保つ）
- [ ] ルート `CLAUDE.md:65` を直す
- [ ] `docs/adr/ADR-implement-verification-tool-tree.md` を起こす

### Phase 4 — 自己適用と記録
- [ ] 実装順序 6 の自己適用を行い、要約 JSON の sha と exit をこの plan.md に記録する
- [ ] ルート `CLAUDE.md`「サブエージェント委譲と worktree」の各条項が、この変更の後も要るかを 1 条ずつ判定し、この plan.md に記録する（消さない）

計画外（PR 本文へ送る）: issue の「比べ方」（次の 10 PR で所見の件数と High の採用件数を見る）。この PR の中では測れない。

## 未確定（実装前に潰す）

（なし）

## 人間レビュー

- [x] 承認済み — 2026-10-25 / 問い: "注釈を `plan.md` へ書き込んでいただくか、このまま承認してくださるか、どちらかをお知らせくださいませ。" / 回答: "OKです"

## plan-review 結果

- リスク: 高（`scripts/**`・`.claude/skills/**`・ルート `CLAUDE.md`・新 ADR。網羅性も要件に含む——偽になる散文の列挙）
- レビュー方式: 独立導出 1 体（Step 2b）。成果物は `workspace/plan-review-1298-2b.md`
- エージェント数: 1

### 要対処（反映済み）
- `.claude/worktrees/` 配下の要約の置き場 — 計画の修正: 既定を gitignore 済みのパスにした — 根拠: 本体の `workspace/` へ書くと、次の巡の 3a の「porcelain が空」と衝突する（レビュアの所見 4。私の計画は `--out` を必須にしていて、置き場を決めていなかった）
- E の `test:powershell` が散文にしか無い — 計画の修正: `--also <npm-script>` を足した — 根拠: `docs/build-commands.md:161` を読んで確かめた（所見 5）
- `SKILL.md:115` の 3c の理由が偽になる — 計画の修正: 理由を差し替える項目を受け入れ条件 10 へ入れた — 根拠: `:115` の逐語「委譲先はビルドとテストと変異注入を実ツリーで走らせるため」（所見 8）
- 本体の porcelain と `sha == HEAD` を道具に確かめさせる — 計画の修正: 受け入れ条件 1 と変異 V5 — 根拠: ADR の受容した残余が「手順に置いたが、機構では塞いでいない」と書いている（判断点 D5）
- 道具の木を `agent-verify-*` と名付けて `clean:worktrees` を変えずに使う — 計画の修正: `clean-worktrees.mjs` を変更ファイルから外し、旧 V5 を差し替えた — 根拠: `clean-worktrees.mjs:33-35` の述語は basename の `agent-` 接頭辞だけを見る。道具の木は detached なので、ブランチ削除（`:74-`）にも当たらない（判断点 D6・ファイルが 1 つ減る）
- 報告を `workspace/` へ写して commit する経路 — 計画の修正: 受け入れ条件 10 — 根拠: issue の「比べ方」は、今日の `verify-*.txt` と同じ形で報告が履歴に残ることを前提にしている（所見 4）
- パーサは `sectionOf` を再利用する — 計画の修正: 変更ファイル一覧 — 根拠: `scripts/governance/lib.mjs:379` に実在し、釣り合わないフェンスで fail-closed（所見 5）

### 軽微
- `docs/check-skill-skeleton-design.md:119` の参照 — 降格: 見出し 3a とその check スキルの列挙は変えないので、参照先も内容も真のまま（`:119` を読んで確かめた）
- レビュアの「`verify` は既に使われている」— 計画は `verify:tree` を使っており衝突しない（`package.json:22` は `verify`）

### 判断の不一致
- **レビュアは「委譲先は isolation 無しで起動するしかない」と導いた。計画は isolation 付きのまま残す。** 計画を維持する。isolation 無しでは cwd が呼び出しごとに本体へ戻り（M4）、変異注入が本体を壊しうる。これはレビュア自身も ⚠️「構造の保護が失われる」として挙げている。委譲先は道具の木へ移れないが（M6）、自分の木を `align` で sha に合わせれば足りる。`code-reviewer.md:11-12` が偽になるというレビュアの所見 2 は、isolation 無しの前提から出たものなので、計画の前提では当たらない
- D7（「緑が何も見ていない」の観点を誰が持つか）— 受け入れ条件 10 の最後の箇条で裁定した

- harness の自動片づけが道具の木（`agent-verify-*`）に触れるか（レビュアの ⚠️）— 当たらない。M8 で消えたのは、停止したエージェント自身の木とブランチだけで、同じ場所にあった `probe2-1298` は残った

### 未検証
- なし。`.delegate/` が委譲先の木を生かし続けることは、承認の前に M9 で測った（`workspace/research.md`）。`--delegate-tree` と同じ手順の手動の予行も、そのとき通った

### 判断
- 実装着手: 可（人間の承認待ち）
- 再レビュー: しない。計画の変更は、独立導出の所見を取り込んだ結果である（ファイルの集合は導出側へ寄って 1 つ減った）。issue の要件は変わっていない

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー 1 体（Step 2b）
- エージェント数: 1（plan-review）。ほかに 3b の敵対的調査 1 体と、計測用 3 体（haiku、M4〜M8）
- 要対処: 自己照合（Step 5a の 1〜5）と `/symmetric-check` で 3 件。`run` の抽出順序、`remove` の不在確認、委譲先の木の破棄経路（`--delegate-tree`・V6）。すべて計画へ反映した
- 未検証: 上の「未検証」と同じ。`/dry-check` は実装後に `/implement` 3a で走らせる（計画の段で比べる相手のコードが無いため）
