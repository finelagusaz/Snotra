# 実装計画 — issue #1139: `G-module-index` / `G-references` を編集時 reminder へ前倒しする

調査は `workspace/research.md`（実測・敵対的調査の採否を含む）。

## 目的

CI の `governance-check` job へ漏れてくる findings の 12/15 を占める 2 検査を、**編集時に鳴る層**として
1 枚足す。**CI は外さない**（issue の「決めること」第 3・`AGENTS.md`「検証の層と、層と層の隙間」）。

**この方向は既にユーザー裁定を得た先例を持つ**——`ADR-dependents-reminder-at-edit-time.md` は
「PR CI の `governance-check` へ報告行として混ぜる」案を**却下**しており、理由は「CI 時点では編集者が
既に離れており、**直せる時点で出る**という reminder の主な価値を失う」（2026-08-19 裁定）。

### 費用（実測・`research.md` に内訳）

| | 現状 | 本計画の後（見積もり） |
|---|---|---|
| `.rs` の編集 | `fmt` + `clippy` + crate test（秒オーダー） | **+70〜130 ms**（subprocess 1 本） |
| `.md` の編集 | 330〜386 ms（`dependentsReminder` の hook 全体・ADR 実測） | **+70〜130 ms**（別 CLI・(D) の決定） |

上限側の 130 ms は `ADR-dependents-reminder-at-edit-time.md` が静的 import を却下したときに払うと決めた
実測値（「代償は node 起動 109〜130 ms」）、下限側の 70 ms は 3b が実 `spawnSync` で測った値。
独立導出の再測は 116 ms（うち判定は 28 ms・支配項は node 起動）で、同じ幅に入る。

**`.rs` の経路に新しく費用が乗ることを明示しておく。** `dependentsReminder` は `.md` 判定を先頭に置いて
`.rs` 経路に一切の費用を載せていない。本計画はその設計を**意図的に破る**——索引を見るのが目的だからである。
代償は最頻操作である `.rs` 編集への +70〜130 ms で、同じ編集で走る `cargo clippy`（秒オーダー）に対して
小さい。
参考: `node scripts/governance-check.mjs` 全体は 0.65 秒で、**毎編集で回しても払えない額ではない**——
**絞る根拠は費用ではなく帰属である**（全体を回すと編集と無関係な既存の赤が毎回鳴り、慣れを作る）。

## 受け入れ条件

1. `.rs` を Edit / Write したとき、**そのファイルが所属 crate の `CLAUDE.md` 索引に無ければ**、
   編集直後に reminder が出る（今日は Write のときに**無条件で**出ており、判定を持たない）
2. `<crate>/CLAUDE.md` を編集したとき、その crate の索引と実ファイルの**双方向**の不整合が reminder に出る
3. ガバナンス文書（`governanceDocs` の 35 件）の `.md` を編集したとき、**その文書内の**実在しない参照が
   reminder に出る
4. reminder は **exit code を動かさない**（gate ではない）。`.rs` の `fmt` / `clippy` / crate test の
   合否に影響しない
5. `npm run governance:check` と `npm test` が緑
6. **`G-hook-fires` / `BUDGETS` / `docs/hooks.md` の発火一覧表 / `post-edit.test.mjs` の id カナリアが
   無傷である**（reminder は `checks.push` を通らないため。これは前提であり、Phase 4 の検証項目）

## 設計の確定（`research.md` の分岐 (A)〜(D) の決着）

| 分岐 | 決定 | 理由 |
|---|---|---|
| (A) gate か reminder か | **reminder**（exit code を動かさない） | 新規 `.rs` を書いた直後に索引が未更新なのは正常な作業順。gate だと正当な途中状態が赤になり無視される（`detector-scope-only-as-tight-as-needed`）。gate は CI 側が持ち続ける |
| (B) トリガー | **`.rs` は Edit も Write も対象**。ただし reminder の内容は**編集した当のファイルに帰属する findings へ絞る** | Edit まで要るのは #629/#630 の形（作成後に索引を書かず以後の編集が全部沈黙）を捕まえるため。帰属で絞るのは 3b の反証（債務窓の間、無関係な Edit のたびに同じ reminder が繰り返し出る）への手当て |
| (C) 届け先 | **`warnings`（`systemMessage`）と `sections`（`additionalContext`）の両方** | #629/#630 の失敗主体はエージェントなので `additionalContext` が要る。人間にも見せる。先例は `TS_LIKE` の情報行（検査でないものを `sections` へ載せる形）。**過去ログでは決まらない**ため設計原則で決めた（`research.md` (C) の ⚠️） |
| (D) `.md` の二重 spawn | **別 CLI にする**（`dependents.mjs` へ相乗りさせない） | 責務が別（`dependents` は合否を持たない計器、こちらは検査由来の findings）。費用差は約 +90 ms で、既に受け入れている `dependentsReminder`（235 ms）より安い |

