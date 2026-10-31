# 独立導出 — issue #1314（`/implement` の本文から経緯を外す）

対象: `.claude/skills/implement/SKILL.md`（HEAD = `fb0914a3`、ブランチ `chore/implement-skill-history`）。`workspace/plan.md`・`research.md`・`adversarial-1314.txt` は読んでいない。
基準値（HEAD の実測）: 180 行・issue 参照 17 個（異なり 16 種。`#934` だけ 2 回）・「実測」5 個。issue 本文の数と一致する。バッククォート語・`#NNN`・`ADR-*` の異なり集合は 118 行（`grep -oE` で数えた）。

振り分けの軸は `/retrospective`「Step 3 — 教訓の配置（トリガーに括り付ける）」の第二軸（`.claude/skills/retrospective/SKILL.md:62`）を使う。**手順**はスキル、**否定の知識**は ADR、**失敗の一次証跡**は issue。この repo では main の commit message と PR 本文も実際の置き場として使われている。

---

## 1. 段落ごとの振り分け（残す ／ 消してよい経緯と、その経緯がいま在る場所）

| 行 | 残す（規則・射程・設計の理由） | 消してよい経緯 → いま在る場所 |
|---|---|---|
| 18 | 1 行で宣言し、Step ごとに報せる規則。「読者は tool の出力を見ていない」。**較正の注記（Fable 5.1・#1230・Opus 5.5）も残す**（→ 2-A） | なし |
| 24–34 (1a) | 3 性質と機械判定の述語。「節の中に限る」と「見出しの存在も確かめる」の 2 つの条件。fail-closed と、その非対称の理由（34 行目） | 31 行目の「（＝自覚なき書き手の産物）」と「（#821）」は削れる。この経緯は commit `14ebd361` の本文 13–15 行目にある（「見出しの存在を落とすと、欄を書かなかった計画が素通りして fail-open へ退化する」）。ただし「fail-open へ退化する」という帰結は規則の理由なので残す |
| 36–68 (1b/1c) | すべて規則 | なし |
| 72 | Red/Green の規則。**「テストだけ先に書いても得られるのはコンパイルエラーであり、実装に当てて落ちたことにならない」は Red の定義そのもの**（→ 2-E） | 「（#745 実測）」だけ削れる。経緯は commit `475932e2` の本文 14 行目にある（「コンパイルエラーにしかならず手順が実行不能だった。todo!()/no-op に当てて落とす形に」） |
| 73–76 | 規則 | なし |
| 77 | 規則の本体は「`.rs` を足す・消したら `CLAUDE.md` のモジュール構成節の索引を同じ変更で直す」。**`MODULE_INDEX_CRATES` に限るという射程は残す**（→ 2-D） | 括弧内の機序の説明（「判定は節の中のバッククォート付きの basename を見るだけで…」「#1214 実測」「#701」「#1008」）。置き場: **`ADR-module-index-reverse-scope`**（`docs/adr/ADR-module-index-reverse-scope.md:3,9`。#1214 の死角）、**`scripts/governance/checks/G-module-index.mjs:23-38`**（#701 のカナリア、#1008 の素通り）、`scripts/governance-check.test.mjs:17-19`（#701 のカナリア）、commit `30d96e0f` と PR #1219 本文。**この段落は `AGENTS.md`「条件別チェック（トリガー → 参照先）」表の「ファイル（`.rs`）を追加/削除」行（#562・G-module-linkage）の写しでもある**。その行を指して 1 行に縮める案が立つ |
| 78 | 規則 | なし |
| 82 | 規則（A〜F は道具の木で、レビューと変異注入は委譲先の木で行う） | なし |
| 84 | 「対象を不変オブジェクトにする」「動かないものを渡す」と、2 つの報告が別の版へ向かない理由（どちらも sha を機械可読で名乗る）。ADR へのポインタ | 「動く対象を渡していた頃は…mtime で鑑定する羽目になった（2026-08-19 に同一セッションで 2 回。うち 1 回は…）」。置き場: **`ADR-implement-verification-delegated-to-worktree`**（`docs/adr/…delegated-to-worktree.md:5-8`。事故 2 件と mtime 鑑定）、PR #1137 本文 12 行目 |
| 86 | 規則（主は速い確認を回し、道具が権威ある一式を回す） | なし |
| 90 | `git status --porcelain` の確認。worktree はコミットから切られるので、未コミット分は検証されない。**しかも緑で返る**。委譲先の木は道具を通らない、という射程 | 「（対象ゼロで自明に green を返す形・#922）」は削れる。同じ型は `AGENTS.md`「検証の作法（全タスク共通）」の最後の段落（#922）が持つ |
| 91 | 規則。「`/start-issue` を経ていない単体起動時に該当しやすい」は徴候なので短くできる | なし |
| 92–93 | 規則と理由（93 の理由は Step 4 との連鎖を示す。残す） | なし |
| 94 | check スキルを主が自分で回す規則。表への委任。**写しであることの前提条件**（→ 2-C）。**計画段階で走っていても、ここでもう一度実行する理由**（→ 2-F） | なし（理由の置き場だった `ADR-plan-ownership-boundary` は #895 で削除済み。`git log --all -- docs/adr/ADR-plan-ownership-boundary.md` → `69fc7f00`） |
| 98 | 規則 | なし |
| 100 | 該当判定は意味で行う規則。E と F は常に回す。`--also` | 「（`.claude/rules/src-tauri.md`・#558）」は残してもよい（ポインタ）。理由は `ADR-implement-verification-tool-tree:18`、`docs/build-commands.md:63` に重複して在る |
| 101 | 「C を要求する巡は直列」と、**その理由（smoke は既に起動していると throw する）**（→ 2-G） | なし |
| 102 | 規則（exit 2 の意味） | なし |
| 104 | 規則（`name` を付ける） | なし |
| 106–107 | `align` を最初に、巡ごとに打たせる。絶対パスを `/` 区切りで渡す。exit 2 で止まる | 「harness の木はセッション開始時点のコミットで切られ、アンカーと一致しない（2026-08-19 実測）」。置き場: **`ADR-implement-verification-delegated-to-worktree`「初回運用で測ったこと」**（「worktree の HEAD はアンカーの sha ではない」）、`ADR-implement-verification-tool-tree:5`（20/20 PR）、PR #1302 本文「背景」。ただし「そのコミットには道具が無いことがある」は、`npm run` ではなく絶対パスで呼ぶ理由なので残す |
| 108–109 | 規則 | なし |
| 110 | 内容ハッシュで照合する（mtime は偽陽性を出す）。赤の理由を assert で確かめさせる。注入するのは委譲先だけ | 「（道具との並走で CPU 競合が増える。#1154・#1179 で委譲先自身の並走による timeout の赤が出た）」。**機序は `ADR-implement-verification-tool-tree`「受容した残余」3 項目目にある。#1154・#1179 という事件の帰属はどこにも無い**（PR #1302 の本文とコメント、issue #1298 の本文、PR #1156/#1184 の本文を grep して 0 件）。消すなら ADR へ日付つきの追記が要る。先例は `ADR-canonical-heading-references.md:29` の「2026-07-26 追記」。issue の「無ければ足す」に当たる唯一の項目 |
| 111–112 | 規則 | なし |
| 113 | `.delegate/` へ書かせる。**木の外へ書かせない**。直列の巡では要約のパスも渡す | 「木が『変更なし』のまま止まると harness が片づけ…（#1298 で実測）」と「`report*.md` は Write が拒否される」。置き場: **`ADR-implement-verification-tool-tree:15`**（3 つ目の却下）、`scripts/verify-tree.mjs:21-22`、PR #1302 本文の表 2 行目。`report*.md` の拒否はルート `CLAUDE.md`「サブエージェント委譲と worktree」にある。ただし「`.delegate/` はどのコミットにも現れないので、次の巡の checkout と衝突しない」は配置の理由なので 1 句残す |
| 115 | 規則（Agent はこの 1 体だけ、計画レビューを再実行しない） | なし |
| 117 | 規則（manual-smoke の目視は委ねる。自動化された部分は実行する） | なし |
| 121 | 規則。理由は「報告の sha == HEAD を保つ」。ADR へのポインタ | 「（待ち時間に次を実装する形を却下した理由と同じ）」は ADR（`…delegated-to-worktree.md` の却下 6 項目目）にあるので短くできる |
| 123–125 | 規則 | なし |
| 126 | 再開の規則と「1 体だけに反しない（再開は起動ではない）」。**利点の (1) と (2)**（→ ⚠ 6-2） | 「（#934 で実測: 2 巡目が ⚠ 1 件を独立検証して…7 件挙げた）」。置き場: **commit `d9b5444a` の本文**（「## Step 4b」節）、PR #958 本文 22 行目。ADR には無い |
| 127–129 | 出口条件。母集団はアンカーのまま。`/race-check` の `--base` の例（射程の例示）。「解消した」は自分で測る。状態でゲートされた挙動 | なし |
| 131–134 | 規則 | なし |
| 138 | **この順序で行うことと、その理由**（PR 前ゲートは要約の sha から HEAD までの `workspace/` 外の変更を照合する。ADR の回収コミットは `workspace/` の外に載る） | 「（#1299・例外は無い）」の #1299 は削れる。経緯は `ADR-pr-gate-verified-sha:12-14` と `.claude/hooks/pre-bash.mjs:4,350` にある。**「例外は無い」は禁止の強さを決めているので残す**（→ 2-H） |
| 140 | ADR 回収の規則・トリガー・書く中身。issue 側へ送る分岐 | 「（#593）」「（#812）」は出所のポインタ。`#812` は `.claude/rules/governance-docs.md` の命名規則と重複するので、「連番を振らない」は rules を指して縮められる |
| 141 | 規則 | なし |
| 142 | **後片づけの手順。exit 1 の 3 つの分岐ごとの対処**（道具の出力はこの対処を持たない → 2-I） | なし |
| 143 | 規則。「未チェックを残したままの削除は、PR 前ゲートの視界から計画を消す」は禁止の理由なので残す | なし |
| 144 | コミットしてから消す規則。**「守らず最後に一括更新した場合…『git 履歴から復元可能』が成り立つのはコミット済みの版だけ」は規則が成り立つ条件**（→ 2-E） | なし |
| 145 | 保護であって障害ではない。`-f` も `checkout` 系も使わない。**git 自身が `-f` を勧めるが従わない**（→ 2-B）。まず未コミットを見る。順序が逆であることを疑う。`git reset` で止め、作業ツリーに触るコマンドを続けて打たない | 「（2026-08-07 の #934 サイクルで、`git reset` の直後に `checkout-index -f` を打って実行記録を全損した。控えは無く、会話の一次出力から再構成した）」。置き場: **commit `d9b5444a` の本文**（「#934 のサイクルでこの矛盾を踏んだ…全損した」）、PR #958 本文 11 行目。**issue #934 の本文には無い**（`gh issue view 934` を grep して 0 件 → 4-1） |
| 149–180 (出力) | すべて規則。176 の「実行不能」を限る 2 条件、177、179 の同期規則、180 | なし |

