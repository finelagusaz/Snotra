# 計画: #1293 既定 ExePath の検知器の母集団を宣言から導き、残る直書き 2 本を揃える

## 目的と受け入れ条件

- 目的
  - 「`-ExePath` の既定を持つスクリプトは直書きせず導出を通る」という性質を、手書きの一覧ではなく、**宣言から導いた母集団**に課す。一覧に無いスクリプトが黙って退行する死角（#1292 の型）を塞ぐ
  - 残る直書き 2 本を導出の形へ揃える
- 合意: セーフティネット（検知器の母集団）の変更について、ユーザーの「着手許可ー」（2026-10-25）を得た
- 受け入れ条件
  1. 既存の It「<_> の既定 ExePath は…」の `-ForEach` が、`BeforeDiscovery` で導いた一覧を受け取る。手書きの一覧は無くなる
  2. 補集合の検査（新しい It）が、次の 2 つを確かめる
     - 直下の `.ps1` のうち、コメント以外の行で `$ExePath` を語るものは、母集団か許可リストのどちらかに入る
     - 許可リストの各項目は、今も `$ExePath` を語っている（古くなった許可を残さない）
     - 許可リストは `run-pester.ps1`（既定を持たない明示 override 専用）だけである
  3. 次の変異で、`npm run test:powershell` が赤になる（変異注入は委譲先が行う）
     - 一覧に無い直書きのスクリプトを足す → 主の It の `Should -BeIn` で赤
     - 正規表現に当たらない宣言（`[Parameter()][string]$ExePath = "..."`）を持つスクリプトを足す → 補集合の It で赤
     - 導出が 0 件を返す → コンテナの失敗で exit 1
  4. `manual-smoke.ps1`（debug）と `measure-memory-stages.ps1`（release）の既定が導出の形になり、母集団 6 本すべてが既存の 3 述語を通る
  5. `manual-smoke.ps1` が PR へ投稿する記録の「バイナリ」行に、リポジトリの外の絶対パスを新しく載せない（リポジトリは PUBLIC）
  6. 文書がスクリプトを名指しで数え上げない。出力の作法は resolver の doc を指す
  7. `manual-smoke.ps1` の記録の commit と投稿先の PR は、測った本体と同じコピーから取る。`-NoLaunch` は導出せず、cargo が無くても止まらない

## 変更ファイルと対象シンボル

1. `scripts/lib/SnotraSmoke.Tests.ps1`
   - `BeforeDiscovery` に足すもの
     - 直下の `.ps1`（`Get-ChildItem -LiteralPath (Join-Path $PSScriptRoot '..') -Filter '*.ps1' -File`）について、コメント以外の行を 1 回だけ読む
     - そこから 2 つの一覧を導く。`$exePathHarnesses` は、行頭の `[string]$ExePath =` に当たるファイル名。`$exePathMentions` は、`\$ExePath\b` に当たるファイル名
   - Describe「起動ハーネスの既定 ExePath…」
     - 主の It: `-ForEach $exePathHarnesses` に変える。手書きの一覧は消す。本体（3 述語）は変えない
     - 新しい It（補集合の検査）: `-ForEach @(@{ Harnesses = $exePathHarnesses; Mentions = $exePathMentions })` で、同じ導出の結果を受け取る
       - 許可リストは It の本体に置くハッシュテーブル `@{ 'run-pester.ps1' = '<理由>' }` とする
       - 確かめること: `Mentions` から `Harnesses` と許可リストを引いて空になること。許可リストの各キーが `Mentions` に在ること
     - ヘッダ: 「対象は数え上げているので…」「残る 2 本を外しているのは…」を、次の形に書き直す
       - 母集団の導出
       - 補集合の検査の意味
       - 射程の外: サブディレクトリの `.ps1`、別名の本体パス（`$BinaryPath` など）、`<# #>` のブロックコメント（コメント除外は行頭の `#` だけを見る）
       - 却下した 2 案と理由: 再帰の glob は `lib/*.Tests.ps1` が自分自身に当たりうる。既知の対象だけのカナリアは、導出と同じ盲点を共有する
       - 反転の理由: #1292 で、数え上げの死角が 1 サイクルで破れた
