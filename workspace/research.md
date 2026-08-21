# 調査: #1095 呼び出し元ゼロの `productionOnly` を消すか

対象 issue: #1095「検討: 呼び出し元ゼロの productionOnly を消すか（ADR が実在を前提に論じている）」（OPEN・起票 2026-08-15・`type:refactor` / `size:S`）
調査日: 2026-08-21 / ブランチ `chore/production-only-decision`

## 0. issue の有効性判定（ユーザーの第一の問い）

**結論: 今も有効である。** 中核の前提 2 つは実測で生きており、外れたのは経緯説明の枝葉 2 点のみ。

| issue の主張 | 今日の実測 | 判定 |
|---|---|---|
| `productionOnly` は呼び出し元ゼロ | 定義は `scripts/governance/checks/G-stale-identifiers.mjs:19`、`export` を持たずモジュールローカル。同ファイル内の実呼び出し 0 件 | **生きている** |
| 削除しても `G-stale-identifiers` は赤くならない | 削除して実測（§2a）。exit 0・出力が削除前とバイト単位で一致・照合件数 404 件も不変 | **生きている（実測で確定）** |
| 言及は 3 か所とも コメント内の**字面** | 数は合うが、3 つ目の着地先は `lib.test.mjs:113`。**「字面」という性格づけが誤りで、うち 2 か所は生きた契約である**（§3） | **外れ（性格づけを要修正）** |
| 唯一の文書での言及は `docs/adr/ADR-stale-identifier-detector-scope.md:108` | 今日は `docs/superpowers/plans/2026-08-15-governance-check-per-check-split.md` にも 3 か所ある | **起票時は真・今日は偽（判断には影響しない）** |

### 起票時と今日の差は、何が動いたからか

起票時点の HEAD は `72005ca`（#1092）。そこでの `git grep productionOnly` は **5 件**で、内訳は `scripts/governance-check.mjs` に 3 件（うち 1 件が定義）・`scripts/governance-check.test.mjs` に 1 件・`docs/adr/ADR-stale-identifier-detector-scope.md` に 1 件だった。今日は **8 件**（定義 `G-stale-identifiers.mjs:19` / banner `:93` / `lib.mjs:571` / `lib.test.mjs:113` / ADR:108 / plans ×3）。

差の 3 件は **#1093（per-check 分割）のマージそのもの**が持ち込んだ。issue は「#1093 実行中」に、マージ後の姿を予測して書かれている。

- `docs/superpowers/plans/2026-08-15-...` の 3 件は #1093 と同時に入った。起票時点では存在しないので、「唯一の文書での言及は ADR」は**起票時には正しかった**
- テスト側の言及の着地先は、予測の `G-stale-identifiers.test.mjs` ではなく `lib.test.mjs` だった
- **どちらも判断を変えない**——`docs/superpowers/` も `docs/adr/` も検査母集団の外である（§2b）

なお issue 本文の「走査対象 33 文書」は今日 34 文書である。差の 1 件は起票後に追加された `docs/design/2026-08-20-governance-meta-demotion-derivations.md`（`git diff --name-status 72005ca..HEAD` で確認）。除外条件は変わっていない。

## 1. 対象と周辺

- `scripts/governance/checks/G-stale-identifiers.mjs`
  - `productionOnly(src)` (:19) — `#[cfg(test)]` 以降を落とす。**呼び出し 0**
  - `stripRustComments(src)` (:13) — `currentVocabulary` (:154) が呼ぶ。**生きている**
  - `currentVocabulary(snapshot)` (:143) — 現行語彙を production ソースの非コメント本文から作る
  - banner コメント :93 — 「受容する残余」節で `productionOnly` に言及
- `scripts/governance/lib.mjs:571` — `headingRefSourceDocs` の JSDoc（**契約**・§3）
- `scripts/governance/lib.test.mjs:113` — 「種 3」テストのコメント（**契約**・§3）
- `docs/adr/ADR-stale-identifier-detector-scope.md:108` — 凍結された歴史（`ADR-adr-frozen-history`）
- `docs/superpowers/plans/2026-08-15-governance-check-per-check-split.md` ×3 — #589 で非規範化された当時の設計（鮮度維持の対象外）

**`export` を持たないので、呼び出し元の母集団は定義ファイル 1 枚に閉じる。** `AGENTS.md` は関数の呼び出し元列挙に LSP findReferences を求めるが、ここでは grep がその 1 枚を尽くすので**完全**である（LSP へ落とす必要が無い、という意味であって省略ではない）。

## 2. 測ったこと（2026-08-21・このブランチの作業ツリーで実測）

