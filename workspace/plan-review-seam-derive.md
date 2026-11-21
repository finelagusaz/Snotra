# plan-review Step 2b 独立導出 — issue #1493

対象: GitHub issue #1493「chore(skills): 工程の継ぎ目に契約を置き、計画の承認をリスクに応じて変える」
導出の基点: ブランチ `chore/1493-seam-contracts`（HEAD 182714bc 時点の作業ツリー）
計画の閲覧: `workspace/plan.md`・`workspace/research.md` は読んでいない。grep/Glob の path にリポジトリルートを渡していない。走査は基本的に `.claude/skills/`・`.claude/hooks/`・`.claude/rules/`・`.claude/agents/`・`.github/`・`scripts/`・`.githooks/`・`docs/`・`AGENTS.md`・`CLAUDE.md`・`SPEC.md` に限定した（`git grep` は pathspec で限定）。**例外として 1 回だけ、Grep を `.claude/` 全体（glob `!worktrees/**`）に掛けた**（指定より広い）。ヒットは `.claude/skills/` の中だけで、`workspace/` の中身は出ていない。計画の内容は目にしていない。
その他の出所: `gh issue view 1493`、`gh pr view` で最近の PR 6 本（#1480・#1485・#1487・#1490・#1491・#1492）の本文の見出しを読んだ（テンプレートの節の裏付け）。
読んだ rules: `.claude/rules/safety-nets.md`・`.claude/rules/governance-docs.md`（Read で）。

---

## 1. 変更ごとのファイルとシンボル

### 変更 1 — `/implement` に PR 作成の工程を足す

ファイル: `.claude/skills/implement/SKILL.md`

| シンボル | いま言っていること | 変更後に偽になる／要る変更 |
|---|---|---|
| frontmatter `description`（3 行目） | 「実装からコミット作成まで自律的に行う」 | 責務が「PR 提出まで」になるので偽。issue が名指し |
| frontmatter `allowed-tools`（5–15 行目） | `Bash(cargo *)`・`Bash(npm *)`・`Bash(git *)`・Read/Edit/Write/… で、**`Bash(gh *)` が無い** | ⚠️ `Bash(gh *)` を足すのが整合的（`/start-issue`・`/deps-update`・`/merge-pr` は持つ。変更 3 の `gh issue view` にも使う）。ただし既存の「1c-A」は `Bash(gh *)` が無いまま `gh issue create` を実行すると書いており、`allowed-tools` が実行を止める hard gate かどうかは未確認。`.claude/settings.json` の権限も走査範囲外で未確認。push は `Bash(git *)` で足りる。本文ファイルは `Write` で足りる |
| 本文冒頭（18 行目）「自律的にフルサイクル開発を行う」 | — | 偽にはならない。変えなくてよい |
| 「Step 4 — 仕上げ」冒頭（141 行目）「ここに残るのは、`workspace/` を畳む前の回収と、ツールの回し直しと、worktree の片づけと、その撤去コミットだけである」 | Step 4 で終わる前提 | PR 工程を Step 4 に足すなら「だけである」が偽。新 Step（例「Step 5 — PR 提出」）を起こすなら、Step 4 の文は真のまま（どちらを選ぶかは §4） |
| 「Step 4 — 仕上げ」1 項目め（143 行目）「PR がまだ無いなら、PR 本文へ転記するまで撤去コミットのメッセージに書いておく」 | PR を書くのは別の誰か、という前提 | PR を同じスキルが作るので、「新 Step で PR 本文の『理由（採らなかった案）』節へ書く」に直すのが自然。現文は偽ではないが、預け先が同じスキル内に来たので簡単にできる |
| 新設: PR 提出の Step | — | `git push -u origin HEAD` と `gh pr create --body-file <path>`。本文は `.github/pull_request_template.md` の節に従う、の 1 行で指す。機械的な条件（空 PR・`workspace/plan.md` の未チェック・ツールの要約の sha）は `.claude/hooks/pre-bash.mjs` が持つ、と参照だけにする（141 行目が既に「PR 前ゲート（`.claude/hooks/pre-bash.mjs`）は…」と参照している。写さない）。本文は Write でファイルに書く（ルート `CLAUDE.md`「フック」）。`git push … && gh pr create …` の鎖にリダイレクトを挟まない（pre-bash の `hasSafeChain`） |
| 「出力」節（154–187 行目） | 報告テンプレートに PR の欄が無い。「到達しなかった Step の欄には『未到達』」 | PR の URL と本文の節（テンプレートとの一致）の欄を足す。到達しなかった場合「未到達」になる。`### コミット` の後に `### PR` など |
| 「出力」節 156 行目「検証の母集団は…レビューの母集団は…」 | — | 変えなくてよい |

