# 独立再導出 — #1172（全角の閉じ括弧で intra-doc link が沈黙する）

対象 issue: #1172
導出日: 2026-09-02（`workspace/plan.md` / `workspace/research.md` は読んでいない）
前提の sha: main `66fffe2`

## 1. 触るファイルとシンボルの一覧

### 新規

| ファイル | シンボル | 根拠 |
|---|---|---|
| `scripts/governance/checks/G-<name>.mjs`（例: `G-mixed-bracket-links`） | `export const id`（ファイル basename と一致必須）・`export function run(snapshot, ctx)`・`scanXxx(snapshot, docs) → { findings, checked }`・`checkXxx(snapshot, docs) → findings` | `registry.mjs` `checkModulesFrom` が `id` / `run` の export と「basename === id」を throw で強制する（`registry.mjs:24-29`）。`scan*` / `check*` の 2 段構えは兄弟 `G-folded-code-spans.mjs` の形（`scanFoldedCodeSpans` / `checkFoldedCodeSpans`） |
| `scripts/governance/checks/G-<name>.test.mjs` | 赤（`[`…`］` と `［`…`]`）／緑（`[`…`]`・`［`…`］`・コード行・`//` インライン）／母集団欠落（`docs` に無いパス → finding） | `governance-check.mjs` 契約「各検査は隣の `*.test.mjs` がフォールトインジェクション red / 正常 green / 判定対象外の不混入を検証する」。fixture は `test-helpers.mjs` の `snap(contents, extraFiles)` で作る |

### 変更（足さないと別の機構が赤になる／写しが腐る）

| ファイル | シンボル・箇所 | 何が起きるか |
|---|---|---|
| `scripts/governance/evidence.mjs` | `assembleEvidence` のテンプレート文字列に `${ev.<newKey>}` を 1 つ足す | **足さなくても赤にはならない**（テンプレートが読むキーの集合が母集団で、書かなければ照合されないだけ）。しかし「検査 N 件」の証跡に照合件数が載らず、`ctx.record` の供給が消えても誰も気づかない（#1098 の穴がこの 1 本だけ開く）。**足すのが筋** |
| `scripts/governance/evidence.test.mjs` | `complete()` fixture へ同じキーを追加 | テンプレートへ読みを足すと、`complete()` に無いキーは `"?"` を返して findings が 1 件立ち、**「緑: すべて記録済みなら finding は出ず」の it が落ちる**（`evidence.test.mjs:32-38`）。テンプレートを触るなら必須 |
| `docs/hooks.md`「検査ではない reminder（発火一覧に現れない）」の表 | 判定列に `` `check<Xxx>` `` を持つ行を 1 行追加 | **`SCAN_SCOPED` へ載せる場合のみ必須**。`G-edit-findings-table` は `SCAN_SCOPED[].check.name` と表の判定名の集合を双方向で照合するので、片方だけなら赤（`G-edit-findings-table.mjs` `scanEditFindingsTable` 末尾の 2 ループ） |
| `scripts/governance/edit-findings.mjs` | `SCAN_SCOPED` へ `{ population: headingRefSourceDocs, check: checkXxx }` を追加（＋ `lib.mjs` から `headingRefSourceDocs` を import） | 任意だが**載せるべき**（理由は §5）。`population(snapshot).includes(rel)` で編集 1 枚に絞るので、`.rs` 編集直後に鳴る |
| `scripts/governance/edit-findings.test.mjs` | 赤ケース 1 本（`a.rs` に混在括弧 → 1 件） | 既存の「赤: コードスパンが行をまたぐ（G-folded-code-spans）」（`:151`）と同型。`SCAN_SCOPED` へ載せたことの検知器になる |
| `docs/comment-guidelines.md`「名指しと正本の指名」 | `:100` の段落「**表の「○」が嘘になる形が 1 つある。**…（#1172）」 | 現状は「rustdoc は何も言わない」で終わっている。検査が入ったら「`governance:check` の `G-<name>` が赤にする」と着地先を書き換える。書かないと**規範が「機構は黙る」と言い続ける写し**になる |
| `docs/comment-guidelines.md`「rustdoc の様式」 `:109` | 「doc コメント内の非リンク角括弧は backtick で包む」の項 | ⚠️ 触らなくてよい可能性が高い。ただし新検査は `` `[…］` ``（バッククォート内）も赤にする設計なら、この条項との整合を一言添える（§6 の死角宣言に含める方が軽い） |
| PR 本文 | `## governance manifest delta` / `+G-<name>` | `scripts/governance-manifest.mjs` の `manifest().checks` は `buildChecks` の id 集合。CI の `governance manifest delta` step（`ci.yml:111-125`）が `PR_BODY` に `+G-<name>` の逐語を要求し、無ければ exit 1（`undeclared`）。**コードではなく PR 本文に書く** |