## 非目標

- **`G-module-linkage`（`mod` 宣言）は前倒ししない** — issue は 2 検査を名指しし、`G-module-linkage` は
  CI 45 日で 0 件＝issue 自身の選定基準に掛からない。`AGENTS.md`:67 の「`mod` 忘れは `governance:check` が
  赤にする」は**真のまま残る**
- **削除（`rm`）は見ない** — `Edit|Write` matcher に届かない。CI が orphan を捕捉する残余は不変
- **CI の `governance-check` job は変更しない**

## 変更ファイル一覧と対象シンボル

| ファイル | 変更 |
|---|---|
| `scripts/governance/edit-findings.mjs` | **新規**。`scopedFindings(snapshot, rel, filterIgnored)` / `reportFor(...)` + CLI。**`checks/` へ置いてはならない**（`registry.mjs` の `checkModulesFrom` が `id`/`run` を要求して throw し、`governance:check` 自体が起動しなくなる） |
| `scripts/governance/edit-findings.test.mjs` | **新規**。`dependents.test.mjs` の型（正常 green / 変異 red / 判定対象外の不混入） |
| `.claude/hooks/post-edit.mjs` | `editFindingsReminder(rel, root, run = spawnSync)` を新設（`dependentsReminder` と同型・subprocess）。`isSourceFileWrite` を**削除**し、`main()` の無条件 WARN をこの reminder へ置き換える。ヘッダ 15 行目の数え上げを下限主張へ倒す |
| `.claude/hooks/post-edit.test.mjs` | `isSourceFileWrite` のテストを `editFindingsReminder` のカナリアへ差し替え。`selectChecks` の期待値は**変えない**（reminder は id を持たない） |
| `docs/hooks.md` | 「検査ではない reminder（発火一覧に現れない）」表の `.rs` 行を書き換え、`.md` 行に参照実在を足す |
| `CLAUDE.md`（ルート）「フック」節 | 「`.md` には検査ではない reminder が **1 つ**在るが」の数え上げを下限主張へ |
| `AGENTS.md`「条件別チェック」表の**ファイル（`.rs`）を追加/削除**行 | **偽にはならない**（`G-module-linkage` を前倒ししないので「索引と `mod` は別々に機構が見る」も「`mod` も足りていると読まない」も真のまま）。だが**索引だけが編集時に見えて `mod` は見えない非対称**が生まれ、名指ししないと「索引も見ていない」と逆に読まれる |
| `AGENTS.md`「条件別チェック」表の**ガバナンス文書を変更**行 | 同じく誤導。「reminder が出なかった＝緑」と読む経路ができる |
| `docs/development-principles.md`「検証の層と、層と層の隙間」の層の表 | 「文書の整合 \| `governance:check`」の**手段の列が不完全**になる。**この節自身が「穴は層の境界に空く」を説いている** |
| `scripts/governance-check.mjs` の契約ヘッダ | 「hook は `.md`・rules・skills に**検査**を割り当てない」は真のまま（reminder は検査ではない）。だが「PR CI と `npm run governance:check` で引き取る」の**口が 3 つになる** |
| `.claude/skills/implement/SKILL.md`（2 箇所） | 「索引漏れは `governance:check` が捕捉する……が PR まで漏らさない」と「**hook が走らせない** `cargo doc` と `npm run governance:check`」——後者は `governance:check` の一部が hook で走るようになるので動く。**memory の `partial-automation-habituates`（hook の沈黙に慣れて hook が走らない検査を飛ばす）がまさにこの行である** |
| `docs/build-commands.md` | 3 箇所。「**検査とは別に gate ではない reminder が在り**」（カテゴリ表の直前）に索引・参照実在を足す。「`.rs` では PostToolUse フックが走るが、**その沈黙は fmt / clippy / test の合格であって見出し参照の着地を含まない**」と「PostToolUse フックは `.md` に検査を割り当てない」の 2 文の射程が動く |
| `.claude/rules/governance-docs.md` | 「**検査ではない reminder は 1 つ出る**」の数え上げ。同項の「`.rs` では逆に hook が走って沈黙するが〜」の射程も動く |
| `docs/adr/ADR-<slug>.md` | **新規**。却下した案（gate 化 / `dependents.mjs` への相乗り / `G-module-linkage` の同梱 / 全体を毎編集で回す）を持つ。**既存の `ADR-dependents-reminder-at-edit-time.md` は書き換えない**（`ADR-adr-frozen-history`） |

