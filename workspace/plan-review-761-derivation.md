# 対象 issue: #761 — 独立導出（`workspace/plan.md` / `research.md` 不読）

導出日: 2026-09-02 ／ ブランチ `chore/close-761-plan-doc-lens-adr`（`git log main..HEAD` は空＝未着手の状態から導出）。
⚠ は確信の持てない項目。

## 変更ファイル一覧（導出結果）

| # | ファイル | 操作 | 根拠 |
|---|---|---|---|
| 1 | `docs/adr/ADR-<slug>.md`（新規・slug は自由。候補: `ADR-plan-doc-lens-not-standing`） | **新規作成** | `AGENTS.md`「ドキュメント参照」の ADR 行（否定の知識が生じた決定のみ・連番を振らない）・`.claude/rules/governance-docs.md`「名前はテーマ・目的が決まった時点で…」・`G-adr-file-names.mjs` の `ADR_FILE_NAME` |
| 2 | PR 本文 | `Closes #761` を含める | `/merge-pr` 手順（PR 本文が閉じる issue を決める。ルート `CLAUDE.md`「Git/GitHub 運用」） |
| — | `.claude/skills/plan-review/SKILL.md` | **触らない** | 方針どおり。ただし新 ADR から引く場合は正準形の見出し参照でなくてよい（ADR は G-heading-refs の走査元外・§1） |
| — | `docs/superpowers/specs/2026-07-29-plan-review-instrument-split-design.md`（#761 を 8 行で参照） | **凍結ゆえ触らない** | 歴史資料（§2） |
| — | `RETROSPECTIVE.md` / `AGENTS.md` / ルート `CLAUDE.md` / `docs/development-principles.md` | **影響なし**（#761・レンズ語彙の出現 0） | §2 の grep |
| ⚠ | 生きた層のどこか 1 か所からの短縮引用 `ADR-<slug>` | **任意**（規約上の義務ではない） | §3 |

## シンボル一覧（新 ADR が引くべき識別子）

| 種別 | 識別子 | 所在（今日の正本） | 引き方 |
|---|---|---|---|
| ファイル名/見出し | `ADR-<slug>.md` / 1 行目 `# ADR-<slug>: <題>` | `scripts/governance/checks/G-adr-file-names.mjs:38,55` | stem = 見出しの引用文字列（`[:：]` のどちらでも可） |
| 既存 ADR | `ADR-risk-tiered-plan-review` | `docs/adr/ADR-risk-tiered-plan-review.md`（#849 で導入・4998405） | 「常時 fan-out を撤去した決定」。**issue が前提とした標準構成（スカウト 3 + 独立導出 1）はもう無い** |
| 既存 ADR | `ADR-window-coordinator-split-rule` | `docs/adr/ADR-window-coordinator-split-rule.md`（旧 `0008-…`・#815 で改名） | MECE レンズの所見「5 原理の線」が規則 R（例外ゼロ）へ着地した先。issue 本文の旧パス `docs/adr/0008-window-coordinator-split-rule.md` は**現存しない** |
| 既存 ADR | `ADR-plan-ledger-population-persistence` | `docs/adr/ADR-plan-ledger-population-persistence.md` | #754 系の「台帳＋ファイル出力」の後日談（#831/#834）。⚠ 引くのは任意 |
| 既存 ADR | `ADR-adr-frozen-history` | `docs/adr/ADR-adr-frozen-history.md` | 新 ADR 自身が凍結される契約の根拠。本文で触れるなら |
| issue | #749 / #759（段 1 実装 PR・5ef346f）/ #762（サイクル終了処理・90f67c2）/ #754（配送硬化・ebdd93e）/ #713（fan-out 費用の実測）/ #849（リスク連動化 PR）/ #914（Step 1 の 6・7 項追加・833a9f1）/ #846（計器分割 spec・5e7288a） | `git log --oneline --grep` で実在確認済み | 経緯 |
| skill 節 | `/plan-review`「Step 1 — plan.md の読み込み」項目 6・7、「リスク判定」、「Step 2b — 独立導出による網羅性レビュー」 | `.claude/skills/plan-review/SKILL.md` | 4 型の受け皿として（§4）。**issue が引く旧見出し「Step 2b — 独立導出 + 差分（常に実施・盲点クラスの漏れ検出）」は現存しない** |
| 規範節 | `AGENTS.md`「検証の作法（全タスク共通）」の全称表現・数え上げ・代理条件の条項 | `AGENTS.md` | 「全称が実装より強い」「数値の腐り」の受け皿 |
| 規範節 | `AGENTS.md`「条件別チェック（トリガー → 参照先）」の「関数・型を新規定義／改名／導入」行（1 タスクに束ねる／`dead_code`） | `AGENTS.md` | 「Phase 単独でビルド不通」の受け皿 |
| 歴史資料 | `docs/superpowers/specs/2026-07-29-plan-review-instrument-split-design.md` §5（内部矛盾の機械化・#761 依存の spike 計画） | 凍結 | 引くなら散文で（バッククォート付きパスは可。G-references の走査元外なので着地照合されない） |

