# 独立導出レビュー — issue #925（G-heading-refs の走査元へ `.rs` を足す）

`workspace/plan.md` / `workspace/research.md` は読んでいない。以下はコード・規範・実測だけから導出した。
実測はすべて**リポジトリを変更せず**、`scripts/governance-check.mjs` の export された純関数を import し、
母集団または `snapshot.read` の返り値だけを差し替えた**複製**に対して行った（`.claude/rules/safety-nets.md`
「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」）。

## 0. 導出した変更ファイルとシンボルの一覧（本レビューの本体）

| # | ファイル | シンボル / 位置 | 変更内容 | 分類 |
|---|---|---|---|---|
| 1 | `scripts/governance-check.mjs` | `headingRefDocs`（1131） | 述語を `.md` → `.md \| .rs` へ。**G-heading-refs / G-near-heading-refs の走査元はこの 1 関数**（`buildChecks` 1523 で 1 度だけ呼び 1552 / 1554 の両検査へ渡す） | 要対処 |
| 2 | `scripts/governance-check.mjs` | **新規** 母集団関数 2 本（例 `headingRefMdDocs` / `headingRefRustFiles`）＋ `headingRefDocs` はその和 | union だけを 0 件検知に使うと**片方の消滅が他方の長さに隠れる**（`staleDocs` / `staleGuides` と同型・1564-1569 のコメントが SSOT） | 要対処 |
| 3 | `scripts/governance-check.mjs` | `buildChecks` の `sink`（1527-1531）＋ `runAll` の 0 件検知（1563） | `sink` へ 2 母集団を載せ、`runAll` に**別々の 0 件検知を 2 本**置く（1 本のままだと沈黙経路が新設される） | 要対処 |
| 4 | `scripts/governance-check.mjs` | `headingRefDocs` の doc コメント（1122-1130） | 「見出し参照はガバナンス文書の外にも書かれ」の列挙と除外の説明が `.md` 前提。`.rs` を含むこと・`.rs` にはコードフェンス除去が効かないことを書く | 要対処 |
| 5 | `snotra-settings/src/tabs/visual.rs` | 395 行のコメント | `` `.claude/rules/safety-nets.md`「稼働中のガードを弱めず複製に変異を当てる」 `` → 現行見出しの逐語（前方一致で足りる） | 要対処 |
| 6 | `src-tauri/src/monitor.rs` | 80 行の doc コメント | `` `SPEC.md` §4.7「バーの位置は行の出没で動かさない」 `` → `` `SPEC.md` §4.7「4.7 結果表示制御（2 窓構成）」 ``（本文の言明は散文として残す形を推奨・実測根拠は §3.2） | 要対処 |
| 7 | `scripts/governance-check.test.mjs` | 860-870「母集団は履歴資料・作業バッファ・凍結された歴史（docs/adr/）を除く全 md」 | **この変更で主張が偽になる唯一のテスト**。`"src/main.rs": ""` を負のカナリアに使っており意味が反転する | 要対処 |
| 8 | `scripts/governance-check.test.mjs` | G-heading-refs / G-near-heading-refs / runAll の各 describe | 種を 6 本追加（§5） | 要対処 |
| 9 | `AGENTS.md` | 「条件別チェック（トリガー → 参照先）」表の 62 行 | トリガーが「ガバナンス文書（`*.md`・…）を変更」のままで、`.rs` のコメント編集が governance:check のトリガーになったことを言わない | 要対処 |
| 10 | `.claude/rules/governance-docs.md` 22 行 | 「これらの編集に PostToolUse hook 検査は走らない」の段 | `.rs` では hook が走り**沈黙する**が、その沈黙は fmt/clippy/test の合格であって見出し参照の着地を含まない（#497 型の false green） | 要対処 |
| 11 | rules の配送（`.claude/rules/governance-docs.md` の `paths`、または `snotra-core.md` / `snotra-settings.md` / `src-tauri.md`） | frontmatter `paths` | 正準形の規範が `.rs` 編集者へ届かない。選択肢と費用は §4.3（**どちらを採るかは決めない**） | 要対処（判断を要する） |
| 12 | `docs/adr/ADR-canonical-heading-references.md` | 追記節 | 「なぜ `.mjs` / `.ps1` へは広げないか」＝否定の知識。ADR 本文は凍結だが**スコープ拡大の追記**は前例あり（同 ADR 29 行の 2026-07-26 追記、`ADR-stale-identifier-detector-scope` 63 行） | 要対処 |
| 13 | `scripts/governance-check.mjs` | `runAll` の `evidence`（1574） | 「見出し参照 N 件を M 文書から照合」の M が 48 → 138（うち 90 が `.rs`）。語を「文書」のままにするか分けるか | 軽微 |
| 14 | `docs/build-commands.md` 130 / 133 | カテゴリ F の説明 | 130 行の検査列挙は「見出し参照の着地」で足りるが、133 行「PostToolUse フックは `.md` に検査を割り当てない…沈黙は『何も走らなかった』」が `.rs` では別の意味になる（#10 と同じ穴の写し） | 軽微 |

