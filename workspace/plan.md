# plan — #992 折返しの規約を実測された害へ縮め、害 1 を検査にする

## 目的

`docs/comment-guidelines.md`「日本語の折返し」が定める「1 段落 1 行」は、実測すると規範として機能していない（全ツリー 3151 件・`.rs` 直近 60 コミットの 88% が新規違反を持ち込む・機構を書く側自身が守っていない）。一方、同節が根拠として挙げる**実測された害**は 2 つに限られ、そのうち害 2（正準形の参照が折れる）は #1156 で機構化済み、害 1（コードスパンが行を跨ぐ）は**却下 3 の受容する残余として今も誰も検知していない**——却下 3 が 0 件へ畳んだあと、規範の下で `.rs` に 2 件が新たに生まれている（実測は `workspace/research.md`）。

**害 1 を `governance:check` の検査として新設し、規約を根拠のある 2 条へ縮める。** 機構が 2 つの実測された害の両方を持ち、規範が実践と一致する状態にする。

## 受け入れ条件

1. `npm run governance:check` が `G-folded-code-spans` を含めて exit 0 を返し、evidence 行にその照合件数が現れる。
2. 実在した 6 箇所が畳まれ、検査が 0 件から始まる。
3. 複製へ行またぎスパンを 1 件植えると exit 1 になる（フォールトインジェクションで実測。**稼働中のガードには当てない**）。
4. `docs/comment-guidelines.md`「日本語の折返し」が、根拠のある 2 条だけを規範として持ち、素の散文の折返しを禁じていない。「折返しは機械が持たない」が「整形器は持たないが、害の 2 形は検査が持つ」へ更新されている（issue の撤去条件）。
5. **この変更で偽になる他文書の記述が直っている**（下の「写しの数え上げ」の 2 件）。
6. 編集時にも同じ判定が鳴る（`edit-findings.mjs` 経由・合否は持たない）。
7. 却下した案（差分ベースの検査・機構を足さない・「1 段落 1 行」の存置・**既製の lint / formatter**・実装の 3 形）の否定の知識が ADR に残っている。

## 承認時の前提（明示する）

**利用者の裁定「項の内部も対象にする」は、縮めた後の 2 条に対する射程として実装する**——「表・箇条書き・コードフェンスは対象外」という現行の除外は**構造行そのもの**を指し、**箇条書きの項の中身の散文には 2 条が当たる**と規約へ明記する。害 1 の述語は箇条書きかどうかを見ないので、機構はこの読みと既に一致している。

## 写しの数え上げ（SSOT を変える前に実施済み・`git grep`）

規約の当該条項を引いている生きた層を数え上げた。**この変更で偽になるのは 2 件**である。

| 引用元 | 引いているもの | 判定 |
|---|---|---|
| `.claude/agents/code-reviewer.md`:28 | 「コードスパンを行またぎさせる折返しは、**どの機械検査も見ない**」 | **偽になる。必ず直す**（全称否定が検査の新設で崩れる） |
| `scripts/governance/checks/G-folded-heading-refs.mjs`:33 | 「バッククォート自体が行を跨ぐ形は今日 0 件で、0 である理由は規範が先に効いているため」 | **偽になる。必ず直す**（0 の理由が「規範」から「検査」へ変わる） |
| `.claude/rules/comments.md`:17 | 節見出しへのポインタ | 見出しは残るので影響なし |
| `docs/adr/ADR-folded-canonical-reference-detector.md`:34 | 「文途中で物理改行を入れない」を規範の向きとして引用 | **凍結された歴史。編集しない**（新 ADR から関係を書く） |
| `docs/superpowers/plans/2026-08-10-search-worker.md`:17 | 「1 段落 1 行」を計画本文へ写している | **凍結された歴史。編集しない**（過去サイクルの計画） |

**事後の網としてもう 1 本回す**: Phase 2 の編集後に `node scripts/governance/dependents.mjs docs/comment-guidelines.md`（節の本文が変わったとき、その節に依存する参照を並べる）。

## 変更ファイルと対象シンボル

