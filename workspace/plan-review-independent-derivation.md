# 独立導出レビュー — issue #992（コードスパンの行またぎ検知器の新設）

対象 issue: **#992**（`governance:check` へ「1 段落 1 行」の検査を足す → 母集団 3000 件超で不成立と実測され、利用者が **「コードスパンが物理改行を跨ぐ形」だけを機構化する**方向へ裁定）。ブランチ `feat/paragraph-wrap-check`、ベース `a0d94d1`。

本書は **`workspace/plan.md` / `workspace/research.md` / `workspace/adversarial-992.txt` を読まずに**、コードと規範文書だけから必要な変更を再導出したものである。概念ラベルの grep はすべて `git grep`（未追跡の `workspace/` が母集団に入らない）で行った。

---

## 0. 測り方（一次証拠の出所）

- ベースライン: `npm run governance:check` → **exit 0 / 検査 21 件 / 対象文書 36 件 / 見出し参照 328 件 / 折れうる位置 24 件**（2026-08-22 実測）
- 違反の数え上げは使い捨てスクリプト 3 本で 3 通りの述語を当て、結果を突き合わせた:
  - `scratchpad/derive992-scan-spans.mjs`（素朴なスタック対応・`.rs` 全行・run 長無制限）→ **22 行**
  - `scratchpad/derive992-scan-spans-v2.mjs`（逐次対応・run 長 3 以上を無視・`.rs` はコメント行のみ）→ **12 行**（＝ 6 組の開始行と終了行）
  - `scratchpad/derive992-scan-spans-v3.mjs`（v2 ＋ 段落単位の状態・開始行 1 件だけを報告）→ **6 件**
  - 独立の道具による対照: `git grep -P` で「バッククォート数が奇数の行」を数えると **250 行**。標本を読むと大半が ` ``` ` フェンス標識で、散文の折れは `PERFORMANCE.md:668/669` の 1 組だけだった（v3 の結果と一致）
- 母集団の実在確認（`scratchpad/derive992-probe-populations.mjs`）: `docs/comment-guidelines.md` は `governanceDocs` / `allHeadingRefDocs` / `staleIdentifierTargets` の**すべて**に居る。`.claude/agents/code-reviewer.md` は `allHeadingRefDocs` に居るが `governanceDocs` には**居ない**。`.ps1` / `.psm1` の走査元は 22 件。

---

## 1. 変更・新規作成が必要なファイル

検査 ID は **`G-folded-code-spans`** を提案する（`G-folded-heading-refs` と同じ族であることが名前で分かり、連番を持たない・`.claude/rules/governance-docs.md`「ガバナンス文書の参照と命名のルール」）。

### 新規（3 枚）

| パス | 中身 |
|---|---|
| `scripts/governance/checks/G-folded-code-spans.mjs` | 検査本体。`export const id` / `export function run` / 判定の純関数 / 射程・死角の宣言ヘッダ |
| `scripts/governance/checks/G-folded-code-spans.test.mjs` | 正例・負例・`checked` の固定。`G-folded-heading-refs.test.mjs` と同じ骨格 |
| `docs/adr/ADR-folded-code-span-detector.md` | 却下した代替案の記録。冒頭は `# ADR-folded-code-span-detector: …` の形（`G-adr-file-names` が stem との一致を要求する） |

`checks/` へファイルを置けばそれだけで検査になる（`scripts/governance/registry.mjs` がディレクトリ走査から導出）。**登録行を足す面は存在しない。**

### 変更（機構）