`SPEC.md` の更新は**不要**（アプリの挙動ではなく開発時セーフティネットの変更）。

**波及先の母集団は、既存 ADR（`ADR-dependents-reminder-at-edit-time.md`「帰結」）が持っている**——
#1140 が同じ訂正を入れた先として「ルート `CLAUDE.md`・`docs/build-commands.md`（2 か所）・`docs/hooks.md`
（2 か所）・`.claude/rules/governance-docs.md`・`post-edit.mjs` の契約コメント」を名指ししている。
**この計画の初版は `docs/build-commands.md` と `.claude/rules/governance-docs.md` を落としていた**
（#977 / #1056 の型——写しを直す当のコミットが取りこぼす）。**PR 本文も数え上げの母集団に含める**
（squash で main の commit message になるがファイルの grep には入らない・#1056）。

## 実装順序と作業項目

**Phase はコミット境界ではない。** Phase 2（hook 配線）と Phase 3（文書同期）は**同じコミット**に束ねる
——配線だけ入って規範文書が古いままの中間状態は、`AGENTS.md`「3 層分担」の
「挙動を変える変更では、両者を同じ変更で整合させる」に触れる。Phase 1（CLI 単体）は
`node scripts/governance/edit-findings.mjs <rel>` として単体で動くので先行してよい。

### Phase 1 — CLI（`scripts/governance/edit-findings.mjs`）

判定を再実装せず、`checkModuleIndex` / `checkReferences` / `MODULE_INDEX_CRATES` / `governanceDocs` /
`gitIgnoredPaths` を import して呼ぶ。

- [x] `scopedFindings(snapshot, rel, filterIgnored)` を書く。`rel` の形で母集団を決める
  - `.rs` は **`MODULE_INDEX_CRATES` の `cfg.src` と `cfg.exts` で判定する**
    （**⚠️ この項の理由付けは 2 度書き直した。経緯と現在の記述は
    `docs/adr/ADR-scoped-governance-findings-at-edit-time.md` が正本である**——当初の
    「前方一致だと偽の reminder になる」も、次に書いた「費用の最適化であって正しさの門ではない」も、
    どちらも実装より強い主張だった。**ここに 3 つ目の言い換えを置かない**——同じ命題の写しが増えるほど、
    次の訂正で取りこぼす枚数が増える）
    → `checkModuleIndex(snapshot, [crate])` を呼び、**finding のメッセージが `rel` を含むものだけ**を返す
  - `<crate>/CLAUDE.md` → `checkModuleIndex(snapshot, [crate])` を**全件**返す（その文書が主語なので全件が帰属する）
  - `governanceDocs(snapshot).includes(rel)` な `.md` → `checkReferences(snapshot, [rel], filterIgnored)`
  - それ以外 → `[]`
- [x] **【実装中に判明・計画の訂正】索引と参照実在は排他ではない** — 計画は上の 3 分岐を排他的に書いていたが、
      `<crate>/CLAUDE.md` は `MODULE_INDEX_CRATES` の索引対象であり、かつ `governanceDocs()` の正規表現
      `/^(snotra-core|…)\/CLAUDE\.md$/` にも**含まれる**。ゆえに両方が帰属する。**累積で組んだ**
      （テスト「`<crate>/CLAUDE.md` は索引と参照実在の**両方**に帰属する」が固定）