| ファイル | 変更 |
|---|---|
| `scripts/governance/checks/G-folded-code-spans.mjs` | **新規**。`id` / `run` / `checkFoldedCodeSpans` / `scanFoldedCodeSpans` を export |
| `scripts/governance/checks/G-folded-code-spans.test.mjs` | **新規** |
| `scripts/governance/lib.mjs` | `linesOfComments(text, file, family = commentFamilyOf(file))` — **第 3 引数を足す**。既定引数なので `refScanLines` も既存 3 検査も動かない |
| `scripts/governance-check.mjs`:103,174 | 散文の「21 本」を**数を持たない形へ倒す**（22 へ書き換えない） |
| `scripts/governance-check.test.mjs`:63 | 同上 |
| `scripts/governance/evidence.mjs` | `assembleEvidence` の行へ照合件数を 1 項追加 |
| `scripts/governance/edit-findings.mjs` | `import` 1 行 + `SCAN_SCOPED` へ 1 行（Phase 4.5） |
| `scripts/governance/edit-findings.test.mjs` | 配線を固定する it を 1 本（Phase 4.5） |
| `docs/hooks.md` | 「検査ではない reminder」の表へ 1 行（Phase 4.5） |
| `scripts/governance/evidence.test.mjs` | `complete()` 固定値へ 1 キー追加（実測: 固定値は列挙形。追加しないと読みが `?` になり finding が出る） |
| `PERFORMANCE.md`:668 | 折れたスパンを畳む |
| `src-tauri/src/egui_shell/launcher_controller.rs`:2028 | 同上（下に代案を起案済み） |
| `src-tauri/src/egui_shell/view.rs`:497 | 同上 |
| `scripts/governance/checks/G-stale-identifiers.mjs`:70 | 同上 |
| `scripts/lib/SnotraSmoke.Tests.ps1`:230 | 同上 |
| `scripts/lib/SnotraTraceInvariants.Tests.ps1`:514 | 同上 |
| `scripts/governance/checks/G-folded-heading-refs.mjs`:33 | 死角の宣言のうち「0 である理由」を検査の新設に合わせて直す |
| `.claude/agents/code-reviewer.md`:28 | 「どの機械検査も見ない」から折返しを外す（写しの型は引き続き見ない） |
| `docs/comment-guidelines.md` | 「日本語の折返し」を 2 条へ縮める |
| `docs/adr/ADR-folded-code-span-detector.md` | **新規**。却下した案の否定の知識（`G-adr-file-names` の stem 一致に従い、独立導出が推した名前を採った） |

**`scripts/governance-check.mjs` の配線は変更しない**（実測で確認）——`record` は `sink[key]` へ書き、`runAll` が `...ctx` で evidence の袋へ広げるので、供給は検査側が `ctx.record` を呼べば自動で届く。**触るのは散文の「21 本」だけである。**

**`docs/build-commands.md`:166 とルート `CLAUDE.md`:29 の列挙には足さない**（独立導出の軽微 1 を採る）。どちらも例示であって全称の主張ではないので偽にならず、足せば次の検査で同じ判断が再来する。

## 判定ロジック（実装前に実データで測ってある）

```
行 → /`{2,}.*?`{2,}/g を落とす（`` ` `` のエスケープ形）
   → /`{2,}/g を落とす（コードフェンスの記号）
   → 残った単独バッククォートの個数 t
   → 連続するコメント行にまたがって「span の内側か」を累積で持ち、
     t が奇数なら反転させる。反転後に内側なら、その行が折返しの開始点
```

**行単独の偶奇ではなく累積で持つのが要点である**（未確定(a) の実測で決まった）。行単独だと**同じ折返しを開始行と終了行の 2 回報告する**（実測: 12 行 = 6 箇所）。累積なら開始点だけが立ち、**6 箇所 = 6 件**になる。`G-folded-heading-refs` の「1 行 1 件で報告する」と同じ方針である。

`.rs` は `refScanLines` が**全行**を返すので、コメント行へ絞る。**絞り方は `lib.mjs` の `linesOfComments` へ第 3 引数 `family` を足して再利用する**（`linesOfComments(text, file, "js")`。既定引数は今日の挙動そのままなので `refScanLines` も既存 3 検査も動かない）。

