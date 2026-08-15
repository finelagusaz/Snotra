# 実装計画 — #1094 facade の再輸出を実際の消費者まで絞る

## 目的

`scripts/governance-check.mjs` の公開面を 56 名から実測した消費者の分（4 名）まで縮め、19 検査モジュールへの静的名指し import を落とす。その結果として、**検査モジュールが隣のテストごと消えたとき、16 本については #1092 の manifest 差分が唯一の検知器になる**——今日は facade の `ERR_MODULE_NOT_FOUND` が先に鳴るため manifest 差分は発火の機会を持たない。

**射程は 2 つの条件で限定される**（3b で実測。詳細は `research.md` の制約 1 / 1b）。

1. **16 / 19 本**——`G-clippy-disallowed` / `G-adr-file-names`（facade の evidence 行）と `G-skill-table`（`instrument.mjs:6` 経由）は絞った後も静的 import が残る。
2. **ペア消失のときだけ**——`G-X.mjs` だけが消えて `G-X.test.mjs` が残る形は、`npm test` が facade と無関係に `ERR_MODULE_NOT_FOUND` で捕まえる（絞る前も後も同じ）。

## 受け入れ条件

1. `scripts/governance-check.mjs` の `export { … }` が実測した消費者の名前だけを持つ。
2. facade が静的 import する検査モジュールは evidence 行が使う 2 本だけになる（`instrument.mjs` 経由の 1 本はこの PR の射程外・残余として明記する）。
3. `scripts/governance-check.test.mjs` の公開面カナリアが、同じ差分で縮んだ一覧へ更新されている（**凍結を外さない**）。
4. `npm test` と `npm run governance:check` が緑。
5. 使い捨てコピーで**ペア消失**を再現したとき、次の 4 点が実測されている。
   - (a) 絞った後: `node scripts/governance-check.mjs` が落ちずに走る
   - (b) 絞った後: `npm test` が緑（他の層も黙っていることの証拠）
   - (c) 絞った後: `governance-manifest.mjs --compare` が exit 1 で当該 ID の `-` 差分を出す
   - (d) **対照**: 絞る前（main）の複製では (a) が `ERR_MODULE_NOT_FOUND` で落ちる
6. 対照として**単独消失**（`.mjs` のみ）で `npm test` が赤くなることが実測されている（制約 1b の上段の裏取り）。
7. この PR が偽にする散文 3 箇所が同じ差分で現在形へ直り、主張が下限の形で書かれている。

## 変更ファイル一覧と対象シンボル

| ファイル | 変更内容 |
|---|---|
| `scripts/governance-check.mjs` | 19 検査モジュールの静的 import を削除（evidence 用 2 本を除く）。`export { … }` を `governanceDocs` / `makeSnapshot` の 2 名へ縮小。再輸出ブロックの導入コメント（`:91-92`）を現状へ更新 |
| `scripts/governance-check.test.mjs` | `MODULE_INDEX_CRATES` の import 元を `./governance/checks/G-module-index.mjs` へ付け替え。公開面カナリアの凍結配列を 4 名へ縮小。カナリアの根拠コメント（`:92-96`）を更新 |
| `scripts/governance-manifest.test.mjs` | フォールトインジェクション節のコメント（`:64-74`）を未来形から現在形へ。射程の 2 条件（16/19・ペア消失）を明記 |
| `docs/build-commands.md:30` | 「名指しの母集団は `scripts/governance-check.mjs` の `REQUIRED_DISALLOWED_METHODS` である」→ 所在を `scripts/governance/checks/G-clippy-disallowed.mjs` へ |
| `src-tauri/clippy.toml:2` | 「群を足すときは `scripts/governance-check.mjs` の `REQUIRED_DISALLOWED_METHODS` へも登録すること」→ 同上。**規範の手順書であり、宛先が空になる** |

**縮小後の公開面（4 名）**: `buildChecks` / `governanceDocs` / `makeSnapshot` / `runAll`。
`buildChecks` / `runAll` は facade 内の `export function` 宣言なので、`export { … }` ブロックに書くのは 2 名。