2. `scripts/lib/SnotraSmoke.psm1`
   - 新しい関数 `Get-SnotraRecordedExePath -ExePath <string> -RepositoryRoot <string>` を足し、export の一覧へ載せる
     - PR へ投稿する記録に書くパスを返す純関数で、規則は次のとおり
       - `$ExePath` が空 → `$null`（呼び出し側が「未起動」と書く）
       - `[IO.Path]::IsPathRooted($ExePath)` が真で、`RepositoryRoot` の下にある → `[IO.Path]::GetRelativePath(RepositoryRoot, ExePath)`（リポジトリ相対）
       - それ以外 → `$ExePath` をそのまま返す（明示された値で、今と同じ挙動）
     - doc には「なぜ相対にするか」を書く。リポジトリは PUBLIC で、`-PostToPr` が記録を PR コメントへ投稿する（3b の D）
     - 「下にある」の判定は、区切りの重複や大文字小文字を `GetRelativePath` に任せる。`..` で始まるか、絶対パスが返ったら「外」とする（文字列の前方一致で判定しない。`C:\repo2` が `C:\repo` に前方一致する）
   - `SnotraSmoke.Tests.ps1` に、この関数の It を足す
     - リポジトリの下の絶対パス → 相対になる
     - リポジトリの外の絶対パス → そのまま
     - 前方一致だけする兄弟ディレクトリ（`<repo>2\x.exe`）→ そのまま
     - 相対パス → そのまま
     - 空 → `$null`
3. `scripts/manual-smoke.ps1`
   - `param()`: `[string]$ExePath = "target/debug/snotra.exe",` を `[string]$ExePath = '',` にし、直上のコメントを「空なら debug 本体を導く（#1179・#1293）。**理由の正本は `Resolve-SnotraCargoExecutable` の doc**。カテゴリ D は `cargo run -p snotra` 相当」とする
   - `Import-Module`（55〜56 行目）の後へ `$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path` を足す
   - 出所の記録（180〜182 行目）の git 3 行を `git -C $repoRoot ...` にする
     - 本体はスクリプトのコピーから導くので、cwd から取った commit と食い違いうる。この不整合は今回の変更で初めて生まれる（Step 2b の所見）
   - `-PostToPr` の投稿先: `$Pr` が 0 のときは `gh pr comment $branch` にする（cwd のブランチではなく、記録した `$branch` の PR）
   - 起動する枝（`if (-not $NoLaunch)`）の中
     - 最初に `if (-not $ExePath) { $ExePath = Resolve-SnotraCargoExecutable -RepositoryRoot $repoRoot -Profile debug }` を置く（1 行）
     - **`-NoLaunch` の枝では導出しない**（Step 2b の所見）。起動しないのに cargo や manifest の不在で落ちる、という新しい失敗を作らないため
     - 存在の確認を `Test-Path -LiteralPath` にする。文言は今のまま（先に `cargo build -p snotra`）
     - 確認の直後に `$ExePath = (Resolve-Path -LiteralPath $ExePath).Path` と `Write-Host "本体: $ExePath"` を置く
   - 記録の表（376 行目）: 「バイナリ」行は `Get-SnotraRecordedExePath` の値を載せる。`$null` のときは「未起動（-NoLaunch）」と書く
