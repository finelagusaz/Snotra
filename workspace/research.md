# research — #925 見出し参照検査の母集団へ `.rs` を足す

## issue の要約

`governance:check` の見出し参照検査（G-heading-refs / G-near-heading-refs）の走査元に `.rs` が入っておらず、`.rs` のコメントに書かれた正準形 `` `<対象>`「<見出し>」 `` は参照先が改題・移動・削除されても沈黙する。#921 では実際に `view.rs` の参照を手で直す必要があり、検査は緑のままだった。

## **issue の一次証拠にある誤り（実装者が最初に踏む罠）**

issue は根拠として `governanceDocs()`（`scripts/governance-check.mjs:1111`）の述語を引いているが、**G-heading-refs / G-near-heading-refs が走査元に使うのは `headingRefDocs()`（同 :1131）である**（配線は `buildChecks` :1552 / :1554）。`governanceDocs()` は G-references / G-spec-sections の母集団であり、ここへ `.rs` を足すと

- G-heading-refs は**盲のまま**（issue の目的を果たさない）
- 一方で G-references（バッククォート内パスの実在）と G-spec-sections（SPEC §N の実在）が 90 件の `.rs` を突然走査し始める（結果は未測定）

という、**何かをしたように見えて目的を外す**変更になる。触るのは `headingRefDocs()` の側である。

## 実測（2026-08-04・main `9ebf3db` の作業ツリー）

`scripts/governance-check.mjs` の `scanHeadingRefs` / `scanNearHeadingRefs` を **import して母集団だけ差し替えて**測った（ライブの検査は変更していない）。

| 母集団候補 | ファイル数 | heading 照合 / finding | near 照合 / finding |
|---|---|---|---|
| `.md`（現行） | 48 | 116 / 0 | 13 / 0 |
| `.rs` | 90 | 27 / **2** | 0 / 0 |
| `.mjs` | 13 | 29 / **9** | 10 / 4 |
| `.ts`/`.tsx` | 1 | 0 / 0 | 0 / 0 |
| `.ps1` | 12 | 5 / 0 | 2 / **1** |
| `.yml` / `.toml` | 7 / 5 | 0 / 0 | 0 / 0 |

### `.rs` の finding 2 件（issue 記載と一致）

- **(a) 真の腐り** — `snotra-settings/src/tabs/visual.rs:395` が `.claude/rules/safety-nets.md`「稼働中のガードを弱めず複製に変異を当てる」を指すが、実際の見出しは「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」。**ただし issue の言う「改題に追随していない」は誤りである**（`git log -S` で実測）——改題は `905edaf`（#623・2026-07-20）、この参照が書かれたのは `3acef09`（#826・2026-07-28）で**改題より後**であり、旧見出しとも一致しない言い換えである。生まれたときから見出し名ではなかった。
- **(b) 見出し名でなく本文の言明の引用** — `src-tauri/src/monitor.rs:80` の `` `SPEC.md` §4.7「バーの位置は行の出没で動かさない」``。§4.7 の見出しは「4.7 結果表示制御（2 窓構成）」であり、`「」` の中身は本文 bullet（「show 時の位置はバー高だけで決まる…」）の言い換えである。

### `.mjs` を入れない判断の一次証拠

finding 9 件の内訳（実測）:

- 6 件 = `scripts/governance-check.test.mjs` の**フィクスチャ**（赤経路を測るため意図的に実在しない名前を持つ。G-adr-citations が `*.test.mjs` を母集団から外したのと同じ理由）
- 2 件 = `scripts/governance-check.mjs` 自身の**説明コメント内の例示**（:1029 の隣接形 vs 非隣接形の対比、:1125 の「実際にそこで腐っていた」実例としての死んだ参照）
- 1 件 = `:320` の `docs/development-principles.md`「6. 検出は構造化された信号で行い…」（`…` で切り詰めた表記ゆえ前方一致が外れる）

つまり `.mjs` を入れると**検出器自身の説明が検出器を赤にする**型（`docs/adr/` を全検査の走査元から外した `ADR-adr-frozen-history` と同クラス）を招く。**ユーザー裁定（2026-08-04）で `.rs` のみに決定。**

### `.rs` 側の構造的事実（誤検出源の下見）

- `.rs` に `^\s*` で始まる ``` 行は **0 件** → `linesOutsideFences` の状態機械は `.rs` では常に「フェンス外」になる（rustdoc の ```` /// ``` ```` は `///` が前置されるため一致しない）。フェンスの取りこぼし・取りすぎは起きない。
- `.rs` 中の `「…」` を含む行は 333 件あるが、正準形（バッククォート直後）に当たるのは 27 件で、うち 25 件は着地する。UI 文字列リテラルの `「」` は誤検出源になっていない。
- **(a) の `visual.rs:395` は `#[cfg(test)]`（同ファイル :351）の内側にある。** `.rs` 母集団から Rust のテストコードを外すと、issue の受け入れ条件 4 が要求する当の 1 件が消える。