- [x] **実物のツリーで CLI を走らせて確認した**（合成 fixture だけで済ませない・ADR の先例「実物で走らせて
      初めて出た欠陥」）: 索引に無い `.rs` → 1 件 / 索引に在る `.rs` → 沈黙 / 壊れた参照を持つ `.md` → 1 件 /
      `tests/` 配下 → 沈黙。**4 ケースとも exit code 0**
- [x] `reportFor(snapshot, rel, filterIgnored)` を書く。findings が 0 件なら**空文字**（呼び出し側は何も出さない）。
      1 行に畳み、件数の上限は `dependents.mjs` の `LISTED = 3` に倣う。全件を見る再現コマンドを文言に含める
- [x] CLI 部（`isMain` ガード・`makeSnapshot(process.cwd())`・**exit code は常に 0**）。
      shebang を置かない（CRLF checkout で vitest の transform が SyntaxError になる）
- [x] `//!` ヘッダに、`checks/` へ置けない理由・静的 import してはならない理由・合否を持たないことを書く
- [x] **実装中は自分で `npm test` を回す** — `scripts/governance/**.mjs` の編集に PostToolUse hook は
      何も走らせない（`docs/hooks.md` 発火一覧の「上記以外」）。**沈黙は「何も走らなかった」である**
- [x] `edit-findings.test.mjs`（正常 green / 3 種の変異で red / 判定対象外が混じらないこと）。
      **`checkModuleIndex` / `checkReferences` はモックせず実物を呼ぶ**——帰属フィルタはメッセージ書式に
      文字列で結合しており、モックするとその書式変更を検知できない（上の不変条件表）。
      **接頭辞関係にあるパスで誤爆しないことも固定する**——`includes` は部分一致なので、
      理屈の上では `rel` が別のファイルのパスの一部になりうる（`.rs` 拡張子ゆえ実害はほぼ無いが、
      境界を測らずに「起こらない」と書かない）

### Phase 2 — hook 配線（`.claude/hooks/post-edit.mjs`）

- [x] `editFindingsReminder(rel, root, run = spawnSync)` を新設。`dependentsReminder` と同型
      （`existsSync` でスクリプトが無ければ静かに `""`・`res.status !== 0` でも `""`・`MAX_BUFFER` と
      `PER_CHECK_TIMEOUT_MS` を共有）
- [x] `isSourceFileWrite` と、それが出していた無条件 WARN を削除する
- [x] `main()` で reminder を `warnings` と `sections` の**両方**へ積む（(C)）。
      `sections` 側は `--- <id>: 失敗 ---` 形式にしない（検査の失敗と読ませない・`TS_LIKE` の情報行に倣う）
- [x] `post-edit.test.mjs` を差し替える（`editFindingsReminder` の注入テスト・`selectChecks` は不変であることの確認）
- [x] **配線をプロセス級の統合テストで固定する** — `dependentsReminder` の先例が「戻り値を `warnings` へ積む
      2 行は誰も見ておらず、消してもユニットテストは 96/96 緑だった」と実測している
      （`ADR-dependents-reminder-at-edit-time.md`「帰結」）。**同じ穴が新 reminder にも空く**。
      一時 git リポジトリへ最小の木を作り hook をプロセス起動する形（#1140 が確立済み）を踏襲し、
      **その変異が赤になることまで確かめる**
- [x] **統合テストの assert は reminder ごとに 1 本ずつ置く** — `.md` の編集では `dependentsReminder` と
      `editFindingsReminder` が**両方鳴る**。assert を「WARN が在る」で済ませると、片方の配線が消えても
      もう片方が埋めて沈黙する（束ねた長さが片方の消滅を隠す形——`runAll` の 0 件検知が母集団ごとに
      1 本ずつ要るのと同型）

### Phase 3 — 文書同期

**数え上げは下限主張（「〜だけではない」）へ倒す**（`universal-claim-fix-regenerates-itself`：
偽の全称を直した文がまた別の形で偽になる連鎖を止める）。`post-edit.mjs`:15 は今日すでに「2 つ」と
書いて 3 項目を挙げており、数を直す方向は同じ誤りを再生産する。

**引用位置は行番号ではなくシンボル名・見出し名・逐語の断片で書く**
（`docs/development-principles.md`「撤去（消す変更）の作法」——行番号は書いた時点で既にずれていることがある）。