### `MODULE_INDEX_CRATES` を facade から外す理由

残すと facade が `G-module-index.mjs` を静的 import し続け、**切り替わらないファイルが 3 本から 4 本へ増える**。付け替えは `governance-check.test.mjs` の import 行 1 本で済み、#1093 が確立した「テストは自分の検査モジュールから直接 import する」形にも揃う。`.claude/skills/health-check/SKILL.md:23` が名指す母集団カナリアは同じファイルに残る（import 元が変わるだけ・3b で裁定済み）。

## 実装順序

### Phase 1 — 消費者の付け替えと公開面の縮小

1. `governance-check.test.mjs` の `MODULE_INDEX_CRATES` を `G-module-index.mjs` からの直接 import へ付け替える。
2. `governance-check.mjs` から、evidence 用 2 本以外の検査モジュール import を削除する。
3. `export { … }` を `governanceDocs` / `makeSnapshot` の 2 名へ縮小する。
4. 公開面カナリアの凍結配列を `["buildChecks", "governanceDocs", "makeSnapshot", "runAll"]` へ更新する。
5. `npm test` / `npm run governance:check` で緑を確認する。**PostToolUse hook は `scripts/**` に検査を割り当てないので、沈黙は「何も走らなかった」である**（3b で `selectChecks` を実測）。手動で回す。

### Phase 2 — 切り替わりの実測（使い捨てコピー）

稼働中の `scripts/governance/checks/` は触らない（`.claude/rules/safety-nets.md`）。スクラッチパッド（`C:\Users\Eoh\AppData\Local\Temp\claude\C--workspace-Snotra\0d224d6b-d96d-4e07-a6fa-43e2945f3b5a\scratchpad`）へ `git worktree` で複製を作る。

**起動形**: 複製へ `cd` してから**相対パス**でスクリプトを叩く。絶対パス起動だと `registry.mjs` の `import.meta.url` 起点と `makeSnapshot` の `process.cwd()` 起点がずれ、#1092 H1 型の混成ツリーになる。

**対象は `G-hook-fires`。`G-ci-table` を選んではならない**（Step 2b が発見・メイン側で再照合済み）——`scripts/governance-manifest.test.mjs:78-80` が `"G-ci-table"` を**逐語で名指し**ており、消すと `base.checks.filter(...)` が no-op になって `diffManifest` が `[]` を返し、**`npm test` が別の理由で赤くなる**。それでは受け入れ条件 5(b)（ローカル緑・manifest だけ赤）という切り替わりの核心が測れない。`G-hook-fires` はテストのアサーションから逐語で名指されていない（`.claude/hooks/post-edit.mjs:123` / `docs/hooks.md:42` / `governance-check.mjs:30` の言及はいずれも散文）。

1. **絞った後の複製**（Phase 1 のコミットから worktree を作る）で `node scripts/governance-manifest.mjs > base.json` を取る。
2. `G-hook-fires.mjs` と `G-hook-fires.test.mjs` を**ペアで**消す。
3. `node scripts/governance-check.mjs` が落ちずに走ることを実測する（旧防御線が消えた証拠）。
4. `npm test` が緑であることを実測する（他の層も黙っている証拠）。
5. `PR_BODY` を宣言なしにして `node scripts/governance-manifest.mjs --compare base.json` が exit 1 と `-G-hook-fires` を返すことを実測する（新検知器が発火する証拠）。
6. **対照 A**: `main`（絞る前）から作った複製で同じペア消失を行い、`node scripts/governance-check.mjs` が `ERR_MODULE_NOT_FOUND` で落ちることを実測する。**前後差が証拠であって、後だけを見ても load-bearing になったことは言えない。**
7. **対照 B**: 絞った後の複製で `G-hook-fires.mjs` **だけ**を消し、`npm test` が `ERR_MODULE_NOT_FOUND` で赤くなることを実測する（制約 1b 上段の裏取り＝下限主張の根拠）。
8. 複製を撤去し、測定値を `plan.md` へ書き戻す。

