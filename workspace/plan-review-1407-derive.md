# #1407 実装範囲の独立導出（plan-review 用）

base: chore/comment-guidelines-shrink（HEAD = 3a98f16c = issue の実測 main と同じ）。`workspace/` の他ファイルは読んでいない。
測定は scratch スクリプト（`docs/comment-guidelines.md` をメモリ上で差し替えた snapshot に `G-heading-refs` の `scanHeadingRefs` を `allHeadingRefDocs` で回す。ライブのファイルは触っていない）。

---

## 0. 測定: 見出しを消すと何が落ちるか

- 現状: checked=335 findings=0（issue の「335 件」と一致）
- 見出しを新設 1 本だけにした版: checked=330（本書自身の外向き参照 5 本が消える）、**findings=18（10 ファイル）**
- 推奨案（§5-1: 「名指しと正本の指名」「rustdoc の様式」「日本語の折返し」「言語（日英）」の 4 見出しを残す）: **findings=5**
  - `.claude/agents/code-reviewer.md:28`「第一原則」
  - `.claude/rules/comments.md:15`「第一原則」
  - `.claude/rules/comments.md:16`「短く保つ」
  - `PERFORMANCE.md:33`「歴史メモの様式」
  - `snotra-core/src/indexer/columns.rs:22`「第一原則: コメントは理由を書く」（**全形**。見出しの頭に `第一原則: コメントは理由を書く` を残さない限り赤）
- 近傍形（`comment-guidelines.md の「…」`）: 0 件（git grep）
- 本書の中身を読む機構（スクリプトが本書のテキストを入力にするもの）: 0 件。`scripts/` で本書を名指すのはコメントだけ（G-folded-code-spans.mjs:16,38 / G-folded-heading-refs.mjs:27,34 / G-fullwidth-doc-link-bracket.mjs:20）と、凍結フィクスチャ `G-stale-identifiers.test.mjs:233-249`（パス名を文字列で使うだけ・本文を読まない）。`G-area-budget` は撤去済み（`ADR-retire-area-budget`・`scripts/` に AREA_BUDGET 0 件）

18 本の全表（見出しを残しても中身を消すと「緑のまま偽」になる依存を右列に書く）:

| 参照元 | 指す見出し | 参照元が依存している中身 |
|---|---|---|
| .claude/rules/comments.md:15 | 第一原則 | 何を書き何を書かないか |
| .claude/rules/comments.md:16 | 短く保つ | 縮め方・測定値の置き場 |
| .claude/rules/comments.md:17 | 名指しと正本の指名 | 記法と面ごとの保証 |
| .claude/rules/comments.md:18 | rustdoc の様式 | `///` `//!` の様式と `# Why` 見出し |
| .claude/rules/comments.md:19 | 日本語の折返し | 折返し |
| .claude/rules/comments.md:20 | 言語（日英） | 訳語 |
| .claude/agents/code-reviewer.md:28 | 第一原則 | 「構造の事実の写し（数・列挙・呼び出し元・到達可能性）」の判定と、例外（一意性の宣言・誤読予防の名指し） |
| .claude/agents/code-reviewer.md:28 | 日本語の折返し | 「素の散文の折返しは指摘しない」 |
| .claude/agents/code-reviewer.md:29 | 言語（日英） | 判定 (A)/(B)・語義ごと |
| .claude/skills/start-issue/SKILL.md:84 | 言語（日英） | 判定 (A)/(B) |
| docs/glossary.md:8 | 言語（日英） | 判定 |
| PERFORMANCE.md:33 | 歴史メモの様式 | L163「実測値には条件を添える…条件のない数値は再検証できない」 |
| snotra-core/src/index_tree.rs:1126 | 名指しと正本の指名 | 面ごとの保証の表（`#[cfg(test)]` の行が ×） |
| snotra-core/src/indexer/columns.rs:22 | 第一原則: コメントは理由を書く | 「経路の数を書かないよう定めている」 |
| scripts/governance/checks/G-folded-code-spans.mjs:16 | 日本語の折返し | L126「2026-08-08 の実測」と `current_thread().id() == …` の例 |
| scripts/governance/checks/G-folded-code-spans.mjs:38 | 日本語の折返し | L118「機構の適用範囲と無関係に全ツリーへ掛かる」 |
| scripts/governance/checks/G-folded-heading-refs.mjs:34 | 日本語の折返し | 「規範だけでは足りず 6 箇所が実在」 |
| scripts/governance/checks/G-fullwidth-doc-link-bracket.mjs:20 | 名指しと正本の指名 | **表の 1 行（production `///` `//!` の「着地を検査する ○」）を守る**と宣言 |