- [x] `docs/hooks.md`「検査ではない reminder」表を書き換える
- [x] `post-edit.mjs` の契約ヘッダ（「検査とは別に、gate ではない reminder が 2 つ在る」の行）
- [x] ルート `CLAUDE.md`「フック」節
- [x] `AGENTS.md`「条件別チェック」表の射程を狭める（**索引は編集時にも見られる / `mod` は `governance:check` のまま**）
- [x] `docs/build-commands.md`（3 箇所）
- [x] `.claude/rules/governance-docs.md`
- [x] `.claude/skills/implement/SKILL.md`（2 箇所）
- [x] `docs/development-principles.md` の層の表
- [x] `scripts/governance-check.mjs` の契約ヘッダ
- [x] `AGENTS.md` の 2 行（**偽にはならないが誤導する**——非対称を名指しする一文を足す）
- [x] **`governanceDocs` の外の `.md` は依然として沈黙することを書く**（`PERFORMANCE.md` / `RETROSPECTIVE.md` /
      `README.md` / `.claude/agents/**` / `docs/adr/**` / `.claude/skills/*/references/**`、および `workspace/`
      ——`makeSnapshot` の走査除外）。**「`.md` を編集すれば参照実在が見える」は偽の全称になる**
- [x] 新機構が**削除を見ない**ことが誤読されない一文を、`docs/hooks.md` の表かその直後に残す
- [x] ADR を新規に書く（却下した案を持つ。既存 ADR は書き換えない）
- [x] **6 枚を直したあと、同じ事実の写しが他に無いか grep で数え直す**（#977 は 5 枚へ書いて 6 枚目を落とし、
      その 6 枚目が規範の根拠として使われていた）

### Phase 4 — 検証（受け入れ条件の実測）

- [x] `npm test`（**814 passed / 34 files**）と `npm run governance:check`（**exit 0・検査 19 件**）が緑
- [x] **実際に hook を発火させて測った**（`run-new-verification-path-before-reporting`: 新設した検証経路は
      完了報告の前にその経路自体を実行する）。索引に載せない `.rs` を実ツリーへ置き、**Write と Edit の
      両方**で reminder が `additionalContext` として会話へ届くことを観測した（旧 `isSourceFileWrite` では
      Edit で沈黙していた形が塞がっている）。probe は撤去済み。
      **変異注入（配線を消して赤になることの確認）は Step 3 の委譲先が worktree で行う**——
      `/implement` は「注入するのはこのエージェントだけである: 主エージェントは同じ木へ注入しない」と定める
- [x] **受け入れ条件 6 を実測**（`G-hook-fires` が緑であることだけを根拠にしない）: 変更前の版を
      `git show f114a28:` で取り出し、`selectChecks` の返り値 14 パス分・`BUDGETS` のキー集合・
      `docs/hooks.md` 発火一覧表の「走る検査 id」列を機械比較した。**いずれも差分ゼロ**
- [x] `.rs` の編集で `fmt` / `clippy` / crate test が**沈黙＝合格**のまま reminder だけが出ることを観測
      （reminder は exit code を動かしていない）

## 不変条件と異常系

| 不変条件 | 壊れたときの検知 |
|---|---|
| reminder は exit code を動かさない | Phase 4 の実測（`.rs` 編集で検査が緑のまま reminder だけ出る） |
| 判定を hook へ**静的 import しない** | 静的 import すると `try { main() } catch` の外で解決が走り、失敗時に**全編集が沈黙する**。`post-edit.test.mjs` が subprocess 呼び出しであることを固定する |
| CLI を `scripts/governance/checks/` へ置かない | 置くと `registry.mjs` が throw して `governance:check` が起動しなくなる（`npm run governance:check` が即座に落ちるので沈黙しない） |
| スクリプトが無いツリー（凍結された worktree）では静かに no-op | `existsSync` ガード。テストで固定 |
| `filterIgnored` を渡し忘れると gitignore 済みパスで**偽の赤**になる | Phase 1 のテストで、ignore 対象の参照が finding にならないことを固定 |
| `MODULE_INDEX_CRATES` の写しを hook 側に作らない | crate 追加時に片方だけ知っている状態を作らない（#500） |
| **`<crate>/tests/**.rs` では鳴らない**（`cfg.src` 配下だけが母集団） | Phase 1 のテストで `snotra-core/tests/foo.rs` が `[]` を返すことを固定する（判定対象外の不混入） |
| **帰属フィルタ（`finding.message.includes(rel)`）が `G-module-index` のメッセージ書式に依存する** | **沈黙側で壊れる**——書式が変われば静かに 0 件になる（`docs/development-principles.md`「検証の層と、層と層の隙間」の「母集団を切り出して述語へ渡す形」）。`finding` は `{file, line, message}` で `file` は**文書側**のパスなので、編集ファイルとの結合は文字列しかない。**検知器は Phase 1 のテストを実 `checkModuleIndex` で書くことである**——fixture でモックすると書式変更を検知できない |