### 変更 2 — `.github/pull_request_template.md` を本文契約の正本に

ファイル: `.github/pull_request_template.md`（現在 6 行: `## 対応Issue` / `- Closes #<issue_number>` / `## 変更内容`）

- 全面書き換え。節（issue の列挙と、最近の PR #1480・#1485・#1487・#1490・#1491・#1492 の実際の見出しで裏付け）:
  - 冒頭の合意行 `**マージは合意してから**（…）`。条件は `.claude/rules/safety-nets.md` のフロントマターの `paths` と規範文書（ルート `CLAUDE.md`・`AGENTS.md`）を**参照**し、写さない
  - 対応 Issue（閉じるなら closing keyword、閉じないなら非 closing の参照、無ければ「なし」。#1485 は「なし」だった）。`/merge-pr` の `--closes` と一致させる
  - 概要 / 変更 / 理由（採らなかった案）/ 検証（`verify:tree` の sha とカテゴリ）/ 残したもの
- 参照する側: `.claude/skills/implement/SKILL.md`（新 PR Step）と `.claude/skills/deps-update/SKILL.md`「Step 4 — コミット & PR」68 行目「PR 本文に更新一覧を載せる。major bump は ⚠ で明示する」を「本文はテンプレートの節に従う」で指す（更新一覧・⚠ は「変更」節の中身として残す）

### 変更 3 — `/implement`「1b. 分岐」に「検討:」「保留」の行

ファイル: `.claude/skills/implement/SKILL.md`

| シンボル | 要る変更 |
|---|---|
| 「1b. 分岐（行き先を決めるのはこの表だけである）」の表（38–45 行目） | 「この表だけ」と宣言しているので、行は**表の中**に足す（表の外に書くと宣言と矛盾）。動きは「`/start-issue`「Step 1 — ISSUE 読み込み & 要求の曖昧さ判定」の判定へ渡して止まる」。判定本文（`.claude/skills/start-issue/SKILL.md` 28 行目）は写さない |
| 「1c. 止まったときの引き渡し」（63–69 行目）「停止は 3 種あり」 | 新しい停止が 1c-A/B/C のどれにも当たらない（A は起票打診、B は計画あり、C は由来不明）。新しい種（1c-D など）を足すなら「3 種」が偽になる。既存へ寄せるなら説明が要る（§4） |
| 「出力」節 162 行目 `- 停止: … <1c-A/B/C の引き渡し先 …>` | 停止の種を足すなら列挙を直す |

参照先（変えない）: `.claude/skills/start-issue/SKILL.md`「Step 1 — ISSUE 読み込み & 要求の曖昧さ判定」28 行目。

### 変更 4 — 計画の承認をリスクに応じて変える

#### `.claude/skills/start-issue/SKILL.md`