### 触らないと決めたもの

| ファイル | 理由 |
|---|---|
| `scripts/governance/registry.mjs` | 登録行は存在しない。`checks/` 直下の `*.mjs`（`.test.mjs` 除く）を `readdirSync` で拾う（`registry.mjs:20-23`）。置くだけでよい |
| `scripts/governance-check.mjs` | `buildChecks` は `CHECK_MODULES.map(...)` で全検査を組む。`ctx.record(key, {findings, checked})` は任意のキーを受ける（`:153-157`）。`runAll` の 0 件検知は母集団の腕ごとに既にある（`refSourceDocs`）。新しい腕を作らなければ足すものは無い |
| `scripts/governance/lib.mjs` | `linesOfComments(text, file, "js")` と `headingRefSourceDocs(snapshot)` を借りるだけで新関数は不要。**`COMMENT_FAMILY` へ `.rs` を足してはならない**（`refScanLines` の意味論が変わり見出し参照 3 検査の母集団が動く・`lib.mjs:257-265` の doc が明示） |
| `scripts/governance/dependents.mjs` / `instrument.mjs` | `.md` の節依存と面積計器。無関係 |
| `scripts/governance-check.test.mjs` / `registry.test.mjs` | 検査数・ID を定数で assert していない（`registry.test.mjs:9` は `length > 0`、`governance-check.test.mjs` は `G-area-instrument` の不在を見るだけ）。`evidence.test.mjs:9` の `checkCount: 19` は fixture の値であり実数と照合しない |
| `docs/build-commands.md` カテゴリ F | 検査の一覧を書かない設計（`:178`「検査の一覧は `checks/` ディレクトリの走査が SSOT」）。`:175` の「G-heading-refs / G-near-heading-refs の走査元に `.rs`」は本件で偽にならない |
| `.claude/rules/comments.md` `:24-25` | `cargo doc` を手で走らせる規範。本検査は `governance:check` 側なので写しは生じない。⚠️ ただし「`#[cfg(test)]` 配下の doc は `cargo doc` の視界の外」の隣に、本検査は cfg(test) 内も見る旨を添えるなら 1 行（§3 の判断に依る） |
| `docs/hooks.md` の PostToolUse 発火一覧 | `selectChecks` の id（fmt / clippy / test）の表。`G-*` は載らない |
| `.github/workflows/ci.yml` | `node scripts/governance-check.mjs` を叩くだけ。変更不要 |
| `Cargo.toml` / `G-workspace-lints.mjs` | `broken_intra_doc_links` の deny は既にあり、本件はその射程外の形を機構で補う話。触らない |
| `docs/adr/` | 否定の知識（却下した代替案）が生まれるなら新設候補だが、本件は issue が既に「RA は代替にならない」を持つ。⚠️ 「`#[cfg(test)]` を絞らない」判断を ADR にするか検査ヘッダに書くかは計画側の裁量。私は**検査ヘッダで足りる**と見る（`G-folded-code-spans` は ADR を持つが、あちらは述語の射程を実データで縮めた履歴がある） |

## 2. `.rs` の doc 行を取り出す既存関数と戻り値の形

**`scripts/governance/lib.mjs:266` `linesOfComments(text, file, family = commentFamilyOf(file))`**。

