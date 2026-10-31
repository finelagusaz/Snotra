# review-1330 (code-reviewer)
align sha: db59c1290b1636fd9b5e27d5f5aa06c9bfdb5228 / base a400ffc3

## Medium
- M1 中身の消えた参照: .claude/rules/src-tauri.md:22「一般則は AGENTS.md「事前調査」」— 生成/破棄・真偽のペアで計画する一般則は 事前調査 から 対称ペアの行(AGENTS.md:49)へ移った。見出しは着地するので governance は緑。derive 2-B が予告済みだが未修正。直し: 参照先を AGENTS.md「条件別チェック（トリガー → 参照先）」の対称ペアの行へ(rules=セーフティネット、合意要)。
- M2 偽の引用(元から着地しない引用〔旧「気づく契機が残るなら…」は base の AGENTS にも無い〕を、向きの逆な引用へ差し替えた): rust-toolchain.toml:10-11 が「壊れたとき緑のまま推移しないなら機構を置かない」を引くが、(a)逐語でない (b)固定で更新が見えなくなる=緑のまま推移する側であり、引いた原則は機構を置く方向を指す。根拠の向きが逆。直し: 引用を外し「気づく契機(/deps-update)が残るので機構は足さない」と素で書く。
- L0(旧 M3・Low 寄り) 部分的に失われた規則〔governance-docs.md「書く約束」(1) と写しの行が隣接規則を持つが、SSOT 変更時の向きは無い〕: 旧 AGENTS 検証の作法「SSOT を変えるときは写しを grep で数え上げてから直す」が新版・正本のどこにも無い(ADR-stale-identifier-detector-scope:41 が逐語引用のみ)。直し: 照合 bullet へ 1 句戻す。

## Low / ⚠
- L1 scripts/manual-smoke.ps1:24 は「届かない検証は実施の有無すら区別できない」を AGENTS 検証の作法に帰すが、その文は旧 CLAUDE 委譲節(#725)にあり今回消えた(帰属の誤りは既存)。
- L2 スキル表前文: 「この表に載せるのは user 起動専用だけ(残りは roster が注入、写すと二重課税)」という編集者向け規則が記述文に変わった。4 本一致は確認済み。
- L3 実行にバイアス削除で「コミット・PR 作成の指示は確認なしに即実行」が消え、SP の「外向き操作は先に確認」だけが残る。依頼があれば確認済みと読めるので実害は小(classify #8 で裁定済み)。
- L4 implement/SKILL.md:84 が委譲節を「読む時刻は制御できない」の根拠に引くが、理由文(読む版が分岐)が消えた。行為は残る。
- L5 足場の行から「workspace/ に置けば squash で main 履歴に残る」が消えた。「リポジトリの外」で覆うが、workspace/ をリポジトリ外と誤解する余地。
- L6 関数・型の行: compile-fail を移行漏れ検出器にする対象から「改名」が落ちた(旧 API 削除のみ)。

## 計器の検算
- AGENTS.md の「## RETROSPECTIVE.md の運用」→「## RETROSPECTIVE」、「4. 事前調査…」→「4. 調査する」で governance:check exit 1、3 件(snotra-core.md:25 / src-tauri.md:22 / retrospective SKILL:103)。巻き戻し後 hash-object f8b127a8… = sha の blob 一致、status clean。死角: 見出しを残して中身を移す形(M1)は緑のまま通る。

## 確認して真だったもの
- disable-model-invocation:true は deps-update/health-check/merge-pr/retrospective の 4 本で表と一致。⚠「skill roster に載らない」はサブエージェント内から観測できず未検証。
- /race-check 行の述語は diff 前後で不変、rules 2 本の SSOT 宣言と整合。
- dev-principles:93 の新しい文は真(AGENTS.md:22-23)。state-check:30 の新参照は着地し中身もある。start-issue・retrospective・hooks.md・window_coordinator.rs の変更は真(/persistence-check Step 5 が history 3 者・config 移行を持つ)。
- governance:check はアンカー sha で exit 0。

## 独立導出(識別子の集合差)
- 旧 2 枚から消えたバッククォート語を comm で取り、各語を正本へ当てた。正本も機構も無いのは L0/L2/L5/L6 のみ。旧 CLAUDE の「不在検知は意図的に置かない」は docs/build-commands.md:168 が持つ(b)。gh pr create ゲート・code-reviewer・mod 宣言は裁定。-D warnings/dead_code は plan-review SKILL:25、history キーは persistence-check Step 5。