---

## 1. 見出し参照検査の走査元は **`headingRefDocs`**（`scripts/governance-check.mjs:1131`）

取り違えうる似た名前が 4 つあるが、いずれも別検査の母集団である。根拠は `buildChecks`（1521-1556）:

- `governanceDocs`（1111）→ `G-references`（1539）・`G-spec-sections`（1540）・`adrCitationDocs` の入力（1551）
- `headingRefDocs`（1131）→ **`G-heading-refs`（1552）と `G-near-heading-refs`（1554）の両方**。`buildChecks:1523` で 1 度だけ呼ばれ、`refDocs` として両者へ渡る
- `staleIdentifierDocs` / `staleIdentifierGuideDocs` / `staleIdentifierTargets`（1255 / 1266 / 1274）→ `G-stale-identifiers`（1553）
- `adrCitationDocs`（1478）→ `G-adr-citations`（1551）

さらに **`REF_EXTENSIONS`（30 行）は罠である**。名前に「REF」を含み `.rs` を既に含むが、これは `checkReferences`（161）が
見る**パスの実在**用であって見出し参照とは無関係。ここを触っても要求 1 は満たされない。

**帰結**: `headingRefDocs` を変えると `G-near-heading-refs` の母集団も同時に広がる。要求 1 は G-heading-refs しか
名指していないが、**実装上は分離できない**（分離するなら 2 母集団関数へ割るという別設計になる）。実測では
`.rs` からの近傍参照は 0 件なので finding は増えないが、**検査の射程が 2 つ広がる**ことは PR 本文と ADR に書くべき事実である。

## 2. 沈黙経路 — **新設される。0 件検知を 1 本のままにしてはならない**（最重要）

`runAll`（1558）の 0 件検知は現状 1 本:

```
if (ctx.refDocs.length === 0) findings.push(finding(".", 1, "G-heading-refs の対象 md が 0 件（母集団の欠落）"));
```

`headingRefDocs` を `.md | .rs` の和にすると、**片方の系統が丸ごと消えても長さが 0 にならない**。実測（複製へ変異、`seed.mjs` 種5）:

```
.rs 全消滅時の union 母集団: [ 'CLAUDE.md' ]  → length>0 ゆえ 0 件検知は鳴らない
.md 全消滅時の union 母集団: [ 'src/a.rs' ]   → length>0 ゆえ 0 件検知は鳴らない
```

これはこのスクリプトが既に一度踏み、コメントで名指しした失敗と同型である（1564-1569）:

> `staleTargets` ではなく `staleDocs` を見る——…**グロブ由来の母集団ごとに 1 本ずつ要る**——束ねると片方が埋めた長さで他方の消滅が隠れる。

**要対処**: 母集団関数を 2 本に割り（`headingRefDocs` はその和を返す形でよい）、`sink` へ両方を載せ、
`runAll` に **`.md` 側と `.rs` 側の 0 件検知を別々に 2 本**置く。1 本のままなら、拡張子述語の腐敗・
`WALK_EXCLUDE_NAMES` への誤追加・crate 移動で `.rs` が母集団から落ちても**永久に沈黙する**。