---

## 1. 新 ADR が満たすべき規約

導出コマンド: `cat .claude/rules/governance-docs.md docs/adr/ADR-adr-frozen-history.md scripts/governance/checks/G-adr-*.mjs`、`grep -c "^日付:" / "^status:" docs/adr/*.md`

| 規約 | 内容 | 正本 |
|---|---|---|
| ファイル名 | `docs/adr/ADR-<slug>.md`、slug は `[a-z][a-z0-9]*(-[a-z0-9]+)*`。連番禁止（#812） | `G-adr-file-names.mjs:38` `ADR_FILE_NAME`・`governance-docs.md`「名前はテーマ・目的が決まった時点で…」 |
| 1 行目 | `# ADR-<slug>: <題>`（`:` か `：`）。stem と一致しないと赤 | `G-adr-file-names.mjs:55-61` |
| 中身 | **否定の知識（なぜ B を却下したか）が生じた決定のみ**書く。採った形は生きた層（skill・コード）が持ち、ADR には置かない | `AGENTS.md`「ドキュメント参照」ADR 行・`ADR-plan-ledger-population-persistence.md` 冒頭の書き方（「採った形はコードと SKILL.md が持つ」） |
| 書式 | 定めない（凍結された歴史ゆえ）。実測: 81 本中 `日付:` 行 7 本・`status:` 行 26 本・どちらも無いもの 21 本。「文脈／決定／却下 N／受容する残余／帰結」が多数派だが規則ではない | `AGENTS.md`「ドキュメント参照」・`docs/adr/` 実測 |
| 引用の形 | 他 ADR は短縮引用 `` `ADR-<slug>` ``（ADR→ADR は G-adr-citations が実在照合する。`adrCitationDocs` が `docs/adr/` を明示的に足す）。**実在しない slug を書くと赤** | `G-adr-citations.mjs:32-41,45` |
| 書いてはいけない 1 | 序数（見出し番号・ファイル連番・検査 ID 以外の数値識別子）で他を指す | `governance-docs.md` 第 1 bullet |
| 書いてはいけない 2 | 機構の実装の詳細（件数・述語の列挙）の写し。今日の `/plan-review` の Step 数や項目数を書くと squash 後に偽になりうる | `governance-docs.md` 第 2 bullet |
| 書いてはいけない 3 | 既に消滅した節名を正準形で書く（issue が引く旧「Step 2b — 独立導出 + 差分…」・旧パス `0008-…` は散文化するか今日の名で引く） | `governance-docs.md`「既に消滅した節の名前を正準形で書かない」 |
| 検査の射程 | ADR 本文は G-references / G-spec-sections / G-heading-refs / G-near-heading-refs の**走査元外**。正準形の見出し参照を書いても照合されず、腐るに任せる契約 | `ADR-adr-frozen-history.md`「決定」・`lib.mjs:563,713` |
| PostToolUse | `docs/adr/**` に検査は割り当てられない。編集時の沈黙は「何も走らなかった」 | `governance-docs.md` 末尾 bullet |
| rules 配送 | `docs/adr/**` を触ると `governance-docs.md` が自動配送される（`paths` に `docs/adr/**`） | `.claude/rules/governance-docs.md` frontmatter |