**要約**: 経緯として消せるのは 18 行目を除き、主に 77・84・107・110・113・126・145 行目の括弧と、31・72・90・138 行目の issue 番号。どの段落も規則の本体は残る。経緯が段落の大部分を占めるのは 77 と 145 だけである。

---

## 2. 経緯に見えて、規則の条件・例外・射程を担っている文（消すと全称に化ける・禁止が弱まる）

- **A. 18 行目「較正の対象: Fable 5.1 以降…#1230。Opus 5.5 も同じ向き」**: `.claude/rules/safety-nets.md:33` が義務づける栓の名乗り（「どのモデルのどの癖への栓かを条項に添える。名乗らない栓は誰も外さない」）。commit `4a8c8478` 本文 8 行目は「どのモデルの癖への較正かを注記」と明言している。**消すと規則違反になる。**
- **B. 145 行目「git 自身が `use … -f to force removal` と勧めてくるが従わない」**: PR #958 本文と commit `d9b5444a` 本文は「git 自身が `-f` を勧めてくる句を引用したうえで『従わない』と明記した——禁止だけでは『git が言うのだから』で越えられる（実際の回避圧力はここだった）」と書いている。経緯の口調だが、回避圧力への栓である。同じ段落の `checkout-index` という語も、列挙された禁止対象（`git checkout` / `git checkout-index` / `git restore`）なので残す。消すのは「2026-08-07…再構成した」の括弧だけ。
- **C. 94 行目「この列挙は写しであり、表と同期している間だけ『出力』の母集団が一致する」**: `docs/adr/ADR-implement-step4-report-slot.md:15` は「前提条件を全称表現と同じ場所に書いた」ことを受容の条件にしている。消すと、179 行目「レビューの母集団は 3a の列挙と一致する」が無条件の全称になる。3a の列挙そのものも残す。同じ ADR が「表を直接指す」案を却下しており、`ADR-meta-harness-thinning.md:33` と `docs/check-skill-skeleton-design.md:119` がこの列挙の存在を前提に書いている。
- **D. 77 行目「ただし `MODULE_INDEX_CRATES` に載っている crate に限る」と、「同名のファイルが同じ節で既に言及されていれば緑のまま通る」**: PR #1219 本文（「規範 3 枚の是正」）は、この SKILL が「索引漏れは捕捉される」を**無条件で主張していた**ことを偽として直した。`docs/superpowers/specs/2026-08-09-detector-scope-audit-design.md:585` も C3 として同じ文を挙げている。射程の但し書きを消すと、偽の全称に戻る。一方、述語の中身（「バッククォート付き basename を見るだけで所有関係を見ない」）を書くのは `.claude/rules/governance-docs.md`「機構の実装の詳細（述語の種類・件数・分岐の列挙）を散文へ写さない」に当たる。**推奨: 「捕捉は一部の crate と一部の形に限る」という帰結を残し、機序は `G-module-index.mjs` のヘッダと `ADR-module-index-reverse-scope` を指す。** 同じ写しが `.claude/skills/health-check/SKILL.md:23` にも並んでいる。
- **E. 72 行目「テストだけ先に書いても得られるのはコンパイルエラーであり、実装に当てて落ちたことにならない」と、144 行目「守らず最後に一括更新した場合…コミット済みの版だけである」**: 前者は Red の定義を決めている。後者は、commit `d9b5444a` が「旧文は…その括弧が偽になる条件を持たなかった」と書いたとおり、旧文の欠落そのものを埋めた条件文である。どちらも消すと規則が緩む。
- **F. 94 行目「同じ check が `/start-issue` の計画段階で走っていても、ここで実行する——対象が計画と実装で別だからである（計画に無い変更は実装中に必ず生じる）」**: 二重実行を「重複」として畳まれないための理由。理由の元の置き場 `ADR-plan-ownership-boundary` は #895 で削除済み。生きた層ではここにしか無い（`docs/superpowers/specs/2026-07-29-plan-review-instrument-split-design.md:264` は歴史資料）。ルート `CLAUDE.md`「コミュニケーション原則」の「意図的なリファクタリングの結果を元に戻さない」がこの種の根拠を要求している。**括弧の「必ず生じる」は全称なので、同じ spec の :44 と :150 が問題にしている。** 書き換えるなら全称を弱める側へ倒す（⚠ 6-3）。
- **G. 101 行目「smoke は Snotra が既に起動していると throw するため（`scripts/lib/SnotraSmoke.psm1`）、木を分けても並走できない」**: C だけを直列にする判別条件。消すと「すべての巡を直列」か「C も並走」のどちらかへ読まれる。`ADR-implement-verification-tool-tree`「受容した残余」3 項目目にも同じ記述がある。
- **H. 138 行目「例外は無い」**: 禁止の強さ。`ADR-pr-gate-verified-sha:12-14` は例外の口を開ける案を 2 つ却下しているので、それを指してもよい。ただし語は残す。
- **I. 142 行目の exit 1 の分岐ごとの対処**: `scripts/verify-tree.mjs` の remove が出すのは「`.delegate/` 以外の変更が残っている——消さない」「ロックされている（委譲先がまだ動いているか）——何も消さない」だけ（`scripts/verify-tree.mjs:315,320` 付近の `console.error`）。「停止を待って打ち直す」「ロックは `--force` 1 回では外れない」「`git worktree list` で他セッションの `agent-*` を確かめてから `clean:worktrees -- --force`」は道具の出力に無い。「道具の出力を読め」へは縮められない。
- **J. 90 行目「しかも緑で返る」と「委譲先の木は道具を通らずに切られるので、ここで確かめる」**: #922 の番号は消せる。ただしこの 2 句は、道具の exit 2 があっても手で確かめる理由（射程）を決めている。
- **K. 113 行目「`.delegate/` はどのコミットにも現れないので、次の巡の checkout とも衝突しない」**: 配置の理由。`ADR-implement-verification-tool-tree:5` の「5 PR で前巡の未追跡の報告が checkout を止めた」に対する栓。

