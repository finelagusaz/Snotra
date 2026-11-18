# 調査: #1012 scripts の .mjs / .ps1 / .psm1 に formatter の CI ゲートを新設するか

## issue の要約

PR #1011 で `REQUIRED_RUSTDOC_LINTS = [` が `=[` へ変わった。`governance:check` も vitest も素通りし、目視だけが検知した。issue が問うのは「formatter ゲートを入れるか」の判断そのものである。数えてから決めること、入れないという結論も成果であること、を issue 自身が課している。

## 前提の裏取り

- **issue 本文の「Rust 側は `cargo fmt --check` が PostToolUse hook と CI の両方で走る」は、もう偽である。** PostToolUse フックは #1255（`88064cea`）で撤去された。今は CI（`.github/workflows/ci.yml` の `cargo fmt --all -- --check`）だけが走る。`.claude/settings.json` の hooks は PreToolUse の `pre-bash.mjs` だけである。
- **混入は 1 行ではなく 2 行だった。** 同じコミット `f2e4b04c`（PR #1011 のブランチ）が `REQUIRED_RUSTDOC_LINTS =[` と `STALE_EXTRA_DOCS =[` を作り、同じブランチの `b68ce6b7` が両方を直した。squash の前に直ったので、main の履歴には現れない。現物は今 `scripts/governance/checks/G-workspace-lints.mjs:46` にあり、スペースは入っている。
- 対象の母集団: `git ls-files` で `scripts/**/*.mjs`・`scripts/**/*.ps1`・`scripts/**/*.psm1`・`.claude/hooks/*.mjs` を数えると 72 ファイル・約 18,000 行ある。`.githooks/` の本体はシェルスクリプトで、`.mjs` はテスト 1 本だけである。

## 捕捉実績（数えた結果）

### 測り方

- 母集団は、main の全コミットと、全 PR のブランチ先端（`refs/pull/*/head` を 945 本取得）から `merge-base..head` の範囲にある非マージコミットを合わせたものである。対象パスに触れたコミットは 714 本あった。
- squash 専用のリポジトリなので、**目視で捕まえた逸脱はブランチの中でしか観測できない**。main の履歴だけを数えると、PR #1011 の件も 0 件に見える。最初に main だけで数えたときは、実際に 0 件と出た。
- 述語は「同じハンクの中で、−行と＋行の空白をすべて除くと一致し、インデント以外の空白だけが違う対」である（スクラッチの `ws-scan.mjs`）。
- **較正**: 既知の 1 件である `f2e4b04c`（混入）と `b68ce6b7`（修正）が両方とも拾われることを確かめた。

### 結果: 28 対 → コードの書式の逸脱は 1 件（PR #1011 の 2 行）

| 種類 | 対の数 | 出所 | formatter の射程か |
|---|---|---|---|
| `=[`（代入の空白の欠落）の混入と修正 | 4（2 行 × 混入・修正） | PR #1011 | **射程内**。今回の 1 件 |
| コメントの継続行のインデント | 12 | PR #1093 と、その squash（`4f5f4d3f`） | 射程外（コメントの中身） |
| コメントの和文と英字の間の空白（`SSOT`） | 3 | PR #623 | 射程外（散文） |
| 文字列リテラルの中の空白 | 3 | PR #834・#1402 | 射程外（文字列） |
| PowerShell のハッシュテーブルの `=` の揃え | 6 | PR #954 と、その squash（`417bd07c`） | 様式の選択で、逸脱を直したのではない |

- 1 件の混入は、レビュアーの目視が同じ PR の中で捕まえた。

### 敵対的調査（3b）で増えた 2 件 — 訂正後は 3 PR・4 行

この述語は −行と＋行の対しか見ないので、直されなかった逸脱と、直すときに同じ行の別の箇所も変えた逸脱を拾えない。追加行を直接見る検出器（3b の `adv-scan.mjs`）で、次の 2 件が見つかった。いずれも一次資料で確かめた。

| PR | 行 | 経緯 |
|---|---|---|
| #1305 | `race-boundaries.test.mjs` の `const g =(...a) =>` | `27da65b9` で混入し、同じブランチの `c0461a0e` で直った。直したコミットは同じ行に `env: env()` も足しているので、上の述語に出なかった。main へは届いていない |
| #132 | `bench-startup.ps1` の `$numericSettings= @(... }| ForEach-Object` | `bc30cbbe` で main に入り、約 3 日後に `ce580ea9`（#195）が行ごと消した。**書式を直したのではなく、コードごと消えた** |

- 書式の逸脱は、714 コミットのうち 3 PR に 4 行あった。main へ届いたのは 1 件（#132）で、残った期間は 3 日、害は無かった。
- この数は下限である。force-push で消えた途中のコミットと、同じコミットの中で混入して直ったものは見えない。
- 現行のツリーで、行末の空白・タブのインデント・CRLF・`識別子=[({` の形は 0 件である（コメントの中にある誤検出 1 件を除く）。

### この測定が見ていないもの