4. `scripts/measure-memory-stages.ps1`
   - `param()`: `[string]$ExePath = "$PSScriptRoot/../target/release/snotra.exe"` を `[string]$ExePath = ''` にし、直上へ 1 行のコメントを置く
   - `.PARAMETER ExePath` と `.DESCRIPTION` の前提（17 行目）を書き換える。「既定は cargo の target から release 本体を導く（`CARGO_TARGET_DIR` に追随）」とする。**ヘルプブロックの中には、resolver の関数名と行頭の `$repoRoot =` を書かない**（`<# #>` は述語のコメント除外をすり抜ける・3b の C）
   - `Add-Type` の後に、次を足す
     - `Import-Module (Join-Path $PSScriptRoot 'lib/SnotraSmoke.psm1') -Force`
     - `$repoRoot = ...`
     - `if (-not $ExePath) { ... -Profile release }`
   - 存在の確認（52 行目）
     - `Test-Path -LiteralPath` にし、文言を「先に cargo build --release -p snotra」にする
     - 正規化して `Write-Host "本体: $ExePath"` を出す
   - 導出と存在の確認は、`Stop-Process -Name snotra`（97 行目付近）より前に置く。本体や cargo が無いときに、実行中の snotra を kill してから落ちないようにする
5. `docs/build-commands.md`
   - 226〜233 行目付近の段落を書き直す
     - 「`bench:startup`・`smoke:startup`・`smoke:egui` も…（`bench:startup` は release、他は debug）」の名指しをやめる
     - 「`-ExePath` の既定を持つスクリプトは、既定の本体を `test:powershell` と同じ形で導く（対象の正本は `SnotraSmoke.Tests.ps1` の既定 ExePath の検査、プロファイルは各スクリプト）」とする
     - 「測った本体の絶対パスはいずれも出力の先頭に出る」は消し、出力の作法は resolver の doc を指す
     - 起点がスクリプトのリポジトリであること・明示した相対パスは cwd 相対であること・#1179 の経緯は残す

## 実装順序

1. `SnotraSmoke.Tests.ps1` の `BeforeDiscovery` と、2 本の It を先に書く。現状（直書き 2 本）に当てて、次の 2 つを確かめる（Red）
   - 主の It が `manual-smoke.ps1` と `measure-memory-stages.ps1` で `Should -BeIn` の赤になる
   - 補集合の It は緑になる（6 本が母集団、`run-pester` が許可リスト）
2. `manual-smoke.ps1` を直す
3. `measure-memory-stages.ps1` を直す
4. Pester が緑になる（Green）
5. 文書とヘッダを直す
6. 検証

## 不変条件と異常系

- 明示された `-ExePath` の意味は変えない。相対パスは cwd 相対のまま
- `manual-smoke.ps1 -NoLaunch` は、本体も cargo も無くても止まらない（今と同じ）。導出は起動する枝の中だけで行う
- `manual-smoke.ps1` の記録の「バイナリ」行に、導出で新しく生まれた絶対パスを載せない。リポジトリ相対に落とす（`Get-SnotraRecordedExePath` を Pester で測る）
- `manual-smoke.ps1` の記録の branch・commit・dirty と、投稿先の PR は、測った本体と同じコピー（`$repoRoot`）から取る
- `measure-memory-stages.ps1` は、本体や cargo が無いときに、実行中の snotra を kill する前に落ちる
- 明示された空文字 `-ExePath ''` は導出に倒れる（他のハーネスと同じ。受容する）
- 本体が無いときは、復旧手順つきで throw する（fail-closed）
- Pester の導出が 0 件を返すと、コンテナごと失敗して exit 1 になる（3b の A で実測）
- 補集合の検査は、`$ExePath` の語を持たない別名の本体パスには黙る（射程の外として宣言する）

## テスト方針と検証コマンド