---

## 3. 他文書が `/implement` の見出し・本文に依存している箇所

**正準形の入力**（G-heading-refs の走査対象 = `docs/superpowers/` と `docs/adr/` を除く `.md`、`.rs`、スクリプトのコメント。`scripts/governance/lib.mjs:539-601`）:

| 着地先の見出し | 参照元 |
|---|---|
| `### 3a. 委譲の前に` | `scripts/race-boundaries.mjs:15`（スクリプトの腕）、`docs/check-skill-skeleton-design.md:119` |
| `### 3b. 委譲へ渡すもの` | ルート `CLAUDE.md:65`、`.claude/skills/start-issue/SKILL.md:82` |
| `## Step 4 — 仕上げ` | `docs/check-skill-skeleton-design.md:167` |

**正準形でない入力**（照合されない。見出しの番号・語に依存）: `.claude/hooks/pre-bash.test.mjs:415`（「`/implement` Step 4 の撤去の形」）、`scripts/verify-tree.mjs:3`（「`/implement` Step 3」）、`docs/build-commands.md:11`（「`/implement` が `--also` で足す」）、`.claude/skills/start-issue/SKILL.md:102`（「`/implement` Step 5 以降」。既に偽 → 4-2）。

**本文の文への意味上の依存**（逐語ではない。消すと参照元が偽になる）:
- 「主エージェントは同じ木へ注入しない」（110 行目）←`.claude/skills/start-issue/SKILL.md:82`（「主は同じ木へ注入しないため」）、`RETROSPECTIVE.md:27`（「`/implement` は主エージェントが同じ木へ注入することを禁じている」）
- `code-reviewer` を worktree で起動し、コミット済みの sha を対象にし、変異注入も担い、A〜F は道具が担う（104–110 行目）←ルート `CLAUDE.md:65`
- 3a の check スキルの列挙と「写し」の前提（94 行目）←`ADR-implement-step4-report-slot:15`、`ADR-meta-harness-thinning:33`、`docs/check-skill-skeleton-design.md:119`
- Step 4 の順序（ADR の回収 → 道具の回し直し → 木の片づけ → 撤去。138 行目）←`ADR-pr-gate-verified-sha:14`
- `--also`（100 行目）←`docs/build-commands.md:11`
- 3a からコミットより前に `/race-check` が呼ばれる（94 行目）←`scripts/race-boundaries.mjs:15`

