# plan-review — ガバナンス文書レイヤー

## 問題なし
- G11 の `headingRefDocs` は `docs/superpowers/` を除外している（`scripts/governance-check.mjs:803`: `!f.startsWith("docs/superpowers/")`）。削除対象「シェル環境（Windows / PowerShell）」を正準形で指すのは `docs/superpowers/plans/2026-07-24-su6.5-flip-hardening.md:20` のみで母集団外ゆえ、research.md の主張どおり G11 は落ちない。
- 削除する 2 bullet は「Git/GitHub 運用」の見出し配下の一部であり、見出し自体は削除されない。CONTRIBUTING.md:16・`docs/adr/0002-squash-merge-issue-autoclose.md:27,33` など見出しを正準形で指す既存参照は影響を受けない（実在確認済み）。
- `AREA_BUDGET`（`scripts/governance-check.mjs:542` `ALWAYS_LOADED_FILES = ["CLAUDE.md", "AGENTS.md"]`）と rules 予算（`.claude/rules/*.md` のみ）はどちらも `docs/hooks.md` を数えない。Phase 4 で `docs/hooks.md` へ知識を追記しても G10 の両面 ratchet に一切乗らない——「常時ロード面から降ろす」という設計と整合する。
- `docs/development-principles.md`「構造的設計原則と強制の階梯」の一項（68行台）「構造が規則を吸収したら…対応するチェックリストを削除する」は本計画の方向性（表を機構へ吸収し記述を削る）そのものを支持しており、矛盾しない。項目6・7（構造化信号での検出・fail-closed を既定値に埋める）も I2・I3 の記述と一致する。
- 削除される知識の受け皿を項目ごとに追跡した——(a) シェル環境表 3 行の「理由（過去の事故）」列は I6 により REMEDY 文言（事故の理由＋代替手段）へ移す設計、(b) `--no-verify` 行の「main への直接 push は GitHub ruleset が拒む」は削除されない同一節の別 bullet（`CLAUDE.md:22`）に既に残っている、(c) `git pull --ff-only` 行の因果説明（非 FF がマージコミットを作り `pre-merge-commit` が拒否する）は `.githooks/pre-merge-commit:2-3` のコメントに独立に存在する。孤立する知識は見つからなかった。
- `docs/adr/` に索引ファイルは存在しない（`docs/adr/*` を glob 済み・8 件のみでインデックスなし）。新規 ADR 追加が索引更新を要求することはない。
- `AGENTS.md`「条件別チェック」表の既存行「セーフティネット（hook・CI・…）を新設/変更 → `.claude/rules/safety-nets.md`」が `pre-bash.mjs` 改修を既に捕捉しており、新規トリガー行は不要。
- issue #768 の完了条件（`CLAUDE.md` から該当記述を削り `AREA_BUDGET.alwaysLoaded` を引き下げる）と計画の変更範囲は一致し、過不足は見当たらない。「フック」表 PreToolUse 行の一般化は issue に明記はないが、PreToolUse の実責務が拡大する以上必要な整合作業であり scope creep ではない。

## 軽微な懸念
- 「フック」表の PreToolUse 行の**ラベル自体**（現状 `CLAUDE.md:46`「PR 作成前 push チェック（PreToolUse）」）が push チェックに限定した名前になっている。計画（plan.md:91）は「発火条件」セルの記述を一般化・`docs/hooks.md` への指しに変えると書くが、行ラベルの改名は明示していない。5 判定が PreToolUse 全体で走るようになった後もラベルが「PR 作成前 push チェック」のままだと、読者は PreToolUse が push チェック専用だと誤解しうる。
- 計画は D1〜D3 のみを ADR の対象とし、D6・D7 は `docs/hooks.md` の既存「受容する未対応リスク」リストへ列挙するとしている（plan.md:94）。この列挙が既存の書きぶり（「意図的迂回」「人間専用」という肯定的な受容の文脈）を踏襲しない場合、`/norm-review` 停止条件 (c)（「検出されないなら使ってよい」と読めないこと）に抵触しうる。計画自体は Phase 6 で `/norm-review` を回す設計なので機構的には塞がれているが、文言そのものはまだ書かれていないため要注意点として記録する。

## 要対処
- **`docs/adr/0006-*.md` は番号が衝突する。** `docs/adr/0006-plan-ownership-boundary.md`（「ADR-0006: 計画の所有を /start-issue に一本化する」）が既に存在する（確認済み・#749 サイクルの成果物）。plan.md の設計判断 D2 も「3 本目を足すときに再検討する（ADR-0006 に残す）」と書いており、既存 ADR-0006 とは無関係の新規 ADR を同じ番号で作る計画になっている。現在の連番は 0001〜0008 まで埋まっているため、新規 ADR は **0009** を使うべきである。governance-check.mjs に ADR 番号の一意性を検査する機構は無い（grep 済み・該当なし）ため、この衝突は機構では捕捉されず、実装前に手動で直す必要がある。plan.md 内の「0006」表記（設計判断 D2 の注記、変更ファイル一覧、Phase 4 のタスク行）はすべて「0009」に読み替える必要がある。

## 未検証（理由）
- 5 件の REMEDY 文言・`docs/hooks.md` への追記本文・ADR 本文はまだ実装されていない（Phase 1/4 未着手）。I6（拒否は必ず復帰手順を含む）や `/norm-review` 停止条件 (a)(b)(c) との整合は、実際の文言が書かれてから確認するしかなく、計画書の記述レベルでは意図の確認までしかできない。
- `scripts/governance-check.mjs` の `AREA_BUDGET.alwaysLoaded` 引き下げ幅・由来コメントの「既存6件と同じ様式」という主張は、対象ファイルが担当レイヤー外（本レビューの対象ファイル一覧に `scripts/governance-check.mjs` は含まれない）のため検証していない。別レイヤーのレビューが担当する前提で残す。
- G1〜G9（モジュール索引・スキル表・rules glob・SPEC 番号等）への影響は、本計画がそれらの母集団に触れる変更を含まないと判断して深く検証していない（CLAUDE.md のスキル表・モジュール構成節・rules ファイルは計画の変更対象に含まれないため、確認は上記の範囲に留めた）。
