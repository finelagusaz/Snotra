# plan — #925 見出し参照検査の走査元へ `.rs` を足す

## 目的

`.rs` のコメントに書かれた正準形 `` `<対象>`「<見出し>」 `` を G-heading-refs / G-near-heading-refs の照合下へ置き、参照先の改題・移動・削除で**沈黙しない**ようにする。あわせて、その照合が初回に見つける腐り 1 件（(a)）と記法の誤用 1 件（(b)）を直す。

## 受け入れ条件（issue の 4 条件に対応）

1. `.rs` 中の SPEC / rules 見出し参照が、参照先の改題・削除で沈黙しない → `npm run governance:check` の `evidence` に `.rs` の照合件数が現れ、腐らせると赤になる（フォールトインジェクションで実測）
2. 見出し名の引用と本文の引用が区別される → **記法の側で分ける**。正準形 `「」` の中身は見出し名だけとし、本文の言明は `「」` の外に散文で書く（検出器・SPEC・規範の判定は変えない）
3. 検出器が効くことをフォールトインジェクションで実測している（合成スナップショットへ種を蒔く＝稼働中のガードを弱めない）
4. (a) `snotra-settings/src/tabs/visual.rs:395` が直っている

## 変更ファイルと対象シンボル

| ファイル | 対象 | 変更内容 |
|---|---|---|
| `scripts/governance-check.mjs` | `headingRefSourceDocs`（**新規 export**） | `.rs` を返す走査元。理由・却下（`.mjs`）・Rust テストを外さない理由を doc コメントに置く |
| 同 | `buildChecks` | `sink.refSourceDocs` を置き、G-heading-refs / G-near-heading-refs へ md + `.rs` を渡す |
| 同 | `runAll` | `.rs` の腕の 0 件検知を 1 本追加・`evidence` を 2 腕表示へ |
| 同 | `headingRefDocs` の doc コメント | 「md の腕」であることと、腕を分ける理由（0 件カナリアが腕ごとに要る）を明記 |
| `scripts/governance-check.test.mjs` | :860 の母集団カナリア | md の腕であることを明示し、`.rs` が `headingRefSourceDocs` 側に入る主張を追加 |
| 同 | 新規 `describe`（4 テスト） | 種・対照・`#[cfg(test)]` カナリア・0 件検知 |
| `snotra-settings/src/tabs/visual.rs` | :395 のコメント | (a) 現行見出しへ追随 |
| `src-tauri/src/monitor.rs` | :80 の doc コメント | (b) 本文引用を `「」` の外へ出し、参照は `` `SPEC.md`「4.7 結果表示制御（2 窓構成）」`` へ |
| `docs/adr/ADR-canonical-heading-references.md` | :31 の母集団記述 | 追記（`.rs` を足した・却下 3 案） |
| `.claude/rules/governance-docs.md` | 正準形の射程 :15 / hook 沈黙の記述 :22 | 2 文追加（コードのコメントも走査元・`.rs` の沈黙は着地を含まない） |
| `AGENTS.md` | 「条件別チェック」表の `governance:check` 行 | `.rs` のコメントの見出し参照もトリガーであることを足す |

**触らない**: `governanceDocs`（G-references / G-spec-sections の母集団・→ research.md「issue の一次証拠にある誤り」）、`scanHeadingRefs` / `scanNearHeadingRefs` の判定本体、`HEADING_REF` / `NEAR_REF` / `collectAnchors`、`SPEC.md`。

## 実装順序（赤を先に観測する）

**コミット境界**: Phase は作業順であってコミットの境界ではない。**Phase 1〜4 を 1 コミットに入れる**——`.github/workflows/ci.yml` の governance-check job は実物のスクリプトを走らせるので、母集団の拡大だけを先にコミットすると中間の commit で CI が赤になる（`/plan-review` 独立導出 §7）。

### Phase 1 — 母集団の拡張と配線

