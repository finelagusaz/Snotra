# plan — issue #728 項目 1: `WALK_EXCLUDE_PREFIXES` → `WALK_EXCLUDE_PATHS`

## 目的

`scripts/governance-check.mjs` の除外リストの**名前と直上コメントが、実装（ルート相対パスの完全一致）を誤って説明している**状態を解消する。挙動は正しいので、変更は識別子と散文のみ。

**このサイクルのスコープは #728 の項目 1 だけである。** 項目 2 は後続 PR（下記「後続サイクルへ送る作業」）、項目 3 は WONTFIX（下記「却下の記録」）。

## 受け入れ条件

1. `scripts/governance-check.mjs` に `WALK_EXCLUDE_PREFIXES` が 1 件も残らない（リポジトリ全体では歴史資料 `docs/superpowers/plans/2026-07-26-skill-workflow-boundary.md` の 3 件のみ残る＝意図どおり）。
2. 定数の直上コメントが「プレフィックス」ではなく「ルート相対パスの完全一致」を述べる。
3. `npm run governance:check` の evidence 行の**全件数が、改名前のベースラインと一致する**（純粋な改名の陽性証明。ずれれば `walk` の退行）。
4. `node --test scripts/governance-check.test.mjs` が通る。
5. PR A の本文が #728 を **closing keyword なしで**参照する（#728 は項目 2 の PR で閉じる）。

## 変更ファイル一覧と対象シンボル

| ファイル | 対象 | 変更 |
|---|---|---|
| `scripts/governance-check.mjs:37` | `WALK_EXCLUDE_PREFIXES`（定義） | → `WALK_EXCLUDE_PATHS` |
| `scripts/governance-check.mjs:47` | `walk` 内の唯一の参照点 | → `WALK_EXCLUDE_PATHS.includes(rel)` |
| `scripts/governance-check.mjs:31-35` | 定数直上のブロックコメント | 「ルート相対プレフィックス」「プレフィックス」→ 完全一致を述べる語へ。`.superpowers` の #722 由来の説明はそのまま残す |

**触らない**: `scripts/governance-check.test.mjs`（旧名を参照しない・grep 0 件）、`docs/superpowers/`（非規範化された歴史資料）。

## 実装順序

1. コメントブロック（`:31-35`）を先に直す — 何を言いたいかを確定させてから名前を選ぶ順序にする。
2. 定義（`:37`）と参照点（`:47`）を改名する。
3. 検証（下記）。

## 不変条件と異常系

- **不変条件**: `makeSnapshot` が返す `files` の集合が変わらない。**検知手段**は evidence 行の件数一致（受け入れ条件 3）— `walk` が 1 ディレクトリでも余分に降りれば「対象文書」「見出し参照」「散文の識別子」の件数が動く。
- **異常系**: 参照点の書き換え漏れは `walk` 内の `ReferenceError` になる。`walk` は毎回必ず実行されるため**自己検知する**（コンパイラ代わりの検出器を別途置く必要はない）。
- **フォールトインジェクションは行わない**: `.claude/rules/safety-nets.md` は種蒔きの射程を「**判定を足す変更**」と定める。純粋な改名は判定を足さない（索引の追随と同類）。代わりに置く測定が受け入れ条件 3 である。

## テスト方針と検証コマンド

```
node --test scripts/governance-check.test.mjs
npm run governance:check
```

**ベースライン（main `b28d2b9`・2026-08-04 実測・改名前）**:

```
検査 18 件 / 対象文書 35 件 / rules 7 件 / skills 13 件 / 恒久規範 常時ロード 12794/15500 字・rules 9879/12000 字 / 見出し参照 114 件を 48 文書から照合 / workspace member 4 件の lints opt-in / 散文の識別子 67 件を 34 文書から照合 / 近傍の見出し参照 13 件 / ADR 29 本の名前 / ADR の短縮引用 169 件
```

`workspace/` は除外リスト自身の要素なので、本サイクルで追加した `workspace/*.md` はこの件数に影響しない（実測: ベースラインは `workspace/` 作成前に取得したが、`walk` はディレクトリ `workspace` で降りるのをやめるため比較可能）。

## SPEC.md・関連文書の更新要否

- `SPEC.md`: **不要**（アプリの挙動に触れない）。
- ガバナンス文書: **不要**。`WALK_EXCLUDE_PREFIXES` を散文で引く規範文書は存在しない（grep 実測。ヒットは `governance-check.mjs` 自身と歴史資料のみ）。
- ただし `npm run governance:check` は変更後に必ず走らせる（AGENTS.md 条件別チェック「ガバナンス文書を変更」— 検査そのものを触るため同格に扱う）。

## 作業項目

### Phase 1 — 改名と検証（PR A）