- 退行の検知器: 主の It（3 述語・導いた母集団）と、補集合の It
- 変異注入（`/implement` の委譲先が、`scripts/` の複製か worktree で行う）
  - M1: 直書きの既定を持つ新しいスクリプトを直下へ足す → 主の It の `Should -BeIn` で赤
  - M2: `[Parameter()][string]$ExePath = "target/debug/snotra.exe"` を持つ新しいスクリプトを足す → 補集合の It で赤
  - M3: `BeforeDiscovery` の宣言の正規表現を、何にも当たらない形へ壊す → コンテナの失敗で exit 1（`FailedContainersCount`）
  - M4: `manual-smoke.ps1` の既定を直書きへ戻す → 主の It で赤
  - M5: `measure-memory-stages.ps1` のヘルプブロックへ、行頭の `$repoRoot = x` を書く → 述語 (2) で赤（3b の C の死角を実測で記録する。直すのではなく、宣言の根拠にする）
  - M6: 許可リストに、実在しない名前を足す → 補集合の It の「許可が古い」側で赤
  - M7: 既定なしで `[string]$ExePath` を宣言し、本体で `target/debug/snotra.exe` を直書きするスクリプトを足す → 補集合の It で赤（宣言の `=` で選ぶ形だけでは漏れる型・Step 2b の M3）
  - M8: 許可リストから `run-pester.ps1` を外す → 補集合の It で赤
  - M9: `run-pester.ps1` の `[string]$ExePath` に直書きの既定を足す → 母集団に入り、主の It で赤
  - M10: `Get-SnotraRecordedExePath` の判定を文字列の前方一致へ変える → 兄弟ディレクトリの It で赤
  - M11: smoke-egui の `-ExePath` を `-Exe` へ改名する → **緑のまま**（`$ExePath` の語が消えるので、どの検査にも映らない）。射程の外として宣言した死角が、宣言どおりであることを記録する
  - 各変異は内容ハッシュで巻き戻しを照合する
- 実機の確認
  - `measure-memory-stages.ps1`: 既定のまま実行し、`本体:` が `target\release\snotra.exe` を指して段階列が取れること。**実行中の snotra を kill し SendKeys を使うので、ユーザーの常用版が終了していることを確かめてから回す**
  - `manual-smoke.ps1`: `Read-Host` を使うのでエージェントは完走できない。記録の整形は `Get-SnotraRecordedExePath` の Pester が自動で測る。配線（呼び出し点）だけは、人に `npm run smoke:manual -- -Only 1` を依頼して確かめてもらう。見るのは次の 2 つ
    - `本体:` の行
    - 出力ファイルの「バイナリ」行と commit 行（リポジトリ相対であること・測った本体と同じコピーであること）
- カテゴリの検証（`docs/build-commands.md`）
  - C（実バイナリの検査を含む）と E: `npm test`、`npm run test:powershell`（統合テストを含む）
  - F: `npm run governance:check`

## SPEC.md・関連文書の更新

- `SPEC.md`: 不要（開発用スクリプトの既定値であり、製品の挙動ではない）
- `docs/build-commands.md`: 要（上記）
- ADR: 起こさない。反転の理由と却下した 2 案は、検知器のヘッダ（射程の正本）に書く（research の「ADR を起こすか」）

## 作業項目

### Phase 1 — 検知器を導出へ変える（Red を確かめる）

- [x] `SnotraSmoke.Tests.ps1` の `BeforeDiscovery` に、母集団と言及の 2 つの一覧の導出を足す
- [x] 主の It の `-ForEach` を、導いた一覧へ変える
- [x] 補集合の It を足す（許可リストは `run-pester.ps1` だけ）
- [x] `npm run test:powershell` で、主の It が manual-smoke と measure-memory-stages で赤になり、補集合の It が緑であることを、出力で確かめる

### Phase 2 — 残る 2 本を導出へ揃える

- [x] `SnotraSmoke.psm1` に `Get-SnotraRecordedExePath` を足して export し、Pester の It を足す（新しい関数と、manual-smoke の呼び出し点の移行を同じフェーズで行う）
- [x] `manual-smoke.ps1`
  - param の既定
  - `$repoRoot`
  - git 3 行を `-C $repoRoot` にする
  - `-PostToPr` の投稿先を `$branch` にする
  - 起動する枝の中だけで導出する
  - 確認・正規化・`本体:`
  - 記録の「バイナリ」行を `Get-SnotraRecordedExePath` 経由にする