- [ ] `headingRefSourceDocs(snapshot)` を追加。述語は **`f.endsWith(".rs")` だけ**とし、md の腕が持つ除外接頭辞（`docs/superpowers/` / `workspace/` / `docs/adr/`）を**共有しない**。理由を doc コメントに書く——`docs/adr/` の除外は `ADR-adr-frozen-history` の「**ADR 本文**は決定日時点の世界の記述として凍結し」という**散文についての契約**であり、`docs/superpowers/` も #589 で非規範化された**文書**である。どちらもコードについては何も決めていない。該当する `.rs` は現状 0 件（`workspace/` は `WALK_EXCLUDE_PATHS` で走査以前に落ちる）ゆえ挙動の差も無い。**決まっていない契約を述語で主張しない**
- [ ] doc コメントに以下を書く
  - `.rs` を入れる理由（#921 で `view.rs` の参照を手で直したのに検査は緑だった・`.rs` 側に正準形が 27 件ある）
  - **Rust のテストコードを外さない**——`.test.mjs` を外す理由は「フィクスチャが赤経路のため意図的に偽の名前を持つ」ことであり、Rust のテストコメントに書かれた参照は本物である。実際 (a) は `#[cfg(test)]` の内側にあった。`productionOnly()` 相当を「対称性の完成」として後から入れると、この腐りが検出できなくなる
  - `.mjs` / `.ps1` を入れない理由（実測: `.mjs` の finding 9 件中 6 件が自分のフィクスチャ、2 件が検出器自身の説明コメント内の例示。**検出器の説明が検出器を赤にする**型）
  - md と腕を分ける理由（0 件検知が腕ごとに要る。束ねると md 48 件が長さを埋めて `.rs` の消滅が隠れる——`staleDocs` / `staleGuides` と同型）
  - **受容する残余 2 つ**: (i) rustdoc のコードフェンス（`/// ` + ```` ``` ````）は `linesOutsideFences` の `/^\s*```/` に当たらないため、rustdoc の例の中に書かれた参照も照合される（実リポジトリでの影響は今日 0 件）。(ii) `#[cfg(test)]` の中のコメントも母集団に入る——`G-stale-identifiers` が `productionOnly` でテストを外すのと**意図的に非対称**である
- [ ] `headingRefDocs` の doc コメントも直す——「見出し参照はガバナンス文書の外にも書かれ」の列挙が `.md` 前提のままになっている
- [ ] `buildChecks`: `sink.refSourceDocs` を追加し、G-heading-refs / G-near-heading-refs の引数を `[...refDocs, ...refSourceDocs]` にする
- [ ] `runAll`: `ctx.refSourceDocs.length === 0` の明示 fail を追加
- [ ] `runAll` の `evidence`: 「見出し参照 N 件を md M 件 + `.rs` K 件から照合」の形へ。**件数は `ctx.refSourceDocs.length` を使い、evidence 側で母集団を再フィルタしない**（`/dry-check`——rules の述語が既に 4 箇所へ写っている前例がある）
- [ ] `headingRefDocs` の doc コメントへ「md の腕である」ことと分離の理由を追記
- [ ] **`npm run governance:check` を実行し、finding 2 件（(a) と (b)）で赤になることを観測**——出力を下の「実測ログ」へ貼る（新設した検証経路を、報告の前にその経路自体で走らせる）

### Phase 2 — 検出器の効きをフォールトインジェクションで固定する（欠落のパターンごとに 1 本）

`scripts/governance-check.test.mjs` へ、合成スナップショット（`snap({...})`）に種を蒔く形で追加する。ライブの検査は弱めない。

**欠落のパターンごとに 1 本ずつ**（`.claude/rules/safety-nets.md`「検出器のカバー範囲は、欠落のパターンごとに検算する」）。