| パス | 触る対象 | 理由 |
|---|---|---|
| `scripts/governance/lib.mjs` | `linesOfComments`（第 3 引数 `family` の追加） | `.rs` のコメント行だけを取りたいが、`COMMENT_FAMILY` へ `.rs` を足すと `refScanLines` の意味論が変わり、既存 3 検査の母集団が同じ変更で動く（→ §6 要対処 A） |
| `scripts/governance/evidence.mjs` | `assembleEvidence` のテンプレート（`ev.codeSpans` を 1 項追加） | 「照合していない」と「差分ゼロ」を分ける証跡（#497）。既存 21 本すべてが `ctx.record` → evidence の対で運用されている |
| `scripts/governance/edit-findings.mjs` | `SCAN_SCOPED` 配列へ 1 行、`import` 1 行 | 編集時 reminder への配線（→ §6 要対処 B） |
| `scripts/governance/edit-findings.test.mjs` | 「赤: コードスパンが行をまたぐ」の it を追加 | 配線の固定 |
| `scripts/governance-check.mjs` | 103 行・174 行の「21 本」 | 検査が 22 本になり **2 か所が偽になる**（→ §6 要対処 C） |
| `scripts/governance-check.test.mjs` | 63 行の「21 本」 | 同上 |
| `scripts/governance/checks/G-folded-heading-refs.mjs` | 32〜33 行の**宣言する死角** | 「バッククォート自体が行を跨ぐ形。今日 0 件で、0 である理由は規範が先に効いているため」——**理由が「新検査が赤にするため」へ変わる**（生きたファイルなので更新する） |

### 変更（規範）

| パス | 触る対象 | 理由 |
|---|---|---|
| `docs/comment-guidelines.md` | `## 日本語の折返し` の 58 行・62 行・63 行・64 行 | 規範を 2 つの害へ縮める・「折返しは機械が持たない」の更新（issue の撤去条件）・62 行に新検査を名指す |
| `.claude/agents/code-reviewer.md` | 28 行 | 「**コードスパンを行またぎさせる折返しは、どの機械検査も見ない**」が**偽になる**。レビュアへ「機構が見るので人が探さなくてよい」を届けないと、費用が二重に乗る |
| `docs/hooks.md` | 「検査ではない reminder（発火一覧に現れない）」の表 | 編集時へ配線するなら発火条件の行が要る（`edit-findings.mjs` の `//!` が「射程の穴の一覧はここに写さない——`docs/hooks.md` が正本」と宣言している） |
| `docs/build-commands.md` | 166 行の説明文の列挙 | 「参照実在・モジュール索引・スキル表・SPEC 番号・rules glob・コマンド写像・見出し参照の着地」に**書き方の族が入っていない**（→ §7 軽微） |

### 変更（実在する違反の是正・6 枚）

`PERFORMANCE.md` / `src-tauri/src/egui_shell/launcher_controller.rs` / `src-tauri/src/egui_shell/view.rs` / `scripts/governance/checks/G-stale-identifiers.mjs` / `scripts/lib/SnotraSmoke.Tests.ps1` / `scripts/lib/SnotraTraceInvariants.Tests.ps1`（内訳は §3）。

### **変更してはならない**もの（凍結された歴史・`ADR-adr-frozen-history`）

- `docs/adr/ADR-comment-guideline-delivery-by-pointer.md`（25〜27 行の「却下 3」・36 行の「行またぎスパンの再発は誰も検知しない」・37 行の「折返し・訳語の条項は 100% 規範であって機構ではない」）——**すべて偽になるが書き換えない**。先例は `ADR-folded-canonical-reference-detector.md` が「却下 3 の理由が陳腐化していたこと」という節を**新しい ADR の側へ**置いた形である。同じ手を採る
- `docs/adr/ADR-folded-canonical-reference-detector.md`（44 行の死角の記述）
- `docs/superpowers/plans/2026-08-10-search-worker.md:17`（「適用は `.rs` である」——PowerShell は既存作法へ合わせよ、と書いてある。新検査は `.ps1` を赤にするので**内容が食い違う**が、#589 で非規範化された歴史資料であり走査母集団の外）

---

## 2. 各ファイルで触るシンボル

### `checks/G-folded-code-spans.mjs`（新規）