| シンボル | いま言っていること | 変更後に偽になる／要る変更 |
|---|---|---|
| frontmatter `description`（3 行目）「人間の承認までを行い、実装はしない」 | 承認は必ず人間 | 自己承認の経路で偽。「承認（人間または自己承認）まで」へ |
| 本文冒頭（16 行目）「明示的な人間の承認を得るまで、実装へ渡さない」 | 全計画に人間承認 | 偽。3 基準のどれかが立つときだけ、に限定 |
| 「Step 4 — 実装計画」の必須 2 節のコードブロック（90–98 行目）`## 人間レビュー` / `- [ ] 承認待ち` | 承認欄の見出しと初期値 | 自己承認も同じ節に置くなら、見出し名が中身と合わなくなる（§4）。「人の裁定が残る」の印の付け方（未確定欄の項目への印）を、この節の `## 未確定（実装前に潰す）` の書式の近くに書く |
| 100 行目「未確定事項を解消したら `- [x]` にする」 | — | 印のついた項目を解消しても印は残す、と書かないと、1a が「人の裁定が残った計画か」を後から判定できない（⚠️ §4） |
| 「Step 5 — 計画検証と人間レビュー」見出し（106 行目） | — | 見出しが偽になりかける（自己承認がある）。改名するなら旧名で `git grep`（現状、この見出しを名指す参照は見当たらない。5a・5b・5c は名指されている） |
| 「5c. 人間の注釈と承認」（146–165 行目） | 全計画で停止してユーザーに依頼 | ここに 3 基準の判定を置く（正本をここにするのが自然）。どれかが立てば従来どおり停止、どれも立たなければ `- [x] 自己承認 — <日付> / 根拠: <3 基準それぞれの判定>` を記録。基準は写さず参照: 仕様変更 = `AGENTS.md`「開発ワークフロー」の 1、戻せない変更 = `AGENTS.md`「条件別チェック（トリガー → 参照先）」の永続形式の行（`/persistence-check` のトリガー）、人の裁定 = 未確定欄の印。セーフティネットの計画も同じ基準、マージ前合意はルート `CLAUDE.md` 最重要ルール 2 で別に残る、の 1 文。165 行目「明示的な承認が無ければ、Step 6 へ進まない」は偽になるので直す。見出し「人間の注釈と承認」も中身とずれる（改名するなら `.claude/skills/implement/SKILL.md` 68 行目が名指している） |
| 5c 153 行目（注釈を反映 → 高リスクのみ `/plan-review` 再実行） | 人間の注釈の経路 | 真のまま（自己承認の経路では注釈が無い） |
| 5c 163 行目「高リスクなら、承認を記録したあとに、5b の『試行』の行を…#1300 へコメントする」 | 承認 = 人間承認 | 自己承認の記録後にもコメントするかを決める（「承認を記録したあと」の語を自己承認にも及ぼすなら変えなくて済む。§4 ⚠️） |
| 「Step 6 — workspace をコミット & プッシュ」169 行目「人間レビューが承認済みであることを確かめてから」 | — | 偽。「承認（人間または自己）が記録されている」へ |
| 「出力」184 行目 `6. 人間レビューの承認記録` / 185 行目 `7. 次のアクション: /implement で実装できること` | — | 6 を「承認の記録（人間か自己承認か・3 基準の判定）」へ。自己承認で `/implement` へ「進む」なら 7 も変わる（§4） |
| 「出力」187 行目 引き渡し契約「人間レビューが明示的に承認済みである」 | `/implement` 1b の「3 つすべて立つ」行がこの契約を参照している | 偽。「承認済み（3 基準のどれかが立つなら逐語の人間承認、どれも立たないなら自己承認）」へ |
| 「5b. セルフレビュー」記録ブロック（132–142 行目） | リスク・plan-review の方式 | 変えなくてよい。ただし `- リスク: 通常 / 高` と承認要否が別物になったことを、ここか 5c に 1 句置くと読み違いを防げる（変更 5 と対） |

#### `.claude/skills/implement/SKILL.md`