## 2. #761 を参照している散文の数え上げ

導出コマンド:
```
git grep -n "761" -- . ':!Cargo.lock' ':!package-lock.json' ':!workspace'
git grep -n -E "文書としての plan|4 レンズ|MECE|節間の覆い|内部矛盾|実行可能性|常設" -- . ':!workspace' ':!Cargo.lock' ':!package-lock.json'
grep -n "レンズ\|761\|MECE\|文書としての" RETROSPECTIVE.md   # 0 件
```

| 所在 | 内容 | 振り分け |
|---|---|---|
| `docs/superpowers/specs/2026-07-29-plan-review-instrument-split-design.md:161,182,184,186,192,213,214,269` | #761 を「`plan.md` の機械可読性」として依存先に置く spike 計画。§5 末尾の順序 3 は「#761 は spike が仮説を支持した場合のみ」 | **凍結ゆえ触らない**。`docs/superpowers/` は歴史資料（#589 で非規範化）——`lib.mjs:544,602,705` がそう扱い、G-adr-citations も母集団外（`G-adr-citations.mjs` ヘッダ「受容する残余」）。生きた層からこの spec への参照は 0 件（`git grep "2026-07-29-plan-review-instrument-split" ':!docs/superpowers'` 空）。**#761 を「常設しない」で閉じると、この spec の順序 3 が宙吊りになるが、それは歴史資料として許容される**。新 ADR がこの spec を散文で名指しして「この依存は解消ではなく打ち切り」と書けば、読者は導かれない |
| `scripts/lib/SnotraWindowColors.Tests.ps1:116` | ヒストグラム値 `761` | 影響なし（数値の偶然一致） |
| `docs/adr/ADR-window-coordinator-split-rule.md:9` | 「計画に対する MECE レビューで…5 つの異なる原理」 | 凍結。影響なし（レンズを常設するか否かに依存しない事実） |
| `docs/superpowers/plans/2026-07-24-su6-config-glue.md:7` / `specs/2026-07-21-su1…:6,18` / `specs/2026-07-24-su6…:5` / `specs/2026-07-24-su7-*.md:5` | 別サイクルの「4 レンズ」「第 4 レンズ」レビュー履歴 | 影響なし（#761 の 4 レンズとは別物） |
| `docs/adr/ADR-check-skill-skeleton.md:43` 等の「実行可能性」・`ADR-folder-location-display-surface.md` の「常設」・`docs/build-commands.md:73` | 語は同じだが概念が別（忠実な読者の実行可能性／目視項目の常設） | 影響なし |
| `RETROSPECTIVE.md` / `AGENTS.md` / ルート `CLAUDE.md` / `docs/development-principles.md` | #762（90f67c2）が RETROSPECTIVE へ書いた「文書としての計画を読むレンズ」の教訓は、その後の上書きで**既に消えている**（現在 0 件）。`development-principles.md:66-68` に残るのは #749 の「所在の散文は移設で偽になる」だけで、レンズの常設可否には触れない | 影響なし。**#761 を閉じても偽になる規範文は無い** |

## 3. 新 ADR を生きた層から引く必要があるか

導出コマンド: `gh pr view 895 --json body`、`sed -n 30,60p scripts/governance/checks/G-adr-citations.mjs`、`git grep -n "ADR-risk-tiered-plan-review"`