**すべて未コミットの作業ツリーを対象にしている。** `git diff main...HEAD` 等の 2 点・3 点形は commit 同士を比べて作業ツリーを見ないため、この検証には使えない（#922）。各測定の後にファイルを復元し、`git status` が clean であることを確認済み。

### 2a. 削除して finding が動くか

`G-stale-identifiers.mjs` の 17〜22 行（JSDoc 2 行 + 関数 3 行 + 空行）を削除して:

- `node scripts/governance-check.mjs` → **exit 0**、出力が削除前と**バイト単位で一致**（`散文の識別子 404 件を 34 文書から照合` も不変）
- `npx vitest run scripts/` → **32 ファイル / 523 テスト passed**

### 2b. なぜ動かないのか（母集団をコードで確認）

`productionOnly` は語彙源（production ソースの非コメント本文）に在る宣言なので、削除は**語彙からの 1 語の消失**である。母集団文書が `` `productionOnly` `` をバッククォートで書いていれば finding へ転ぶ。実測で転ばなかった理由は `scripts/governance/lib.mjs:639-660` のコードが示す:

- `staleIdentifierDocs` = `.claude/{skills,rules,agents}/**.md` のみ
- `staleIdentifierGuideDocs` = `docs/**.md` から **`docs/superpowers/` と `docs/adr/` を除外**
- `STALE_EXTRA_DOCS` = `SPEC.md` / `CLAUDE.md` / `AGENTS.md` / `snotra-settings/SETTINGS-DESIGN.md`

言及する 2 文書はどちらも除外側に落ちる。**banner の散文ではなくコードで確認した**——#1093 以降 4 本の governance コミットが入っており、散文は腐りうる（この検査自身が捕まえようとしている対象がそれである）。

### 2c. 計器が実際に赤くなることの確認（`safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」）

§2a の「差分なし」が**検出力のある計器の緑**であることを確かめた。母集団に実在する `docs/build-commands.md` の末尾へ `` `thisIdentifierDoesNotExistZz` `` を一時的に書き足して実行 → **exit 1**、`散文に、現行語彙に無い識別子が残っている` が出力された。復元済み。

### 2d. `.mjs` のコメント内の**識別子**は誰も見ない（見出し参照は逆に見られている）

削除後に残る 3 か所の言及（`G-stale-identifiers.mjs:93`・`lib.mjs:571`・`lib.test.mjs:113`）は**すべて `.mjs` のコメント内**である。`G-stale-identifiers` の母集団は上の 3 群のみで `.mjs` を含まない。

**つまり、識別子の消し忘れは何も鳴らさない。** `AGENTS.md`「機構・層・ファイル群を撤去する」行が名指しする「散文の語彙・幽霊識別子」の形そのものである（#1155 で 9 群が残った経路）。

**この結論を「これらのコメントは誰にも見られていない」へ一般化してはならない。** 同じ 3 か所は `G-heading-refs` の `.mjs` の腕（`headingRefCommentDocs`・#1138）の母集団に**入っている**——ベースライン行の `見出し参照 324 件を … スクリプトのコメント 111 件から照合` がそれである。実際、`G-stale-identifiers.mjs` の banner は `` `ADR-stale-identifier-detector-scope`「その後（#975・述語へ lowercase snake_case を足し、`.rs` への母集団拡大は却下した）」 `` のような正準形を持つ。見られていないのは**識別子だけ**である。

ゆえに散文化では**正準形の参照を 1 物理行に保つ**こと——#1156（4 日前）が物理改行で折れた参照を赤にする検査を入れており、文言に合わせて JSDoc の段落を折り返し直すのがちょうどその踏み方である。これは沈黙せず `governance:check` が赤にする。**編集後に赤が出たら、まず削除ではなく折れた参照を疑う。**

### 2e. 禁止を守っているのは名前か、機械か（**A/B を分ける決定的測定**）

`lib.mjs:571` と `lib.test.mjs:113` が禁ずる変更——`productionOnly` 相当を `G-heading-refs` の `.rs` の腕へ持ち込む——を実際に注入した。`scanHeadingRefs` (`G-heading-refs.mjs:41`) の読み取り直後に `if (doc.endsWith(".rs") && text != null) text = text.split(/^#\[cfg\(test\)\]/m)[0];` を加えて `npx vitest run scripts/governance/lib.test.mjs`:

- **「種 3: `#[cfg(test)]` の内側のコメントも見る」が赤くなった**（`expected [] to have a length of 1`）
- 他 68 テストは無傷——変異が本来の回帰の姿と同じ強さであることの裏づけ（`safety-nets.md`「注入したことと、注入が正しい強さであることは別である」）
- 復元後に再実行して 69 passed を確認

**結論: 禁止は機械が守っている。** `productionOnly` という名前はその禁止の**説明**であって、禁止を**支えてはいない**。名前が消えても、禁止された変更は種 3 が止める。