---

## 1. 変更が必要なファイルと箇所

### 1-a 本体
- `docs/comment-guidelines.md` 全体（L1-188）
  - **置き換える（核）**: L9-35「第一原則」、L37-60「配置基準（3 層）」、L62-78「短く保つ」（「測定値の置き場」含む）、L154-165「歴史メモの様式」、L177-181「コメントと実装の同期」、L183-188「テストコードのコメント」→ 問い 1 つ「この行を単純にされたら、何が壊れるか」と答えごとの置き場の表へ
  - 表に要る行（issue の WHAT）: テストが落ちる→テスト名／落ちないが壊れる→直上に約束 1〜2 行／何も壊れない→書かない／どう至ったか→コミット・PR、コメントには `#NNN`
  - 原則: 理由は守る行の隣。例外は 1 か所にしか置けないもの（測定値。L72-78 の要点＝`ADR-measurement-canon-in-code-doc` を指す 1 行で足りる）
  - **現行 L67「同じ主張が別の面に既に在るなら指す」・L25/L82「`[X]` は正本はあちら」と、新原則「理由をあちらと指さない」を両立させる 1 文が要る**（§5-2）
  - L5（配送と ADR の指し）・L7（スコープ: 既存の一括書き直しはしない）は残すか 1 行へ
  - L3 は「rustdoc / TSDoc」を言う。TSDoc 節（L133-138）を消すなら直す
- 残す節の中の経緯（§5-1 の推奨で残す 4 節にも経緯がある）: L94-102（#1172・#1116・2026-08-25 実測）、L107（#1106・305 件）、L109（SU5・PR #647）、L114-131（#992・2026-08-08/08-20/08-22 実測）、L173-175（#1251・#1252・#1307）。**削るなら §2 の中身依存を先に見る**（G-folded-code-spans.mjs:16 は L126 の例に依存）

### 1-b 参照元（推奨案で落ちる 5 本＋中身依存）
- `.claude/rules/comments.md:15-16` — 「第一原則」「短く保つ」を新見出しへ。L11「要約は置かない」はそのまま
- `.claude/agents/code-reviewer.md:28` — 「第一原則」を新見出しへ。**判定と例外（一意性の宣言・誤読予防の名指し）の正本は第一原則**と書いているので、新しい本文がその例外を持たないならこの行の文言も直す（一意性の宣言は新しい表の「テストは落ちないが壊れる→約束」に写像できる）
- `snotra-core/src/indexer/columns.rs:22` — 全形の見出し参照。新見出しへ直すか、括弧内の「規約が経路の数を書かないよう定めているのに…」という経緯（＝issue 的には書かない側）を落として理由だけにする
- `PERFORMANCE.md:33` — 「理由は…歴史メモの様式が持つ」。歴史メモ節が消えるなら、理由 1 文（条件のない数値は再検証できない）を隣へ置く（§5-3）
- `docs/development-principles.md:72` — 「書式・粒度・**定型ラベル**は `docs/comment-guidelines.md` を SSOT とする」。定型ラベル節（L140-152）を消すなら直す（見出し参照ではないので機構は赤にしない）
- `CONTRIBUTING.md:13` — 「rustdoc / TSDoc の様式・粒度」。TSDoc 節を消すなら語を直す（G-references はリンクの実在だけを見る）

### 1-c 新設
- `docs/adr/ADR-<slug>.md`（例: `ADR-comment-guideline-one-question`）— §3 の否定の知識。`G-adr-file-names`: stem が `^ADR-[a-z][a-z0-9]*(-[a-z0-9]+)*$`、H1 の見出しが stem と一致すること（`scripts/governance/checks/G-adr-file-names.mjs:23`）。本書から引くなら `G-adr-citations` が実在を照合する
- `docs/glossary.md` — 「約束」を規約の語として使うなら（§5-6）

### 1-d 触らないが列挙して確かめたもの
- `.claude/skills/start-issue/SKILL.md:84`・`docs/glossary.md:8`・`.claude/agents/code-reviewer.md:29` — 「言語（日英）」を残せば不要
- `snotra-core/src/index_tree.rs:1126`・`G-fullwidth-doc-link-bracket.mjs:20` — 「名指しと正本の指名」の見出しと**表**を残せば不要
- `G-folded-code-spans.mjs:16,38`・`G-folded-heading-refs.mjs:34` — 「日本語の折返し」の見出しと**2 形・全ツリー適用の宣言・L126 の実測例**を残せば不要
- `.claude/skills/*`・`.claude/rules/*` の他ファイルにコメント規約の言い直しは無い（`git grep` で「回帰テスト:|保守注意|歴史メモ|現在の形の理由」等 0 件。`.claude/rules/governance-docs.md:13` は正準形で書けという書式の規範で、指すか書くかの規範ではない）

