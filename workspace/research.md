# 調査: #1407 コメント規約を、書く瞬間の問い 1 つと置き場の表へ縮める

測定は 2026-11-07・main 3a98f16c。

## issue の要約

- `docs/comment-guidelines.md` が、書く瞬間に読める長さではない。経緯と自身の訂正の記録で太っている
- あるべき姿: 書く瞬間の問いを「この行を単純にされたら、何が壊れるか」の 1 つにし、答えごとの置き場（テスト名・直上の 1〜2 行・書かない・コミット/PR）を表にする。理由はそれが守る行の隣に書き、遠くの doc を指す形を原則にしない（例外は 1 か所にしか置けないもの）。規約の経緯は ADR へ移す。外した条項を読む箇所を数え上げて合わせる

## 事実

### 規約の大きさ

- 188 行・14,617 字（33,071 バイト）。`#NNN` を含む行 24
- 推移（字）: 2026-07-18 4,017 → 07-26 4,584 → 08-09 9,373 → 08-24 11,646 → 09-29 14,389 → 11-03 14,615。編集したコミット 31 本
- 節（`##`）は 12: 第一原則・配置基準（3 層）・短く保つ（### 測定値の置き場）・名指しと正本の指名・rustdoc の様式・日本語の折返し・TSDoc の様式・定型ラベル・歴史メモの様式・言語（日英）・コメントと実装の同期・テストコードのコメント

### 節の性質（縮め方を分ける軸）

- **何を書き、どこに置くか**を述べる節: 第一原則・配置基準（3 層）・短く保つ・歴史メモの様式・コメントと実装の同期・テストコードのコメント（の前半）。issue の「問い 1 つ + 置き場の表」が置き換える対象。内容が互いに重なる（「経緯は `#NNN` へ」が第一原則・配置基準の層の中の分担・歴史メモの様式の 3 か所に在る。「振る舞いを保つリファクタを生き延びるか」が配置基準・短く保つ・コメントと実装の同期の 3 か所に在る）
- **道具の事実**を述べる節: 名指しと正本の指名（rustdoc / `cargo doc` / rust-analyzer が面ごとに何を確かめるかの表）・rustdoc の様式（`///` の帰属・非リンク角括弧）・日本語の折返し（機構 `G-folded-code-spans` / `G-folded-heading-refs` が見る 2 形）。問いでは置き換わらない。経緯を外して残す対象
- **語の選び方**: 言語（日英）。問いでは置き換わらない。`/start-issue`・`code-reviewer`・`docs/glossary.md` が判定 (A)/(B) を名指しで使う
- **ほぼ死んでいる**: TSDoc の様式（TS は `vitest.config.ts` だけ。`.mjs` へ当たる 1 点だけが生きている）・定型ラベル（ラベルの一覧）

### 外から見出しを指す参照（正準形・`governance:check` が照合する）

見出しの前方一致で照合される（`scripts/governance/checks/G-heading-refs.mjs`）。ADR は凍結層で照合の走査元から外れるが、参照先の着地は照合しないので、改題すると ADR 内の参照は黙って古くなる。

| 見出し | 生きた層 | ADR |
|---|---|---|
| 第一原則 | `.claude/agents/code-reviewer.md:28`・`.claude/rules/comments.md:15`・`snotra-core/src/indexer/columns.rs:22`（「第一原則: コメントは理由を書く」の全形） | — |
| 短く保つ | `.claude/rules/comments.md:16` | — |
| 名指しと正本の指名 | `.claude/rules/comments.md:17`・`snotra-core/src/index_tree.rs:1126`・`scripts/governance/checks/G-fullwidth-doc-link-bracket.mjs:20` | — |
| rustdoc の様式 | `.claude/rules/comments.md:18` | — |
| 日本語の折返し | `.claude/agents/code-reviewer.md:28`・`.claude/rules/comments.md:19`・`scripts/governance/checks/G-folded-code-spans.mjs:16,38`・`scripts/governance/checks/G-folded-heading-refs.mjs:34` | `ADR-folded-canonical-reference-detector` 2 件・`ADR-folded-code-span-detector` 2 件 |
| 言語（日英） | `.claude/agents/code-reviewer.md:29`・`.claude/rules/comments.md:20`・`.claude/skills/start-issue/SKILL.md:84`・`docs/glossary.md:8` | — |
| 歴史メモの様式 | `PERFORMANCE.md:33` | `ADR-measurement-record-provenance` 2 件 |

見出しを指さない参照（ファイル名だけ）: `AGENTS.md:16`・`CONTRIBUTING.md:13`・`docs/development-principles.md:72`・`.claude/rules/comments.md:11` ほか。

### 配送と機構

- 配送: `.claude/rules/comments.md`（4 crate の `.rs` を `paths` に持つ router。要約を置かず、6 節を名指しする）
- rule 本体へ統合しない判断と、2026-08-08 時点で条項に機械検査を付けなかった判断: `docs/adr/ADR-comment-guideline-delivery-by-pointer.md`（統合しない理由の 1 つ目は当時の面積上限 `G-area-budget` だったが、その検査は今の `scripts/governance/checks/` に無い。機械検査は後に一部の形へ付いた。下の 3b の F4）
- `scripts/governance/checks/G-stale-identifiers.test.mjs` は `docs/comment-guidelines.md` の過去の版（`f1827b0`）の行の骨格をフィクスチャに凍結しているだけで、今の本文を読まない