| シンボル | いま言っていること | 変更後に偽になる／要る変更 |
|---|---|---|
| 「Step 1 — 入口判定」24 行目「未確定がゼロで、人間が承認済みの計画がある」 | — | 偽。「承認済み（人間または自己承認）」へ |
| 「1a」28 行目「3 つを確かめる」・32 行目「人間承認（機械判定）」 | 承認行は `- [x] 承認済み — <日付> / 問い: "…" / 回答: "…"` の 1 形だけ | `- [x] 自己承認 — <日付> / 根拠: <3 基準それぞれの判定>` を第 2 の形として受ける。ただし受け入れ条件「どれかが立つ計画は、逐語の人間承認が無ければ通らない」を満たすには、自己承認行が在るだけでは足りない。根拠欄の 3 判定がすべて「立たない」であることを機械的に読めるよう書式を固定し、さらに計画本文の独立した手がかり（未確定欄の印・`SPEC.md` 更新要否の欄）と照合する（§4 ⚠️）。「エージェント自身の判断…を承認の代わりにしない」（32 行目末尾）は自己承認と字面で衝突するので書き直す |
| 「1b」表 40 行目・41 行目「人間承認」 | — | 「承認」へ |
| 「1c」68 行目 1c-B「人間承認が無いなら、`/start-issue`「5c. 人間の注釈と承認」へ戻す。`/implement` が…承認を自分で記録したりしてはならない」 | — | 「承認が無いなら 5c へ」。`/implement` が自己承認を記録しない（自己承認を書くのは `/start-issue` 5c だけ）は残す。5c を改名したらここの見出し名も直す |
| 「出力」161 行目 `- 入口: 計画あり — <同一性/未確定ゼロ/人間承認の根拠>` | — | 「承認の根拠（人間／自己承認と 3 判定）」へ |

### 変更 5 — `/plan-review`「リスク判定」と人間承認の切り離し

ファイル: `.claude/skills/plan-review/SKILL.md`

| シンボル | 要る変更 |
|---|---|
| 「リスク判定」41 行目の段落 | 「高リスクは独立レビューを厚くする理由であり、人間承認の要否は `/start-issue`「5c …」の 3 基準が決める」を 1 文で足す（基準を写さない） |
| 「Step 3 — 結果の統合と報告」96 行目「それ以外は、主エージェントが照合し直した後、人間の承認へ渡す」 | 偽になる（自己承認の経路がある）。「承認の判定（`/start-issue` 5c）へ渡す」へ |
| 「Step 3」出力ブロック 93 行目 `- 実装着手: 可 / 要修正 / 人間の裁定待ち` | 真のまま（「人間の裁定待ち」は変更 4 の「人の裁定が残る」と同じ語で、むしろ整合）。変えなくてよい |
| frontmatter `description` | 承認に触れていない。変えなくてよい |

---

## 2. issue 本文が名指していない波及先