---

## 2. 変えてはならないもの（または変えるなら同時に直すもの）

| 守るもの | 根拠（指している側） | 失敗の形 |
|---|---|---|
| 見出し「名指しと正本の指名」と面ごとの保証の表（L86-90） | G-fullwidth-doc-link-bracket.mjs:20、index_tree.rs:1126、comments.md:17 | 見出しを消すと赤。**表だけ消すと緑のまま G-fullwidth の存在理由の宣言が偽** |
| 見出し「日本語の折返し」と、禁じる 2 形（L126-127）・全ツリー適用（L118）・L126 の実測例 | G-folded-code-spans.mjs:16,38、G-folded-heading-refs.mjs:34、code-reviewer.md:28、comments.md:19 | 同上（中身だけ消すと緑のまま偽） |
| 見出し「言語（日英）」と判定 (A)/(B)・語義ごと・適用範囲 | code-reviewer.md:29、start-issue/SKILL.md:84、glossary.md:8、comments.md:20 | 同上 |
| 見出し「rustdoc の様式」 | comments.md:18 | 赤 |
| 見出しの頭「第一原則: コメントは理由を書く」 | columns.rs:22（全形・前方一致で頭まで要る） | 消す・改名なら columns.rs を同時に直す |
| 本書から外へ出る正準形 5 本（`docs/development-principles.md`「ドキュメントとコードの分担（陳腐化の予防）」、`AGENTS.md`「条件別チェック（トリガー → 参照先）」（L23）、`.claude/rules/governance-docs.md`「ガバナンス文書の参照と命名のルール」、`PERFORMANCE.md`「この文書へ記録するときの規約」、`AGENTS.md`「開発ワークフロー」） | 本書は G-heading-refs の走査元（`headingRefDocs`） | 残す文には照合が掛かる。消すのは自由（checked が 335→330 に減るのは期待どおり） |
| 本書の本文のパス・識別子 | 本書は `governanceDocs`（G-references）と `staleIdentifierGuideDocs`（G-stale-identifiers）の両方に入る（lib.mjs:548-553, 689-693） | 新本文に書くバッククォートのパス・識別子は実在が要る。例示に架空の `X::y` やパスを書くと赤 |
| `.claude/rules/comments.md` の `paths` frontmatter | 配送の正本（ADR-comment-guideline-delivery-by-pointer 却下 2） | 触る必要なし |

**守らなくてよい（凍結）**: `docs/adr/` は走査元の外（`headingRefDocs`・`governanceDocs` とも除外、`ADR-adr-frozen-history`）。次は旧規約の姿を描いたまま古くなるが、契約どおり直さない:
- `ADR-comment-guideline-delivery-by-pointer.md:14`（「router は 4 つの節を名指し」「死んだ TSDoc 節」）
- `ADR-measurement-record-provenance.md:31,37`（「唯一の綱は…「歴史メモの様式」への正準形の参照」）
- `ADR-folded-code-span-detector.md:5,74,77`、`ADR-folded-canonical-reference-detector.md:27,34,43`
- `ADR-stale-identifier-detector-scope.md:260,325,347`
- ⚠️ `G-stale-identifiers.test.mjs:110` のコメント「第一原則が模範例として指していた関数」は過去形の事実なので偽にならない（直さなくてよいと判断）

---

## 3. まだどの ADR にも無い否定の知識（新 ADR の候補）

確かめ方: `git grep -F` で `docs/adr` を「#1185」「305 件」「禁則」「定着語」「誤配属」「#995」「#1116」「#1172」「48 件」「銀行家」「コメントの長さ」「行数の上限」「文字数の上限」で引いて 0 件。「上限」のヒット（ADR-doc-minimization-cap-enforcement 等）は `.md` 恒久規範の面積の話で、コメント長ではないことを冒頭を読んで確認。