（1 回目の注入は sed のエスケープで `\[` が落ち、正規表現が `#[cfg(test)]` にマッチしない形になっていた。緑が出たがこれは壊れた計器であり、根拠にしていない。2 回目は `split` が実際に効くことを `node -e` で確かめてから測った。）

## 3. issue が見落としていた論点

issue は 3 か所の言及を「コメント内の**字面**」と括ったが、**うち 2 か所は字面ではなく契約である**。

`scripts/governance/lib.mjs:566-572`（`headingRefSourceDocs` の JSDoc）:

> **Rust のテストコードを外さない。** …… `productionOnly` 相当を「G-stale-identifiers との対称性の完成」として後から入れてはならない（その非対称は意図である）。

`scripts/governance/lib.test.mjs:111-114`（「種 3」テストのコメント）:

> `productionOnly` 相当を「G-stale-identifiers との対称性の完成」として入れると、この it が落ちる——非対称は意図である

これらは **隣の検査へ `productionOnly` 相当を持ち込むな**という禁止であり、その禁止対象を**隣に在る関数を指して**名づけている。issue 本文の「削除する前に確かめること 1」は ADR についてこの懸念を書いていたが、**同じ構造が生きた層にも在ることは書かれていない**。

ただし §2e の実測により、**この論点は削除の障害にならない**——禁止の実体はテストであり、名前は説明である。

### 3b. その説明は、書かれた時点で既に偽だった（`git log -S` で実測・Step 5a の独立導出が発見）

`productionOnly` は **G-stale-identifiers のために書かれた関数ではない**。`git log -S "productionOnly" -- scripts/` で追うと:

| コミット | 状態 |
|---|---|
| `43475aa`（#793・config 値の到達性検査 G12 を追加） | 導入。**呼び出し 2 件**（母集団側と読み手側） |
| `066cb3f`（#885） | 呼び出し 2 件のまま |
| **`b2ff79c`（#897・config 到達性検査の撤去）** | **G12 ごと撤去され、呼び出しが 0 になった** |
| `fa1dbba`（#931・見出し参照検査の走査元へ `.rs` を足す） | **死んだ後に**、`lib.mjs`（当時 `governance-check.mjs:1153`）と `lib.test.mjs` の禁止コメントが書かれた |
| `4f5f4d3`（#1093） | `stripRustComments` との**物理的な隣接だけを理由に** `G-stale-identifiers.mjs` へ移送 |

ゆえに `lib.mjs:571` の「`productionOnly` 相当を『**G-stale-identifiers との対称性の完成**』として後から入れてはならない（**その非対称は意図である**）」は、**#897 以降ずっと偽である**——`productionOnly` が呼ばれていない以上、G-stale-identifiers の語彙源も `.rs` の腕も**どちらも `#[cfg(test)]` の内側を見ている**。名指された非対称は存在しない。

**禁止そのものは実在し、種 3 が守っている**（§2e）。偽なのはその説明が前提にしている対比のほうである。ゆえに書き換えは「宙に浮いた識別子の改名」ではなく、**偽の前提を落として規範を変換の名で言い直す**作業になる。

同じ理由で、削除対象の JSDoc（`:17-18`）が現在形で書く「**母集団と読み手の両方に適用する**……（`visible_rows` で実測）」も、**撤去済みの層（G12）の記述**である。約 5 か月間どの機構もこれを赤にしていない——`AGENTS.md`「機構・層・ファイル群を撤去する」が言う「撤去した層の語彙」の残余（#1155 / #1157 と同クラス）そのものであり、**issue が「デッドコードの削除」と呼んだものは、実際には撤去の取りこぼしの回収である**。

## 4. 選択肢

| 案 | 内容 | 代償 |
|---|---|---|
| **A. 削除する（推奨）** | 定義を消し、`.mjs` の 3 コメントから関数名への参照を断つ（「`#[cfg(test)]` より前だけを見る述語」等へ散文化）。ADR と plans は触らない | 散文化漏れを鳴らす検知器が無い（§2d）ので、同じ差分で 3 か所を確実に処理する必要がある |
| **B. 残す** | 呼ばれないことが意図である旨を JSDoc へ 1 行書き、以後の「デッドコードでは」を止める | 呼び出し 0 の関数が残る。**語彙源に 1 語を寄付し続ける**（下記） |

**A を推奨する理由は 2 つある。**