- [ ] `scripts/governance-check.mjs:31-35` のコメントを「ルート相対パスの完全一致」を述べる形へ直す
- [ ] `scripts/governance-check.mjs:37` の定義を `WALK_EXCLUDE_PATHS` へ改名する
- [ ] `scripts/governance-check.mjs:47` の参照点を改名する
- [ ] `Grep` で `WALK_EXCLUDE_PREFIXES` を全文検索し、残存が `docs/superpowers/plans/2026-07-26-skill-workflow-boundary.md` の 3 件だけであることを確認する
- [ ] `node --test scripts/governance-check.test.mjs` が通る
- [ ] `npm run governance:check` の evidence 行を上のベースラインと**逐語比較**し、全件数の一致を確認する（不一致なら `walk` の退行として調査する）
- [x] #728 へ「項目 3 の WONTFIX 裁定」と「項目 2 の修正範囲の判断」をコメントする（2026-08-04 投稿済み: https://github.com/finelagusaz/Snotra/issues/728#issuecomment-5176434379 ）
- [ ] PR A を作成する。本文は #728 を closing keyword **なしで**参照し、後続作業（項目 2）をチェックリストに載せる

## 後続サイクルへ送る作業（#728 項目 2・PR B）

**この PR の作業項目ではないため `- [ ]` を使わない**（`.claude/hooks/pre-bash.mjs:331` の PR 前ゲートは未チェック項目を PR の未完了と読む・#749）。PR A のマージ後に `/start-issue 728` を再実行し、新しい `plan.md` で扱う。

1. `.claude/skills/implement/SKILL.md:72`「**サインは実装の途中で立つこともある。** その場合もその時点で止める。」へ `（1c-A）` を付す。
2. **1b 表の 52 行目（別タスクの残骸）には 1c ラベルを付けない。** 判別式は「**行き先が行内に書かれていない停止にだけラベルが要る**」である — 52 行目は「下の『計画が無く…』の 2 行のどちらかへ進む」と行き先を明記しており、かつ skill 外へ引き渡さないので 1c（引き渡しの 3 種）に属さない。72 行目は「止める」としか書かず行き先が無い。**この判別式を書き残さないと、後続サイクルは再導出するか 52 行目を誤ってラベル付けする。**
3. PR B が #728 を閉じる（`Closes #728`）。`/merge-pr` の `closingIssuesReferences` 確認はこちらで効く。

**なぜ 2 PR に割るか**: #728 本文が #489 に従い項目 1 の単独実施を指定している。`.claude/skills/implement/SKILL.md` は `governance-check.mjs` の `refDocs`（`:1133`）＝**検査対象**であり、同一 PR に載せると緑が「検査が生きている」証拠なのか「検査が壊れて対象を見なくなった」結果なのか区別できない。

**`workspace/` の扱い**: Phase 1 の全項目が `- [x]` になった時点で `/implement` の規則どおり `workspace/` を削除してステージへ含める（`implement/SKILL.md:123`）。`readPlanState` が `plan.md` を見つけられなくなり `gh pr create` が通る。**後続サイクルのためにバッファを残さない** — 新しい `plan.md` を書き下ろすのが文書化された流儀である。

## #728 へのコメント草案（ユーザー承認後に投稿）

```markdown
## 状況確認（2026-08-04）— 項目 3 を WONTFIX、スコープを 2 項目へ

### 項目 1・2 は現存し、有効

- 項目 1: `scripts/governance-check.mjs:37` に `WALK_EXCLUDE_PREFIXES` が定義され `:47` で `.includes(rel)` 照合。同クラスの #819 / #825 が CLOSED になったため、**「識別子・主張が実装を誤って説明する」腐りとして残る最後の 1 件**。
- 項目 2: `.claude/skills/implement/SKILL.md:72` の無ラベル停止行は現存。

### 項目 3 は WONTFIX

`docs/superpowers/` は #589（close: 2026-07-19・**本 issue 起票より前**）で非規範化された。`scripts/governance-check.mjs` は同ディレクトリを `docs/adr/` と同じ除外クラスへ揃えており（`:1104` `:1113` `:1133` `:1166` `:1266`）、`.claude/rules/governance-docs.md` はそのクラスの規範を「凍結された歴史であり腐るに任せる」と定める。直す動機（一貫性のみ）より放置の動機（凍結歴史）が勝つと判断した。

なお参照先の見出し（`/start-issue`「5b. …」「Step 6 — …」）は両方とも現存しており、腐ってはいない。

### 項目 1 の改名先は `WALK_EXCLUDE_ROOTS` ではなく `WALK_EXCLUDE_PATHS` とする

本文の「トップ 1 段の完全一致」は不正確で、`.claude/worktrees` は 2 段である（`walk` は `.claude` で降りてから `.claude/worktrees` で弾く）。ゆえに `ROOTS` も同じ誤りを引き継ぐ。兄弟の `WALK_EXCLUDE_NAMES`（任意の深さの**名前**照合）と対称な `WALK_EXCLUDE_PATHS`（ルート相対**パス**の完全一致）を採る。**直上コメント（`:31-35`）の「ルート相対プレフィックス」「プレフィックス」も同じ改修範囲に含める** — 識別子だけ直すと当の誤説明が残る。

### 項目 2 で直すのは 72 行目だけ（1b 表の 52 行目には付けない）

判別式は「**行き先が行内に書かれていない停止にだけラベルが要る**」。52 行目（別タスクの残骸）は「下の『計画が無く…』の 2 行のどちらかへ進む」と行き先を明記し、かつ skill 外へ引き渡さないので 1c（引き渡しの 3 種）に属さない。72 行目は「止める」としか書かず行き先が無い。

### PR は 2 本に割れる

本 issue が #489 に従い項目 1 の単独実施を指定しているため。`.claude/skills/implement/SKILL.md` は `governance-check.mjs` の検査対象（`refDocs`・`:1133`）なので、同一 PR では緑が「検査が生きている」証拠なのか「検査が壊れて対象を見なくなった」結果なのか区別できない。**項目 1 の PR は本 issue を閉じず、項目 2 の PR が閉じる。**
```