- `.rs` は `COMMENT_FAMILY` に無いので既定引数では throw する（実測: `linesOfComments: コメント記法を持たない対象（受け取った値: a.rs）`）。**第 3 引数に `"js"` を明示して借りる**——`G-folded-code-spans.mjs` の `spanScanLines` が同じ形（`file.endsWith(".rs") ? linesOfComments(text, file, "js") : ...`）
- 戻り値は `[lineNo(1 始まり), raw]` の配列。**判定は `raw.trim()` で行うが、返すのは trim 前の raw**（`lib.mjs:297,318` `const line = raw.trim(); ... out.push([i + 1, raw])`）。インデントが残る

実行（node、`family="js"`）:

```
入力:
pub struct A;
    /// 消費者は [`read_bar_anchor`］ を見る
    //! inner
fn f() {
    let x = 5; // tail
    /* block
       body */
}
#[cfg(test)]
mod tests {
    /// in test [`X`]
}

出力:
[[2,"    /// 消費者は [`read_bar_anchor`］ を見る"],[3,"    //! inner"],[6,"    /* block"],[7,"       body */"],[11,"    /// in test [`X`]"]]
```

含意:

- **`///` `//!` だけではなく `//` 行頭コメントと `/* */` ブロックも返る**。母集団を production doc へ絞るなら、新検査側で `trim()` して `startsWith("///") || startsWith("//!")` を掛ける必要がある（`////`（4 本）は Rust では doc ではないので `!startsWith("////")` も添える。現行ツリーで `////` は 0 行）
- 行末インラインコメント（`let x = 5; // tail`）は返らない。表で × の面なので望ましい
- rustdoc のコードフェンス内はマスクされない（`G-folded-code-spans` ヘッダが同じ残余を宣言）

## 3. `#[cfg(test)]` の内側を母集団から外すべきか

**結論: 絞らない（`///` `//!` 全体を見て、`#[cfg(test)]` 内の検出は無害な過剰として受容する）。**

根拠:

1. **テキスト走査で境界を切るのは非自明かつ二重に誤る。** `#[cfg(test)]` の付き方は 2 形あり（実測: `mod` 直前 73 箇所、個別アイテム 20 箇所）、しかもアイテム形では **doc コメントが属性より前に来る**（`/// doc` → `#[cfg(test)]` → `pub(crate) fn empty()`。`snotra-core/src/history.rs:110` 等）。「`#[cfg(test)]` 以降を落とす」変換では属性の前にある doc が production 扱いのまま残り、絞ったつもりで絞れていない
2. **既存の走査元は「テストを外さない」を意図として固定している。** `lib.mjs:623-634` `headingRefSourceDocs` の doc と `lib.test.mjs:111` 種 3（`#[cfg(test)]` の内側のコメントも見る）。`headingRefSourceDocs` を借りるなら変換を入れないのが整合的で、独自の母集団を切り出すと `runAll` の 0 件検知を 1 本増やす必要も出る
3. **過剰側の実害は無い。** 検出は「直せば消える」形で、cfg(test) 内で `［`…`]` を書き直しても壊れるものが無い。逆に issue の更新節が言うとおり、赤にならない側（production で黙る）が守りたい唯一の面であり、絞らない実装はその面を確実にカバーする
4. **書くべき散文**: 検査ヘッダに「守るのは `docs/comment-guidelines.md`「名指しと正本の指名」の表の 1 行目（production の `///` `//!`）。2 行目（`#[cfg(test)]` の中の `///`）も母集団に入るが、これは境界判定を持たないことによる過剰であって保証ではない」。フォールトインジェクションは **production の doc へ注入して**赤を見る（issue の追加条件）

規模の参考（実測・node で `///`/`//!` 行を数え、`#[cfg(test)]` の次行が `mod` のときだけ内側と判定）: production doc 7625 行（リンク形 521）、cfg(test) `mod` 内 1569 行（リンク形 96、15 ファイル）。

⚠️ 母集団を `headingRefSourceDocs`（全 `.rs`）にすると `snotra-core/tests/*.rs` 4 本と `build.rs` 2 本も入る。rustdoc は integration test と build script を文書化しないので、これも cfg(test) と同じ「無害な過剰」。`crateSourceFiles`（`<member>/src/` 配下）へ絞る手もあるが、lib.mjs はこれを `G-module-index` と畳んではならない 2 本目の導出と位置づけており、新しい消費者を足す理由が薄い。**`headingRefSourceDocs` を推す**

