# research — issue #1172 全角の閉じ括弧で intra-doc link が沈黙する

## issue の要約

doc コメントの intra-doc link で開きが ASCII `[`・閉じが全角 `］` だと、rustdoc はリンクと認識せずリテラル出力する。`broken_intra_doc_links = "deny"` は「壊れたリンク」を赤にするが「リンクですらないもの」には黙る。PR #1171 で 3 本存在した状態で `cargo doc` / clippy / test / `governance:check` / PostToolUse がすべて緑だった。見つけたのは `/code-review` の 2 巡目。

要求（issue 本文 + 2026-08-25 の更新）:

1. `governance:check` に検査 `G-<name>` を 1 つ足す。混在（`[`…`］` と `［`…`]`）を赤にする
2. 母集団は `docs/comment-guidelines.md`「名指しと正本の指名」の表で「着地を検査する」が ○ の面 ＝ production の `///` / `//!`。`#[cfg(test)]` 内と `//` インラインは × 宣言済みなので赤にする理由が無い
3. `#[cfg(test)]` の境界を**絞るか絞らないか**を選び、理由を検査の doc へ書く
4. 受け入れ条件: (a) production の doc へ注入して赤（フォールトインジェクション実測）(b) 現行ツリーで誤検出 0 (c) 検査の doc に射程と死角、および上の表のどの行を守るかを書く
5. rust-analyzer は代替にならない（issue が実測済み。再測不要）

## 関連ファイル・モジュール・関数（実在を grep で確認済み）

| 対象 | 役割 |
|---|---|
| `scripts/governance/registry.mjs` `checkModulesFrom` | `checks/` 直下の `*.mjs` を走査して検査に登録する。**登録行は存在しない**——ファイルを置けば検査になる。`id` と basename の一致を throw で強制 |
| `scripts/governance/checks/G-folded-code-spans.mjs` | 同族の隣接検査。`run(snapshot, ctx)` → `ctx.record(key, {findings, checked})`。`.rs` は `linesOfComments(text, file, "js")` でコメント行だけを取る（`refScanLines` は `.rs` を全行へ落とすので使わない・#489） |
| `scripts/governance/lib.mjs` `linesOfComments(text, file, family)` | コメント行を `[lineNo, text]` で返す。`"js"` 族は `//` 始まり（`///` / `//!` を含む）と `/* */` ブロック。**`///` と `//` を区別しない**——区別は呼び出し側で行う |
| `scripts/governance/lib.mjs` `allHeadingRefDocs(snapshot)` | `.md` + `.rs` + コメント記法スクリプトの和。`buildChecks` が `ctx.allRefDocs` として渡す |
| `scripts/governance-check.mjs` `buildChecks` / `runAll` | `ctx.record` は `sink[key] = r.checked` を積む。evidence は `evidenceView` 越しに読み、**読んだキーが未記録なら finding**（供給断の検知）。記録したが読まないキーには何も起きない |
| `scripts/governance/evidence.mjs` `assembleEvidence` | 要約行の文字列。`codeSpans` 等のキーを名指しで読む。新キーを載せるならここへ 1 句足す |
| `scripts/governance/edit-findings.mjs` `SCAN_SCOPED` | 編集時 reminder の判定一覧。`{population, check}` の配列。`G-folded-code-spans` は `checkFoldedCodeSpans` として登録済み |
| `scripts/governance/checks/G-edit-findings-table.mjs` | `SCAN_SCOPED` の判定名集合と `docs/hooks.md`「検査ではない reminder（発火一覧に現れない）」の表を照合する。**片方だけ足すと赤** |
| `docs/hooks.md` 101〜115 行 | 上の表。`checkFoldedCodeSpans` の行が書式の手本 |
| `scripts/governance-manifest.mjs` `undeclared` | 検査 ID の集合が main と差分を持つと、**PR 本文に `G-<name>` が逐語で現れる**ことを要求する（書式は自由） |
| `scripts/governance/test-helpers.mjs` `snap(contents)` | テスト用 snapshot |
| `docs/comment-guidelines.md`「名指しと正本の指名」（80 行〜） | 守る表。既に「表の「○」が嘘になる形が 1 つある」として #1172 を名指し済み |

## 再利用できる既存パターン

- **検査モジュールの骨格**: `G-folded-code-spans.mjs` をそのまま型にする。`export const id`・`run`・`scanX`（`{findings, checked}`）・`checkX`（findings のみ、edit-time 用）。ヘッダに「宣言する死角——沈黙側 / 赤側」を分けて書く様式
- **`.rs` のコメント行取得**: `linesOfComments(text, file, "js")`。doc 行への絞り込みは呼び出し側で `line.trimStart().startsWith("///") || line.trimStart().startsWith("//!")` とする。**返る行は `raw`（インデント込み・trim 前）である**——`lib.mjs` は判定にだけ `trim()` 済みの `line` を使い、`out.push([i + 1, raw])` する（2026-09-02 実測。当初の調査は「trim 済み」と誤記しており、敵対的調査がこれを壊した。`trimStart()` 無しでは実ツリーの大半を占めるインデント済み `///` 行を全滅で見逃し、フォールトインジェクションが注入先しだいで偽の緑になる）
- **テスト**: `G-folded-code-spans.test.mjs` の「赤 / 緑 / 証跡（checked）」の 3 節構成
- **edit-time への相乗り**: `SCAN_SCOPED` へ 1 行 + `docs/hooks.md` の表へ 1 行（`G-edit-findings-table` が対応を強制）