逐語の文としての依存は見つからなかった。「1体だけ」「checkout-index」「内容ハッシュ」「mtime で鑑定」「計画に無い変更は実装中」「Step 3 を終えていない」を `docs/superpowers/`・`workspace/`・implement 自身を除いて `git grep` し、どれも 0 件。

**見出しは変えずに済む。** 経緯の除去は本文の括弧と文の削除に限られ、見出しの語に経緯は含まれていない。ただし `## Step 3 — 検証とレビュー（委譲・最大5サイクル）` の名を持つ参照元は ADR だけ（`…delegated-to-worktree.md:47`。凍結された歴史で照合対象外）。

---

## 4. この変更で偽になる・既に偽である他文書の記述

1. **issue #1314 本文の前提「事件の再話 → その issue が既に持っている」は #934 について偽。** `gh issue view 934` の本文を「checkout-index|全損」で grep して 0 件。145 行目の事件は commit `d9b5444a` の本文と PR #958 の本文にだけある（issue 番号の無い PR）。110 行目の #1154・#1179 は、どの PR・issue 本文にも事件として記録されていない（1 節の 110 行目）。**消す前に、置き場を issue 以外（commit/PR・ADR 追記）に確定させる必要がある。**
2. **既に偽（この変更が作ったものではない）**: `.claude/skills/start-issue/SKILL.md:102` の「`/implement` Step 5 以降」。Step 5 は無く、コミットは 3a にある。正準形でないので governance:check は黙る。
3. **既に偽・ただし射程外**: `docs/adr/` の中の `/implement`「4a. check スキルの実行」（`ADR-implement-step4-report-slot:3,15`、`ADR-check-skill-skeleton:15`、`ADR-race-check-population-tooling:47`）、「4b. code-reviewer エージェント」（`ADR-norm-review-seeding:65`）、「Step 5 — コミット」（`ADR-check-skill-skeleton:48`）。凍結された歴史なので直さない（`ADR-adr-frozen-history`）。
4. **この変更で偽になりうるもの**（2 節の文を消した場合）: C を消す → `ADR-implement-step4-report-slot:15` が受容の前提を失う。110 行目の注入禁止を消す → `start-issue/SKILL.md:82` と `RETROSPECTIVE.md:27`。138 行目の順序を消す → `ADR-pr-gate-verified-sha:14`。100 行目の `--also` を消す → `docs/build-commands.md:11`。77 行目を AGENTS.md の行へのポインタに縮める場合は、`.claude/skills/health-check/SKILL.md:23` と、射程の主張が食い違わないか見る。
5. 経緯のポインタとして `ADR-…` を新しく本文に書くなら、G-adr-citations が実在を照合する（`scripts/governance/checks/G-adr-citations.mjs`）。`/race-check` 末尾の形（「改修するときに読む——検査で従うのは本文である」）に倣い、引用先は `ADR-implement-verification-delegated-to-worktree`・`ADR-implement-verification-tool-tree`・`ADR-implement-step4-report-slot`・`ADR-pr-gate-verified-sha`・`ADR-module-index-reverse-scope` の 5 つが候補になる。