- **規約上の義務は無い。** `G-adr-citations` は「引用 → 実在」の向きしか照合せず、被参照ゼロの ADR は赤にならない（`G-adr-file-names.mjs` ヘッダが「誰も引用しなければ静かに通る」と明記）。
- **ただし被参照ゼロは削除候補になる前例がある。** PR #895（2026-08-03・69fc7f0）は「どこからも参照されない 6 本」を全拡張子 grep で検算して削除した。将来の同種の畳み込みで、新 ADR は同じ検算に掛かる。
- **同型の先例**: `ADR-risk-tiered-plan-review` は生きた層からの引用が **0 件**（引くのは凍結 spec `2026-07-28-plan-review-loop-design.md:3` の Superseded 注記だけ）。スキルを触らない前提で、`/plan-review` 系の否定の知識は現状「被参照ゼロで存続」している。新 ADR も同じ扱いになりうる。
- **引くとしたら規約上正しい候補**（`adrCitationDocs` の母集団内で、かつ「その面の役割」に合う場所）:
  1. ⚠ `AGENTS.md`「条件別チェック」の「サブエージェントへ委譲する・worktree を使う」行、またはルート `CLAUDE.md`「サブエージェント委譲と worktree」——「計画を文書として読むレンズは常設しない。要るときは臨時に回す（`ADR-<slug>`）」の 1 句。**ただし規範文書はセーフティネットに含まれ（ルート `CLAUDE.md`「最重要ルール」2）、合意が要る**。本 PR の「スキル・rules・hook を触らない」方針と同じ理由で、規範文書も触らないのが整合的
  2. `docs/development-principles.md`「デバッグ・バグ修正」または「規範を書くときの作法」——`governanceDocs` に含まれ G-adr-citations の母集団（`lib.mjs:563`）。#762 が同 doc へ #749 の教訓を置いた前例あり。**規範ではなく原則の層**なので、セーフティネット合意の対象外 ⚠（`AGENTS.md`「条件別チェック」のセーフティネット行は「規範＝ルート CLAUDE.md / AGENTS.md 等」と書き、development-principles は名指ししていない）
  3. `RETROSPECTIVE.md`——`/retrospective` の上書き対象で、引用は次サイクルで消える。恒久の引用先には向かない
- **推奨**: 引かないか、引くなら候補 2 の 1 文（「計画文書そのものの内部整合は臨時レンズで見る。常設しない判断は `ADR-<slug>`」）。候補 1 は本 PR の方針と衝突する。

## 4. 経緯として引くべき既存の ADR / issue / PR / コミット

導出コマンド: `git log --oneline --grep='#749\|#754\|#759\|#713\|#761'`、`git log --oneline -S'スカウト' -- .claude/skills/plan-review/SKILL.md`、`git show 90f67c2 --stat`、`git show 4998405 --stat`