補足（別軸の残余・新設ではない）: `checked`（照合件数）が 0 でも fail しない構造は `.md` について既にそうであり、
今回新設される穴ではない。ただし 0 件検知を 2 本置けば「ファイルは在るが参照が 1 件も無い」状態は
evidence 行の数字でしか見えない点は変わらない。

## 3. `.rs` を足すと何件照合され、何件の finding が出るか（実測）

`makeSnapshot("C:/workspace/Snotra")` に対し、母集団だけを差し替えて `scanHeadingRefs` / `scanNearHeadingRefs` を直接呼んだ結果:

| 母集団 | 文書数 | G-heading-refs checked | findings | G-near-heading-refs checked | findings |
|---|---|---|---|---|---|
| 現行（`.md` のみ） | 48 | 116 | 0 | 13 | 0 |
| 追加分（`.rs` のみ） | **90** | **+27** | **2** | **0** | **0** |
| 合計 | 138 | 143 | 2 | 13 | 0 |

`.rs` は 90 件（`git ls-files '*.rs'` も 90。除外プレフィクス配下の `.rs` は 0 件＝`workspace/` / `docs/adr/` /
`docs/superpowers/` の除外は `.rs` に対して**現状 inert** である。だからこそ fixture でしか固定できない・§5 種6b）。

### 3.1 finding 1 — `snotra-settings/src/tabs/visual.rs:395`

```
（`.claude/rules/safety-nets.md`「稼働中のガードを弱めず複製に変異を当てる」）。
```

参照先の現在の見出しは `.claude/rules/safety-nets.md:22` の
`## フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる`。

**由来（issue の因果説明と食い違うので、そのまま書き写さないこと）**: `git log -S` で追うと

- 見出しは `## 故障注入では、稼働中のガードを弱めない——複製に変異を当てる` として存在し、#623（`905edaf`・2026-07-20）で
  「故障注入」→「フォールトインジェクション」へ**改題**された
- `visual.rs` のこの参照は #826（`3acef09`・2026-07-28）＝**改題より後**に書かれ、しかも旧見出し
  （`故障注入では、…弱めない——…`）とも一致しない「稼働中のガードを弱めず…」という**言い換え**である

つまり「改題に追随できなかった」のではなく、**生まれたときから見出し名ではなかった**。直し方は同じ（現行見出しの逐語）だが、
**根本原因が違う**: `.rs` のコメントを書く者に正準形の規範が配送されていない（§4.3）ため、記憶から言い換えて書いた。
issue の前提をそのまま PR 本文へ写すと、この根本原因が視界から落ちる。

**直し方**: `` `.claude/rules/safety-nets.md`「フォールトインジェクションでは、稼働中のガードを弱めない」 ``
（照合は正規化後の**前方一致**なので `——複製に変異を当てる` まで書かなくてよい。書いてもよい）。
複製へこの置換を当てて実測: `.rs` 27 件 checked / findings **0**。

### 3.2 finding 2 — `src-tauri/src/monitor.rs:80`

```
/// だけでバーが隣モニターへ飛ぶ**（`SPEC.md` §4.7「バーの位置は行の出没で動かさない」を
/// 破る）。
```

`SPEC.md` §4.7 の見出しは `### 4.7 結果表示制御（2 窓構成）`（185 行）。
「バーの位置は行の出没で動かさない」は**本文の言明**で、`SPEC.md:450`（§8.2 マルチモニター対応）にも散文で現れる。
これが issue の言う「`「」` の中身が見出し名でない」ケース。

**この repo の正準形は「番号を label に含める」形である**——照合は前方一致で、ATX アンカーが `4.7 結果表示制御（2 窓構成）`
だからである。既存の着地する実例: `docs/architecture.md:82` の `` `SPEC.md` §4.7「4.7 結果表示制御（2 窓構成）」 ``。

**推奨する直し方（実測で選ぶ）**: 見出し label への単純置換ではなく、**本文の言明を散文として残したまま正準形を並記**する形。