## 却下の記録

- **項目 3 の正準化**: 却下（上記）。否定の知識としては `.claude/rules/governance-docs.md` の既存規範の適用にすぎず、新しい ADR は起こさない。
- **1 PR に束ねて evidence 行の差分で担保する案**: 却下。差分測定は退行を捕まえうるが、#728 本文が #489 に従う単独実施を指定しており、issue が仕様である。

## 未確定（実装前に潰す）

- [x] 項目 3 を今回のスコープに含めるか — **WONTFIX**（2026-08-04・ユーザー選択。根拠は `research.md`）
- [x] 改名先の名前 — **`WALK_EXCLUDE_PATHS`**（2026-08-04・ユーザー選択。`ROOTS` を却下した理由は `.claude/worktrees` が 2 段であること）
- [x] 項目 1 の範囲にコメントブロックを含めるか — **含める**（`:31-35` が散文で「プレフィックス」と書いており、識別子だけ直すと誤説明が残る）
- [x] 項目 2 で 1b 表 52 行目も直すか — **直さない**（判別式を上記に記録）
- [x] #728 の closing keyword をどちらの PR に載せるか — **PR B（項目 2）**。PR A は参照のみ
- [x] 改名の陽性検知をどう置くか — **evidence 行の件数の逐語一致**。ベースラインを main で実測済み（上記）
- [x] `workspace/plan.md` の未チェック項目が PR A を阻む問題 — **項目 2 を `- [ ]` にしない**ことで解消。`/implement` が `workspace/` を削除する規則（`implement/SKILL.md:123`）に乗せる

## セルフレビュー

- リスク: **通常**（挙動不変の改名 1 件・変更ファイル 1 件・参照点 1 件）
- plan-review: **未実施**。`/plan-review`「リスク判定」の高リスク条件（永続形式・並行性・状態遷移・網羅性・ガバナンス文書の移動/圧縮/分割）にいずれも該当しない
- エージェント数: **0**
- 主エージェントの自己照合（5a の 5 項目）:
  1. **issue の全要件に作業項目が対応する** — 項目 1 は Phase 1 が全部を覆う。項目 2 は後続サイクルへ明示的に送り、項目 3 は却下理由を記録した（黙って落としていない）
  2. **境界条件と検証** — 唯一の境界は「`walk` が降りる/降りない」の判定。検証は evidence 行の件数一致（対象文書 35 / 見出し参照 114 / 散文の識別子 67 / 近傍 13）で、どれか 1 つでも動けば退行として検知される
  3. **新しい状態・リソース・プロセス** — **無い**（定数の改名のみ。生成/破棄の対はそもそも発生しない）
  4. **より単純な既存パターンで置き換えられないか** — 置き換えではなく改名そのものが最小手。除外リストを 1 本へ統合する案は、`NAMES`（任意の深さ）と `PATHS`（ルート相対）が**別概念**なので取らない
  5. **不変条件の検知手段** — 上記 2 と「参照漏れは `walk` 内の `ReferenceError` で自己検知」
- 条件別チェックの振り分け:
  - 「セーフティネットを変更」→ `.claude/rules/safety-nets.md` 参照済み。**種蒔きは対象外**（射程は「判定を足す変更」）
  - 「関数・型を新規定義／改名」→ **呼び出し元を grep 済み**（参照点 1 件）。`/dry-check` は不実施（重複ロジックの検出が目的であり、定数の改名は論理を足さない）
  - `/norm-review`・`/symmetric-check`・`/state-check`・`/race-check`・`/persistence-check`: いずれもトリガーに該当しない
- 要対処: **0 件**
- 未検証: **なし**（ベースラインは実測済み。CI の実測は PR 作成後に行う）

## 人間レビュー

- [x] 承認済み — 2026-08-04 / 問い: "**計画の承認** — `workspace/plan.md` へ注釈を書き足していただくか、明示的に承認してください。 / **#728 へのコメント投稿の可否** — 作業項目に「#728 へコメント」が入っています（草案は `plan.md` 末尾に全文あり）。…承認をいただいてから `gh issue comment` を実行します。" / 回答: "承認。issue へのコメントも投稿していい"