- **行頭の記号を見る正規表現を自前で書いてはならない**——Rust のデリファレンス代入文 59 行をコメントと数える（3b 所見 4a・実測）。
- **`COMMENT_FAMILY` へ `.rs` を足してはならない**——`refScanLines` の意味論が変わり、`G-heading-refs` / `G-near-heading-refs` / `G-folded-heading-refs` の母集団が同じ変更で動く（`.rs` の走査対象が全行からコメント行へ縮む）。#925 以来の意図的な母集団であり #489 に当たる。
- **`.rs` を全行走査したまま char リテラルを特別扱いする形も採らない**——綴りに依存し、Rust の文字列中のバッククォートで再発する（実例: `snotra-settings/src/tabs/backup.rs`:233 の `find('` + バッククォート + `')`）。そもそも規範の対象はコメントである。
- **`linesOfComments` の js 族が自ら宣言している死角を継承する**——「テンプレートリテラルの中で `//` から始まる行をコメントと誤判定する」。Rust では raw string 内の同じ形が当たる。**向きは赤（沈黙ではない）ので受容し、宣言を写さず「同じ死角を継承する」と 1 行で書く。**

**測定結果**: `allHeadingRefDocs` の 262 文書・走査 25956 行に対し**折返しの開始点 6 箇所・偽陽性 0・偽陰性 0**。

## 実装順序

**Phase 1 と Phase 2 は 1 つのコミットへ束ねる。** 検査を入れた時点で実在の 6 箇所が赤くなるので、分けると `governance:check` が落ちる中間状態が生まれる（`AGENTS.md`「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」と同型の理由）。Phase を分けてあるのは作業の順序を示すためであって、コミット境界ではない。

### Phase 1 — 検査の新設

- [x] `G-folded-code-spans.mjs` を書く。母集団は `allHeadingRefDocs`（`G-folded-heading-refs` と同一。**新しい母集団定義を作らない**——`ADR-folded-canonical-reference-detector` の決定に揃える）
- [x] `.rs` のコメント行抽出を状態追跡で書く。**`COMMENT_FAMILY` へ `.rs` を足してはならない**——`refScanLines` の母集団が動き `G-heading-refs` ほかの検査対象が変わる（#925 以来の意図的な母集団・#489）
- [x] 累積の偶奇で「折返しの開始点」だけを finding にする
- [x] `{findings, checked}` を返し `ctx.record` する（「差分ゼロ」と「照合していない」を分ける証跡・#497）
- [x] 検査のヘッダに**射程と死角を宣言する**——`docs/development-principles.md`「検証の層と、層と層の隙間」が要求する「その手段は何を見ていないか／出力は消費する層まで届いているか」に答える形で:
  - 母集団はファイル種別に依らない（`G-folded-heading-refs` と同じ理由）
  - **`.ps1` / `.psm1` にはコメント作法の生きた規範が一つも無い**ので、規範の無い面で機構だけが赤を出すことを名乗る
  - **述語は「行末で span が開いたままか」を見るのであって、折返しの意味は見ない**——非実在の例示コードも同じ重みで赤にする（実例は `launcher_controller.rs` の畳み）
  - 累積を捨てる境界（コメント行が途切れた地点）を宣言する
  - **`linesOfComments` の js 族から継承する死角**（raw string 内の `//` 始まり）を 1 行で名乗る
- [x] **ヘッダとテストが自分自身を赤にしないことを確かめる**——`checks/` はこの検査の走査母集団に入る（`G-stale-identifiers.mjs`:70 が実際にヒットしている）。**例示に実在の折れを置かない**（`G-folded-heading-refs` が同じ理由で明文化している）。`.md` 側（ADR・規約）へ折れた例を書くなら**コードフェンスの内側**に置く
- [x] `evidence.mjs` の行と `evidence.test.mjs` の `complete()` を更新する
- [x] `scripts/governance-check.mjs`:103,174 と `governance-check.test.mjs`:63 の「21 本」を**数を持たない形**（「`checks/` の全検査」等）へ倒す。**22 へ書き換えない**——`AGENTS.md`「検証の作法」の「数え上げは偽になる時点が確定している。数ではなく正本を指す」に当たる
- [x] `G-folded-code-spans.test.mjs` を書く