1. **`.claude/skills/implement/SKILL.md` の `allowed-tools`** — `Bash(gh *)` が無い（`gh pr create`・`gh issue view` に要る）。
2. **`.claude/skills/implement/SKILL.md`「1c」の「停止は 3 種」と「出力」の `1c-A/B/C`** — 変更 3 の停止を新しい種にするなら両方を直す。
3. **`.claude/skills/implement/SKILL.md`「Step 4 — 仕上げ」141 行目「…だけである」と 143 行目の PR 本文への預け方** — PR 工程を足すと文の前提が変わる。
4. **`.claude/skills/implement/SKILL.md`「出力」** — PR の欄（URL・テンプレート節との一致）。
5. **`.claude/skills/start-issue/SKILL.md`** の承認語彙の写し全部: `description`（3）・冒頭（16）・Step 4 コードブロック（95–97）・Step 5 見出し（106）・5c 見出しと本文（146–165）・Step 6（169）・「出力」6/7（184–185）・引き渡し契約（187）。
6. **見出しの相互参照**: `/implement` 68 行目が `/start-issue`「5c. 人間の注釈と承認」、40 行目が `/start-issue`「出力」を名指す。`/plan-review` 41 行目と `/start-issue` 144 行目が「5b. セルフレビュー（plan-review 固有の補完）」を名指す。5c・Step 5・`## 人間レビュー` を改名するなら、旧名で `git grep` して同じ変更で直す（G-references は見出しを照合しない＝`.claude/rules/governance-docs.md`）。
7. **`.claude/skills/plan-review/SKILL.md`「Step 3」96 行目**「人間の承認へ渡す」。
8. **`.claude/skills/deps-update/SKILL.md`「Step 4 — コミット & PR」68 行目** — テンプレートを指す（issue は名指しているが、現行の「更新一覧・⚠」の扱いをどの節に入れるかは本文に無い）。
9. **`.claude/skills/merge-pr/SKILL.md` 18 行目**「PR テンプレートが `Closes` を埋めるので、書いた覚えが無くても残る」 — 新テンプレートが対応 Issue 節に closing keyword の雛形を残すなら真のまま。雛形から closing keyword を外す（閉じる／閉じないを選ばせる）なら偽になる（⚠️ §4）。
10. **テンプレートが `governance:check` の母集団外** — `governanceDocs`（`scripts/governance/lib.mjs`）は `.github/` を含まない。テンプレートに書く `.claude/rules/safety-nets.md` などのパス参照は G-references が照合しない。受け入れ条件「governance:check が通る」はテンプレートの中身について何も言わない（⚠️ 母集団へ足すかは §4。足すなら `scripts/` の変更＝セーフティネット）。
11. **テンプレート内の案内文と closing keyword** — PR 本文の closing は `closingIssuesReferences`（`scripts/merge-pr.mjs` 189 行目付近）で照合される。案内文（HTML コメントを含む）に `Closes #…` の例を書くと、GitHub が閉じる宣言として拾うかもしれない（⚠️ HTML コメント内の keyword を GitHub が無視するかは未確認）。案内文に closing keyword を書かない。
12. **`.claude/rules/safety-nets.md` 47 行目**「CI での実測は、計画ではなく PR 本文のチェックリストに書く」 — 変えなくてよいが、テンプレートの「検証」節がこの置き場になる（節の説明で受ける）。
13. **テンプレート「検証」節が受ける既存の預け先** — `docs/build-commands.md` 105・108 行目（手動 smoke の記録を `-PostToPr` か貼り付けで PR に残す／「PR 本文の目視表」）と、`/implement`「3b. 委譲へ渡すもの」119 行目の `-PostToPr`。`-PostToPr` は PR が在って初めて使える。PR 工程を Step 4 の後に置くと、3b で目視判定を人間へ委ねる時点では PR がまだ無い——順序（PR を先に作るか、記録を後から PR へ貼るか）を新 Step で決める必要がある。
14. **`/plan-review` が未確定欄へ足す経路** — 「Step 1 — plan.md の読み込み」末尾「調査や裁定が要るなら、未確定欄へ追加する」と「Step 3」出力の「未確定欄への追加」。「人の裁定が残る」の印の付け方をここにも届けないと、plan-review が足した裁定項目は印なしになり、自己承認をすり抜ける。`.claude/skills/plan-review/SKILL.md` は変更 5 以外にもこの 2 か所が触る候補。
15. **見出し参照（§2-6 の追加）** — `docs/check-skill-skeleton-design.md` 119 行目が `/implement`「3a. 委譲の前に」、167 行目が `/implement`「Step 4 — 仕上げ」を名指す。Step を挿入・改名する案（例: PR 工程を Step 4 の前に入れて番号が動く）では参照側に入る。新 Step を末尾に足すなら影響なし。
16. **素の「承認」での再測** — `.claude/skills/` 全体に `承認` 単独で Grep した。ヒットは `implement`・`start-issue`・`plan-review`（96 行目）と `retrospective` 79 行目だけ。後者は issue 起票の「はい」の話で、計画承認とは無関係（変えない）。

## 3. 変えなくてよいと判断したファイル