| 案 | G-heading-refs | G-near-heading-refs | 失うもの |
|---|---|---|---|
| A: label を見出しへ置換 | checked 1 / findings 0 | 0 / 0 | 「行の出没で動かさない」という**この doc コメントの要点**が消える |
| B: `` `SPEC.md` §4.7「4.7 結果表示制御（2 窓構成）」の「バーの位置は行の出没で動かさない」を破る `` | checked 1 / **findings 0** | checked 0 / **findings 0** | なし |

案 B も 裁定「参照側の記法を正準形へ直す」に適合する（検出器も `SPEC.md` も規範も変えない）。
2 つ目の `「…」` が近傍検査で誤爆しないことは実測済み（`ADJACENT_REF` に当たる先頭形が優先され、
2 つ目の引用の直前はバッククォートで閉じた target ではないため `NEAR_REF` にも当たらない）。

### 3.3 2 件を直した後の全体

複製へ両方の置換を当てて実測: `.rs` 母集団 27 件 checked / findings **0**、近傍 0 件 checked / findings 0。

## 4. 規範・散文で古くなるもの

### 4.1 スクリプト内のコメント（要対処）

- `headingRefDocs` の doc コメント（1122-1130）: 「見出し参照はガバナンス文書の外（`PERFORMANCE.md`・`.claude/agents/`）にも書かれ」
  の列挙に `.rs`（Rust のコメント）が無い。除外の説明も `.md` 前提。**`.rs` にはコードフェンス除去が効かない**
  ことも書くべき（§6 の残余）
- G-heading-refs のヘッダブロック（931-941）: 「アンカーは…**この repo の参照実態に合わせた**」と「受容する偽陰性」は
  `.md` の実態から導かれた記述。`.rs` を加えた後の実態（27 件 / 誤爆 0）を根拠として足すのが筋

### 4.2 `AGENTS.md`「条件別チェック（トリガー → 参照先）」62 行（要対処）

現在のトリガーは「ガバナンス文書（`*.md`・スキル表・モジュール索引・rules・workflow）を変更」。
この変更の後、**`.rs` のコメントに正準形の参照を書く／参照先の見出しを改題する**ことが governance:check のトリガーになる。
AGENTS.md 自身が「注意事項は『トリガー』に括り付けてある」と宣言している以上、ここが経路の SSOT であり、
更新しなければ「トリガーも機構も在るのに実行漏れ」（MEMORY の `pr-governance-check-before-pr` が同型の再発を記録）が起きる。
機構の後ろ盾は無い（`G-check-skill-enumeration` はこの節を読むが check スキルの列挙だけを見る）。

### 4.3 rules の配送 — `.rs` 編集者に正準形の規範が届かない（要対処・**判断を要する**）

`.claude/rules/governance-docs.md` の `paths` は `AGENTS.md` / `CLAUDE.md` / `docs/adr/**` / `scripts/*.mjs` /
`scripts/*.ps1` / `scripts/lib/**` で、**`.rs` では 1 度も配送されない**。この rule が持つ
「他を指すときは正準形で書く」「既に消滅した節の名前を正準形で書かない」が、いま新しく検査対象になる
Rust コメントの書き手へ届かない。§3.1 の言い換えはまさにこの穴の産物である。

選択肢（**どちらを採るかは決めない。費用を名指しするに留める**）:

- (a) `governance-docs.md` の `paths` に `**/*.rs` を足す → **全 Rust 編集でこの rule が配送される**（注意の面積の費用。
  rules は `AREA_BUDGET.rules` の対象でもある）
- (b) 既に `**/*.rs` で発火する `snotra-core.md` / `snotra-settings.md` / `src-tauri.md` へ 1 行のポインタを足す →
  **`snotra-egui-runtime/**/*.rs` を覆う rule が存在しない**（rules 一覧に無い・実測）ので、その crate だけ穴が残る

どちらでも、足した glob は 1 件以上にマッチしないと `G-rules-globs`（`checkRulesGlobs`:541）が赤になる。

### 4.4 hook の沈黙の意味（要対処）