### Phase 3 — 散文の同期

1. `governance-check.mjs:91-92` の再輸出ブロック導入コメントを、縮小後の消費者と「なぜこれだけか」へ書き換える。
2. `governance-check.test.mjs:92-96` の公開面カナリア根拠コメントを更新する（「検査を移すたびに書き足す面」ではなくなった。凍結の理由は残る）。
3. `governance-manifest.test.mjs:64-74` を現在形へ書き換え、**射程を下限の形で書く**。含める要素:
   - manifest 差分が唯一の検知器になるのは **ペア消失**（`.mjs` + `.test.mjs`）のとき
   - 単独消失は `npm test` が引き続き捕まえる（19/19 のテストが隣を静的 import・`vitest.config.ts` の `include`）
   - evidence 経由 2 本 + `instrument.mjs` 経由 1 本の計 3 本は依然 import エラーで落ちる
   - 検知の**性質**が変わる: ハードエラー（push でも PR でも赤・回避不能）→ PR 限定かつ本文への宣言で通るゲート
4. **`REQUIRED_DISALLOWED_METHODS` の所在を指す 2 箇所を直す**（Step 2b が発見・メイン側で再照合済み）。どちらも「`scripts/governance-check.mjs` の `REQUIRED_DISALLOWED_METHODS`」と書いており、絞った後は宛先が空になる。所在は `scripts/governance/checks/G-clippy-disallowed.mjs` である。
   - `docs/build-commands.md:30` — 「名指しの母集団は〜である」
   - `src-tauri/clippy.toml:2` — 「群を足すときは〜へも登録すること」。**規範の手順書**であり、腐ると禁止集合を足す人が登録先を見失う
   - **`governance:check` はこの腐りを 1 件も捕まえない**——識別子は `G-clippy-disallowed.mjs` に実在し続けるので G-stale-identifiers は緑、パスも実在し続けるので G-references も緑。所在が偽になる形は機械検査の射程外である
5. `npm run governance:check` を再実行する。

## 不変条件と異常系

- **公開面カナリアの凍結を外さない。** sorted equality の形を維持し、配列の中身だけを縮める。
- **`export *` にしない**（`governance-check.mjs:92` の契約）。
- **facade の契約「依存ゼロ・決定的」を維持する。** import を減らすだけで新しい依存は入れない。
- **全称表現を使わない。** 「検査ファイルの消失は manifest 差分として現れる」は (i) 残余 3 本と (ii) 単独消失の 2 方向で偽になる。書くのは下限の形（「ペア消失のとき、16 本については〜」）。
- 異常系: 削除した import のうち 1 つでも facade 本体が使っていれば `npm test` が落ちる（`buildChecks` / `runAll` / evidence の算出が即座に鳴る）。沈黙経路は無い。

## テスト方針と検証コマンド

- `npm test` — 公開面カナリア・配線カナリア（`G-references.test.mjs` / `G-stale-identifiers.test.mjs`）・manifest テストが同時に鳴る。
- `npm run governance:check` — evidence 行（`clippyDisallowedCount` / `adrFiles`）が壊れていないことの確認。
- Phase 2 の使い捨てコピーでの 4 点測定 + 対照 2 本。
- **実 CI の赤は PR が在って初めて測れる**（`.claude/rules/safety-nets.md`）。`governance manifest delta` step が実際に走ることの確認は **PR 本文のチェックリスト**へ送る（#858 / #749）。

## `SPEC.md`・関連文書の更新要否