### 見送る隣接論点の実測（別 issue 候補）

- **G-spec-sections の `.rs` 拡張**: `.rs` 中の `SPEC §N` 参照は 31 件で finding 0 件。足しても今日は赤くならないが、検出器 2 本ぶんのフォールトインジェクションが要り PR の目的が 2 つになる。**ユーザー裁定で見送り。**
- **`.ps1`**: `scripts/manual-smoke.ps1:73` に近傍形が 1 件（`.claude/rules/governance-docs.md`【 の】「序数で他を指してはならない」）。母集団に入れれば拾えるが、腕ごとに 0 件カナリアと種が要る。**見送り。**

## 関連ファイル・シンボル（すべて grep で実在確認済み）

| パス | シンボル / 行 | 役割 |
|---|---|---|
| `scripts/governance-check.mjs` | `headingRefDocs` :1131 | G-heading-refs / G-near-heading-refs の走査元（**変更対象**） |
| 同 | `buildChecks` :1521・:1552・:1554 | 走査元の配線・`sink.refDocs` |
| 同 | `runAll` :1558・:1563 | 母集団 0 件の明示 fail・`evidence` 文字列 |
| 同 | `scanHeadingRefs` :977 / `scanNearHeadingRefs` :1060 | 判定本体（**変更しない**） |
| 同 | `governanceDocs` :1111 | G-references / G-spec-sections の母集団（**触らない**——上記「issue の誤り」） |
| 同 | `adrCitationDocs` :1478 | 先例: `.rs`/`.mjs` をコード側母集団に持つ唯一の既存検査 |
| `scripts/governance-check.test.mjs` | :860「母集団は履歴資料・作業バッファ・凍結された歴史（docs/adr/）を除く全 md」 | **`src/main.rs` が除外されることを現に主張している**カナリア（更新が要る） |
| 同 | :801 `describe("G-heading-refs …")` | 種の書き方の手本（複製への変異） |
| `snotra-settings/src/tabs/visual.rs` | :395（`#[cfg(test)]` 内） | (a) 真の腐り |
| `src-tauri/src/monitor.rs` | :80（`point_monitor_work_area` の doc） | (b) 本文引用 |
| `docs/adr/ADR-canonical-heading-references.md` | :31「母集団は追跡下の全 `*.md` から…」 | **この変更で古くなる記述**（追記が要る） |
| `.claude/rules/governance-docs.md` | :15 / :19 | 正準形の規範。射程が md 間と読める |

## 再利用できる既存パターン

1. **腕ごとに 0 件カナリアを 1 本ずつ置く** — `runAll` :1564-1569 の `staleDocs` / `staleGuides` が先例（「束ねると片方が埋めた長さで他方の消滅が隠れる」）。`refDocs` の `=== 0` 判定は md が 48 件ある限り沈黙するので、`.rs` の腕には別の判定が要る。
2. **母集団カナリアをテストで固定する** — :909「母集団カナリア: `adrCitationDocs` は `docs/adr/` を明示的に含む」と同型。
3. **フォールトインジェクションは合成スナップショット（`snap({...})`）への変異** — `.claude/rules/safety-nets.md`「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」。CI を待たずローカルの vitest で完結する。
4. **正準形で SPEC の節を指す形は `.rs` に既に 7 件ある** — `strings.rs:20`・`search_state.rs:265` 等が `` `SPEC.md`「4.7 結果表示制御（2 窓構成）」`` のように**番号を `「」` の内側に含める**。生きた `.md`（`SPEC.md:193` 他）も同じ形。この形なら改番でも赤くなる（`§4.7「見出し」` の形は番号が照合対象外になる）。

## 技術的制約

- **`.claude/rules/safety-nets.md` の管轄**（検出器の変更）。「効いていることを一度は実測する」「稼働中のガードを弱めず複製に変異を当てる」「検出器のカバー範囲は欠落のパターンごとに検算する」が要求される。
- **免除注記の機構を設けない**（`scripts/governance-check.mjs` 冒頭の契約）。`.rs` の特定ファイルを除外リストで逃がす解は取れない。
- **PostToolUse hook は `.mjs` / `.md` に検査を割り当てない**（沈黙は「何も走らなかった」）。`.rs` を編集すると cargo 系が走る。
- 面積予算に余裕あり（実測 常時ロード 12958/15500・rules 9879/12000）。

## 未解決の疑問

なし（下記はユーザー裁定済み）。

- 母集団の広さ → **`.rs` のみ**
- (b) の扱い → **参照側を正準形へ直す**（検出器も SPEC も規範も変えない）
- G-spec-sections の `.rs` 拡張 → **含めない**