- **`.claude/hooks/pre-bash.mjs`（とテスト）** — PR 前ゲートは `workspace/plan.md` の未チェック数（`countUnchecked`・`readPlanState`）・ツールの要約（`judgeSummary`・`verifiedSummaryState`）・upstream/未 push（`readGitState`）だけを見る。`## 人間レビュー` の節も承認行の形も読まないので、自己承認の追加でも、PR 工程の追加でも判定は変わらない。issue も「スキルは参照だけ」と言っている。注意点として、`/implement` の新 Step で `git push … && gh pr create …` を書くなら `hasSafeChain` の形（リダイレクトを挟まない）と、`cd` を `gh pr create` の前に置かないこと（`CWD_CHANGE` で block）に従う（後者は本調査中に grep のコマンド文字列だけで実際に block された）。
- **`scripts/merge-pr.mjs`・`scripts/verify-tree.mjs`・`.githooks/*`・`.github/workflows/*`** — テンプレート・承認記録・`検討:` のいずれも参照していない（grep で 0 件）。
- **ルート `CLAUDE.md`** — 最重要ルール 2（マージ前の合意・PR 本文冒頭の合意行）は変更後も真。テンプレートは合意行の条件を「参照」する（写さない）ので、正本は CLAUDE.md 側のまま。変えると規範文書の変更（合意要）になるので避けるのがよい。
- **`AGENTS.md`** — 「開発ワークフロー」の 1（バグ/仕様変更の判定）と「条件別チェック」の永続形式の行は、3 基準の参照先として使うだけで文言は変わらない。承認について書いていない。
- **`.claude/skills/merge-pr/SKILL.md`** — 18 行目以外は不変（§2-9 の条件付き）。スコープ外（マージの自動化）にも当たらない。
- **`.claude/skills/start-issue/SKILL.md`「Step 1」28 行目** — `/implement` から参照される側。判定本文は不変。
- **`.claude/skills/start-issue/SKILL.md` 104 行目**（コミット以降を作業項目に置かない）— PR 作成を `/implement` が行うようになっても文は真。
- **`.claude/agents/code-reviewer.md`・`.claude/rules/*`（safety-nets 以外も）・`docs/*`（`docs/glossary.md`・`docs/development-principles.md`・`docs/build-commands.md`）** — 承認・PR テンプレート・`/implement` の責務範囲を述べる行が無い（grep で確認）。`docs/superpowers/`・`docs/design/`・`docs/adr/` は凍結された記録なので直さない。
- **`SPEC.md`** — 製品の仕様で、開発プロセスは書いていない。
- **`.claude/skills/health-check`・`retrospective`・check 系スキル** — `/implement` の責務範囲や承認に依存する記述なし。

## 4. 設計上の判断が要る点