### 3-a 本文または commit 本文にだけ在る却下（→ 新 ADR へ）
1. **コメントに行数・文字数の上限を置かない**（L64「#1185 の非目標」。commit 9916cff2 本文「hard limit は置かない」）。理由: 長さではなく層の問題、行数で測ると 1 箇条の長さ（921 字）が見えない
2. **doc の欠落を lint で deny 化しない**（L107。workspace で 305 件該当・2026-08-16 実測。commit db7f77df）
3. **禁則処理の規則を置かない**（L131。`.rs` コメント 8030 行で違反候補 0 件・2026-08-08）
4. **`SPEC.md` / `CLAUDE.md` での定着を訳語を残す理由にする「定着語」除外を廃した**（L174。それが直訳語を保護し #1251・#1252・#1307 が届かなかった。commit 3df9cf8f）
5. **`//` インラインの角括弧の禁止を解いた**（commit e440c64f 本文: 18 件中 12 件が禁止後に書かれた＝規範 1 行で止まらなかった。全件が「正本はあちら」の宣言だった）
6. **`///` の「N つ」「N 箇所」を赤にする検知器**（commit ac39cb26 本文: 48 件中少なくとも 14 件が許容形）— 却下そのものは `ADR-comment-guideline-delivery-by-pointer` 却下 4 に別の測定（10 件中 7 件偽陽性）で在る。追加の測定値だけが commit にしか無い（⚠️ ADR へ足すほどかは任意）
7. ⚠️ **ADR への訳語整備は `ADR-adr-frozen-history` の例外**（L175）。却下ではなく**凍結契約の改変**で、今は本書にしか無い。本文から外すなら、生きた置き場（「言語（日英）」節に残す、が最も素直）が要る。ADR に移すと、凍結契約の例外が凍結文書にだけ書かれる形になる

### 3-b このサイクル自身が生む否定の知識（→ 新 ADR の本体）
8. **「理由はあちら」と遠くの doc を指す形を原則にしない**（現行 L67「在るなら指す」の反転）。理由: issue の実測——コメントを指す参照約 330 件は `governance:check` がどれも照合しない（正準形 335 件はすべて照合される）
9. **コードのコメントを一括で書き直さない／規約側を縮める**。理由: issue の実測——コードのコメント 19,679 行のうち経緯の形は 2.4%（取りこぼし込み 4〜5%）。重いのは規約の側（188 行・14,617 字・`#NNN` を含む行 24）
10. ⚠️ 却下した案があれば（例: 現行の節構成のまま文を詰める、router に表を直接置く）、その理由も。issue には書かれていないので計画で決める

### 3-c ADR に入れないもの（純粋な経緯 → `#NNN` を残して本文から落とす）
- #977 の古くなった 3 件の実例（L23）、#984/#989/#732 の自己訂正の話（L28）、#995 のレビューの話（L32）、#1106 の `///` 帰属事故の描写（L107 前半）、SU5・PR #647（L109）、#878 の銀行家丸めの実例（L187）、#562 の由来（L108）
- 既に ADR が持つもの（指すだけ）: 1 段落 1 行の撤去と測り直しの数（`ADR-folded-code-span-detector`「文脈」）、rustfmt `wrap_comments`/clippy（同 ADR）、測定値の置き場（`ADR-measurement-canon-in-code-doc`）、配送を router にした理由と条項へ機械検査を付けない理由（`ADR-comment-guideline-delivery-by-pointer`）、入れ子鉤括弧の見出し（`ADR-nested-quote-heading-ref-labels`）、見出しだけをアンカーにする（`ADR-heading-refs-headings-only`）

---

## 4. 実行すべき検証（`docs/build-commands.md`「変更後の検証チェックリスト」）

- **F（常に）**: `npm run governance:check`。`.md` と `.claude/rules`・`.claude/agents`・`.claude/skills` を触る。F の箇条 1 のとおり `.rs` の見出し参照（columns.rs:22）を直す・参照先を改題する場合も必須。期待値: findings 0、checked は 335 から本書の外向き参照の増減ぶん動く（新本文で正準形を何本書くかで決まる。増減を説明できることを確かめる）
- **A（columns.rs / index_tree.rs を触るなら）**: fmt・check・clippy・`cargo test -p snotra-core`・`cargo doc --workspace --no-deps --document-private-items`（doc コメントを触るので `.claude/rules/comments.md` のトリガーにも当たる）
- **E（`scripts/governance/checks/*.mjs` のヘッダを触るなら）**: `npm test`
- 変異の確認（`.claude/rules/safety-nets.md`「効いていることは…実測する」）: 改題後に、残した見出しを 1 つメモリ上で消して参照元が赤になること（本稿の scratch プローブと同型）
- B・C・D は該当しない

---