- 同じコミットの中で混入して直ったもの。force-push で消えたブランチの途中のコミット。
- 行の構造が変わる種類の逸脱（折り返しの位置・括弧の配置・引用符の種類）。これは formatter なら報告するが、上の述語は見ない。
- **既定の様式からの drift の総量**（prettier の既定設定で `--check` したときのファイル数とハンク数）。npm から prettier を取得する必要があるので、測っていない。PowerShell 側は PSScriptAnalyzer が手元に無く（`Get-Module -ListAvailable` で 0 件）、`Invoke-Formatter` でも測れない。
  - この 2 つは、ゲートを入れたときの**費用**を測る。費用と便益を比べて決める以上、結論に効かないとは言えない（3b の所見）。代わりの手がかりとして、`scripts` 配下の `.mjs`（43 本）では 120 桁を超える行が 1,031 行ある（当初 514 行と書いたのは、bash の `**` が深い階層を拾わなかった数え違い）（`awk 'length>120'`）。prettier の既定（80 桁）を当てれば、全体を整形することになる。そのときは `ADR-rustfmt-gate` と同じく、全体を整形する PR と `.git-blame-ignore-revs` の 2 段が要る。
  - PowerShell の formatter が #132 の形を直すかどうかも測っていない。

## 関連するファイル・機構

- （3b の訂正）手元では、`docs/build-commands.md` のカテゴリ A にある `cargo fmt --all -- --check` を `npm run verify:tree` が回す。**自動で**走るのは CI だけ、が正しい言い方である。
- `.claude/rules/safety-nets.md` — 新設の規律。「検知器を足す前に、その検知器が単独で検出する経路を数える」「装置は捕捉実績で身の証を立てる」
- `docs/adr/ADR-rustfmt-gate.md` — Rust 側の先例。導入の決め手は「初期コミットの drift が 0 で、様式が在った」ことと、460 ハンクが実際に積み上がっていたこと（PR #857 で実害が出た）である
- `docs/adr/ADR-meta-harness-thinning.md` — 走査対象がハーネス自身である層を撤去した決定（2026-09-29）。（3b の訂正）同 ADR が撤去したのは、ハーネスを照合する検査・編集時 reminder・PostToolUse である。対象の 72 ファイルのうち約 34 本は製品の計測・ビルド・smoke の道具なので、「formatter ゲートが同じ撤去単位に当たる」とは言えない。この ADR から引けるのは、編集時の自動検証を持たないという判断までである
- `.git-blame-ignore-revs`・`docs/build-commands.md`「git blame と整形コミット」 — 整形を全体に当てるときの手順の先例
- `package.json` — devDependencies は `@tauri-apps/cli` と `vitest` だけで、formatter は入っていない
- `.github/workflows/ci.yml` — Node 側の検査は vitest と `governance:check` である

## 技術的な制約

- formatter を入れると、devDependency が 1 つ増え、CI に job か step が 1 つ増える。既定の様式から drift していれば、全体整形の PR を先にマージし、その SHA を `.git-blame-ignore-revs` へ書く 2 段の手順が要る（`ADR-rustfmt-gate`）。
- PowerShell 側の formatter（PSScriptAnalyzer の `Invoke-Formatter`）は、CI の runner に入っているかを別途確かめる必要がある。
- ルート `CLAUDE.md` の最重要ルール 2 により、セーフティネットを新設するかは利用者と合意して決める。

## 敵対的調査（3b）の採否

全文は `workspace/adversarial-1012.txt` にある。

| 争点 | 判定 | 採否 | 理由 |
|---|---|---|---|
| PostToolUse は撤去済みで、fmt は CI だけ | 一部壊せた | 採る | `verify:tree` が手元で回す。「自動で走るのは CI だけ」へ直した |
| #1011 の混入は 2 行で、main に届いていない | 壊せなかった | — | — |
| 射程内の逸脱は #1011 の 1 件だけ | 壊せた | 採る | #1305 と #132 を一次資料で確かめ、3 PR・4 行へ直した |
| 現行ツリーの逸脱は 0 件 | 壊せなかった（条件つき） | 採る | CRLF の 0 は index の内容についてだけ真である（`core.autocrlf=input`）。現行ツリーの主張としては真のまま |
| drift を測っていないことは結論に効かない | ⚠️ | 採る | 費用の側も比較に入るので、言い過ぎだった。120 桁を超える行の数（1,031 行）を代わりの手がかりとして書いた |
| formatter は meta-harness-thinning の撤去単位に当たる | 壊せた | 採る | 72 本のうち約 34 本は製品の道具である。ADR から引ける範囲へ狭めた |
| CI の runner に PSScriptAnalyzer が入っているか | ⚠️（争点外） | 保留 | 入れると決めたときにだけ要る |

## 未解決の疑問

- 入れるか入れないか（利用者の判断。測定の結果を添えて聞く）
- 入れない場合、却下の理由を ADR に残すか。否定の知識（「数えたら 1 件で、目視が捕まえていた」）は生じているので、ADR の対象になりうる。issue を閉じるコメントだけで足りるかどうか