### Phase 2 — 実在した 6 箇所を畳む

- [x] `src-tauri/src/egui_shell/launcher_controller.rs`:2028 — **スパンを 2 つへ割る**（未確定(c) で起案し、説明が成立することを確認済み）:
  ```
  /// **代償は整形と書き方への脆さで、向きは赤側である**: `let should = crate::egui_shell::…;` と
  /// `if should {` へ割る分解や、rustfmt が `if` とパスのあいだで折り返す形はここを赤にする。
  ```
- [x] 残る 5 箇所を畳む（`PERFORMANCE.md`:668 / `view.rs`:497 / `G-stale-identifiers.mjs`:70 / `SnotraSmoke.Tests.ps1`:230 / `SnotraTraceInvariants.Tests.ps1`:514）
  - `PERFORMANCE.md`:668 は折り返し点を移すだけにする——**実測値の写しを増やさない**（同文書「この文書へ記録するときの規約」）
  - `.ps1` の 2 件は**同じ文言の重複**（`-ErrorAction SilentlyContinue` の説明）。片方を直して片方を忘れる形が起きやすいので**同じコミットで両方**
- [x] `npm run governance:check` が exit 0・照合件数が evidence 行に出ることを確認

### Phase 3 — 規約を実測された害へ縮める

- [x] `docs/comment-guidelines.md`「日本語の折返し」を書き換える。**見出し名は変えない**（生きた正準形の参照が 3 か所ある。凍結 ADR 側の参照は照合の母集団外なので、改名すると赤くならずに死ぬ）:
  - **58 行（規範の本文）**: 規範を 2 条にする — **コードスパン（バッククォートで囲んだ識別子・コマンド）を行またぎさせない**／**正準形の見出し参照を物理改行で折らない**。どちらも検査が持つ（`G-folded-code-spans` / `G-folded-heading-refs`）
  - **58 行（射程の分割）**: 「適用は新規に書くコメントと、その変更で触った段落だけである」は**この 2 形については消える**——機構が全ツリーを毎回走るため。**「この 2 形は既存・新規を問わず機構が見る。それ以外の書き方の条項は新規・touched だけに適用する」と射程の分割を明示する**。しないと `ADR-folded-canonical-reference-detector` が退けた「機構と規範が逆を向く」形になる
  - **素の散文の折返しは禁じない。** 実測（全ツリー 3151 件・害に当たるのは 0.1%）を根拠として 1 文で書き、**再導入されないようにする**
  - **箇条書きの項の内部にも 2 条が当たる**ことを明記する（除外は構造行そのもの＝表の行・箇条書きの行頭記号・コードフェンスを指す）
  - **62 行（害 1）**: 末尾に `G-folded-code-spans` を名指す（63 行が `G-folded-heading-refs` を名指すのと平仄を揃える）
  - **63 行（害 2）**: 「**この 1 形だけは**規範ではなく機構が見る」が**偽になる**（2 形とも機構が見る）。書き換える
  - **64 行**: 「折返しは機械が持たない」→「**整形器は持たないが、害の 2 形は検査が持つ**」（issue の撤去条件）。**`wrap_comments` をバッククォートで囲まないこと**——この文書は `staleIdentifierTargets` に居り、現行語彙に無い外部識別子を span へ置くと `G-stale-identifiers` が赤になる
  - **射程が `.rs` を越えることを規範側にも書く**。検査は `.md` と `.ps1` / `.psm1` も見るが、本書の配送は 4 crate の `**/*.rs` だけである。⚠️ 冒頭 3 行が自ら「コードコメントの書き方」と名乗っているので、`.md` の散文の折れ（`PERFORMANCE.md`:668 が実例）は**規範が一言も無い面**である。1 文で受ける
  - 「禁則処理の規則は置かない」の段は**そのまま残す**（別の論点）。ただし**節の前半を縮めた後に論旨の接続先を失っていないか読み直す**——あの実測は「1 段落 1 行」の体制下で取られたものである
- [x] 偽になる 2 件を直す — `.claude/agents/code-reviewer.md`:28 と `G-folded-heading-refs.mjs`:33（上の表）
- [x] `node scripts/governance/dependents.mjs docs/comment-guidelines.md` を回し、数え上げが取りこぼした依存が無いか見る
- [x] 全称表現を検算する——書いた「2 条だけ」「実測された害は 2 つ」に対し「何が増えたらこの文は偽になるか」を 1 つ挙げ、前提条件として書き添えるか下限の主張へ弱める（`AGENTS.md`「検証の作法」）。**全称否定（「どの検査も見ない」型）を新しく書かない**——今回まさにその形が偽になった

### Phase 4 — 否定の知識を ADR へ

- [x] `docs/adr/ADR-folded-code-span-detector.md` を新設し、却下した案を実測つきで残す（1 行目は `# ADR-folded-code-span-detector: <題>`。`G-adr-file-names` が stem との一致を要求する。**連番を振らない**）:
  - **却下 A**: 差分ベースの `G-` 検査（`checks/` 初の git 依存・CI は depth 1 で base を持たない・fail-open の `checked=0` が正常状態・費用は毎 PR 中央 19〜20 件で**上限ではなく下限**）
  - **却下 B**: 「1 段落 1 行」を**検査の代わりに** post-edit reminder で鳴らす（母集団は構造的に一致するが、中央 19 件はほぼ毎編集で鳴り、鳴っていることが「見た」を意味しなくなる）。**縮めた 2 条を検査に加えて前倒しすることは却下していない**——それは Phase 4.5 で採用しており、件数が 0 から始まるので同じ問題を持たない
  - **却下 C**: 機構を足さず規約だけ縮める（害 1 の再発が誰も検知しないまま残り、却下 3 の受容する残余が生き延びる）
  - **却下 D**: 「1 段落 1 行」を規範として残す（実測 3151 件・機構の書き手自身が守っていない）
  - **却下 3 との関係を正確に書く**——`ADR-comment-guideline-delivery-by-pointer`「却下 3」は 2026-08-08 に同案を退けており、`ADR-folded-canonical-reference-detector` はその**理由**を陳腐化と宣言しつつ「射程も違う」として**決定そのものは再考していない**。**対称性の議論はこの決定が独自に立てるものである**と名乗り、支える一次証拠（`git blame` による規範導入日との前後・`.rs` に 2 件）を書く
  - **却下（実装の形）**: `COMMENT_FAMILY` へ `.rs` を足す／`.rs` を全行走査したまま char リテラルを除外する／折れを吸収して母集団へ取り込む（検査ではなく `grep` の害なので吸収先が無い）
  - **却下: 既製の lint / formatter へ委ねる**（利用者の問いから生じた。**固定版 1.98 で実測**）:
    - **rustfmt**: `wrap_comments = true` は `Warning: can't set ... unstable features are only available in nightly channel` を出して**無視され**、折れは 1 文字も直らない（exit 0）。仮に nightly へ移しても**向きが逆**である——`comment_width = 80` へ**折り込む**オプションであり、8000 行超のコメントを一斉に折って害 1 を量産する
    - **clippy**: pedantic + `doc_markdown` + `doc_lazy_continuation` + `doc_link_code` + `doc_broken_link` + `rustdoc::all` を折れたスパンへ当てて **0 件**（完全な沈黙）。`doc_paragraphs_missing_punctuation` だけが鳴るが、**「。」を終端と認めない誤検出 2 件**であり日本語 doc では使えない
    - **母集団が届かない**: 実在 6 箇所の内訳は `.md` 1 / `.rs` 2 / `.mjs` 1 / `.ps1` 2。完璧な Rust lint でも **6 件中 2 件**しか覆わない
    - **原理**: 折れたスパンは**正しい CommonMark** であり rustdoc は soft line break を跨いで正しく描画する。害は `grep` にしか出ず、**整形器も lint も「人間が正規表現で検索する」ことをモデルに持たない**。規約自身の「壊れるのは検索だけである（だから気づかれない）」がこの理由の正本である
  - **凍結層との非対称を 1 文で名乗る**: 生きた散文からは「21 本」という数を捨てるが、`ADR-governance-meta-demotion` の**見出し**は数を含んだまま残り、`ADR-folded-canonical-reference-detector`:19 がそれを正準形で引用している。見出しは凍結された歴史なので触らない
  - **受容する残余**: 「1 段落 1 行」を捨てたことで、素の折返し（3149 件）は今後も誰も見ない。#984 型の見落としは再発しうる
- [x] 既存 ADR は**編集しない**（凍結された歴史）。`ADR-comment-guideline-delivery-by-pointer` の「却下 3」「行またぎスパンの再発は誰も検知しない」「折返し・訳語の条項は 100% 規範であって機構ではない」は**すべて偽になるが書き換えない**——`ADR-folded-canonical-reference-detector` が「却下 3 の理由が陳腐化していたこと」を**新しい ADR の側へ**置いた先例に倣う

### Phase 4.5 — 編集時への前倒し（利用者の裁定・2026-08-22）

- [x] `edit-findings.mjs` へ `import { checkFoldedCodeSpans }` と `SCAN_SCOPED` の 1 行を足す（走査元を編集ファイル 1 枚へ絞るので帰属は構造的に決まる。前倒しの条件は `ADR-edit-time-check-scope`「決定」——本検査は**着地先を持たない**ぶん既存 4 本より強く条件を満たす）
- [x] `edit-findings.test.mjs` へ「赤: コードスパンが行をまたぐ」の it を 1 本足す
- [x] `docs/hooks.md`「検査ではない reminder（発火一覧に現れない）」の表へ 1 行足す（**射程の穴の一覧はここが正本**であり、`edit-findings.mjs` の `//!` へ写さない）

### Phase 5 — 検証と確定

- [x] フォールトインジェクション: **複製の worktree** へ行またぎスパンを 1 件植え、`node scripts/governance-check.mjs` が exit 1 を返すことを実測する。**変異が本来の回帰と同じ強さか**を確かめる（実際に起きた 6 箇所と同じ形を植える）
- [x] 逆向き: 変異を戻して exit 0 に戻ることを実測する
- [x] `npm test`（vitest）が緑
- [x] `.rs` を 2 枚触るので `docs/build-commands.md` カテゴリ A、ガバナンス文書を触るのでカテゴリ F を実行
- [x] 実装差分を確定させる（**PR 本文で構造母集団の変更を宣言する**——`governance-manifest.mjs` の `KEYS` に `checks` が入っており、宣言が無いと `governance manifest delta` step が落ちる・#1088）

## 不変条件と異常系

- **`refScanLines` の母集団を動かさない。** `.rs` のコメント絞り込みは新設の検査の内側に持つ。
- **`checks/` の外に置かない。** `checks/` 直下に `<id>.mjs` + `<id>.test.mjs` の対で置く。ファイル名と `id` の食い違いは `registry.mjs` が throw で拒む。
- **依存ゼロ・決定的**（Node 標準のみ・ネットワーク/時刻/環境変数に非依存）。この検査は git を触らない——それが案 A を採らなかったことの構造的な帰結である。
- **`checked` が 0 に落ちたら異常である。** 母集団が空になる経路は `runAll` の既存の 0 件検知が既に見ている。**新しい 0 件検知は足さない**（`ADR-governance-meta-demotion` の格下げの線）。
- **読めない文書は finding にする**（`G-folded-heading-refs` と同型）。
- **累積の状態はコメント行が途切れたら捨てる。** 捨てないと、別のコメントブロックの偶奇が混ざって沈黙側・誤報側の両方へ倒れうる。
- **`## 日本語の折返し` の見出し名を変えない。** 生きた参照 3 件は `G-heading-refs` が捕まえるが、凍結 ADR 側の 3 件は照合の母集団外なので**赤くならずに静かに死ぬ**。
- **`linesOfComments` の第 3 引数は既定引数で足す。** 既存の呼び出し点の挙動を 1 ミリも変えない。

## テスト方針と検証コマンド

`G-folded-code-spans.test.mjs`:

1. 行またぎスパンを検知する（フォールトインジェクション red）
2. 同一行で閉じるスパンを検知しない
3. コードフェンスの記号（3 連）で誤検知しない
4. `` ` `` のエスケープ形で誤検知しない
5. **Rust のデリファレンス代入文をコメントと数えない**（3b 所見 4a の回帰テスト）
6. **1 つの折返しを 2 回報告しない**（累積の偶奇。未確定(a) の実測を固定する）
7. **コメントブロックが途切れたら累積を捨てる**
8. 読めない文書を finding にする

コマンド: `npm run governance:check` / `npm test` / 複製でのフォールトインジェクション。

## SPEC.md・関連文書の更新要否

- **`SPEC.md`: 不要。** 製品の挙動を変えない。
- **`AGENTS.md`「条件別チェック」: 不要。** 新検査は `npm run governance:check` に入るので既存のトリガー行が既に覆う。
- **`docs/hooks.md`: 要**（Phase 4.5 で「検査ではない reminder」の表へ 1 行）。
- **各 `CLAUDE.md` のモジュール索引: 不要**（索引の母集団は 4 crate の `.rs`）。

## 裁定済みの論点

- **編集時 reminder（`edit-findings.mjs` の `SCAN_SCOPED`）へ配線する** — 利用者の裁定（2026-08-22）。同じ変更で `edit-findings.test.mjs` の it 1 本と `docs/hooks.md`「検査ではない reminder」の表 1 行が要る。→ Phase 4.5 として下に置いた
- **既製の lint / formatter には委ねられない** — 利用者の問いに対し固定版 1.98 で実測（上の ADR 却下案）

## 未確定（実装前に潰す）

（なし——下の 4 件はすべて実測で解消し、結果を計画本文へ反映済み）

- [x] **偶数個のバッククォートに化ける偽陰性の実例が在るか** — **0 件**。`allHeadingRefDocs` 全体で累積の偶奇と行単独の偶奇を突き合わせ、「累積は内側なのに行単独が偶数」の行は存在しなかった。**代わりに設計上の発見があった**——行単独だと同じ折返しを 2 回報告する（12 行 = 6 箇所）。累積で持つ形へ変更し、判定ロジックの節へ反映した
- [x] **`launcher_controller.rs`:2028 を畳む形が説明の意味を保つか** — **保つ**。スパンを 2 つへ割る形（Phase 2 に逐語で起案）で、「`let` と `if` へ分解すると赤になる」という説明はそのまま成立する。フェンスへ出す案は不要
- [x] **`evidence.test.mjs` のカナリアが何を要求するか** — `complete()` は**キーの列挙形の固定値**であり、キーを足さないと `evidenceView` の読みが `?` になって finding が出る。**`governance-check.mjs` 側は変更不要**（`record` → `sink` → `...ctx` で自動的に届く）。変更ファイル表を修正済み
- [x] **編集時 reminder を足すか** — **利用者が「配線する」を裁定した**（2026-08-22）。分岐が消えたので Phase 4.5 として作業項目へ落とし、負けた枝は削除した
- [x] **既製の lint / formatter で強制できないか**（利用者の問い） — **できない。固定版 1.98 で実測**。rustfmt の `wrap_comments` は stable では警告して無視され（折れは 1 文字も直らない）、nightly でも向きが逆（折り込む側）。clippy は doc lint を全部有効にしても折れたスパンに **0 件**、`doc_paragraphs_missing_punctuation` だけが「。」を終端と認めず誤検出 2 件。加えて母集団が届かない（実在 6 箇所のうち Rust は 2 件のみ）。ADR の却下案へ記録した

## 人間レビュー

- [x] 承認済み — 2026-08-22 / 問い: "`workspace/plan.md` をこの内容で承認いただけますか。承認後は workspace をコミット・push し、実装は `/implement` へ渡します。" / 回答: "承認、進めよう"

## plan-review 結果

- リスク: **高**（ガバナンス機構の新設 + 規範文書の縮小 + 他文書の全称否定の訂正）
- レビュー方式: 独立導出 1 体（`--deep`。`workspace/` の 3 枚を読ませず、issue の WHAT と利用者の裁定だけを渡してコードから再導出させた）
- エージェント数: 2（Step 3b の敵対的調査 1 体 + 独立導出 1 体）
- 全文: `workspace/plan-review-independent-derivation.md`

**導出 ∖ plan（漏れ候補・すべて採用。根拠は主エージェントが再照合済み）**

- **A**: `.rs` のコメント絞り込みは `linesOfComments` の第 3 引数で再利用する（自前の述語より安全で、`refScanLines` を動かさない）
- **C**: 散文の「**21 本**」が 3 か所で偽になる（`governance-check.mjs`:103,174 / `governance-check.test.mjs`:63。`git grep` で実在を確認）。**22 へ書き換えず、数を持たない形へ倒す**
- **D**: 規範の適用範囲の文（「新規・touched だけ」）と機構の射程（全ツリー）が食い違う。射程の分割を明示する
- **E**: 規範の配送は 4 crate の `.rs` だけなのに検査は `.md` / `.ps1` も見る。射程を規範側にも書く
- **F**: `checks/` はこの検査の走査母集団に入るので、**ヘッダに実在の折れを例示すると自分の finding になる**
- `docs/comment-guidelines.md`:**63 行**「この 1 形だけは規範ではなく機構が見る」が偽になる（実在を確認）
- `wrap_comments` をバッククォートで囲むと `G-stale-identifiers` が赤になる
- ADR のファイル名を `ADR-folded-code-span-detector.md` にする（`G-adr-file-names` の stem 一致）
- 軽微 5〜7（`PERFORMANCE.md` の写しを増やさない・`.ps1` 2 件を同じコミットで・禁則段落の論旨の接続）

**plan ∖ 導出（スコープ過剰候補）**: なし。**違反箇所の数え上げは 6 件で一致した**（独立に述語を 3 通り書いて突き合わせている）。

**降格した項目**: 軽微 1（`docs/build-commands.md`:166 とルート `CLAUDE.md`:29 の列挙へ 1 語足す）— **足さない**。どちらも例示であって全称の主張ではないので偽にならず、足せば次の検査で同じ判断が再来する（要対処 C と同じ型）。導出側も足さない側を推している。

**未検証**: なし（独立導出が推した `edit-findings.mjs` への配線は、利用者が「配線する」を裁定して Phase 4.5 になった）。

**判断**: 実装着手可（人間の承認を待つ）。

## 自己照合（5a）

1. **issue の全要件に作業項目が対応するか** — ⚠️ **対応していない部分がある。** issue が求めた「1 段落 1 行の検査」は実測で退け、利用者の裁定で害の側へ振り替えた。issue の撤去条件（規約の一文の更新）は Phase 3 が満たす。**#984 型の見落とし（素の折返し）には応えない**——意図的であり、ADR の「受容する残余」に残す
2. **境界条件と検証** — フェンスの記号・エスケープ形・デリファレンス代入文・同一行で閉じるスパン・二重報告・ブロックの切れ目・読めない文書。テスト方針に 1 件ずつ置いた
3. **新しい状態・リソース・プロセスの経路** — 新しい永続状態を持たない（純関数・snapshot 注入）。走査中の累積状態だけが state であり、**捨てる経路**（ブロックの切れ目）を不変条件とテスト 7 で固定した
4. **より単純な既存パターンで置き換えられないか** — `G-folded-heading-refs` と同一の母集団・返り値形・ファイル配置を採っており、新しい構造を作っていない
5. **壊してはならない不変条件に検知手段があるか** — `refScanLines` の母集団を動かさないことは、動かせば evidence 行の照合件数が変わるので気づく。検査自身が効くことは Phase 5 のフォールトインジェクションで実測する