**異常系**: `git` が無い / repo でない → `gitIgnoredPaths` が空集合を返す（何も免除しない＝赤側・安全）。
subprocess が非 0 で終わる → reminder は空文字（沈黙）。**この沈黙は「不整合が無い」を意味しない**
——reminder は検査ではないので、この非対称は文書に書く。

## テスト方針と検証コマンド

- `node scripts/governance-check.mjs`（0.65 秒）
- `npm test`（`edit-findings.test.mjs` と `post-edit.test.mjs` を含む）
- 変異注入は**複製かメモリ上のラップ**で行い、稼働中の `.claude/hooks/` と `scripts/governance/checks/` を
  変更しない（`.claude/rules/safety-nets.md`）

## 未確定（実装前に潰す）

計測は `<scratchpad>/measure-scope.mjs`（変異は `snapshot` のラップで注入・稼働中のファイルは無変更）。

- [x] **帰属フィルタの述語を代表入力で測る** — 済（2026-08-19）。索引に無い `.rs` を 2 件同時に注入すると
      `checkModuleIndex` は 2 件返し、**メッセージが `rel` を含むか**で絞ると当のファイルの 1 件だけが残った。
      逆方向のメッセージは `実ファイル <フルパス> が索引（本文のバッククォート）に見当たらない` の形で
      `rel` のフルパスを含む。**順方向（`索引に記載の \`X\` に対応する実ファイルが無い`）は `rel` を含まないので
      落ちる**——これは設計どおり（編集ファイルに帰属しない。CI が引き取る層）
- [x] **`filterIgnored` を絞った呼び出しへ渡したとき免除が効くか** — 済（2026-08-19）。
      `docs/hooks.md` へ `` `node_modules/vitest/vitest.mjs` ``（`.gitignore` 済み）の参照を注入したところ、
      **`filterIgnored` あり 0 件 / なし（既定引数）1 件**。**渡し忘れると偽の赤になる**ことが実測で確定した。
      対照（免除対象 1 件 + 実在しない 1 件）では実在しない側だけが 1 件残る
- [x] **Edit まで広げたときの実発火率** — 済（2026-08-19）。**編集粒度は原理的に測れない**——git が記録するのは
      コミットだけで、編集の中間状態は履歴に無い（`dependents.mjs` も実装後にコミット粒度で測り、
      「編集 1 回あたりはこれより低い」と but 書きを残している）。**コミット粒度の代理は測れた**:
      新規 `.rs` を含む直近 20 コミットのうち `CLAUDE.md` を同時更新していないのは 2 件で、**どちらも
      `tests/` 配下＝`G-module-index` の母集団外**。すなわち `src/` 配下の新規 `.rs` は **18/18 が同時更新**で、
      **コミット粒度では索引債務が 0 に保たれている**（3b の 12/12 と独立に一致）。
      ゆえに 3b が挙げた「債務窓の間の反復」は**現実には稀**だが、起きうる以上は帰属フィルタで手当てする。
      **受容する残余**: 編集粒度の発火率は実装後にも体系的には測れない

## 重複に見えるが意図的に分けた構造（レビューへ先に渡す）

`/dry-check` 実施済み（手書き重複 0 件）。下の 3 件は**意図的な相似**であり、DRY 違反ではない。

| 相似 | 根拠 |
|---|---|
| `reportFor` が `dependents.mjs` と同名・同型 | 責務が別（あちらは合否を持たない計器、こちらは検査由来の findings）。件数上限 `LISTED` も共有しない——**片方だけが変わる将来を挙げられる**（参照の一覧と索引の一覧では読みやすい件数が違う） |
| `editFindingsReminder` が `dependentsReminder` と同型の spawn 定型を持つ | 2 回目ゆえ DRY 規約の許容内（`docs/development-principles.md`「2 回まで許容し 3 回目で抽出」）。判定条件も違う（`.md` 限定 vs `.rs`+`.md`）。**3 本目が来たら抽出する** |
| `makeSnapshot(process.cwd())` + `isMain` の CLI 定型 | 既存 3 本（`dependents` / `governance-check` / `governance-manifest`）が抽出していない 1 行の定型。4 本目も倣う |