---

## 5. 検収の推奨

1. **識別子の集合差**（`.claude/rules/governance-docs.md`「見出しを移動・圧縮・分割したら…」）:
   ```
   git show HEAD:.claude/skills/implement/SKILL.md | grep -oE '`[^`]+`|#[0-9]+|ADR-[a-z0-9-]+' | sort -u > before
   grep -oE '`[^`]+`|#[0-9]+|ADR-[a-z0-9-]+' .claude/skills/implement/SKILL.md | sort -u > after
   comm -3 before after
   ```
   HEAD で 118 行を返すことを実測した。落ちた語は 1 件ずつ、1 節の表の「いま在る場所」と突き合わせて裁く。
2. **正準形 `` `X`「Y」 `` の集合差は Git Bash の `grep -o` で取らない。** `grep -oP` / `-oE` で `「…」` を抜くと多バイトが壊れ（`�`）、12 件中 2 件しか出なかった（実測）。Grep ツール（`-o`）か `rg -o` か Node で取る。HEAD の外向き参照は 12 件（Grep ツールで実測。行 40, 67×2, 78, 84, 94, 98, 111, 127, 149, 176, 179）。
3. **落とした issue 番号・語が他所に在るか**は `npm run prose:sweep -- <落とした語…>` で切り詰めずに全件出す（`scripts/prose-sweep.mjs`）。特に #934・#1154・#1179・#821・#745・#922 を見る。
4. **2 節の A〜K を検収表にし、新版で 1 件ずつ逐語か同義で残っていることを示す。** 識別子の集合差は散文の条件（例: 「例外は無い」「しかも緑で返る」「git 自身が勧めるが従わない」）を見ないので、これが本体になる。
5. `npm run governance:check`（カテゴリ F）。見出しを変えない限り G-heading-refs の件数は不変のはず。前後の件数を記録する。
6. 規則を落としていないことの独立裏取りは、`/plan-review`「Step 2b」相当を旧版を SSOT として当てる（issue の「未確定」が挙げるとおり）。
7. 前提: スキルはセーフティネットなので、変更はルート `CLAUDE.md`「最重要ルール」2 の合意の対象である。

---

## 6. ⚠ 確信のない所見

1. **110 行目の #1154・#1179**: 事件の記録がどこにも見つからなかった。調べたのは PR #1302 の本文とコメント、issue #1298、PR #1156/#1184 の本文、`git log --grep`。issue #1154・#1179 本体のコメントは見ていない。そこにあれば ADR への追記は要らない。
2. **126 行目の利点 2 つ**（検算できる・⚠ を追える）: 再開を選ぶ理由で、ADR には無い（commit `d9b5444a` と PR #958 本文だけにある）。「再開は起動ではない」だけ残して利点を消すと、新規起動へ戻す改修を止める理由が本文から消える。規則の理由（残す）か、却下案の理由（ADR へ）かは判断が分かれる。
3. **94 行目の「計画に無い変更は実装中に必ず生じる」**: 全称である。歴史資料 `docs/superpowers/specs/2026-07-29-plan-review-instrument-split-design.md:44,150` は書き換え対象として挙げていたが、まだ残っている。経緯の除去と同時に直すと issue の範囲を越える。
4. **77 行目を `AGENTS.md` の表の行へ縮める案**: 表の行は #562 と G-module-linkage（`mod` 宣言）を持つが、`MODULE_INDEX_CRATES` の射程の但し書きは持たない。縮めると但し書きの置き場がこの SKILL と `health-check/SKILL.md:23` の 2 か所から 1 か所に減る。射程の主張をどこに残すかは要判断。
5. 34 行目の fail-closed の理由（非対称）と 121 行目の理由は「設計の理由」として残す側に置いた。ADR が持っていれば消せる。34 行目の理由を持つ ADR は見つけていない（`ADR-plan-ownership-boundary` は削除済み）。
6. 18 行目の「Opus 5.5 も同じ向き」が今も真かは測っていない。

---

付記: 基準値の抽出のため、scratchpad（リポジトリ外）に `ids-before.txt` を 1 つ書いた。リポジトリ内で書いたのはこのファイルだけ。
