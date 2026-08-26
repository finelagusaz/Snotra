# plan-review — issue #1188（`HEADING_REF` を1段の入れ子まで受け入れる）

対象: `workspace/plan.md`（読むだけ・編集していない）
参照: `workspace/research.md` / `workspace/adversarial-1188.txt` / `gh issue view 1188`
走査制約: すべての grep/find/ripgrep から `.claude/worktrees/` を除外して実施。
検証範囲: team-lead 指定の 2 観点のみ（下記）。他の観点は一律に持ち込んでいない。

---

## 観点 1 — 「導出の入力を変更した」下流全段の追跡が完全か

### 実測した経路

- `scripts/governance/evidence.mjs`: `ev.headingRefs` / `ev.nearRefs` / `ev.foldedRefs` は
  `assembleEvidence` が evidence 行の文字列へ**印字するだけ**。永続化なし（`evidenceView` は
  毎回 `runAll` が新しく作る）。`HEADING_REF` を直接読まない
- `scripts/governance-check.mjs:126-151`（`buildChecks`）: `sink[key] = r.checked` は
  `runAll` が毎回新規に作る `ctx` オブジェクトへの書き込みで、プロセス終了とともに消える。
  永続化なし
- `scripts/governance-manifest.mjs:1-9`（doc 冒頭 `//!`）: **設計として散文母集団（見出し参照数など）を
  対象外にしている**（「直近 20 コミットの実測で構造母集団の変動 0〜1 回・散文母集団は 11 回」と
  明記）。`manifest()` が拾う `checks` は検査 ID の配列のみで `checked` 件数を含まない。
  PR 承認との突き合わせ（`diffManifest`）にも `HEADING_REF` の一致件数は乗らない
- `scripts/governance/edit-findings.mjs`（`SCAN_SCOPED`）・`.claude/hooks/post-edit.mjs` の
  `editFindingsReminder` / `dependentsReminder`: **通る。** `editFindingsReminder` は
  `edit-findings.mjs` を subprocess で呼び、`SCAN_SCOPED`（61-70 行）が
  `{ population: allHeadingRefDocs, check: checkHeadingRefs }` を持つ——`checkHeadingRefs` は
  `G-heading-refs.mjs:47` の `HEADING_REF` をそのまま使う関数なので、**編集時 reminder は
  HEADING_REF の変更を透過する**。`dependentsReminder` も `dependents.mjs`（消費者 2）を直接
  subprocess 実行するので同様に透過する。**ただし永続化はしない**——`spawnSync` の
  stdout を hook が読んで会話へ出すだけで、ディスクにもプロセス外の状態にも残らない
  （`.claude/hooks/post-edit.mjs:195-245` を実読）
- `scripts/governance/instrument.mjs` / `scripts/governance-manifest.mjs`: `grep -n "HEADING_REF"` が
  0 件（実行結果空）。研究の「触らない」列に無いこの 2 本も無関係と確認

### 結論: 「永続化する消費者」は見つからなかった

団評価対象の全消費者（`evidence.mjs`・`governance-check.mjs` の `sink`・`edit-findings.mjs`・
`post-edit.mjs` の reminder）は**毎呼び出しで作り直す一時オブジェクトか stdout 出力**であり、
#755/#801 型（導出値がどこかへ保存されて恒久化する）の経路は無い。

### ⚠️ 軽微 — 編集時 reminder（#1139/#1140）は計画の検証コマンド一覧（V1〜V8）に載っていない

上記の通り `HEADING_REF` を実際に通る 3 つ目の経路である（`G-heading-refs.mjs` / `dependents.mjs` に
続く、間接的な 3 つ目）。today 0 件の母集団では危険は無いが、`checkHeadingRefs` を直接再利用して
いるだけで判定ロジックは重複していないため、正しさのリスクは無い。**計画へ追記するなら**
「editFindingsReminder / dependentsReminder は checkHeadingRefs / dependents.mjs をそのまま
再利用するので独立した検証は不要」の 1 文で足りる（新規テストは不要という判断そのものは正しい）。

### ⚠️ 要対処 — `research.md`/`plan.md` の「G-near-heading-refs の 2 定数は REF_HEAD（頭のみ）から
組まれている」という消費者機序の記述が不正確

`scripts/governance/checks/G-near-heading-refs.mjs` を実読すると、**同ファイルは 2 つの定数を持つ**:

- `ADJACENT_REF`（49 行）: `` new RegExp(`${REF_HEAD}「`) `` — 確かに `REF_HEAD` 由来で頭だけを見る
- `NEAR_REF`（45 行）: `` new RegExp("`([^`\\n]+)`([^`\\n]{1,8}?)「([^「」\\n]+)」", "g") ``
  ——**`REF_HEAD` を経由せず、`HEADING_REF` と同じラベル文字クラス `[^「」\n]+` を独立に
  手書きしている**。こちらが「近傍形が実際に着地するか」を判定する主たる正規表現であり
  （45-87 行）、`ADJACENT_REF` は「すでに隣接形なら見送る」ための除外フィルタに過ぎない
  （69 行 `if (ADJACENT_REF.test(m[0])) continue;`）。

research.md「関連ファイル・シンボル」節・plan.md「触らないもの」節はどちらも
「`G-near-heading-refs.mjs` の `ADJACENT_REF` = `REF_HEAD` + 「」」とだけ書き、`NEAR_REF` の
存在と、それが `HEADING_REF` と同一のラベル字句クラスを別に持つ事実に触れていない。

**実測（`node` で `NEAR_REF` を直接実行）**: `` 詳細は `docs/x.md` の「見出し「入れ子」だ」を見よ ``
に対し、

```
{ whole: '`docs/x.md` の「見出し「入れ子」', target: 'docs/x.md', gap: ' の「見出し', label: '入れ子' }
```

——`label` 群が `[^「」\n]+` のため 1 文字目の内側 `「` で止まれず、代わりに `gap` 群
（`[^`\n]{1,8}?`。`「` を除外しない）が外側の `「見出し` を丸呑みして再マッチし、
**意図しない「入れ子」だけを label として誤って抽出する**（検知は「片方だけが変わる将来」の
実例——`HEADING_REF` を直しても `NEAR_REF` は追随しない独立実装である）。

**この個別のバグ自体は本 PR が作ったものではなく、`HEADING_REF` を広げても `NEAR_REF` は
変わらないという研究/計画の**結論（`checked` 19 件不動）**は実測どおり正しい**——`NEAR_REF` が
`HEADING_REF` を import も継承もしないため。誤っているのは**機序の記述**（「REF_HEAD から
組まれている」は `ADJACENT_REF` だけに当たり、`NEAR_REF` には当たらない）。

**対処の要否**: `NEAR_REF` を直すことは issue #1188 の射程外（近傍形の検出精度は別の話題）。
ただし research.md の「消費者は 2 つである」節・plan.md「触らないもの」節の**記述**は、
「`ADJACENT_REF` は REF_HEAD 由来で不変。`NEAR_REF` は独立実装で `HEADING_REF` を参照しないため
同じく不変（ただし入れ子ラベルに対しては元から誤った label を抽出する既知の別バグを持つ——
本件の射程外）」という形へ訂正すべき。**数の主張（19 件不動）は崩れないので実装は変えなくてよい
——直すのは散文の機序説明だけ**。

---

## 観点 2 — 「この変更で偽になる散文」の数え上げの網羅性

### 実施した grep（team-lead 提示語 + 自分で追加した語）

`入れ子` / `鉤括弧` / `かぎ括弧` / `文字クラス` / `不可視` / `死角` / `照合されない` / `指せない` /
`書けない` / `前方一致` / `あえて` / `短く書いて` を `.rs`/`.md`/`.mjs`/`.ps1`/`.psm1`/`.ts`/`.yml`/
`.toml`/`.claude/` 全域（`.claude/worktrees/` 除外）に当てた。多くはヒットしたが、
**HEADING_REF のラベル入れ子の話題と結び付くものは 0 件**（他はすべて無関係な話題での同語再利用）:

- `入れ子`: ヒット 20 ファイル中、19 は無関係（`scan_all` の走査根・`G-module-linkage` の
  ブロックコメント・`launcher_controller.rs` の `fn` 入れ子・`lsp-config.mjs` の cargo target 等）。
  残り 1 つ `docs/adr/ADR-dependents-reminder-at-edit-time.md` は「節を入れ子にしていなかった」
  という別の意味（節境界の計算）で、本件と無関係
- `鉤括弧`: `build.rs`（plan が既に対象にしている 2 行）と `workspace/plan.md`/`research.md`
  自身のみ
- `かぎ括弧`: 0 件
- `文字クラス`: `G-folded-heading-refs.mjs:48` がヒットするが、これは**同ファイルが自前で持つ
  `OPEN_UNCLOSED` 定数**（`REF_HEAD「[^「」\n]*$`・折れの形 B）についての記述であり、
  `HEADING_REF` のラベルクラスとは別の値なので偽にならない（`REF_HEAD` はこの PR で 1 文字も
  触らない。plan の主張どおり）

**`build.rs:35-36` 以外に、この変更で偽になる生きた層の散文は見つからなかった**——plan/research/
adversarial の 3 者と独立に、より広い語彙で再確認した。

### 「射程外と決めたもの」（`docs/comment-guidelines.md:9` を戻さない）の妥当性 → 妥当

- 現在の見出しは `## 第一原則: コメントは理由を書く`（`git log -p` で確認。直近の rename は
  9916cff2 #1187、その前の rename も `「なぜ」` → 理由 の向き）。**鉤括弧を含まない**ので、
  この見出し自体は本件と無関係（issue が問題にする「見出し名に入れ子鉤括弧」の実例ではもう無い）