1. **PR 工程の置き場所** — Step 4 の末尾に足すか、新 Step（「Step 5 — PR 提出」）を起こすか。Step 4 冒頭の「…だけである」と「この順序で行う」（ツールの最後の実行のあと、撤去コミットのあと）の制約から、新 Step のほうが既存文を偽にしない。
2. ⚠️ **自己承認の機械判定の強さ** — 受け入れ条件「どれかが立つ計画は、逐語の人間承認が無ければ通らない」を 1a がどこまで機械で保証するか。(a) 自己承認行の存在と根拠欄の 3 判定がすべて「立たない」の形式だけを見る（書いた本人の判定を信じる）か、(b) 計画本文の独立した手がかり（未確定欄の印の有無・`SPEC.md` 更新要否・`/persistence-check` を check 欄に挙げているか）と照合するか。(a) だけだと、立っているのに「立たない」と書いた計画が通る。(b) は計画の書式（`SPEC.md` 更新要否の欄の書き方）を固定する必要がある。
3. ⚠️ **「人の裁定が残る」の印の寿命** — 印を付けた未確定項目は、裁定を得て `- [x]` になっても印を残すのか。残さないと、1a は後から「人の裁定が残った計画」を判別できず、自己承認が通ってしまう。また、裁定を得た後でも人間承認を要するのか（裁定＝承認とみなすか）も決める。
4. **承認欄の見出し** — `## 人間レビュー` のまま自己承認行も置くか、`## 承認` などへ改名するか。改名なら `/implement` 1a（32 行目）・`/start-issue` Step 4 コードブロック・5c コードブロックを同時に直す。hook は見出しを読まないので影響なし。git 履歴の既存 `plan.md` との互換は不要（`workspace/` は撤去される）。
5. ⚠️ **自己承認のとき `/start-issue` が `/implement` へ「進む」の意味** — `/start-issue` が同じターンで `Skill` から `/implement` を呼ぶのか、Step 6 で止まって次のアクションとして `/implement` を示すのか。前者なら `/start-issue` の `description`「実装はしない」・「出力」7 も変わる。後者なら「人に問わずに」は満たすが人の起動は 1 回残る。
6. **「検討:」「保留」の行の位置づけ** — 計画が在って承認済みでも止めるのか（`/start-issue` を経ていれば Step 1 で判定済み）、計画が無い経路にだけ効かせるのか。受け入れ条件は「計画や実装に入る前に止まって問う」で、承認済み計画のある経路を明示していない。また `/implement` は `$ARGUMENTS` が散文のことが多く、issue のタイトルを知るには `gh issue view` が要る（issue 番号が無ければ判定できない）。
7. **新しい停止の種別** — 1c-D を足すか、1c-C（問う）へ寄せるか。
8. ⚠️ **テンプレートの対応 Issue 節の雛形** — `Closes #<N>` を残すか、閉じる／閉じない／なしを選ばせる案内にするか。後者は `/merge-pr` 18 行目を偽にする。案内文に closing keyword を書くと GitHub が拾う懸念（未確認）。
9. ⚠️ **テンプレートを `governance:check` の母集団へ入れるか** — 入れないと、テンプレートに書いたパス参照は誰も照合しない。入れるなら `scripts/governance/lib.mjs` の `governanceDocs` を変える（セーフティネット・スコープの拡大）。issue は求めていない。
10. **「残したもの」節と `/implement` Step 4 の否定の知識** — 143 行目の「ADR か PR 本文か」の PR 本文側をテンプレートの「理由（採らなかった案）」へ、やらないと決めた作業項目（146 行目の「issue へ記録」）を「残したもの」へ、と対応づけるか。
11. ⚠️ **#1300 の試行コメント（`/start-issue` 5c 163 行目）** — 自己承認でも高リスクなら記録後にコメントするか。「承認を記録したあと」をそのまま自己承認にも及ぼせば文言は変えずに済むが、読み手が「人間承認」と読む可能性がある。
12. **`/deps-update` は `disable-model-invocation: true`（ユーザー起動専用）** — ルート `CLAUDE.md` の表に載る。テンプレート参照を足しても表の行は変わらない。
13. ⚠️ **`/plan-review` Step 2 の発動条件と基準 3 の関係** — `/plan-review`「リスク判定」は「issue に書かれていない人間の裁定が計画の前提にある」とき Step 2 を使う。この「既に下された裁定」を「人の裁定が残る」に数えるか。数えないなら、基準 3 は「未解決の裁定」だけを指すと明記しないと両者が読み違えられる。
14. ⚠️ **テンプレートを正本にしたときの扱い** — (a) `AGENTS.md`「ドキュメント参照」へ「PR 本文の契約: `.github/pull_request_template.md`」の索引行を足すか（規範文書の変更＝合意要）。(b) `.claude/rules/safety-nets.md` の `paths` に入れるか（契約の正本だが、現状 `.github/workflows/**` だけが入っている）。どちらも issue は求めていない。
15. **「`/persistence-check` のトリガー」の指し先** — `AGENTS.md`「条件別チェック」表の行（「永続形式・識別子/キー形式を変更」）か、`.claude/skills/persistence-check/SKILL.md` の `description`（「シリアライズ・on-disk 形式…の追加・変更時」）か。文言が違い、issue の「永続形式・設定キー」はどちらとも完全には一致しない。1 つを選んで参照する。
16. **この変更自体がセーフティネットの変更**（`.claude/skills/**`、`.github/pull_request_template.md` は `paths` の外だが `.claude/skills/**` が当たる）— PR 本文冒頭に「マージは合意してから」。計画の承認は新基準を自分に当てると「人の裁定が残る」が立ちうる（承認の方式という好みの判断を含む）。