- [x] `measure-memory-stages.ps1`: param の既定、ヘルプの書き換え（関数名と行頭の `$repoRoot =` を書かない）、import、導出、確認・正規化・`本体:`。導出と確認を `Stop-Process` より前に置く
- [x] `pwsh -NoProfile` で、2 本の import と導出の部分だけに相当する式を評価し、導出が通ることを確かめる（アプリは起動しない・3b の ⚠️）
- [x] `npm run test:powershell` が緑になる
- [x] （実装中に追加）`manual-smoke.ps1` の記録には、明示された `-ExePath` の**元の形**を渡す。起動する枝で絶対形へ正規化するため、そのまま渡すと、リポジトリの外を指す明示の相対パスが絶対パスとして記録に載る（不変条件「明示された値は今と同じに載せる」を破る）
- [x] （レビューで追加）`-PostToPr` でブランチが取れないとき、gh の既定（cwd のブランチ）へ倒さず、`-Pr` を求めて止める（R2・Medium）。`$exePathGiven` のコメントを実物に合わせて弱める（R6）

### Phase 3 — 文書とヘッダ

- [x] 検知器のヘッダを書き直す（導出・補集合・射程の外の 3 つ・却下した 2 案・反転の理由）
- [x] `docs/build-commands.md` の段落を書き直す（名指しをやめる・出力の全称を消して resolver の doc を指す）
- [x] `npm run governance:check` が緑になる

### Phase 4 — 検証

- [x] `npm test` が緑になる — 612 件
- [ ] `measure-memory-stages.ps1` を既定のまま実行する（ユーザーの常用版の終了を確かめてから）。`本体:` と段階列を確かめる
- [ ] 人へ `npm run smoke:manual -- -Only 1` を依頼する。`本体:` の行と、記録の「バイナリ」行がリポジトリ相対であることを確かめてもらう

## 未確定（実装前に潰す）

- [x] 3b の敵対的調査の所見を反映する — A〜F をすべて採った（research の 3b の表）。最大の変更は 2 つある。(i) カナリアを「既知の対象を含む」から補集合の検査へ変えた（B）。(ii) manual-smoke の PR 投稿へ絶対パスを載せない規則を足した（D・リポジトリは PUBLIC を `gh repo view` で確認）
- [x] 母集団の glob を直下にするか再帰にするか — **直下にする。** 起動ハーネスは `package.json` から呼ばれる入口で、直下にしか無い。再帰にすると `lib/*.Tests.ps1` が自分自身に当たりうるので、除外の機構が要る。サブディレクトリの死角はヘッダに宣言する
- [x] ADR を起こすか — **起こさない。** 却下されるのは「数え上げて止める」だけで、既存 ADR の判断の反転である。反転の理由と、この巡で却下した 2 案は、検知器のヘッダ（射程の正本）に書く

## 人間レビュー

- [x] 承認済み — 2026-10-25 / 問い: "計画書へ注釈を書き込んでくださるか、このまま承認してくださいますか。" / 回答: "OK"

## セルフレビュー

- リスク: **高**
  - セーフティネット（検知器）の母集団を変える。網羅性が要件である（「既定を持つスクリプトすべて」に性質を課す）
  - 公開リポジトリへ投稿される記録の内容を変える
- plan-review: 実施する（下の結果を参照）
- エージェント数: 1（3b）＋ plan-review の分
- 照合 5 点
  1. issue の 4 つの「やること」は、受け入れ条件の 1〜4 と Phase 1〜2 が受け持つ
  2. 境界条件
     - 母集団 0 件 → M3
     - 正規表現から漏れる宣言 → M2
     - 許可リストが古い → M6
     - `-NoLaunch` → 不変条件
     - 明示した相対パス → 不変条件
     - リポジトリの外の絶対パス → 記録の規則の else 枝
  3. 新しい状態やリソースは無い
  4. より単純な既存パターン: 導出の形は既存 4 本の写しである。補集合の検査は新しいが、手書きの一覧（死角を実際に踏んだ）より単純な代替は無い
  5. 壊してはならない不変条件
     - 「既定が直書きへ戻らない」→ 主の It
     - 「母集団から漏れない」→ 補集合の It
     - 「0 件で黙らない」→ Pester のコンテナの失敗（実測）