- **`SPEC.md`: 不要。** 製品挙動を一切変えない。
- **manifest delta 宣言: 不要。** `manifest()` の 4 列（`checks` / `docs` / `rules` / `skills`）に export 名は含まれない（3b で `governance-manifest.mjs:23-33` を検算）。検査 ID・対象文書・rules・skills はいずれも変わらない。
- **`.claude/skills/health-check/`（`references/mechanized-checks.md:9` / `SKILL.md:23`）: 更新不要。** 「`governance-check.mjs` は re-export のみ」「`governanceDocs()` が走査元の正本」は `governanceDocs` が公開面に残るため真のまま（3b で裁定）。
- **`AGENTS.md` / ルート `CLAUDE.md`: 不要。** どちらも facade の公開面に言及していない。
- **`docs/development-principles.md`「検証の層と、層と層の隙間」: 不要。** 表の行は手段の粒度（`npm test` / `governance:check`）であり、その内訳は変わらない。層の移動は `governance-manifest.test.mjs` のコメント（正本）が持つ。

## 作業項目

### Phase 1 — 消費者の付け替えと公開面の縮小

- [ ] `governance-check.test.mjs` の `MODULE_INDEX_CRATES` を `G-module-index.mjs` からの直接 import へ付け替える
- [ ] `governance-check.mjs` から evidence 用 2 本以外の検査モジュール import を削除する
- [ ] `export { … }` を `governanceDocs` / `makeSnapshot` の 2 名へ縮小する
- [ ] 公開面カナリアの凍結配列を 4 名へ更新する
- [ ] `npm test` / `npm run governance:check` が緑（hook は走らないので手動）

### Phase 2 — 切り替わりの実測

- [ ] 使い捨てコピー（worktree・スクラッチパッド）を作り、`cd` してから相対パスで base manifest を取る
- [ ] `G-hook-fires` をペアで消し、`node scripts/governance-check.mjs` が import エラーで落ちないことを実測する
- [ ] 同じ複製で `npm test` が緑であることを実測する
- [ ] 同じ複製で `--compare` が exit 1 と `-G-hook-fires` を返すことを実測する
- [ ] 対照 A: main（絞る前）の複製で同じペア消失が `ERR_MODULE_NOT_FOUND` を起こすことを実測する
- [ ] 対照 B: 絞った後の複製で単独消失（`.mjs` のみ）が `npm test` を赤にすることを実測する
- [ ] 複製を撤去し、実測値を `plan.md` へ書き戻す

### Phase 3 — 散文の同期

- [ ] `governance-check.mjs` の再輸出ブロック導入コメントを更新する
- [ ] `governance-check.test.mjs` の公開面カナリア根拠コメントを更新する
- [ ] `governance-manifest.test.mjs:64-74` を現在形へ書き換え、射程 2 条件と残余 3 本と検知の性質変化を下限の形で明記する
- [ ] `docs/build-commands.md:30` の `REQUIRED_DISALLOWED_METHODS` の所在を `G-clippy-disallowed.mjs` へ直す
- [ ] `src-tauri/clippy.toml:2` の同じ所在参照を直す
- [ ] `npm run governance:check` が緑

## plan-review 結果

- リスク: **高**（`/plan-review`「リスク判定」の「hook、CI、rules、skills、ガバナンス文書を変更する」に該当。保証を層から層へ移す変更）
- レビュー方式: 独立導出 1 体（Step 2b）
- エージェント数: 2（3b の敵対的調査 1 体 + Step 2b の独立導出 1 体）

### 要対処（メイン側で根拠を再照合し、採否を判定した）