| 事実 | 所在 | ADR での使い方 |
|---|---|---|
| #749 の計画に 4 レンズを回し 15 件超（成果物 `workspace/plan-review/consistency-*.md`・`mece-*.md`） | 成果物は #762（90f67c2）で**削除**され、内容は git 履歴（`git show 90f67c2^:workspace/plan-review/mece-responsibility.md` 等）にだけ残る | 「レンズのプロンプトと成果物はこのコミットの親にある」と散文で指す。写さない |
| 所見の着地先: MECE → 規則 R | `ADR-window-coordinator-split-rule`「決定」1・「検討した代替案」4（`//!` から z-order を落とした） | 「4 型のうち 1 型は個別 ADR へ着地済み」 |
| 標準構成（スカウト 3 + 独立導出 1）の撤去 | #849（4998405）→ `ADR-risk-tiered-plan-review`。#713 で 4 体 52 万トークンの実測 | **issue の「考えどころ 1（常設で費用倍）」は前提が変わった**——比較対象の常時 fan-out 自体が無くなり、通常リスクは自己レビュー 0 体。「同じ議論になりうる」は fan-out 撤去の決定で先に決着している |
| 「Step 2b をトリガ列挙 0% で常時実施へ倒した前例」 | #495/#502（133abd5）。**その後 #849 で高リスク限定へ戻った**（SKILL.md「リスク判定」「Step 2 または Step 2b のどちらか一方」） | issue の前提 1 が今日は偽であることを ADR に書く（凍結後に読む読者のため） |
| 台帳＋ファイル出力の硬化 | #754（ebdd93e）→ #827/#834 で `scripts/plan-review-ledger.mjs` へ外出し → **#849 で SKILL.md から台帳の参照が消えた**（`grep ledger .claude/skills/*/SKILL.md` 0 件。`package.json` の `plan:ledger` と `ADR-plan-ledger-population-persistence` だけが残る） | issue の考えどころ 3「常設するなら同じ配送規約に載せる」——載せる先の配送規約は今日はファイル出力（Step 2 / 2b の `workspace/plan-review-<slug>.md`）だけ。⚠ 台帳スクリプトが孤児化している事実は #761 の範囲外（別 issue 候補として報告のみ） |
| 4 型の受け皿の現状 | 内部矛盾（数値の腐り）→ `AGENTS.md`「検証の作法」の数え上げ条項（#822 fd5826e で強化）／全称が実装より強い → 同・全称表現条項／実行可能性（Phase 単独ビルド不通）→ `/plan-review` Step 1 項目 6（#914 833a9f1・「1 タスクに束ねる」）／節間の覆い（宣言と実行の分離・散文の腐り）→ Step 1 項目 7（概念ラベル grep・#914）が部分的に／責務分割 MECE → 個別 ADR の規則 R のみ・一般解は無し | 「常設しない」の根拠: 4 型のうち 3 型は Step 1 の自己照合と検証の作法が既に受け、残る MECE は計画ごとの設計判断であって汎用レンズで拾う性質でない（⚠ これは導出者の判定であり、ユーザー裁定の理由と一致するかは未確認） |
| 内部矛盾の機械化を #761 依存で計画した spec | `docs/superpowers/specs/2026-07-29-plan-review-instrument-split-design.md` §5（#846・5e7288a）。spike が実施された痕跡は無い（`git log --grep` に「spike」「呼び出し元 grep」の道具化コミット無し ⚠ 網羅は未確認） | 「機械化の系譜はここで途切れる」と散文で名指し。依存先 #761 が閉じるのは解消でなく打ち切り |
| 観点の定義の保存先（issue 考えどころ 4） | 4 レンズのプロンプト自体は会話にしか無かった。成果物側の定義文は 90f67c2^ の `workspace/plan-review/*.md` 各冒頭（「レンズ: 上から順に実行したとき指示が一意に定まるか」等）に残る | ADR 本文に 4 観点の一行定義を置くのは可（凍結の歴史として）。それが唯一の保存先になる |

## 5. 検証コマンド

- `npm run governance:check`（`docs/build-commands.md` カテゴリ F）。ベースライン実測（本ブランチ・変更前）: **全検査 passed／ADR 81 本／短縮引用 457 件**。新 ADR 追加後は「ADR 82 本」になり、新 ADR 内の `ADR-<slug>` 引用が全件実在すること、1 行目が stem と一致することが赤/緑で分かる。
- `git grep -n "ADR-<新 slug>"`——引いた場合の着地確認（引かない場合は 0 件で正常）。
- PR 作成前: `gh pr view <PR> --json closingIssuesReferences` に #761 だけが載ること（`/merge-pr` 手順・ルート `CLAUDE.md`「Git/GitHub 運用」）。今日 #761 に紐づく PR は 0 件（`closedByPullRequestsReferences` 実測）。
- ⚠ PostToolUse は `docs/adr/**` に検査を割り当てないため、編集直後の沈黙を合格と読まない。

## 補足（範囲外だが報告）

- `scripts/plan-review-ledger.mjs` と `npm run plan:ledger` はどのスキルからも呼ばれていない（#849 以降）。#761 とは独立の残骸候補。
- issue 本文の 2 つの参照（旧パス `docs/adr/0008-…`・旧見出し「Step 2b — 独立導出 + 差分…」）は今日どちらも実在しない。ADR に写すときは今日の名で引くか散文化する。