- [ ] **種 1（着地しない）**: `.rs` が指す見出しを改題した複製で赤。**対照**: 着地する参照で緑・`checked` が 1 進む
- [ ] **種 2（対象が解決できない）**: `.rs` が消えたパスを指す形で赤
- [ ] **種 3（`#[cfg(test)]` カナリア）**: `#[cfg(test)]` の内側に置いた参照でも腐りが赤になる。**このテストが落ちるのは、誰かが `.rs` 母集団から Rust テストコードを外したときである**——(a) の実データがまさにその位置（`visual.rs:351` 以降）にあることをコメントに書く
- [ ] **種 4（G-near-heading-refs も `.rs` を見る）**: 助詞が挟まった近傍形を `.rs` に置いて赤。**実リポジトリでは `.rs` の近傍参照が 0 件なので、射程が広がったことは fixture でしか示せない**
- [ ] **種 5（`.rs` の母集団が 0 件）**: md が非空でも `.rs` が 0 件なら `runAll` が明示 fail
- [ ] **種 6（`.md` の母集団が 0 件）**: `.rs` が非空でも鳴る。**種 5 と別の `it` にする**——束ねると片方が他方を埋める
- [ ] **種 7（不混入）**: `.ts` / `.mjs` / `.ps1` / `.toml` / `.md` は `.rs` の腕に入らない。**除外接頭辞の `.rs` 版は fixture で固定しない**——md の腕の契約であって `.rs` について何も決まっていないため（Phase 1）。決めていない契約を fixture で凍らせない
- [ ] **種 8（配線カナリア）**: `buildChecks` / `runAll` 経由で `.rs` の腐りが findings に出る（走査元を渡し忘れたら落ちる）
- [ ] 既存の母集団カナリア（:860）を更新——`headingRefDocs` は **md の腕**であり `src/main.rs` を含まない、という主張は**維持する**（題も「md の腕」と分かる形へ）。そのうえで `headingRefSourceDocs(s)` が `src/main.rs` を返すことを追加で主張する。**`src/main.rs` は負のカナリアから正のカナリアへ役割が変わる**ので、md 側の「判定対象外が混じらない」枠を別の非 md（`Cargo.toml` 等）で張り直す
- [ ] `npx vitest run scripts/governance-check.test.mjs` が緑

### Phase 3 — 検出された 2 件を直す

- [ ] (a) `visual.rs:395` の参照を現行見出し「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」へ直す
- [ ] (b) `monitor.rs:80` を書き換える。参照は `` `SPEC.md` §4.7「4.7 結果表示制御（2 窓構成）」``（**番号を `「」` の内側に含める**——`.rs` に既に 7 件・生きた `.md`（`docs/architecture.md:82` 他）にも在る優勢な形で、改番でも赤くなる）。**本文の言明はこのコメントの要点なので落とさず、`「」` を外した散文として正準形の外に残す**（`/plan-review` 独立導出 §3.2 の案 B は 2 つ目の `「」` を残す形だが、それは AC2 が消したい曖昧さを字面上は残す——`「」` を使わなければ「見出しの指し」と「本文の引用」が構造で分かれる）。形の例:

```rust
/// だけでバーが隣モニターへ飛ぶ**（バーの位置を行の出没で動かさない規則に反する・
/// `SPEC.md` §4.7「4.7 結果表示制御（2 窓構成）」）。
```

**複製へ両方の置換を当てて実測済み**（2026-08-04）: `.rs` 27 件照合 / finding 0、近傍 0 件 / finding 0。さらに「将来 SPEC §4.7 にこの言明の太字リードが足された」複製でも 27/0・0/0 のまま（この形は 2 つ目の `「」` を持たないので、アンカーが増えても近傍検査と無関係でいられる）
- [ ] `npm run governance:check` が緑（`evidence` の照合件数が md 116 + `.rs` 27 相当へ増えている）

### Phase 4 — 古くなる記述を直す

