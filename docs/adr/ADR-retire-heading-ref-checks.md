# ADR-retire-heading-ref-checks: 見出し参照と散文の折れを照合する検査を撤去する

## 文脈

`governance:check` には、散文の書き方を照合する検査が 4 本あった。

- `G-heading-refs`: 正準形 `` `<対象>`「<見出し>」 `` の参照が、実在する見出しに当たるか
- `G-near-heading-refs`: 正準形に見えて、助詞が挟まり照合から外れた参照
- `G-folded-heading-refs`: 正準形の参照が物理改行で折れた形
- `G-folded-code-spans`: コードスパンが物理改行で折れた形

これらの検査は、`ADR-canonical-heading-references` が参照に構文を与えて機械照合できるようにしたところから育った。その後の ADR（`ADR-heading-refs-headings-only`・`ADR-folded-canonical-reference-detector`・`ADR-folded-code-span-detector`・`ADR-nested-quote-heading-ref-labels`）が、照合の死角を 1 つずつ塞いできた。

その結果、見出しは参照の着地点として固定された。見出しを直すと、参照している文書・スクリプト・`.rs` のコメントがすべて CI を赤にする。`docs/development-principles.md` の組み直し（#1416）では、見出しを保つために節の構成を曲げ、参照の付け替えに作業の多くを使った。参照されているから見出しを直せない、文章を直せない、という状態は本末転倒である（利用者の判断・2026-11-09）。

## 決定

4 本の検査と、この 4 本だけが使っていた `scripts/governance/lib.mjs` の部分を撤去する（#1417）。

- 参照は、ファイル名と見出し名で書く。見出し名は読み手の手がかりであって、検査は照合しない
- 見出しは自由に直してよい。直したら旧名で `git grep` し、見つかった参照を同じ変更で直す（`.claude/rules/governance-docs.md`「ガバナンス文書の参照と命名のルール」）
- ファイルの実在は `G-references` が、ADR の短縮引用は `G-adr-citations` が、引き続き照合する
- コードスパンを行またぎさせない規範は残す（`docs/comment-guidelines.md`「日本語の折返し」）。grep で辿れなくなる害は検査と無関係に実在するためである。守るのは書く人とレビューである

## 検討した代替案と却下理由

### 見出し参照の検査を残し、CI を赤にしない警告へ格下げする

却下。警告の一覧を誰がいつ読むかが決まらない。読まれない警告は、撤去と同じ結果になる。そのうえ、検査の保守（死角を塞ぐ ADR と fixture）は残る。

### 見出し参照の検査を撤去し、参照の書き方も自由にする

却下。ファイル名を書くことまで手放すと、`G-references` の照合も効かなくなる。ファイル名は改名がまれで、読み手が開く先を一意に決める。ここは守る価値がある。

### `G-stale-identifiers` も同じ理由で撤去する

却下（利用者の判断）。撤去した識別子の名前が散文に残っていないかを見る検査で、撤去の漏れを見つけてきた実績がある。文章を直す自由を縛るのではなく、消した機構を在る前提で書く誤りを止める側にある。

## 受容した残余

- 見出しを直して旧名の grep を怠ると、参照は黙って古くなる。読み手は参照先のファイルを開き、見出し名で検索して辿ることになる
- `/` を含まない素の名前（`AGENTS.md`・`SPEC.md` など）は、`G-references` の照合の外にある。ルート直下の文書が消えることはまれなので、受け入れる
- コードスパンの行またぎは、レビューで見落とせば残る

## 反転条件

古くなった見出し参照が原因で、作業者が誤った節を読んで誤りを持ち込んだ事例が続いたとき。そのときは、検査を戻す前に、参照の数そのものを減らせないか（ファイル名だけの参照で足りないか）を先に問う。

---

status: Accepted
関連: #1417 ・#1416 ・`ADR-canonical-heading-references` ・`ADR-heading-refs-headings-only` ・`ADR-folded-canonical-reference-detector` ・`ADR-folded-code-span-detector` ・`ADR-nested-quote-heading-ref-labels`
