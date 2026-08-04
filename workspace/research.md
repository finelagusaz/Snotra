# research — issue #728 項目 1（`WALK_EXCLUDE_PREFIXES` の改名）

## issue の要約

#728 は #725 のレビューで deferred にした「記述の不正確さ」3 件の束。ユーザーの依頼は「状況が変わったので今も有効か確認から」であり、**まず有効性の検証を行った**。

| 項目 | 内容 | 有効性の判定（2026-08-04 実測） |
|---|---|---|
| 1 | `scripts/governance-check.mjs` の `WALK_EXCLUDE_PREFIXES` が実際は完全一致で、名前が実装を誤って説明する | **有効。現存**（`:37` 定義・`:47` で `.includes(rel)`）。同クラスの #819 / #825 は CLOSED ゆえ**最後の生き残り** |
| 2 | `.claude/skills/implement/SKILL.md` の停止指示 1 行だけ 1c ラベルが無い | **有効。現存**（`:72`）。**本サイクルの対象外**（下記「PR 分割」） |
| 3 | `docs/superpowers/specs/2026-07-26-skill-workflow-boundary-design.md` の順序参照が散文形 | **WONTFIX 裁定**（2026-08-04・ユーザー承認済み。理由は下記） |

### 項目 3 を WONTFIX とした根拠

- `docs/superpowers/` は #589（close: 2026-07-19）で非規範化された。**issue 起票（2026-07-26）より前**なので「状況が変わった」のではなく、起票者が既に知って本文に書いていた事実である。
- 変わったのは根拠の重み。`scripts/governance-check.mjs` は `docs/superpowers/` を `docs/adr/` と**同じ除外クラス**へ揃えている（`:1104` `:1113` `:1133` `:1166` `:1266`）。
- `.claude/rules/governance-docs.md` はそのクラスの規範を「凍結された歴史であり腐るに任せる」と定める（`ADR-adr-frozen-history`）。規範文は ADR を名指しし `docs/superpowers/` を名指ししていないため決め手ではないが、**直す動機（一貫性のみ）より放置の動機（凍結歴史）が勝つ**。
- なお参照先の見出し（`/start-issue`「5b. セルフレビュー（plan-review 固有の補完）」・「Step 6 — workspace をコミット & プッシュ」）は**両方とも現存**する。腐ってはおらず、形だけの問題である。

## 関連ファイル・シンボル（grep で実在確認済み）

| パス | 対象 | 現況 |
|---|---|---|
| `scripts/governance-check.mjs:36` | `WALK_EXCLUDE_NAMES` | `Set([".git","node_modules","target","dist"])`。**任意の深さ**で名前照合 |
| `scripts/governance-check.mjs:37` | `WALK_EXCLUDE_PREFIXES` | `["workspace",".claude/worktrees",".superpowers"]`。**改名対象** |
| `scripts/governance-check.mjs:31-35` | 定数直上のブロックコメント | 散文で「ルート相対プレフィックス」「プレフィックス」と書く。**同じ改修範囲** |
| `scripts/governance-check.mjs:47` | `walk` 内の唯一の参照点 | `!WALK_EXCLUDE_PREFIXES.includes(rel)` |
| `scripts/governance-check.test.mjs` | — | `WALK_EXCLUDE_PREFIXES` を**参照しない**（grep 0 件）。テスト側の変更は不要 |
| `docs/superpowers/plans/2026-07-26-skill-workflow-boundary.md:27,79,83` | 旧名の写し 3 件 | **非規範化された歴史資料ゆえ触らない**（項目 3 と同じ理由） |

`WALK_EXCLUDE_PREFIXES` の出現箇所はリポジトリ全体で上記のみ（`Grep` 全文検索・切り詰めなし）。

## 実装の事実（名前が誤っている機序）

`walk` はディレクトリのときだけ再帰の可否を判定し、`rel` は**ルート相対パス**である。

```js
const rel = path.relative(root, path.join(dir, e.name)).replaceAll("\\", "/");
if (e.isDirectory()) {
  if (!WALK_EXCLUDE_NAMES.has(e.name) && !WALK_EXCLUDE_PREFIXES.includes(rel)) walk(...);
}
```

- 照合は `.includes(rel)` ＝ **完全一致**。接頭辞判定ではない。
- ただし一致した時点で降りないため、配下すべてが落ちる。**外から見た結果は接頭辞判定と一致する**（挙動は正しい／`docs/.superpowers` も `.superpowers-extra` も巻き込まない）。

### issue 本文の側の不正確さ（本サイクルで訂正する）

issue は「トップ 1 段の完全一致」と書くが、`.claude/worktrees` は **2 段**である（`.claude` で降りてから `.claude/worktrees` で弾く）。ゆえに issue の改名候補 `WALK_EXCLUDE_ROOTS` の「ROOTS」も同じ誤りを引き継ぐ。

**採用名は `WALK_EXCLUDE_PATHS`**（2026-08-04・ユーザー承認済み）。兄弟の `WALK_EXCLUDE_NAMES`（任意の深さの**名前**照合）と対称になり、「ルート相対**パス**の完全一致」という実装をそのまま説明する。

## 再利用できる既存パターン

- **数値の差分をもって「純粋な改名」を証明する**: `governance-check.mjs:1572` の evidence 行が `検査 N 件 / 対象文書 N 件 / … / 見出し参照 N 件を M 文書から照合 / …` を出力する。`walk` が退行すれば**対象文書**の件数が動くため、改名前後で全件数が一致することが陽性の検知になる（`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」が求める測定の代替として、変更の性質に見合う形）。
- **ベースラインの実測値は `plan.md`「テスト方針と検証コマンド」を正本とする**（`workspace/` のコミット後に取り直し、追加前の値と逐語一致することを確認済み）。
- `workspace/` は `WALK_EXCLUDE_PREFIXES` 自身の要素なので、本サイクルで追加する `workspace/*.md` は件数に影響しない（`walk` はディレクトリ `workspace` で降りるのをやめる。実測で確認）。
- **テストランナーの罠**: `scripts/governance-check.test.mjs` は vitest 製で、`node --test` で起動すると runner 未初期化の `TypeError: Cannot read properties of undefined (reading 'config')` で落ちる（改名と無関係な赤・2026-08-04 実測）。`npx vitest run <path>` を使う。

## 技術的制約

- **#489（検査対象を変更しながら検査を走らせない）により、項目 1 は単独 PR である**（issue 本文の明示指定）。項目 2 は `.claude/skills/implement/SKILL.md` を触り、このファイルは `governance-check.mjs` の `refDocs`（`:1133`）に含まれる＝**検査対象**。同一 PR に載せると、緑が「検査が生きている」証拠なのか「検査が壊れて対象を見なくなった」結果なのか区別できない。
- **PreToolUse hook は `workspace/plan.md` の未チェック `- [ ]` が 1 件でも残ると `gh pr create` を拒む**（`.claude/hooks/pre-bash.mjs:331`・#749）。`/implement` は全項目 `- [x]` を確認してから `workspace/` を削除しステージへ含める（`implement/SKILL.md:123`）。ゆえに **`workspace/` の寿命は 1 PR**であり、項目 2 の作業項目を `- [ ]` として本 plan に置くと PR A が作れない。項目 2 は「後続サイクルへ送る作業」として散文で記録する。
- `.claude/rules/safety-nets.md` の種蒔き（フォールトインジェクション）は「**判定を足す変更**」に仕事があると定める。純粋な改名は判定を足さないため対象外。

## 未解決の疑問

なし（下の計画で潰した項目は `plan.md`「未確定」欄を参照）。