`.claude/rules/governance-docs.md:22` は「これらの編集に PostToolUse hook 検査は走らない（`selectChecks` が
ガバナンス文書に検査を割り当てず空集合を返す）」と書く。`.rs` では `selectChecks` が fmt/clippy/test を割り当てる
（`post-edit.mjs:127` の `isRust`）ため hook は走り、**通れば沈黙する**。ルート `CLAUDE.md`「フック」の規約により
「検査が割り当てられているファイルでは沈黙 = 合格」なので、**Rust コメントの見出し参照が腐っていても沈黙が合格に見える**。
これは #497 が名指した false green の再来にあたり、`docs/build-commands.md:133` の同趣旨の文にも同じ穴がある（#14）。
規範側で「`.rs` の沈黙は fmt/clippy/test の合格であって見出し参照の着地を含まない」と明記するのが最小の手当て。

### 4.5 ADR（要対処）

`docs/adr/ADR-canonical-heading-references.md` は G-heading-refs の決定記録で、
`ADR-adr-frozen-history` により**本文は凍結**（生きた層の改名に追随させない）。ただし**スコープ拡大の追記**には前例が 2 つある:
同 ADR 29 行の「2026-07-26 追記」、`ADR-stale-identifier-detector-scope` 63 行の失効注記。
今回は「`.mjs` / `.ps1` へは広げない」という**否定の知識**が既に裁定済みで、
`AGENTS.md`「ドキュメント参照」が ADR を否定の知識の置き場と定めている以上、追記節が置き場として正しい。
（`docs/adr/ADR-adr-frozen-history.md:9` の「G-heading-refs の走査元」記述は `docs/adr/` 除外の話なので、
`.rs` 追加では偽にならない。触らなくてよい。）

### 4.6 触らなくてよいもの（誤って広げないための線引き）

- `docs/superpowers/**`（`2026-07-28-plan-review-loop-design.md:193` の「母集団（68 文書）」等）— #589 で非規範化された歴史資料
- `.claude/skills/retrospective/SKILL.md:105` — `RETROSPECTIVE.md` が母集団に入る話で、`.rs` 追加では偽にならない
- `.claude/rules/governance-docs.md:15` の「対象は `<path>.md` か `/skill-name`」— これは**参照先**の形の話であり、
  走査元とは別軸。真のまま

## 5. フォールトインジェクションの種 — **6 本 +（緑の対照）**

`.claude/rules/safety-nets.md`「検出器のカバー範囲は、欠落のパターンごとに検算する」に従い、欠落のパターンごとに 1 本ずつ。
すべて合成スナップショット（複製）へ蒔く。以下は**すべて実測済み**（`seed.mjs`）:

| 種 | 欠落のパターン | 期待 | 実測 |
|---|---|---|---|
| 1 | `.rs` の正準形参照が**着地しない**（見出しの改題・消滅） | 赤 | ✅「見出し参照が着地しない: `` `CLAUDE.md`「Git 運用」 ``」 |
| 2 | `.rs` の参照**対象が解決できない**（パスの消滅） | 赤 | ✅「見出し参照の対象が解決できない: `` `docs/gone.md`「節」 ``」 |
| 3 | **G-near-heading-refs も `.rs` を見る**（助詞が挟まった近傍形） | 赤 | ✅「見出し参照が正準形でない（G-heading-refs の視界外）…」 |
| 4 | **`.rs` の母集団が 0 件**（拡張子述語の腐敗・walk 除外の誤追加） | 赤 | ⚠️ 現行の 1 本構成では**鳴らない**（§2）。#2/#3 の実装後に赤になることを固定する |
| 5 | **`.md` の母集団が 0 件**（既存の検知が `.rs` の長さに隠れないこと） | 赤 | ⚠️ 同上。**種 4 と束ねてはならない**——束ねると片方が他方を埋める（1564-1569） |
| 6a | 判定対象外の**不混入**（`.ts` / `.mjs` / `.ps1` / `.toml` は母集団に入らない） | 除外 | ✅ union は `CLAUDE.md` と `src/a.rs` だけを返す |
| 6b | 除外**プレフィクスが `.rs` にも効く**（`workspace/x.rs`・`docs/adr/x.rs`） | 除外 | 未実装（現実には該当 0 件＝inert ゆえ **fixture でしか固定できない**） |
| 対照 | `.rs` の正しい参照 | 緑 / checked=1 | ✅ findings 0 / checked 1 |