| # | 指摘 | 再照合の結果 | 対応 |
|---|---|---|---|
| D | 実測対象に `G-ci-table` を選んではならない。`governance-manifest.test.mjs:78-80` が逐語で名指しており、消すと `npm test` が別の理由で赤くなる | **成立**。当該 3 行を実読。`base.checks.filter(id => id !== "G-ci-table")` が no-op 化 → `diffManifest` が `[]` → assertion 失敗 | **計画を修正**。対象を `G-hook-fires` へ（テストのアサーションから逐語で名指されていないことを grep で検算済み） |
| E | `docs/build-commands.md:30` と `src-tauri/clippy.toml:2` が `REQUIRED_DISALLOWED_METHODS` の所在を facade として名指しており、宛先が空になる。`governance:check` はこの腐りを捕まえない | **成立**。両行を実読。識別子・パスとも実在し続けるため G-stale-identifiers も G-references も緑のまま素通りする | **計画を修正**。Phase 3 へ 2 項目を追加 |
| A | evidence の `clippyDisallowedCount` / `adrFiles` を `ctx.record` 経由へ移せば、残る 2 本の静的 import も落ちて切り替わりが 18/19 になる | **成立するが射程の拡大**。`governance-check.mjs:66-67` が当該 import を「ファイルが消えれば鳴る」意図として明記しており、ルート `CLAUDE.md`「意図的なリファクタリングの結果を元に戻さない」に当たる | **人間の裁定へ回す**（下記） |
| B | `instrument.mjs:6` の `G-skill-table` 静的 import を「移す or 明記して受容」のどちらかで決めること | **成立**。既に `research.md` 制約 1 と計画の射程 2 条件で明記済み | **明記して受容**（現案）。A を採る場合は同時に移送 |
| C | A を採ると新しい沈黙経路が生まれる（`record` 呼び忘れで evidence が `undefined` を印字して exit 0・テストも全緑） | **成立**（Step 2b が複製で実測） | A を採る場合のみ「evidence に `undefined` を含まない」カナリアを必須とする |

### 軽微

- `governance-manifest.mjs:14` を `lib.mjs` 直取りへ変えて公開面を 2 名まで削る案 — **採らない**。狙いに影響せず（`lib.mjs` は検査ではない）、`mechanized-checks.md:9` が偽になる副作用が付く。
- 検査件数（19 件）を凍結する見張りがローカルに無い（`governance-check.test.mjs:74-78` は `buildChecks` 由来の自己参照ゆえ 19→18 でも緑）。PR 本文へ「件数の見張りは PR CI だけになった」と書く。
- `.claude/skills/health-check/SKILL.md:23` / `references/mechanized-checks.md:9` — Step 2b は要更新に挙げたが、**メイン側の再照合で不要と判定**。前者が名指す母集団カナリアは同じファイルに残り import 元が変わるだけ、後者の `governanceDocs` は縮小後も公開面に残る。どちらも字面が真のまま。

### 未検証

- **実 CI 上での切り替わり**（`ci.yml:111-129` の base worktree 生成と `gh pr view --json body`）。`ci.yml` は `pull_request` でのみ起動するため PR が在って初めて測れる（`.claude/rules/safety-nets.md`）。**PR 本文のチェックリストへ送る**（#858 / #749）。
- Step 2b が挙げた「`vitest.config.ts` の include 外にテストが在るかもしれない」— **メイン側で潰した**。`git ls-files "*.test.mjs"` が 31 件を返し、そのすべてが `.claude/hooks/` / `.githooks/` / `scripts/` のいずれかに在る（include 外ゼロ）。
- 56 名について「名前を出さずに facade の所有を語った散文」の網羅性。概念ラベル 6 種 + 識別子名での走査で補ったが、**全称は主張しない**。

### 判断

- 実装着手: **人間の裁定待ち**（下記の射程の選択のみ）

## 未確定（実装前に潰す）

- [x] **射程の選択** — 2026-08-16 にユーザーが**最小差分・16/19** を選択。拡大案（evidence の `ctx.record` 化 + `skillFiles`/`modelHiddenSkills` の `lib.mjs` 移送で 19/19）は採らない。ゆえに要対処 A / C は**この PR の射程外**となり、B は「明記して受容」で確定する。`governance-check.mjs:66-67` の意図的設計は無変更で保存する

## 人間レビュー

- [x] 承認済み — 2026-08-16 / 問い: "`workspace/plan.md`「人間レビュー」は `- [ ] 承認待ち` のままです。次のどちらかをお願いいたします。1. `workspace/plan.md` へ注釈を追加する 2. 計画を明示的に承認する" / 回答: "OK"

（射程の選択は別途 `AskUserQuestion` で確認済み。問い: "再輸出を絞る射程をどちらにいたしましょう。" / 回答: "最小差分・16/19（推奨）"）