### 隣接する規範

- `docs/development-principles.md`「ドキュメントとコードの分担（陳腐化の予防）」: 判定基準「この一文は、振る舞いを保つリファクタを生き延びるか？」と、散文ドキュメントから精密事実を指すときは doc コメントを正本に指名する、を持つ。規約の「配置基準（3 層）」はこれに従うと書く
- `docs/development-principles.md:72`: コメントの書式・粒度・定型ラベルの SSOT は `docs/comment-guidelines.md` と宣言

## 再利用できる既存のパターン

- ルーター `.claude/rules/comments.md` は節を名指しするだけなので、節を減らせば行を減らすだけで合う
- 「消える経緯の行き先は ADR」: `ADR-folded-code-span-detector`・`ADR-stale-identifier-detector-scope`・`ADR-measurement-canon-in-code-doc` など、規約の経緯の多くは既に ADR を持つ。規約から経緯を外しても、知識はそこに残る
- 見出しの改題は `governance:check` が赤にするので、改題した節の参照は機構で全件が挙がる（ADR 内は除く）

## 技術的な制約

- 正準形の照合は前方一致。見出しを残して後ろに語を足すのは安全、前を変えると赤
- ADR は凍結（`ADR-adr-frozen-history`）。ADR 内の参照が改題で古くなるのは受容されている扱いか、要確認（未解決 1）
- 規約はセーフティネットの「規範」に当たりうる。PR 本文の冒頭に「マージは合意してから」と書く（ルート `CLAUDE.md`「最重要ルール」2）
- `.claude/rules/comments.md` の編集は `.claude/` 配下で、分類器に flag されうる

## 敵対的調査（3b）の反映

出力: `workspace/adversarial-1407.txt`。採否:

- **F1 採る**: `docs/superpowers/plans/2026-08-10-search-worker.md:17` にも「日本語の折返し」の参照がある。非規範化された履歴資料で走査元の外。見出しを残すので影響なし
- **F2 採る**: 見出しだけでなく中身に依存する読み手がいる。`code-reviewer.md:28`（第一原則を「書いてよい例外」の正本に使う）・`columns.rs:22`（全形「第一原則: コメントは理由を書く」）・`index_tree.rs:1126` と `G-fullwidth-doc-link-bracket.mjs:20`（名指しの表の行）・`PERFORMANCE.md:33`（測定値の出所の理由を「歴史メモの様式」に持たせる）・(A)/(B) の判定（3 か所）。→ これらの見出しと、指されている中身（一意性の宣言の例外・表の行・出所の理由・(A)/(B)）は残す
- **F3 採る**: ファイル名だけの参照が中身を述べている（`AGENTS.md:16` の書式ラベル・`CONTRIBUTING.md:13` の「rustdoc / TSDoc の様式」・`development-principles.md:72` の「書式・粒度・定型ラベル」）。機構は見ないので、縮めた後に手で合わせる作業項目にする
- **F4 採る**: 「条項に機械検査を付けない」は 2026-08-08 の判断で、今は折返しの 2 形と全角括弧の形に検査がある（上の「配送と機構」の記述を訂正）。規約本文は `G-references` と `G-stale-identifiers` の走査対象なので、新本文に書くパス・識別子は実在が要る
- **F5 採る（一部）**: 定型ラベルは死んでいない。`不変条件:` 35 行・`回帰テスト:` 20 行が使われ、`保守注意:`・`実装メモ:`・`検出マーカー:` の 3 つが使われていない。`.mjs` の `@param` は 28 行あるので、「`@param` を必須にしない」の 1 点は残す。境界で揺れる条項（`//!` の配置・`# Why` 見出し・ADR の語の直し・テストの「弾く」の主張）は計画で行き先を 1 つずつ決める
- **F6 採る**: 「遠くを指さない」を一般則にすると、`.rs` の正準形参照 140 件と `` [`X`] `` の名指しが一斉に違反に見える。→ 2 分にする: **理由**は守る行の隣に書く（道案内の参照で済ませない）。**精密事実**（1 か所にしか置けない値・契約）は正本を 1 つ決めて指す。既存の道案内の削減はフェーズ 2（#1408）
- **F7 採る**: 「`#NNN` を含む行 24」は `#\d{2,}` で数えた行（字面のプレースホルダ `#NNN` の 6 行は含まない）。推移の基点は 2026-07-18 の最初のコミット（4,017 字）
- **F8 採る**: 凍結した ADR 内の参照が改題で古くなるのは受容済み（`ADR-folded-code-span-detector.md:77`）。未解決 1 は解消

## 未解決の疑問

1. 改題・削除する節を ADR が指している場合（「日本語の折返し」「歴史メモの様式」）、ADR 側を直すのか、見出しを残すのか
2. 「理由は守る行の隣に。遠くを指さない」と、`docs/development-principles.md`「ドキュメントとコードの分担（陳腐化の予防）」の「散文ドキュメントから精密事実を指すときは doc コメントを正本に指名する」は衝突するか。衝突しないなら境界をどう書くか（md → コードの指名は残し、コメント → コメントの道案内だけを原則から外す、が候補）
3. 経緯を外すとき、どの経緯がまだどの ADR にも無いか（無いものだけを新しい ADR へ移す）
4. 「テスト名に理由を書く」を、`テストコードのコメント` 節の「`回帰テスト:` ラベルで出自を書く」とどう合わせるか（出自は経緯であり、問いの表では PR 側に入る）