1. **§2e で A の代償が実測により消えた。** 当初 A の代償と見ていた「禁止が概念だけになる」は成立しない——禁止は種 3 が機械的に守る
2. **B は、この検査自身が掲げる原則への違反を維持することになる。** banner (`G-stale-identifiers.mjs`) は語彙源を狭める理由をこう書いている——**「語彙を寄付してよいのは『現に動いている実装』だけである」**。呼び出し元ゼロの関数は現に動いている実装ではない。ゆえに `productionOnly` が語彙に居ることは**将来のリスクではなく現在の違反**である。具体的な害も同じ形で出る: `CLAUDE.md` / `AGENTS.md` / `SPEC.md`（いずれも `STALE_EXTRA_DOCS` に入る検査対象）の散文に将来 `` `productionOnly` `` と書かれた場合、**呼ばれない関数が現行語彙としてそれを免罪する**。今日 0 件だが、残す限り開いたままの経路である

**ADR と plans を触らない理由**（「凍結だから」より正確な形）: Rust テストの語彙寄付という残余の記述は、**生きた家（`G-stale-identifiers.mjs:93`）と凍結された写し（ADR:108）の二重**で存在する。A では生きた家を直し、写しは残す。plans (`docs/superpowers/`) は #589 で非規範化され鮮度維持の対象外——根拠は ADR とは別だが結論は同じく編集不要である。

なお削除後、`git grep productionOnly` は `workspace/research.md` と `workspace/plan.md` にも当たる。`AGENTS.md`「機構・層・ファイル群を撤去する」の分類では「撤去を描写している」側であり、対処不要（`workspace/` は `makeSnapshot` の走査から明示的に除外されてもいる）。

## 5. 未解決の疑問

- **A と B のどちらを採るか。** これは実装判断ではなく要求判断である——「呼ばれないコードを置かない」と「採らなかった対処の実体を議論の参照点として残す」のどちらを採るかであり、ユーザーの裁定を仰ぐ（`plan.md` の未確定欄）

当初の未解決 2 件は解消した。

- ~~A で散文化が禁止の強さを保てるか~~ → §2e で**保てる**（禁止の実体はテスト）
- ~~B で「いつ消えるのか」を書けるか~~ → A を推奨するため争点から外れる。B を採る場合のみ再浮上する

## 6. 敵対的調査（Step 3b）の結果と採否

`general-purpose` / `sonnet` 1 体を起動し、`research.md` の全主張を母集団として反証を求めた。出力は `workspace/adversarial-1095.txt`。

### 壊せた項目（2 件・いずれも**採用**して本文を修正済み）

| 指摘 | 採否 | 反映 |
|---|---|---|
| 「今日の 7 件」は誤りで **8 件**。research.md 自身の §1 の内訳と数えても 8 で、自己矛盾していた | **採用** | §0 を 8 件へ修正し、内訳を明記 |
| 「起票時 5 件・**すべて** facade の `governance-check.mjs` 内」は誤り。ADR 1 件と `governance-check.test.mjs` 1 件を含む。しかも 2 行下の「唯一の文書での言及は ADR」と直接矛盾する | **採用** | §0 を正確な内訳へ書き換え。**`AGENTS.md`「検証の作法」の全称表現の条項を、それを引用している文書自身が破っていた** |

### 壊せなかった項目（6 件）

(a) 呼び出し 0・`export` なし・grep で尽きる ／ (b) 母集団が `docs/adr/`・`docs/superpowers/` を除外 ／ (c) 削除して出力一致・523 テスト passed ／ (d) `.mjs` コメントの識別子を見る検査は存在しない（全 21 検査を確認） ／ (e) `lib.mjs:571`・`lib.test.mjs:113` は生きた契約である ／ (f) ADR と plans は編集不要

**(e) は敵対側も独立にフォールトインジェクションで確認した**が、A/B を分ける中核の判定ゆえ**主エージェント自身が測り直した**（§2e。`AGENTS.md`「検証の作法」の「判定の中核は自分で測る」）。両者の結論は一致する。

### ⚠️（確信の持てない所見・3 件）

| 所見 | 採否 |
|---|---|
| 母集団文書数 33 → 34 の差分が未追跡 | **採用**——追跡して §0 に記載（新規 1 文書の追加が理由・除外条件は不変） |
| §4 の「ADR と plans は凍結ゆえ触らない」が、性格の違う 2 つを「凍結」で一括りにしている | **採用**——§4 で根拠を分けて書き直した |
| BREAK-1/2 は中心的結論に影響しない | **採用**（同意）——誤りは経緯説明の数値に限られる |

### 測定環境への疑い（必須枠）

敵対側が `lib.mjs` の `WALK_EXCLUDE_PATHS` を読み、`workspace/` が**パス名で明示的に**走査除外されていることを確認した（git 管理外だからではない）。また §2a の計器が実際に赤くなることを人工変異で実測した（§2c はその再現）。**測定手法は健全と判断された。**