- plan.md の fixture（96-98 行）が使う `` `docs/c.md`「第一原則: コメントは「なぜ」を書く」 `` は
  **実在の `docs/comment-guidelines.md` を指すものではなく**、旧見出し文言を借りた合成 fixture
  （ターゲットが `docs/c.md`）——実装時に実ファイルと取り違えるリスクは無い
- 「どちらの綴りでも照合される」（機構上の利得 0）は実際に正しい: 新 regex 適用後は
  入れ子ありの旧綴りへ戻しても着地するし、今の綴り（鉤括弧無し）は元から着地している。
  revert する動機が無いという判断は妥当

### 母集団の凍結層判定（`docs/adr/`・`docs/superpowers/`・`.superpowers/`）→ 妥当（コードで確認）

`scripts/governance/lib.mjs` を実読して確認:
- `.superpowers/` は `WALK_EXCLUDE_PATHS`（39 行）に入っており `makeSnapshot` の**走査自体から
  除外**（fs 歩行に現れない）
- `docs/superpowers/` は `governanceDocs`（530, 680 行）で明示的に `!f.startsWith("docs/superpowers/")`
  除外——511 行の doc コメントが「歴史資料（#589 で非規範化）ゆえ除外」と明記
- `docs/adr/` も同じ 2 箇所で `!f.startsWith("docs/adr/")` 除外、608 行の doc が
  `ADR-adr-frozen-history` を根拠として引く

3 ディレクトリとも「読んでも書かない対象」であることをコード自身から確認できた
（adversarial の ADR 文面確認と、独立に一致）。

---

## 分類

### 要対処

1. **`research.md`「関連ファイル・シンボル」節・plan.md「触らないもの」節の記述訂正**——
   `G-near-heading-refs.mjs` の `NEAR_REF`（45 行、`ADJACENT_REF` とは別の定数）は `REF_HEAD` 由来
   ではなく、`HEADING_REF` と同一のラベル文字クラス `[^「」\n]+` を独立に手書きしている。
   「2 定数は REF_HEAD（頭のみ）から組まれている」という機序の主張は `NEAR_REF` に対して偽。
   **結論（`checked` 19 件が本件で不動）自体は実測で正しい**ため実装への影響は無いが、
   散文の訂正が要る。ついでに「`NEAR_REF` は入れ子ラベルに対し元から誤った label を切り出す
   （本件と無関係の既存バグ、射程外）」を宣言する死角として書き添えるかは実装者の判断に委ねる

### 軽微

1. `editFindingsReminder` / `dependentsReminder`（`.claude/hooks/post-edit.mjs`）が
   `HEADING_REF` を透過的に通ることが、計画の検証コマンド一覧（V1〜V8）に明記されていない
   （実装・正しさへの影響は無い——`checkHeadingRefs`/`dependents.mjs` をそのまま再利用しており
   判定ロジックの複製が無いため）

### 未検証

（なし——観点 1・観点 2 とも、上記の要対処・軽微以外は実測で裏取りできた）

---

## 参照した一次資料

- `scripts/governance/lib.mjs`（130-200 行）
- `scripts/governance/evidence.mjs`（全文）
- `scripts/governance/edit-findings.mjs`（全文）・`edit-findings.test.mjs`（100-169 行）
- `scripts/governance-check.mjs`（100-170 行）・`governance-check.test.mjs`（1-135 行）
- `scripts/governance-manifest.mjs`（1-30 行）・`governance-manifest.test.mjs`（grep）
- `scripts/governance/checks/G-near-heading-refs.mjs`（全文）・`G-folded-heading-refs.mjs`（1-60 行）
- `scripts/governance/dependents.mjs`（1-70 行）
- `.claude/hooks/post-edit.mjs`（195-260 行）
- `scripts/governance/lib.test.mjs`（grep + 290-330 行）・`evidence.test.mjs`（grep）
- `snotra-core/src/search/build.rs`（28-40 行）・`PERFORMANCE.md`（2050-2060 行）
- `docs/comment-guidelines.md`（1-15 行）・`git log -p --follow` 同ファイル
- `docs/adr/ADR-canonical-heading-references.md`（33 行）
- `node` での `NEAR_REF` 直接実行（scratchpad/near-ref-nesting-check.mjs）