種 4・5 は**別々の it として書く**こと。種 6a/6b は `.claude/rules/safety-nets.md`「検査の入力集合を、具体対象で検算する」が
要求する双方向（守る対象が入力に現れる／対象外が混じらない）の後者にあたり、`.rs` という**新しい次元**について
両方向を張り直す必要がある。

なお **稼働中のガードを弱めない**という要求は、①実ファイル（`visual.rs` / `monitor.rs` / `safety-nets.md` / `SPEC.md`）を
種のために書き換えない、②`.claude/rules/safety-nets.md` の見出しを実験のために改題しない、で満たされる。
本レビューの実測はすべて `snapshot.read` のオーバーレイか合成スナップショットで行った。

## 6. 既存テストのうち主張が偽になるもの

**1 件だけである**（`scripts/governance-check.test.mjs` を `headingRefDocs` / `scanHeadingRefs` / `scanNearHeadingRefs` /
`collectAnchors` / `resolveRefTarget` で grep して確認）。

- **860-870 `it("母集団は履歴資料・作業バッファ・凍結された歴史（docs/adr/）を除く全 md")`** —
  fixture に `"src/main.rs": ""` を置き `headingRefDocs(s).sort()` が
  `[".claude/agents/code-reviewer.md", "PERFORMANCE.md"]` と**完全一致**することを主張する。
  `.rs` を足すと `src/main.rs` が入り**この一致が壊れる**。it の題（「除く全 md」）も偽になる。
  さらに **`src/main.rs` はここで「判定対象外が混じらない」ことを示す負のカナリアとして置かれており、
  その役割が反転する**——置き換え先（`src/a.ts` / `scripts/x.mjs` / `Cargo.toml` 等）を同時に用意しないと、
  「拡張子の取り違え」を捕まえる枠が消える。加えて `workspace/x.rs` / `docs/adr/x.rs` を足して
  除外プレフィクスが `.rs` にも効くことを固定する（種 6b）。

偽にならないもの（確認済み）:

- 801-858 の G-heading-refs 群・1161 以降の G-near-heading-refs 群は母集団を**リテラルで渡す**（`["docs/x.md"]`）ため無影響
- 878-907（凍結された歴史）は `headingRefDocs` を呼ぶが `.rs` を含まない fixture なので無影響
- 708-713 `runAll`（空母集団）は `findings.length > 0` としか言わないため**偽にはならないが、
  §2 の新しい 0 件検知を何も証明しない**——種 4・5 は別の it が要る

## 7. 実行順序（1 点だけ）

`.github/workflows/ci.yml:58` は実物の `node scripts/governance-check.mjs` を走らせる。
**母集団の拡大（#1）と 2 件の修正（#5・#6）は同一コミットに入れる**こと。分けると、その間の commit で CI が赤になる。
要求 1〜3 は文面上は分離できるが、機構上は 1 と 2 が不可分である。

---

## 要対処（まとめ）

1. **`runAll` の 0 件検知を 2 本に割る**（§2）。union だけを見る 1 本のままだと `.rs` 母集団の消滅が永久に沈黙する。
   同型の失敗が同ファイル 1564-1569 に前例つきで書かれている
2. **走査元は `headingRefDocs`（1131）である**（§1）。`governanceDocs`・`REF_EXTENSIONS` は別検査の母集団で、
   触っても要求を満たさない。また `headingRefDocs` の変更は **G-near-heading-refs の射程も同時に広げる**
3. **`scripts/governance-check.test.mjs:860-870` が偽になる**（§6）。負のカナリア（`src/main.rs`）の役割が反転するので、
   置き換えと除外プレフィクスの `.rs` 版を同時に用意する
4. **finding 2 件の直し方**（§3.1・§3.2）。`visual.rs:395` は現行見出しの逐語、`monitor.rs:80` は
   `` §4.7「4.7 結果表示制御（2 窓構成）」 ``（番号込みが repo の正準形。本文の言明は散文として残す案 B を推奨・実測根拠あり）