## 4. 現行ツリーの全角角括弧 `［` `］` の件数

**0 件**（追跡ファイル全体・`.rs` に限らず）。

```
node -e '... git ls-files 全件を読み /[［］]/g を数える ...'
→ TOTAL 0 {}
```

（`git grep -P "[［］]"` は Bash ツール経由で空を返したので、node で読み直して確定した。）

ゆえに受け入れ条件「現行ツリーで誤検出 0 件」は、どんな述語でも自明に満たす。**述語の妥当性は fixture と注入で測るしかない**——実データによる偽陽性の検算はここでは効かないことを計画へ書いておくべき。

## 5. `SCAN_SCOPED` へ載せるべきか

**載せるべき。** #992 が `checkFoldedCodeSpans` を載せた理由（`edit-findings.mjs` の `SCAN_SCOPED` 内コメント「着地先を持たない判定である……書いた瞬間に鳴ることが動機」）が逐語で当てはまる——本件も 1 枚の中で完結し、書いた瞬間に直せる形である。

載せた場合の連鎖:

- `G-edit-findings-table` が「編集時に走る判定が表に無い: `checkXxx`」で赤 → `docs/hooks.md`「検査ではない reminder」表へ 1 行（判定列は `` `checkXxx` `` のバッククォート必須。散文だと「判定列にバッククォート括りの判定名が無い」で赤）
- `docs/hooks.md` の表直後の前提「編集したファイルが `.rs` か `.md`」は本件（`.rs` のみ）で真のまま
- `edit-findings.mjs` の `population` は **`allHeadingRefDocs` ではなく `headingRefSourceDocs`** を渡す（`.md` / `.mjs` を編集しても呼ばれないよう構造で絞る。呼んでも `.rs` でなければ 0 件だが、`linesOfComments(…, "js")` を `.md` に当てると `//` 始まりの散文行を拾う経路が生まれる）

## 6. 述語と死角（検査 doc へ書くべき項目の素案）

- 述語: doc 行（trim 後 `///` or `//!`、`////` 除く）の中で `\[[^\[\]［］]*］` または `［[^\[\]［］]*\]` に当たる → finding。1 行 1 件、`checked` は doc 行数か `[`/`［` の開き数
- **沈黙側の死角**: 全角同士 `［…］`（リンクではなく、意図した表記として扱う）／`//` インライン（表で ×）／文字列リテラル内／行をまたいで開閉が割れたリンク（`[` と `］` が別行）／`.md` 側の `[text](url)`
- **赤側へ倒れる形**: rustdoc コードフェンス内・バッククォート内（`` `[a］` ``）でも赤。`docs/comment-guidelines.md:109`「非リンク角括弧は backtick で包む」で包んだ形も、混在なら赤にする（混在は常に打鍵ミスであり正当な用途が無い、と宣言できる）
- **例示の注意**: `checks/` 自身は本検査の母集団ではない（`.rs` のみ）ので、`G-folded-code-spans` と違い自己言及の罠は無い。ただし `.rs` の doc に例示を置くなら混在形を書けない

## ⚠️ 確信の持てない点

1. evidence テンプレートへ読みを足すか。足さない設計（`ctx.record` を呼ぶが印字しない）も「赤にならない」点では通るが、#1098 の供給断検知から外れる。**足す方を推す**が、載せる語（「混在括弧 N 件」等）は計画側で決める
2. `population` に `headingRefSourceDocs`（全 `.rs`）と `crateSourceFiles`（`src/` 配下）のどちらを使うか（§3 末尾）
3. ADR を新設するか検査ヘッダで足りるか（§1「触らない」の `docs/adr/`）
4. `.claude/rules/comments.md:25` へ「本検査は cfg(test) 内も見る」を添えるか——添えないと「cfg(test) 配下の doc はどの機構も見ない」と読まれうるが、添えると写しが 1 つ増える。**添えない**に傾く（正本は検査ヘッダと `docs/comment-guidelines.md` の表）
5. `git grep -P "[［］]"` が Bash ツール経由で空を返した原因は未特定（引数の文字コード変換の疑い）。件数 0 は node で確定しているので結論には影響しない