- 要対処: 4 件を計画へ反映した（下の plan-review 結果）
- 未検証: manual-smoke の完走は人に依頼する

## plan-review 結果

- リスク: 高
- レビュー方式: 独立導出 1 体（Step 2b・`--deep`。計画と調査を読ませず、issue の WHAT だけを渡した。成果物は `workspace/plan-review-1293-derive.md`）
- エージェント数: 1

### 要対処（根拠は主エージェントが再照合した）

- **manual-smoke の `-NoLaunch` の枝で導出しない** — 計画の修正。枝の外で導出すると、起動しないのに cargo や manifest の不在で落ちる新しい失敗が生まれる。記録には「未起動（-NoLaunch）」と書く
- **manual-smoke の記録の git 3 行と投稿先を `$repoRoot` に揃える** — 計画の修正
  - 再照合: 180〜182 行目の git は cwd 起点で、`-PostToPr` の `gh pr comment` も cwd のブランチを使う
  - 本体をスクリプトのコピーから導くと、cwd が別のコピーのときに、記録の commit と測った本体が食い違う。既定が cwd 相対だった今は一致していたので、この変更が初めて生む不整合である
  - `gh pr comment [<number> | <url> | <branch>]` はブランチ名を受け取る（`--help` で確認）
- **記録のパスの整形を純関数 `Get-SnotraRecordedExePath` にして Pester で測る** — 計画の修正。manual-smoke はエージェントが完走できないので、公開 PR へ載る内容を自動で測れる形にする
  - 判定は `GetRelativePath` に任せる。実測では、兄弟ディレクトリは `..\repo2\x.exe`、別ドライブは絶対パスのまま、大文字小文字が違っても相対になった
- **measure-memory-stages の導出と確認を `Stop-Process`（97 行目）より前に置く** — 計画の修正。本体が無いときに、実行中の snotra を kill してから落ちないようにする

### 軽微

- 母集団の選び方を、AST（`ExePath` という param の有無）にする案 — 採らない。補集合の検査が同じ失敗の形を赤にする（型無しの宣言・属性の前置・既定なしで宣言して本体で直書き）。変異 M7 で測る
- glob を再帰にする案 — **採らない（実測で裁定した）。** 補集合の検査を再帰で回すと、`scripts/lib/SnotraSmoke.Tests.ps1` 自身が、コメント以外の行に `$ExePath` を 1 か所持つので当たる（`grep -v '^\s*#' | grep -c '\$ExePath\b'` で 1）。許可リストに自分自身を足すことになる。直下にとどめ、サブディレクトリの死角はヘッダに宣言する
- 述語 (3) が `<# #>` のブロックコメントを数える — 述語は直さない。ヘッダの射程の宣言に書き、measure-memory-stages のヘルプでは関数名を書かない（変異 M5 で死角を記録する）
- 補助センサ（`target/(debug|release)/snotra.exe` の直書きを、コード全体で探す）— 採らない。`prepare-sidecar.ps1` のような正当な直書きが当たり、許可リストが太る。別名の本体パスは射程の外として宣言する
- ADR — 起こさない（両方の枠が同じ推奨だった）

### 未検証

- measure-memory-stages で psm1 を import したときに、計測（フォーカス・SendKeys）へ影響しないか — Phase 4 の実機確認で測る
- `CARGO_TARGET_DIR` 追随の枝 — resolver の既存の Pester が受け持つ。今回は resolver を変えない

### 判断

- 実装着手: 可（人間の承認待ち）