5. **`visual.rs` の件は「改題への追随漏れ」ではない**（§3.1）。改題（#623）より後に書かれた**言い換え**であり、
   根本原因は「`.rs` 編集者へ正準形の規範が配送されていない」こと。issue の因果説明をそのまま写さない
6. **`AGENTS.md` 62 行のトリガー行**（§4.2）・**`.claude/rules/governance-docs.md:22` の hook 沈黙の記述**（§4.4）・
   **`headingRefDocs` の doc コメント**（§4.1）を同じ変更で直す
7. **rules 配送の穴**（§4.3）は選択を要する。(a) 全 Rust 編集で配送 / (b) crate 別 rules へポインタ（ただし
   `snotra-egui-runtime` を覆う rule が無い）。足す glob は `G-rules-globs` の対象
8. **種は 6 本 +（緑の対照）**（§5）。とくに `.rs` 0 件と `.md` 0 件は**別々の it** にする
9. **母集団拡大と 2 件の修正は同一コミット**（§7）

## 軽微

- `runAll` の evidence 行（1574）「見出し参照 N 件を M 文書から照合」の M が 48 → 138 になる。`.rs` を「文書」と呼ぶかは好み
  （`staleTargets` が同様に混成の母集団を 1 語で数えている前例あり）
- **rustdoc のコードフェンスは閉じない**: `linesOutsideFences`（69）は `/^\s*```/` で判定するため `/// ``` ` は
  フェンスと見なされず、rustdoc の例の中に書かれた参照も照合される（実測: 合成 fixture で finding が出る／
  素の ` ``` ` 行なら従来どおり抑止される）。**今日の実リポジトリでは影響 0 件**（`.rs` の finding は §3 の 2 件だけ）。
  受容する残余として `headingRefDocs` の doc コメントに 1 行置くのが妥当
- `headingRefDocs` という名が「docs」を含んだまま `.rs` を返すのは misnomer になる。改名すると
  `docs/**` / `.claude/**` の散文の写しを追う必要が出る（腐れば `G-stale-identifiers` が拾う）。費用対効果は低い
- `.rs` はテストコード（`#[cfg(test)]`）のコメントも母集団に入る（`visual.rs:395` がまさにそれ）。
  `G-stale-identifiers` が `productionOnly` でテストを外すのと非対称だが、**規範への参照はテストコードでも
  腐れば同じ害**なので入れて正しい。非対称であること自体は 1 行書いておくと後の読者が迷わない
- `docs/build-commands.md:133` の「沈黙は『何も走らなかった』」も `.rs` では別の意味になる（§4.4 と同じ穴の写し）

## 未検証

- **`npm run governance:check` の実行そのもの**を回していない（リポジトリを変更しない制約と、
  「検査対象を変更しながら検査を走らせない」規律による）。数値は export された純関数を import して母集団だけ
  差し替えた**複製**に対する実測であり、`runAll` を通した end-to-end の 143 件 / 0 findings は未確認。
  実装後に実物のコマンドで測り直すこと
- **CI（governance-check job）での実測**は PR が在って初めて行える（`ci.yml` は `pull_request` 起動）。
  `.claude/rules/safety-nets.md` が指示するとおり **PR 本文のチェックリストへ送る**項目であり、
  計画の検証項目に置くと `gh pr create` の block と循環する
- **`.rs` の近傍参照（G-near-heading-refs）には生きた証拠が無い**。実測 checked 0 / findings 0 で、
  evidence 行の「近傍の見出し参照 13 件」も変わらないため、**`.rs` が近傍検査の母集団に入ったことは
  fixture の種（§5 種 3）でしか示せない**。実リポジトリの数字は証拠にならない
- **`.mjs` / `.ps1` を足したら何件出るか**は測っていない（裁定済みで射程外のため）。ADR の追記節に
  「却下」を書くなら、却下の根拠として件数が要るかは書き手の判断
- **`snotra-egui-runtime` の rules 不在**は `.claude/rules/` の一覧（7 ファイル）と各 `paths` の実測から言っている。
  harness が別経路で何かを配送している可能性までは検証していない
- 案 B（§3.2）の日本語としての読みやすさは**未評価**。照合が通ることだけを測った