- [ ] `docs/adr/ADR-canonical-heading-references.md` の帰結（:31「母集団は追跡下の全 `*.md` から…」）へ日付つき追記（先例: :29 の「2026-07-26 追記」）。**否定の知識をここに置く**——却下した 3 案（`.mjs` / `.ps1` へ広げる・`SPEC.md` に太字リードを足して本文引用を着地させる・検出器で見出し引用と本文引用を記法で分ける）と、それぞれの理由。**受容する残余も 1 行**——正準形の規範は `.rs` の書き手へ配送されない（`paths` に `.rs` を足さない裁定・2026-08-04）ため、記憶からの言い換えは事前には防がれず、CI の赤が事後に捕まえる
- [ ] `.claude/rules/governance-docs.md` へ 2 文（面積は rules 9879/12000 で余裕あり）
  - 正準形の射程がガバナンス文書間に読める箇所を正し、コード（`.rs`）のコメントからの参照も走査元であることを示す
  - :22 の「これらの編集に PostToolUse hook 検査は走らない」の段へ——**`.rs` では hook が走って沈黙するが、その沈黙は fmt / clippy / test の合格であって見出し参照の着地を含まない**（#497 型の false green の予防）
- [ ] `AGENTS.md`「条件別チェック（トリガー → 参照先）」の `governance:check` 行へ、`.rs` のコメントに正準形の参照を書く／その参照先の見出しを改題することもトリガーであることを足す（表が経路の SSOT であり、機構の後ろ盾が無い箇所）
- [ ] `npm run governance:check` を再実行（自分の追記が新しい参照を作るため）

## 不変条件と異常系

| 不変条件 | 壊れたときの検知 |
|---|---|
| md 側 116 件の照合結果は変わらない（判定本体を触らないため構造的に不変） | Phase 1 直後の `governance:check` が (a)(b) 以外の finding を出さないこと |
| `.rs` の腕が消えたら明示 fail（沈黙しない） | Phase 2 種 3 |
| Rust テストコードが母集団から落ちない | Phase 2 種 2 |
| 走査元の配線漏れ（母集団関数だけ作って渡し忘れ）で沈黙しない | Phase 2 種 4 |
| 免除注記の機構を導入しない（スクリプト冒頭の契約） | 除外リストを一切書かない |
| 正準形の `「」` は見出し名だけを持つ | (b) の書き換え。今後の違反は G-heading-refs が赤で示す |

**異常系**: `.rs` が読めない場合は `scanHeadingRefs` の既存経路（「対象文書が読めない（母集団の欠落）」）が finding を出す——新しい沈黙経路は作らない。

## テスト方針と検証コマンド

- `npm run governance:check` — Phase 1 で**赤 2 件**、Phase 3・4 で緑。`evidence` の件数を証拠として貼る
- `npx vitest run scripts/governance-check.test.mjs` — 種 4 本 + 更新したカナリア
- `npm test` — 全体（`.mjs` を触るため）
- `.rs` 編集の cargo 系検査は PostToolUse hook が自動実行する（**沈黙 = 合格**）。コメントのみの変更だが `snotra-settings` / `src-tauri` の両方に触るので、失敗が届かないことを確認する
- **フォールトインジェクションはローカルの vitest で完結する**（CI 待ちが要らないので PR 本文へ送らない）
- **PR 本文のチェックリストへ送る 1 項目**: CI の governance-check job が `.rs` を含む母集団で緑になること。`ci.yml` は `pull_request` でしか起動せず、計画の検証項目に置くと `gh pr create` の未チェックガード（#749）と循環する（`.claude/rules/safety-nets.md`「検出器のカバー範囲は、欠落のパターンごとに検算する」）

## SPEC.md・関連文書の更新要否