## 技術的制約

- **述語は 1 物理行の中だけを見る**（`G-folded-code-spans` と同じ判断。行を跨ぐ角括弧は intra-doc link として rustdoc も解決しない）。プローブ済み: `/\[[^\[\]［］\n]*］|［[^\[\]［］\n]*\]/g` は issue の実例 2 件・逆向き・素のリンク形 `[Type::method］` に当たり、正しい `[`X`]`・全角対 `［注意］`・`` `[今すぐ更新]` `` には当たらない（2026-09-02 node で実測）
- **母集団は `.rs` の doc 行だけ**にする。`.md`（`[`x`]` 単独はリンクではない）・`.mjs` / `.ps1`（TSDoc は `{@link}`、intra-doc link の概念が無い）では「表の ○ が嘘になる」構造が無いので、issue の更新後の要求どおり production の `///` / `//!` に限る
- **`#[cfg(test)]` は絞らない**（issue の 2 択の後者）。理由: (1) テキスト走査で `mod tests` の入れ子と個別アイテムの `#[cfg(test)]` を判定するのは非自明で、誤った境界は**沈黙側**へ倒れうる (2) `#[cfg(test)]` 内の検出は「直して困らない過剰」であり、表の × は「保証が無い」であって「書いてよい」ではない。この判断を検査の doc へ書く
- **現行ツリーの誤検出**: `［` / `］` は `*.rs` / `*.mjs` / `*.ps1` / `*.psm1` / `*.ts` / `*.md` のいずれにも 0 件（2026-09-02 `git grep` 実測）。よって実装後の `governance:check` は 0 件でなければならず、1 件でも出れば述語の誤りである
- **`checks/` は自分自身の走査母集団ではない**（この検査は `.rs` だけを見る）ので、`G-folded-code-spans` が抱える「ヘッダの例示に実在の形を置かない」制約は掛からない。ただし `.md` のテスト fixture は文字列リテラルなので無関係
- **manifest**: 検査 ID が 1 つ増えるので、PR 本文に新 ID を逐語で書く
- **フォールトインジェクション**: production の `///` へ注入して赤、`#[cfg(test)]` 内へ注入しても赤（絞らない設計なので）。**巻き戻しを SHA256 で照合**する（#1171 の手順）。注入先は `src-tauri/src/egui_shell/window_coordinator.rs` の既存 doc 行（#1171 で `］` を消した当のファイル）
- **PostToolUse**: `scripts/*.mjs` の編集は `selectChecks` が検査を割り当てる（`node-check` 相当）。`docs/hooks.md` / `docs/comment-guidelines.md` の編集は沈黙＝何も走らない。`npm run governance:check` と `npm test` を手で打つ

## 解決した疑問

- `/** … */` 形の doc: `git grep -cE '^\s*/\*\*' -- '*.rs'` は **0 件**（2026-09-02）。doc 行の絞り込みは `///` / `//!` の行頭判定とし、`/** */` は**沈黙側の死角**として検査の doc に宣言する
- evidence 要約行へ新キーを載せる（`G-folded-code-spans` と同じ扱い・#497 の趣旨で「照合していない」と「0 件」を区別する）。`assembleEvidence` の 1 句追加を作業項目に含める

## 敵対的調査（`workspace/adversarial-1172.txt`）の採否

| 所見 | 採否 | 理由 |
|---|---|---|
| 【重大】`linesOfComments` は trim 前の `raw` を返す。`startsWith("///")` はインデント済み doc 行を全滅で見逃す | **採る** | 主エージェントも並行して `out.push([i + 1, raw])` を読んで同じ結論に到達。計画の述語へ `trimStart()` を入れ、**インデント済み `///` を fixture に持つ赤テスト**を必須にする（この誤りを機構で再発不能にする） |
| `#[cfg(test)]` 内への注入も同じ理由で沈黙しうる | 採る（上の帰結） | `trimStart()` で解消。Phase 3 の注入は**インデント済みの行**へ行うことを明記する |
| evidence の未読キーは何も起こさない（裏付け） | 採る | 要約行へ載せる方針は変えない |
| ⚠️ `git grep` は追跡ファイルだけを見る。`makeSnapshot` との母集団差は未追跡 `.rs` があれば生じる | 採る（射程の注記として） | 今のツリーで未追跡 `.rs` は 0 件。実装後の 0 件確認は `git grep` ではなく **`governance:check` の `checked` と findings** で行う——それが検査自身の母集団で測る唯一の形 |
| ⚠️ `〔〕` `【】` 等の他の全角括弧を含めるべきか | 却下（理由を doc へ書く） | それらは ASCII `[` `]` の互換文字ではなく、打鍵ミスで `[`…`〕` が生じる経路が無い。issue の実例も全角角括弧 `］` だけである。検査の doc の「沈黙側の死角」に**理由つきで**宣言する |

壊せなかった項目（報告の逐語ではなく要旨）: 混在正規表現の当たり方 11 パターン・現行ツリー `［` `］` 0 件（node のバイト走査で再現）・`checks/` に登録行が無いこと・manifest の逐語要求・`#[cfg(test)]` を絞らない理由。

## 未解決の疑問

- なし
