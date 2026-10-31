# 調査 — #1314 `/implement` の本文から経緯を外す

## issue の要約

`.claude/skills/implement/SKILL.md`（180 行・28,084 バイト）は、手順の本文と経緯（事件の再話・却下した案の理由・機構の死角の説明）が同じ段落に織り込まれている。経緯を種別で振り分け、本文を「何をするか」に寄せる。手本は `/race-check`（経緯を末尾 1 行で ADR へ預ける）。

## 関連ファイル・シンボル（実在確認済み）

- 対象: `.claude/skills/implement/SKILL.md`
- 本文が引く ADR（すべて実在）: `ADR-implement-verification-tool-tree`・`ADR-implement-verification-delegated-to-worktree`・`ADR-implement-step4-report-slot`・`ADR-module-index-reverse-scope`・`ADR-doc-minimization-cap-enforcement`
- **見出しを名指しで引く生きた参照**（見出しの文言を保つ必要がある）:
  - `「3b. 委譲へ渡すもの」` ← `.claude/skills/start-issue/SKILL.md`・ルート `CLAUDE.md`
  - `「3a. 委譲の前に」` ← `scripts/race-boundaries.mjs` のコメント
  - `「1a」` / `「出力」` / `「Step 3 — 検証とレビュー（委譲・最大5サイクル）」` ← ADR（凍結層）
- 本文中の issue 参照は 16 種（#934 だけ 2 回）

## 経緯の所在（2026-10-31 実測）

| SKILL の行 | 経緯 | 既に在る所 |
|---|---|---|
| 18 | 報せの較正（Fable 5.1 以降・#1230） | — （`.claude/rules/safety-nets.md` が「どのモデルのどの癖への栓かを条項に添える」を求める。**残す**） |
| 31 | 見出しの存在を見る理由（#821） | 規則の一部として短く残す |
| 34 | fail-closed に倒す非対称の理由 | — （設計の理由。1 文に縮める） |
| 77 | `.rs` 索引の検査の死角（#1008・#701・#1214） | `ADR-module-index-reverse-scope`、`scripts/governance/checks/G-module-index.mjs` |
| 84 | 不変オブジェクトの理由・2026-08-19 の mtime 鑑定 | `ADR-implement-verification-delegated-to-worktree`（mtime・2026-08-19 とも grep で一致） |
| 101・110 | smoke の並走不可・timeout の赤（#1154・#1179） | `scripts/lib/SnotraSmoke.psm1`。#1154・#1179 は ADR に無い |
| 113 | `.delegate/` の理由（#1298 で harness が片づけた） | PR #1302 の本文（main の squash commit） |
| 126 | `SendMessage` 継続の利点（#934 で 2 巡目が ⚠ を決着・7 件） | — |
| 145 | `git rm` を `-f` で押し通さない（2026-08-07・#934 で `checkout-index -f` により全損） | PR #958 の本文（main の squash commit `d9b5444a`）に `checkout-index` が 3 回 |

**issue #1314 本文の「事件の再話 → その issue が既に持っている」は不正確**: #934 の issue 本文・コメントに全損の記述は無く、経緯は PR #958 の本文（squash で main の commit message）にある。#1298 の「片づけ」も issue ではなく PR #1302 にある。振り分けの行き先は「issue」ではなく「main の履歴（PR 番号で引ける）」である。

## 再利用できる既存パターン

- `/race-check` 末尾: 「述語・打ち切り規則の経緯は `ADR-…`… **改修するとき**に読む——検査で従うのは本文である」
- `.claude/rules/governance-docs.md`: 組み直しの前後でバッククォート語・issue 番号・ADR 名の集合差を `comm` で取る手順

## 技術的制約

- 見出しの文言は変えない（上の生きた参照。G-heading-refs が正準形を照合する）
- 本文の規則は 1 つも落とさない。落としてよいのは経緯（なぜ・いつ・何が起きたか）だけ
- `.claude/rules/safety-nets.md`: モデルの癖への較正は、どのモデルのどの癖かを名乗ったまま残す
- このスキル自身が実装に使われるので、変更後の `/implement` で自分を実装することになる（自己適用）。入口判定・Step 3・Step 4 の手順の意味を変えてはならない

## 敵対的調査（3b）の所見と採否

全文は `workspace/adversarial-1314.txt`。

**壊せなかった**:
- 測定環境: governance:check の走査元に SKILL.md が入っている。`git archive` の複製で見出しを改変すると、生きた参照から赤になった
- 行 18 の較正は残す
- 行 84 の経緯は ADR に在る
- 行 101・110・113 の規則部分は残す
- PR #958 / #1302 は MERGED で、本文に該当の記述が在る

| 所見 | 採否 | 反映 |
|---|---|---|
| 生きた参照が表に足りない。`docs/check-skill-skeleton-design.md:119`（「3a. 委譲の前に」）・`:167`（「Step 4 — 仕上げ」）の正準形 2 件と、非正準形の `pre-bash.test.mjs:415`・`verify-tree.mjs:3` | **採る**（手で再測した。主の最初の grep は先頭の `/` がパス変換されて取りこぼしていた） | 見出しは全部そのまま保つ。この方針は変えない |
| 行 77 の括弧は、経緯ではなく「governance:check が捕捉する」の射程を限る条件である | **採る** | 射程の 1 文（「`MODULE_INDEX_CRATES` の crate に限り、同じ節で同名が言及されていれば素通りする」）は残し、issue 番号の列と再話は消す |
| 行 145 の「git が `-f` を勧めても従わない」と、checkout 系 3 コマンドの名指しは規則である（禁止だけでは git の勧告に負ける・PR #958） | **採る** | 規則として残し、2026-08-07・全損・控え無しの再話だけ消して `PR #958` を指す |
| 行 126 の経緯は PR #958 に、行 113 の経緯は `ADR-implement-verification-tool-tree` にも在る | **採る** | 消してよい側の根拠として使う |
| ⚠ 行 34 の非対称は設計の理由 | **採る** | 1 文に縮めて本文に残す |
| ⚠ 行 84 の太字（sha を機械可読で名乗るから、別の木でも報告が別の版へ向かない）は設計の理由 | **採る** | 縮めて残す |
| ⚠ 行 110 末尾の「注入するのはこのエージェントだけ」と、行 127 の太字（`ADR-pr-gate-verified-sha` が引く）は、他文書が散文で依存しており照合の機構が無い | **採る** | 逐語で維持する |
| 付随: `start-issue/SKILL.md:102` が、存在しない「`/implement` Step 5 以降」を指している（#1314 より前からある） | **採る**（範囲の内側。`/implement` を指す偽の写し） | 「コミット・push・PR 作成・マージ」へ書き換え、Step の序数を外す |
| ⚠ issue 参照の数 16 と 17 の差 | 解消 | #934 が 2 回出るので、種類は 16・出現は 17 |

## 未解決の疑問

1. 経緯を消すとき、ADR に無い理由（行 34 の非対称・行 126 の継続の利点）は 1 文の理由として本文に残すか、ADR へ足すか
2. 行 101・110 の #1154・#1179（並走の timeout）は規則の根拠として残すか
3. 縮めた結果の検収: バイト数・行数のどれを目安にするか（`ADR-doc-promise-over-area-ratchet` は面積の合否を置かない）