- `SPEC.md`: **不要**（挙動を変えない・(b) は参照側の記法の修正）
- `docs/build-commands.md`: **不要**（検査の一覧は `scripts/governance-check.mjs` のコメント見出しが SSOT。検査 ID は増えない）
- `docs/adr/ADR-canonical-heading-references.md`: **要**（Phase 4）
- `.claude/rules/governance-docs.md`: **要**（Phase 4・1 文）
- `docs/adr/ADR-adr-frozen-history.md`: **不要**（`docs/adr/` 配下に `.rs` は無く、除外の主張は成り立ったまま）
- `docs/adr/ADR-canonical-source-without-pointer-indirection.md` :32: **不要（検算した）**——「照合される形」に正準形の見出し参照が、「無い形」に **rustdoc からの*パス*** が挙がっている。`.rs` を足しても G-references（パスの実在）の母集団は md のままなので、この区別は真のまま
- `.claude/skills/retrospective/SKILL.md` :105・`docs/adr/ADR-adr-frozen-history.md` :9・`docs/adr/ADR-doc-promise-over-area-ratchet.md` :16・`docs/build-commands.md` :130: **不要**（いずれも母集団を md に限る主張をしていない）

## 却下した案（理由つき）

- **`governanceDocs()` に `.rs` を足す**（issue の一次証拠が指す形）: 却下。G-heading-refs はそこを見ていない。目的を果たさないまま G-references / G-spec-sections の母集団だけを未測定に広げる
- **`.mjs` / `.ps1` へ広げる**: 却下（ユーザー裁定）。実測で `.mjs` の finding 9 件中 8 件が検出器自身のフィクスチャと説明コメント
- **`SPEC.md` §4.7 の該当 bullet へ太字リードを足して (b) を着地させる**: 却下（ユーザー裁定）。SPEC の見出し構造がコードコメントの引用文言に従属し、「本文の言明には太字リードを与える」という規範条項の新設も要る
- **検出器で見出し引用と本文引用を記法で分ける**: 却下（ユーザー裁定）。正準形が 2 系統になり、既存 116 件すべてに新しい判定が要る
- **G-spec-sections を `.rs` へ広げる**: 見送り（ユーザー裁定）。実測 31 件・finding 0 件。検出器 2 本ぶんの種が要り PR の目的が 2 つになる
- **`adrCitationDocs`（:1486）のソース腕と母集団関数を共有する**: 却下（`/dry-check`）。述語の差（`.mjs` の有無・`.test.mjs` の扱い）は**別々の裁定の結果**である——G-adr-citations は `.mjs` を必要とし（検出器自身が ADR を短縮名で引く）、G-heading-refs は実測を根拠に `.mjs` を外す。畳むと 2 つの概念が 1 つの表層形に同居する（ルート `CLAUDE.md`「検証の作法」の「同じ表層形が複数の概念を担っていないか」）
- **`headingRefDocs` の述語へ `|| f.endsWith(".rs")` を足すだけ（関数を増やさない）**: 却下。1 行で済み一見単純だが、`runAll` の 0 件検知が `refDocs.length === 0` の 1 本しかないため、**md 48 件が長さを埋めて `.rs` の腕の消滅が永久に沈黙する**。`staleDocs` / `staleGuides` を分けた先例（`runAll` のコメント「グロブ由来の母集団ごとに 1 本ずつ要る——束ねると片方が埋めた長さで他方の消滅が隠れる」）と同じ理由で腕を分ける
- **モジュール rules（`snotra-core.md` / `src-tauri.md` / `snotra-settings.md`）へ「コードのコメントでも正準形を使え」と書く**: 却下。3 箇所の写しになる。赤が自己説明的であり、射程の宣言は `governance-docs.md` 1 箇所で足りる

## 実測ログ（実装中に埋める）

- Phase 1 直後の `governance:check`: <貼る>
- Phase 3 後の `governance:check`: <貼る>

## 未確定（実装前に潰す）