**`MODULE_INDEX_CRATES` を使うことは `G-module-linkage.mjs`:27 の「使わない」判断と矛盾しない**——
あちらが使わないのは「リンク性は `CLAUDE.md` の有無と無関係」だからであり、本 CLI は**まさに
`CLAUDE.md` の索引**を見る。どちらを使うかは母集団の意味で決まる。

## plan-review 結果

- リスク: **高**（hook・rules・ガバナンス文書の変更／網羅性が要件）
- レビュー方式: **独立導出 1 体**（Step 2b。`workspace/` を読ませず、grep の走査範囲からも除外させた
  ——`repo-wide-grep-contaminates-independent-derivation`）。成果物は `workspace/plan-review-hook-frontload.md`
- エージェント数: **2**（3b の敵対的調査 1 体 + 独立導出 1 体）

### 要対処（すべて反映済み・根拠は主エージェントが自分で grep して再照合した）

- **導出 ∖ plan の漏れ 4 枚** — `docs/development-principles.md` の層の表 / `scripts/governance-check.mjs` の
  契約ヘッダ / `.claude/skills/implement/SKILL.md` の 2 箇所目 / `AGENTS.md` の 2 行目。4 箇所とも
  `sed` で実在と逐語を確認して変更ファイル一覧へ追加した。**波及先は 6 枚から 10 枚になった**
  （初版は 6 枚のうち 2 枚も落としていたので、**都合 6 枚を後から足したことになる**——#977 の型が
  この計画自身で 2 回起きた）
- **`governanceDocs` の外の `.md` は依然として沈黙する** — `PERFORMANCE.md` / `RETROSPECTIVE.md` /
  `README.md` / `.claude/agents/**` / `docs/adr/**` / `workspace/`。`governanceDocs` の定義を読んで確認した。
  「`.md` を編集すれば参照実在が見える」は偽の全称になるので、Phase 3 の作業項目に加えた
- **`.rs` 経路への費用**（`dependentsReminder` は載せていない設計を意図的に破る）を費用表へ明記
- **帰属フィルタの接頭辞誤爆**をテストで固定する

### 軽微

- 判定の不一致 1 件: `AGENTS.md` の「`mod` も足りていると読まない」を、私は「偽になる」と書いていたが
  **導出の「偽にはならないが誤導する」の方が正確**（`G-module-linkage` を前倒ししないため）。訂正した
- `scripts/governance/**.mjs` の編集に hook が走らないことを Phase 1 の作業項目へ

### 未検証

- Edit まで広げたときの**編集粒度**の発火率（未確定欄 3 で「原理的に測れない」と裁定済み）
- CI の Windows runner での費用（`governance-check` job は `ubuntu-latest` なので該当しない）

### 判断

- 実装着手: **人間の裁定待ち**（セーフティネットの変更ゆえ `CLAUDE.md` 最重要ルール 2 に従う）

## 人間レビュー

- [x] 承認済み — 2026-08-19
  - 問い: "現行の「`.rs` を Write したら無条件に索引更新を促す WARN」（isSourceFileWrite）を、実際に索引へ
    無いときだけ鳴る判定つき reminder へ置き換えてよいですか？" / 回答: **"置き換える（推奨）"**
  - 問い: "発火の広さと届け先をどうしますか？（#629/#630 は「作成後に索引を書かず、以後の編集が全部沈黙する」
    形の再発でした）" / 回答: **"Edit も対象 + 両チャネル（推奨）"**
  - 問い: "この計画で実装（/implement）へ進んでよいですか？" / 回答: **"承認する"**

**この承認は `CLAUDE.md` 最重要ルール 2（セーフティネットの変更は合意してから）を満たす**——
`.claude/hooks/post-edit.mjs` の発火条件の拡大・既存 reminder の置き換え・エージェントへ届く面の追加の
3 点について、名指しで合意を得た。