```
export const id = "G-folded-code-spans";
export function run(snapshot, ctx)          // → ctx.record("codeSpans", scanFoldedCodeSpans(snapshot, ctx.allRefDocs))
export function scanFoldedCodeSpans(snapshot, docs) → { findings, checked }
export function checkFoldedCodeSpans(snapshot, docs) → findings   // edit-findings.mjs 用の口
const spanScanLines(text, file, findings)   // .md → linesOutsideFences / .rs → linesOfComments(text, file, "js") / それ以外 → linesOfComments
const runsOf(line)                          // /`+/g
```

- **母集団は `ctx.allRefDocs`（`allHeadingRefDocs`）**。新しい母集団定義を作らない——`ADR-folded-canonical-reference-detector`「決定」が同じ選択をしており、`ADR-governance-corpus-reduction-rejected` の実測（検査の改修費用 21 件のうち 17 件が母集団・メタ層の変更由来）が新設の費用を名指している
- **`ctx.record` のキーは `codeSpans`**（`checked` ＝候補スパン数。実測 17628 件）

### `lib.mjs`

`linesOfComments(text, file, family = commentFamilyOf(file))` — 既定引数は今日の挙動そのままで、`refScanLines` も `COMMENT_FAMILY` も動かない。

### `evidence.mjs`

`assembleEvidence` の文字列へ `` ` / コードスパン ${ev.codeSpans} 件` `` を 1 項。

### `edit-findings.mjs`

`import { checkFoldedCodeSpans } from "./checks/G-folded-code-spans.mjs";` と `SCAN_SCOPED` へ `{ population: allHeadingRefDocs, check: checkFoldedCodeSpans }`。

### `docs/comment-guidelines.md`「日本語の折返し」

見出し名 **`## 日本語の折返し` は変えない**——生きた正準形の参照が 3 か所ある（`.claude/rules/comments.md:17` ・`.claude/agents/code-reviewer.md:28` ・`scripts/governance/checks/G-folded-heading-refs.mjs:33`）。凍結 ADR 側の参照は `G-heading-refs` の母集団外なので、改名しても**赤くならずに死ぬ**（→ §7 軽微）。

触る 4 行:

- 58 行（規範の本文）: 「文途中で物理改行を入れない（1 段落 1 行）」を捨て、**禁じるのは 2 形だけ**（コードスパンの行またぎ／正準形の参照の折れ）へ縮める。**箇条書きの項の内部にも当たる**ことを書き、除外は「構造行そのもの（表の行・箇条書きの行頭記号・コードフェンス）」だと言い直す。「適用は新規に書くコメントと、その変更で触った段落だけである」は**この 2 形については消える**（機構が全ツリーを見るため。→ §6 要対処 D）
- 62 行（害 1・grep）: 末尾に新検査の名指しを足す。63 行が `G-folded-heading-refs` を名指すのと**平仄を揃える**
- 63 行（害 2・機構の入力）: 「**この 1 形だけは**規範ではなく機構が見る」が**偽になる**（2 形とも機構が見る）
- 64 行（機械が持たない）: issue の撤去条件。「整形器は持たないが検査は持つ」へ。**`wrap_comments` をバッククォートで囲まないこと**——`docs/comment-guidelines.md` は `staleIdentifierTargets` に居り（実測）、リポジトリの現行語彙に無い外部識別子を span に置くと `G-stale-identifiers` が赤になる（`ADR-stale-identifier-detector-scope` が同じ行について記録している）

---

## 3. 実在する違反箇所（自分で数え上げた結果）

**判定の述語**（自分で決めたもの）:

1. 走査行は `.md` ＝ `linesOutsideFences`（フェンス内を落とす）、コメント記法を持つファイル ＝ `linesOfComments`、`.rs` ＝ コメント行のみ
2. 各行のバッククォート **run**（連続する `` ` ``）を左から**逐次**に対応付ける。開いた run と**同じ長さ**の次の run が閉じる
3. **run 長 3 以上は無視する**——フェンス標識と「3 連を例示する 4 連」がここに落ちる
4. 段落（連続する走査行）をまたいで状態を持ち、**開いた行と閉じた行が違う**ものだけを finding にする。空行・行番号の不連続で状態を捨てる

**結果: 6 件**（候補スパン 17628 件・母集団 262 文書）。

| # | ファイル:行 | 折れているスパン |
|---|---|---|
| 1 | `PERFORMANCE.md:668` | `` `if seen.insert(key) { push }` `` |
| 2 | `src-tauri/src/egui_shell/launcher_controller.rs:2028` | `` `let should = crate::egui_shell::…; if should {` `` |
| 3 | `src-tauri/src/egui_shell/view.rs:497` | `` `RawInput.system_theme = Some(Light)` `` |
| 4 | `scripts/governance/checks/G-stale-identifiers.mjs:70` | `` `gh pr view <PR> --json closingIssuesReferences` `` |
| 5 | `scripts/lib/SnotraSmoke.Tests.ps1:230` | `` `-ErrorAction SilentlyContinue` `` |
| 6 | `scripts/lib/SnotraTraceInvariants.Tests.ps1:514` | `` `-ErrorAction SilentlyContinue` `` |

**除外した偽陽性 10 行**（v1 が拾い v3 が落としたもの・すべて目視で分類）:

- `snotra-settings/src/tabs/backup.rs:233,234` — Rust の char リテラル `'` + バッククォート + `'`。**`.rs` の非コメント行を見ると必ず起きる**（→ §6 要対処 A の根拠）
- `src-tauri/src/icon.rs:441,444` — rustdoc のコードフェンス標識（run 長 3）
- `scripts/governance/lib.mjs:72,146,147,588,589` — `` `` … `` `` の 2 連区切り／3 連の例示。**逐次対応にすれば消える**（スタック対応だと誤って開いたままになる）
- `scripts/governance/lib.test.mjs:552` — 4 連と 3 連の混在

**母集団の外の対照**: `docs/adr/**` ＋ `docs/superpowers/**`（151 文書・バッククォートを持つ行 7838）で **0 件**。ゆえに **母集団を `allHeadingRefDocs` に取ることで是正対象が減ることは今日は無い**（この一致は今日の実測であって将来の保証ではない）。

**修正の形**: 「1 段落 1 行へ畳む」ではなく、**スパンが 1 物理行に収まる位置へ折り返し点を移す**。規範から一般則を落とす以上、素の散文の折返しは残してよい。

---

## 4. この変更で偽になる散文（識別子ではなく概念ラベルで grep した）

grep したラベル: `折返し` / `折り返し` / `折れ` / `1 段落 1 行` / `コードスパン` / `行またぎ` / `行をまたい` / `バッククォート` / `21 本` / `どの機械検査も見ない` / `機構ではない`。

**生きた層（直す）**

| file:line | 現在の主張 | どう偽になるか |
|---|---|---|
| `.claude/agents/code-reviewer.md:28` | 「この型と、コードスパンを行またぎさせる折返しは、**どの機械検査も見ない**」 | 全称否定が偽になる。**レビュア 1 体ぶんの費用が浮く行**なので、直さないと「機構が見ているのに人も探す」二重課税が残る |
| `docs/comment-guidelines.md:58` | 「文途中で物理改行を入れない（1 段落 1 行）」「適用は新規・touched だけ」 | 一般則を捨てる／機構は全ツリーを見る |
| `docs/comment-guidelines.md:63` | 「**この 1 形だけは**規範ではなく機構が見る」 | 2 形とも機構が見る |
| `docs/comment-guidelines.md:64` | 「折返しは機械が持たない」 | issue の撤去条件そのもの |
| `scripts/governance/checks/G-folded-heading-refs.mjs:32-33` | 死角「バッククォート自体が行を跨ぐ形。**0 である理由は規範が先に効いているため**」 | 0 を保つ主体が規範から機構へ移る |
| `scripts/governance-check.mjs:103` | 「母集団が空になれば **21 本**は空虚に緑を返す（21 本自身の故障）」 | 22 本になる |
| `scripts/governance-check.mjs:174` | 「静かに過小になるだけで、**21 本**の合否は動かない」 | 同上 |
| `scripts/governance-check.test.mjs:63` | 「倒れても **21 本**の合否は動かない」 | 同上 |
| `docs/build-commands.md:166` | governance:check の内容の列挙 | 書き方の族が入っていない（軽微） |
| `docs/hooks.md:110` | 編集時 reminder の発火条件「着地しない正準形・近傍形・物理改行で折れた形」 | 編集時へ配線するなら足りない |

**凍結層（直さない・新 ADR で受ける）**

`docs/adr/ADR-comment-guideline-delivery-by-pointer.md:25,27,36,37` ・ `docs/adr/ADR-folded-canonical-reference-detector.md:23,30,44` ・ `docs/adr/ADR-edit-time-check-scope.md:18`（「21 本すべてを編集時に配線する」）・`docs/superpowers/plans/2026-08-10-search-worker.md:17`。

**偽にならないもの（確認済み・触らない）**

- `SPEC.md:306` の「この行のコードスパンには実出力だけを置く」——別概念（スパンの中身の規約）
- `.claude/rules/comments.md:17` / `.claude/rules/governance-docs.md:18` ——見出し名を変えなければそのまま真
- `.claude/skills/health-check/references/mechanized-checks.md` ——旧 Check → G の対応表であり、新設検査は行を持たない
- `scripts/governance-manifest.mjs` / `registry.test.mjs` / `governance-manifest.test.mjs` ——検査 ID を**逐語で列挙していない**（走査から導出）ので変更不要

---

## 5. 検証手段——何が担保され、何が担保されないか

**担保される**

| コマンド | 担保するもの |
|---|---|
| `npm run governance:check` | 新検査を含む 22 本が findings 0 件・evidence 行に `コードスパン 17628 件` 相当が出る（数字は是正後に再測する）。**是正漏れが 1 件でもあれば exit 1** |
| `npm test`（`vitest run`） | `G-folded-code-spans.test.mjs` の正例・負例・`checked`、`registry.test.mjs` の形式検証、`edit-findings.test.mjs` の配線、`governance-check.test.mjs` の evidence 供給カナリア |
| CI `governance-check` job | `skip-ci` 非対象・`if` ガード無しで常時実行（`.github/workflows/ci.yml:58,73`）。**Markdown-only PR でも走る** |
| CI `governance manifest delta` step | 構造母集団の差分 `+G-folded-code-spans` が **PR 本文に逐語で**書かれているか（`ci.yml:111`）。本文へ `## governance manifest delta` と `+G-folded-code-spans` を置くこと |
| フォールトインジェクション（`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」） | **複製に変異を当てる**（稼働中を弱めない）。最低 2 方向: ①是正済みの 1 件を折り直して赤になるか ②判定の中核（run 長 3 の除外／段落の切れ目／`.rs` のコメント抽出）を恒等写像へ倒して**テストが落ちるか**。**変異が本来の回帰より強くないこと**まで確かめる（#872 で同型 3 回） |

**担保されない（宣言する死角）**

1. **`docs/adr/**` ・`docs/superpowers/**` ・`workspace/` は見ない**（母集団の外）。今日 0 件と実測したが、将来そこに折れが生まれても赤くならない
2. **`.json` はコメント記法を持たないので入らない**（`commentFamilyOf` が `null`）
3. **rustdoc のコードフェンスの内側**——`linesOutsideFences` は `///` に続く ` ``` ` を落とさない（`headingRefSourceDocs` が既に受容する残余として宣言）。フェンス内に単独のバッククォートが 1 個あれば偽陽性になる（今日 0 件）
4. **run 長 3 以上のスパンの折れは見ない**（フェンス標識と区別できないため）。今日 0 件
5. **PowerShell のバッククォートはエスケープ文字である**——コメントの中で `` `n `` 等を説明する行が将来増えれば偽陽性になりうる（今日 0 件。`.ps1` / `.psm1` の走査元 22 件で実測）。**向きは赤（うるさい側）であって沈黙ではない**
6. **削除は編集時に見えない**（`rm` は `Edit|Write` matcher に届かない・`docs/hooks.md`）。CI が引き取る
7. **evidence 行の `コードスパン N 件` は、この検査自身の劣化に対して無効である**——`checked` は候補スパンを数えた時点で確定し、段落状態や `.rs` のコメント抽出を参照する前に決まる。`G-folded-heading-refs` の「受容する残余 2」と**同型**であり、劣化を捕まえるのは fixture の側である
8. **規範の意味的な妥当性**（縮めた規範が読者を正しく導くか）は機構の外。`ADR-retire-norm-review` により規範へフォールトインジェクションは当てない
9. **`npm run governance:check` を「差分ゼロ」で回しても意味が無い形**——未コミットの是正に `git diff main...HEAD` の 3 点形を当てると作業ツリーを見ない（#922）。本件の検証は**全ツリー走査**なのでこの罠には当たらないが、PR 本文へ「差分で確かめた」と書く形は採らないこと

---

## 6. 所見 — **要対処**

### A. `.rs` の走査を「コメント行だけ」に絞る手段を、`refScanLines` を動かさずに用意すること

**根拠**: `snotra-settings/src/tabs/backup.rs:233-234` は `let start = s.find('` + バッククォート + `')? + 1;` という **char リテラル**であり、`.rs` の全行を走査すると必ず偽陽性になる（実測）。`refScanLines` は `.rs` を散文側（`linesOutsideFences`）へ落とす設計で、**その選択自体が #925 の意図**（`lib.mjs:265` の doc が「`.rs` は #925 から全行を走査しており、その母集団を同じ変更で動かさない（検査対象を変更しながら検査を検証しない・#489）」と宣言している）。

**採るべき形**: `linesOfComments(text, file, family = commentFamilyOf(file))` の第 3 引数。既定引数は今日の挙動を保つので `refScanLines` も既存 3 検査も 1 ミリも動かない。

**採ってはならない形**:
- `COMMENT_FAMILY` へ `.rs → "js"` を足す — `refScanLines` の意味論が変わり、`G-heading-refs` / `G-near-heading-refs` / `G-folded-heading-refs` の母集団が同じ変更で動く。`.rs` の見出し参照 102 件の走査対象が「全行」から「コメント行」へ縮む
- `.rs` を全行走査したまま char リテラルを特別扱いする — 綴りに依存し、Rust 文字列中のバッククォートで再発する。そもそも**規範の対象はコメントである**

⚠️ **未確定の余地**: `linesOfComments` の js 族の字句解析は「テンプレートリテラルの中で `//` から始まる行をコメントと誤判定する」と自ら宣言している。Rust では raw string の中の `//` 始まりの行が同じ形になる。**向きは赤**なので沈黙ではないが、この宣言をコピーせず「同じ死角を継承する」と 1 行で書くこと。

### B. 編集時 reminder（`edit-findings.mjs`）への配線を、採否どちらであれ**明示的に決める**こと

**根拠**: #992 の一次証拠は「**目視では守れない**——違反を直す編集そのものが新しい違反を 2 件持ち込んだ」である。`docs/adr/ADR-edit-time-check-scope.md`「決定」が前倒しの条件として挙げるのは「**走査元（参照や識別子が書かれている側）を編集ファイル 1 枚へ絞れる判定**」で、本検査は**着地先を持たない**ぶんこの条件を既存 4 本より強く満たす。母集団も既に配線済みの `allHeadingRefDocs` と同一。費用は既存 subprocess の内側（同 ADR「帰結」の実測 59〜109 ms）。

**配線するなら同じ変更で要るもの**: `edit-findings.test.mjs` の it 1 本、`docs/hooks.md`「検査ではない reminder」の表 1 行。

**配線しないなら**、その否定の知識を新 ADR へ書くこと（`ADR-edit-time-check-scope` が「今回は却下」の形で `G-module-linkage` について既に先例を作っている）。⚠️ 私の判断は**配線する側**だが、issue が明示していないので裁定は利用者に属する。

### C. 「21 本」を **22 本へ書き換えるのではなく、数を持たない形へ倒す**こと

**根拠**: `scripts/governance-check.mjs:103,174` と `scripts/governance-check.test.mjs:63` の 3 か所。`AGENTS.md`「検証の作法（全タスク共通）」は「**数え上げは偽になる時点が確定している**——足すたびに腐る。数ではなく正本（分岐そのもの）を指す」と定めている。22 へ直すのは、次の検査で同じ作業を要求する形を再生産する。「`checks/` の全検査」「registry が導出する検査群」のように**正本を指す**形へ。

⚠️ ただし `docs/adr/ADR-governance-meta-demotion.md` の**見出し**が「…止めたとき 21 本の合否が信用できなくなるか」であり、`ADR-folded-canonical-reference-detector.md:19` がそれを正準形で引用している。**見出しは凍結された歴史なので触らない**——生きた散文が数を捨てても、凍結層の数は歴史の記述として残る。この非対称を新 ADR で 1 文名乗ること。

### D. 縮めた規範の**適用範囲の文**を、機構の射程と一致させること

**根拠**: 現在 58 行は「適用は新規に書くコメントと、その変更で触った段落だけである」と宣言し、冒頭 7 行も「既存コメントの一括書き直しは本書のスコープ外」と言う。**新検査は全ツリーを毎回走る**ので、この 2 文と機構の射程が食い違う。今日は是正 6 件で 0 件から始まるので実害は無いが、**規範だけが「既存は対象外」と言い続ける**状態は `ADR-folded-canonical-reference-detector`「検討した代替案と却下理由」が退けた「機構と規範が逆を向く」形そのものである。

**採るべき形**: 「この 2 形は既存・新規を問わず機構が見る。それ以外の書き方の条項は新規・touched だけに適用する」と、**射程の分割を明示**する。

### E. 規範の射程が `.rs` を越えることを、規範側にも書くこと

**根拠**: 検査は `.md` 262 文書・`.ps1` / `.psm1` 22 件を含む全ツリーを見るのに、`docs/comment-guidelines.md` の配送は `.claude/rules/comments.md` の `paths`（4 crate の `**/*.rs`）だけである。`G-folded-heading-refs` は同じ非対称に対して**検査ヘッダの射程宣言が赤を受け取った者の唯一の拠り所になる**と書いた（`ADR-folded-canonical-reference-detector`「帰結」）。新検査のヘッダにも同じ宣言が要る。

⚠️ **加えて**、`docs/comment-guidelines.md` の冒頭 3 行が「コードコメント（rustdoc / TSDoc / インラインコメント）の書き方」と自ら射程を名乗っている。`.md` の**散文**の折れ（`PERFORMANCE.md:668` が実例）はこの射程の外にあり、**規範が一言も無い面で機構だけが赤を出す**。1 文で受けるか、`.claude/rules/governance-docs.md` へポインタを 1 行置くかの判断が要る。

### F. 新検査の**ヘッダとテストが自分自身を赤にしない**ことを確かめること

**根拠**: `checks/` は `headingRefCommentDocs`（コメント記法を持つ全ファイル）に入るので、**この検査の走査母集団に自分自身が含まれる**。`G-folded-heading-refs` は同じ理由で「例示に実在の対象を置かない」を明文化した。折れた例をヘッダのコメントに書けば自分の finding になる。**テストの fixture は文字列リテラル**なので `linesOfComments` が落とす（#1138 の意味の写像）が、`.md`（ADR・comment-guidelines）へ折れた例を書くなら**コードフェンスの内側**でなければならない。

---

## 7. 所見 — **軽微**

1. **`docs/build-commands.md:166` の列挙**（「参照実在・モジュール索引・スキル表・SPEC 番号・rules glob・コマンド写像・見出し参照の着地」）と、ルート `CLAUDE.md:29` の「決定的な項目（参照実在・索引・スキル表・SPEC 番号・rules glob・コマンド写像）」。どちらも**例示の列挙**であって全称の主張ではないので厳密には偽にならないが、**書き方の族が新しく入る**ので 1 語足すかどうかの判断が要る。⚠️ 足すと次の検査で同じ判断が再来する（C と同じ型）ので、私は**足さない**側を推す。
2. **見出し `## 日本語の折返し` の改名は避ける**。生きた参照 3 件は `G-heading-refs` が捕まえるが、凍結 ADR 3 件（`ADR-comment-guideline-delivery-by-pointer` / `ADR-folded-canonical-reference-detector` / superpowers の plan）は**照合の母集団外なので赤くならずに死ぬ**。`.claude/rules/governance-docs.md` が「ADR 本文内の参照は照合されない——凍結された歴史であり腐るに任せる」と定めている以上、改名の費用は「静かに腐る 3 件」である。
3. **新 ADR のファイル名**は `ADR-folded-code-span-detector.md` を推す。`G-adr-file-names` が「冒頭が `# ADR-<slug>: <題>` の形」と stem の一致を要求するので、1 行目を `# ADR-folded-code-span-detector: …` にすること。**連番を振らない**。
4. **新 ADR が記録すべき却下案**（私が導出した範囲）:
   - 「1 段落 1 行」を全ツリーで機構化する（＝ issue の原案）——母集団 3000 件超で不成立。**この数字は利用者の裁定の根拠なので ADR に残す**
   - `HEADING_REF` 側と同じく「折れを吸収して母集団へ取り込む」——検査ではなく grep の害なので吸収先が無い
   - `.rs` を全行走査したまま char リテラルを除外する（A の却下形）
   - `COMMENT_FAMILY` へ `.rs` を足す（A の却下形・既存 3 検査の母集団が動く）
   - 素の散文の折返しを規範として残す（＝規範だけが言い続ける形）——`ADR-folded-canonical-reference-detector` が「機構と規範が逆を向く」を退けた向きの、鏡像の適用
5. **`PERFORMANCE.md:668` の是正**は「この文書へ記録するときの規約」（`PERFORMANCE.md` 内）に触れる可能性がある。折り返し点を移すだけなら値も出所も動かないが、**実測値の写しを増やさない**こと。
6. **`scripts/lib/*.Tests.ps1` の 2 件は同じ文言の重複**（`-ErrorAction SilentlyContinue` の説明）。片方を直して片方を忘れる形が起きやすい。**両方を同じコミットで**。
7. **書き換える 4 行の直後の段落を宙に浮かせないこと**——`docs/comment-guidelines.md:66` の「**禁則処理の規則は置かない。** `.rs` のコメント 8030 行を走査して禁則違反候補は 0 件だった（2026-08-08 実測）」は、**1 段落 1 行の体制下で測られた過去の観測**である。観測としては生き残るが、節の前半を縮めた後に「既に守られているものへ規範を足さない」という論旨だけが接続先を失う形になりうる。節を書き直すときに一緒に読むこと。

---

## 8. 所見 — **未検証**

1. ⚠️ **是正後に `governance:check` が実際に 0 件になることを、私は測っていない**——リポジトリを変更しない制約のため。6 件の是正差分を当てた後に全ツリーを再走査すること。**是正編集そのものが新しい折れを持ち込む**のが #992 の一次証拠なので、**是正後の再走査は必須**である（`AGENTS.md`「条件別チェック」の「レビュー指摘へ修正（fix-forward）を当てた」行と同じ型）。
2. ⚠️ **候補スパン 17628 件という evidence の数字は、私の述語での値である**。実装の述語が違えば動く。**evidence へ載せる数の定義**（候補スパンか、走査行か、バッククォートを持つ行か）を検査ヘッダで名乗ること。
3. ⚠️ **`.toml` / `.yml` の腕を実際に赤にしうるか測っていない**。`COMMENT_FAMILY` に入っているので走査はされるが、今日 0 件であり「0 である理由」を測っていない（そもそもコードスパン記法を使わないのか、たまたま折れていないのか）。
4. ⚠️ **段落の切れ目の判定**（空行・行番号の不連続で状態を捨てる）は今日の 6 件では効き方を分離できていない。**捨てないと**、スパンを閉じない孤立したバッククォート（PowerShell のエスケープ等）が遠方の行と対になって偽陽性を作る。**恒等写像への変異で今日何件増えるか**を測ること——`G-folded-heading-refs` の `stripLead` について同じ測定（17 件全滅）が行われた先例がある。
5. ⚠️ **`.md` の表のセル内**でスパンが折れる形を測っていない。表の行は 1 物理行なのでセル内では起きないはずだが、**「除外は構造行そのものを指す」という規範の言い直しが表に対して何を意味するか**は決めていない。
6. ⚠️ **CI での実測は PR ができてから**（`.claude/rules/safety-nets.md`「検出器のカバー範囲は、欠落のパターンごとに検算する」）。`ci.yml` は `pull_request` でのみ起動するので、**manifest delta の宣言が実際に効くこと**は PR 本文のチェックリストへ送ること。
7. ⚠️ **CRLF での挙動を測っていない。** 導出した述語は構成上 CRLF 安全である（行末の `\r` はバッククォートの run に触れず、空行判定の `trim()` が食う）が、**測っていないことは変わらない**。`G-folded-heading-refs.test.mjs` は CRLF fixture を 1 本明示的に持っている（「windows runner は `core.autocrlf=true` でチェックアウトする」）——同じ fixture を新検査のテストにも置くこと。
8. ⚠️ **`.claude/agents/code-reviewer.md:28` を直すと、レビュアの探索対象が 1 つ減る。** その減少が「機構が本当にその族を全部見ている」ことに依存する。**機構の射程はコードスパンの行またぎだけで、`docs/comment-guidelines.md` の他の条項（写し・数え上げ）は依然レビュアが唯一の検出器である**——同じ箇条書きの中で 2 つが並んでいるので、片方だけ消す編集が他方まで消さないか目で確かめること。