- [x] **正準形の規範を `.rs` の書き手へ配送するか** → **手当てしない**（ユーザー裁定・2026-08-04）。機構の赤（governance:check）が事後に捕まえるので受容する残余とし、ADR の追記節に名指しで残す。実測: `.claude/rules/governance-docs.md` の `paths` は `AGENTS.md` / `CLAUDE.md` / `docs/adr/**` / `scripts/*.{mjs,ps1}` / `scripts/lib/**` で、**`.rs` では 1 度も配送されない**。(a) の腐りはこの穴の産物である（改題への追随漏れではなく、規範を知らないまま記憶で言い換えた——下記「(a) の由来」）
  - 選択肢 A: 手当てしない（**推奨**）。機構の赤が事後に捕まえる。`governance-docs.md` は 1677 字で、全 Rust 編集へ配送すると文書ガバナンス固有の内容が常に注意の面積を取る
  - 選択肢 B: `governance-docs.md` の `paths` へ `**/*.rs` を足す（配送 1677 字 × 全 Rust 編集）
  - 選択肢 C: crate 別 rules（`snotra-core.md` / `snotra-settings.md` / `src-tauri.md`）へ 1 行のポインタ。**`snotra-egui-runtime/**/*.rs` を覆う rule が存在しない**（`.claude/rules/` 7 ファイルの `paths` を実測）ので、その crate だけ穴が残る＋ 3 箇所の写しになる

## (a) の由来（issue の因果説明の訂正・実測）

issue は (a) を「改題に追随していない」と書くが、`git log -S` で追うと違う。

- 見出しの改題（「故障注入では、…」→「フォールトインジェクションでは、…」）は `905edaf`（#623・2026-07-20）
- `visual.rs` の当該参照が書かれたのは `3acef09`（#826・2026-07-28）＝**改題より後**であり、旧見出しとも一致しない**言い換え**

つまり生まれたときから見出し名ではなかった。**直し方は同じだが根本原因が違う**——PR 本文へ issue の因果説明を写さない。

## セルフレビュー

- リスク: **高**（rules・ガバナンス文書・CI が走る検出器を変更する）
- plan-review: **独立導出（Step 2b）1 体**。成果物は `workspace/plan-review-heading-refs-rs.md`
- エージェント数: 1
- 要対処: 9 件。うち 6 件を計画へ反映（0 件検知の 2 本化は反映済み・除外接頭辞の共有述語と種 7・種 4 の近傍カナリア・`monitor.rs` の案 B・`AGENTS.md` のトリガー行・`governance-docs.md` :22 の hook 沈黙・(a) の由来訂正・同一コミット）。1 件（rules 配送の穴）は**未確定欄へ送りユーザーの裁定を待つ**。2 件（走査元の同定・テスト :860 の更新）は計画と一致していた
- 降格: `docs/build-commands.md` :133 の hook 沈黙の記述は**再照合して不要と判定**——「PostToolUse フックは `.md` に検査を割り当てない」と `.md` へ明示的にスコープしており偽にならない
- 差異（採らなかった提案）: (1) `headingRefDocs` を「2 母集団の和」にする案は採らず、md の腕のまま残して兄弟を足す（既存テスト :860 の主張が偽にならず、名前も嘘にならない）。(2) 除外接頭辞を両腕で共有する案は採らない——`ADR-adr-frozen-history` の凍結は**ADR 本文**についての契約であり、コードについては何も決まっていない。(3) `monitor.rs` は案 B ではなく `「」` を使わない形（案 B′）——実測で両案とも finding 0 だが、AC2 が消したい曖昧さを字面に残さない方を採る
- 未検証: `runAll` を通した end-to-end の実行（Phase 1 と Phase 3 で実施する）・CI での実測（PR 本文へ送る）

## 人間レビュー

- [x] 承認済み — 2026-08-04 / 問い: "`workspace/plan.md` を承認して実装（`/implement`）へ進んでよいですか。" / 回答: "承認する"
- 同時の裁定 — 問い: "正準形の規範（`.claude/rules/governance-docs.md`・1677 字）は `paths` に `.rs` を持たず、**Rust の書き手へ 1 度も配送されません**。…エージェント設定の変更に当たるため裁定をお願いします。" / 回答: "手当てしない（推奨）"