## 5. 判断が割れうる点と推奨

1. **「縮める」の範囲**。推奨: フェーズ 1 は核（第一原則・配置基準・短く保つ・歴史メモの様式・同期・テストコード）を問い 1 つと表へ置き換え、機構の入力・外からの参照が集中する 4 節（名指しと正本の指名・日本語の折返し・言語（日英）・rustdoc の様式）は**見出しと中身を残す**。参照元の修正は 5 本（§0）で済む。全部を縮める案は 18 本（`.rs`・`.mjs`・`.claude/` にまたがる）と、G-fullwidth / G-folded-* のヘッダの存在理由の書き直しを伴う。残す節の中の経緯の削減（L94-102・L114-131 等）は、中身依存（§2）に当たらない文だけに限る
2. **「指す」と「隣に書く」の両立**。推奨: 1 文で線を引く——**事実**（数・識別子・正本が 1 つしかないもの）は `[X]` で指す、**理由**は守る行の隣に書く。線を引かないと「名指しと正本の指名」・code-reviewer.md:28 が新しい核と矛盾して読める
3. **PERFORMANCE.md:33**。推奨: 理由 1 文を隣へ置く（issue の原則そのもの）。`ADR-measurement-record-provenance` が却下した「理由を PERFORMANCE.md へ書く」の根拠（二重化）は本書側の文が消えれば無くなる。同 ADR:37 の「唯一の綱」は凍結された歴史として古くなるに任せる（`ADR-adr-frozen-history`）。代案: 新しい表の「測定値」の例外行へ正準形で指し直す（見出し名を固定する負債が残る）
4. **経緯は ADR か `#NNN` か**。`AGENTS.md`「ドキュメント参照」は ADR を「否定の知識が生じた決定だけ」と定める。issue の「規約の経緯は ADR へ」を文字どおり全部 ADR へ入れると、否定を含まない物語まで ADR に入る。推奨: §3 の 3 分割（却下 → 新 ADR／ADR 既存 → 指す／物語 → `#NNN` だけ残して落とす）
5. **`回帰テスト:` ラベル（.rs で 18 件）と L185「出自を明記する」**。新しい表は「どう至ったか → コミット・PR、コメントには `#NNN`」で、「テストが落ちる → テスト名」。`回帰テスト:` で出自を書く現行様式と食い違う。推奨: ラベルは残し、書く中身を「`#NNN` と、そのテストが守る約束」に絞る（既存 18 件は書き直さない＝L7 のスコープ）。⚠️ 定型ラベル節（L140-152）を丸ごと消すと development-principles.md:72 の「定型ラベル」が宙に浮く
6. ⚠️ **「約束」という語**。リポジトリでは既に「書く約束」（一次規範の意・`.claude/rules/governance-docs.md` ほか ADR 4 本）が在る。新しい表の「いま守っている約束」（＝コードが守る不変条件・契約）とは語義が違う。`docs/comment-guidelines.md`「言語（日英）」の (A) と glossary の「2 つ目のファイルで足す」に当てて、「不変条件」「契約」で足りるなら新語にしない。使うなら glossary に足す
7. ⚠️ **router（comments.md）へ表を直接置くか**。核が十数行になれば「書く瞬間に読めない」は router に置けば最も効く。`ADR-comment-guideline-delivery-by-pointer` 却下 1 の第一の理由（G-area-budget が赤）は `ADR-retire-area-budget` で失効している。ただし残る理由（人間向けの入口・常時配送の費用）と写しが 2 つになる問題がある。推奨: フェーズ 1 ではやらない（issue の WHAT にない）。ADR の「却下した案」に、失効した理由と残る理由を書いておく
8. **セーフティネットの扱い**。`.claude/rules/comments.md`・`.claude/agents/code-reviewer.md`（と §5-1 を外せば `.claude/skills/start-issue/SKILL.md`・`scripts/governance/checks/*.mjs`）を触るので、ルート `CLAUDE.md` 最重要ルール 2 に当たる。PR 本文の冒頭に「マージは合意してから」。`.claude/` 配下の編集は分類器に拒否されうる
9. ⚠️ **新見出しの名前**。後から指される名前になる（フェーズ 2・3 がこの規約に沿う）。前方一致なので、後置の括弧注記を足しても参照は壊れないが、頭を変えると壊れる。鉤括弧の入れ子は 1 段まで照合される（`ADR-nested-quote-heading-ref-labels`）が、「」を含めない名前が安全（#1187 で同じ理由で外した前例: commit 9916cff2）
