# 調査: #1293 既定 ExePath の検知器の母集団を宣言から導き、残る直書き 2 本を揃える

## issue の要約

- `scripts/lib/SnotraSmoke.Tests.ps1` の Describe「起動ハーネスの既定 ExePath はスクリプトの住むコピーから導かれる」は、対象を `-ForEach` に手で並べている。一覧は resolver の呼び出し元から選ばれたので、直書きのまま resolver を呼ばないスクリプトは候補に入らない（#1292 の `smoke-egui.ps1` がこの形で漏れた）
- やること
  - (1) 母集団を `[string]$ExePath =` の宣言から導く。除外が要るなら、理由つきの許可リストにする
  - (2) 母集団に守りたい対象が現れること、対象外が混じらないことを示す
  - (3) 一覧に無い直書きのスクリプトを足す変異で赤になることを確かめる
  - (4) `manual-smoke.ps1`（debug）と `measure-memory-stages.ps1`（release）を導出の形へ揃える
- 合意: 検知器の母集団の変更（セーフティネット）について、ユーザーから「着手許可ー」を得た（2026-10-25）

## 母集団の実測（`git grep -n -E '^\s*\[string\]\s*\$ExePath\s*=' -- 'scripts/*.ps1' 'scripts/**.ps1'`、2026-10-25・`efc1191`）

- `scripts/` 配下の `.ps1` は 17 本ある（`find scripts -name '*.ps1'`）
- 既定つきで `[string]$ExePath =` を宣言するのは 6 本である
  - 既定が `''` のもの: `bench-startup.ps1`・`smoke-egui.ps1`・`smoke-startup.ps1`・`visual-input-metrics.ps1`（いまの手書きの一覧と同じ 4 本）
  - 直書きのもの: `manual-smoke.ps1:3`（`"target/debug/snotra.exe"`）・`measure-memory-stages.ps1:44`（`"$PSScriptRoot/../target/release/snotra.exe"`）
- 宣言の形で外れるもの
  - `run-pester.ps1:4` の `[string]$ExePath` は既定を持たないので、正規表現（`=` を要求する）に当たらない。明示 override 専用であり、外れるのが正しい
  - `visual-check-colors.ps1` は `-ExePath` param を持たない
  - `scripts/lib/*.Tests.ps1` のうち `SnotraSmoke.Tests.ps1` は、判定の正規表現を文字列として含む。ただしその行は `$paramDefaults = ...` で始まるので `^\s*\[string\]` に当たらない
- ゆえに (4) を直した後は、**許可リストが空のまま**、導いた母集団 6 本がすべて述語を通るはずである。除外の機構は、要る事例が現れるまで置かない

## 関連ファイル・シンボル（grep で実在を確認済み）

- `scripts/lib/SnotraSmoke.Tests.ps1`
  - `BeforeDiscovery`（31 行目）: 既に在り、セッションのロック判定を行っている。母集団の導出もここへ置ける
  - Describe「起動ハーネスの既定 ExePath…」（206 行目）: 述語は 3 つある。(1) 既定が空リテラル、(2) `$repoRoot =` が 1 行で `$PSScriptRoot` を含む、(3) resolver の全呼び出しが `-RepositoryRoot $repoRoot` を渡す。コメント行（`^\s*#`）は除く
  - ヘッダ（222〜228 行目）: 「対象は数え上げているので…」「残る 2 本を外している…」。母集団を導出に変えると、この説明は書き直しになる
- `scripts/manual-smoke.ps1`
  - `param()` の 1 番目が `[string]$ExePath = "target/debug/snotra.exe",`
  - `Import-Module (Join-Path $PSScriptRoot 'lib/SnotraSmoke.psm1')` は 55 行目に既に在る
  - `$ExePath` の使用点
    - 存在の確認（203 行目、`if (-not $NoLaunch)` の中）
    - 目印の確認 `Resolve-Path $ExePath`（207 行目）
    - `Start-Process -FilePath $ExePath`（228 行目）
    - 記録の表 `| バイナリ | ``$ExePath`` |`（376 行目）
  - `$repoRoot` の語は 0 件である
  - `-NoLaunch` のときは本体を起動しない（人が自分で起動する）。存在の確認はその枝の外では行わない
- `scripts/measure-memory-stages.ps1`
  - `param()` の最後が `[string]$ExePath = "$PSScriptRoot/../target/release/snotra.exe"`（末尾カンマなし）
  - `SnotraSmoke.psm1` を import して**いない**。resolver を使うには import を足す必要がある
  - 存在の確認は 52 行目（`Test-Path $ExePath`）、起動は 104 行目（`Start-Process -FilePath $ExePath`）
  - `.DESCRIPTION` の前提（17 行目）に「既定 `target/release/snotra.exe`」と書いてある
- `scripts/lib/SnotraSmoke.psm1` の `Resolve-SnotraCargoExecutable`: 呼び出し側の作法の正本。「この作法は `SnotraSmoke.Tests.ps1` のソース述語が守っている（射程と死角はあちらの宣言が正本）」
- `docs/build-commands.md` 226〜233 行目付近の段落「`bench:startup`・`smoke:startup`・`smoke:egui` も既定の本体を…導く（`bench:startup` は release、他は debug）」: スクリプトを名指しで数え上げている。2 本増えると古くなる。名指しをやめて、検知器（正本）を指す形にする
- `docs/adr/ADR-harness-executable-path-derivation.md`: 凍結された歴史なので触らない。代替案の「ソース述語を広げる」で「対象を数え上げる死角（4 本目に黙る）は宣言して止める」と決めた。今回はそれを覆すので、**否定の知識の更新**になる。新しい ADR を起こすかは計画で決める

## 再利用できる既存パターン

- 既定 `''` → `Import-Module` → `$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path` → `if (-not $ExePath) { ... Resolve-SnotraCargoExecutable -RepositoryRoot $repoRoot -Profile <p> }` → `-LiteralPath` で存在の確認 → 絶対形にして `本体:` を出力する。この形は `smoke-startup.ps1`・`smoke-egui.ps1`・`bench-startup.ps1` に既に在る
- `BeforeDiscovery` で値を計算し、`-ForEach` でテストへ渡す。`-ForEach` で渡したハッシュテーブルのキーは、It の本体で変数として読める（Pester の data-driven tests）

## 技術的制約

- ~~`-ForEach` に空配列を渡すと、It は 1 本も作られず黙って通る~~ **偽だった（3b の A・自分で実測）。** 空配列ではコンテナごと失敗し、`run-pester.ps1` は exit 1 になる。導出が 0 件を返す形は既に赤になる。残る穴は**部分的な取りこぼし**（宣言の書き方が正規表現から漏れる）で、それは補集合の検査で塞ぐ（3b の B）
  - **カナリアは同じ導出を使わなければ意味が無い。** 正規表現を 2 か所に書くと、片方だけ直したときに食い違う。`BeforeDiscovery` で 1 回だけ導き、その結果を `-ForEach @(@{ Population = ... })` でカナリアにも渡す
- Pester は discovery と run が別相である。素のループ変数は It の本体に届かない（既存のヘッダに実測の記録がある）。値は `-ForEach` で渡す
- 既定値の導出は `param()` の中に書けない（resolver の doc）
- `manual-smoke.ps1` の `-NoLaunch` の枝では、本体が無くても止めてはならない。存在の確認と `Resolve-Path`（絶対形への正規化）は、起動する枝の中に置いたままにする。導出（resolver の呼び出し）は枝の外へ置いてよい。`cargo metadata` を 1 回呼ぶだけで、失敗すれば throw する
  - `-NoLaunch` のときも、記録の表の「バイナリ」行に導いた本体のパスが載る。これは今も同じ（既定の文字列が載る）で、挙動は変わらない
- `manual-smoke.ps1` は `Read-Host` を使うので、エージェントは完走できない。ただし `-NoLaunch` を付けずに起動すれば、`Read-Host` に達する前に「バイナリ」まわりの挙動は観測できる。完走の確認は人に依頼する
- `measure-memory-stages.ps1` は実行中の snotra を kill し、`SendKeys` で注入する。エージェントが回すとユーザーの常用版を巻き込みうるので、実行するならユーザーに確認する
- `scripts/*.ps1` の編集では、編集時に自動で走る検査は無い。検証は `npm run test:powershell` で手で行う

## 未解決の疑問

- なし（下の 3b の表と、計画の未確定欄で決着した）

## 3b 敵対的調査の所見と採否（`workspace/adversarial-1293.txt`）

測定環境: `CARGO_TARGET_DIR` は Process・User・Machine のすべてで未設定で、`.cargo/config.toml` も無い。`target_directory` は実行したコピーの `target` を指した。Pester が読むのは、実行したコピーの `scripts/` 自身である。

| 所見 | 判定 | 採否と理由 |
|---|---|---|
| A: 空の `-ForEach` は黙って通る、は偽。Discovery で ArgumentException になり、コンテナごと失敗する | 壊せた | **採る。** 自分で測った。Pester 6.0.1 で空配列を渡すと `Passed=0 Failed=0 FailedContainers=1 Result=Failed` になり、`run-pester.ps1:52` は `FailedContainersCount` を見て exit 1 にする。上の技術的制約の 1 項目目は誤りだった。0 件の導出は既に赤になるので、カナリアの役目は「件数が 0 でない」ではなく、**部分的な取りこぼし**を捕まえることに変える（次の行） |
| B: 母集団の導出と述語 (1) が同じ正規表現なので、盲点を共有する。型無し、`[System.String]`、属性を前置した形、1 行の `param(...)` は当たらない。既知の対象が 1 本居るだけのカナリアでは、他が脱落しても緑のまま | 壊せた | **採る。** カナリアを**補集合の検査**にする。直下の `.ps1` のうち、コメント以外の行に `$ExePath` を含むものから、母集団と許可リスト（`run-pester.ps1`: 既定を持たない明示 override 専用）を引くと、空になること。宣言の書き方が正規表現から漏れても、`$ExePath` の語が在る限りここで赤になる。別名（`$BinaryPath` など）は射程の外として宣言する。現リポジトリで漏れている起動ハーネスは 0 本（17 本を全数確認） |
| C: `measure-memory-stages.ps1` のヘルプブロック `<# #>` は行頭が `#` でないので、述語のコメント除外をすり抜ける | 壊せた | **採る。** `.DESCRIPTION` を書き換えるときは、resolver の関数名と、行頭の `$repoRoot =` を書かない。ヘッダの射程の宣言にも「ブロックコメントは除かない」を足す |
| D: `manual-smoke.ps1` の記録表の「バイナリ」行は、`-PostToPr` で PR コメントへ投稿される。既定を絶対パスにすると、利用者のディレクトリ構成が載る | 壊せた | **採る。** 自分で確かめた。リポジトリは PUBLIC（`gh repo view`）で、投稿は 460〜465 行目である。記録には、`$repoRoot` の下にあるならリポジトリ相対のパスを書き、外にあるなら明示された値をそのまま書く（今と同じ）。コンソールの `本体:` は絶対パスでよい（投稿されない） |
| E: `docs/build-commands.md` の「測った本体の絶対パスはいずれも出力の先頭に出る」は、manual-smoke と measure-memory-stages では偽になる | 壊せた | **採る。** 段落はスクリプトを名指しせず、出力の作法は resolver の doc（「解決後は絶対形へ正規化して出力へ載せる」）を指す。2 本にも `本体:` の出力を足して、作法に揃える |
| F: 2 本は同じ型ではない。manual-smoke は cwd 起点で #1179 と同じ型、measure-memory-stages は `$PSScriptRoot` 起点で `CARGO_TARGET_DIR` に追随しないだけ | 壊せた | **採る。** issue の記述どおりで、計画の重さは変えない。PR 本文でこの区別を書く |
| 軽微: `-NoLaunch` でも導出が枝の外なので、cargo が無いと throw する | 壊せた | 受容する。開発機には cargo があり、`cargo metadata` は 1 回だけ。research は既に認めていた |
| 再帰の glob でも母集団は同じ 6 本で、自分自身には当たらない | 壊せなかった | — |
| `-ForEach @(@{ Population = $x })` で、It の本体から `$Population` が読める | 壊せなかった | 補集合の検査は、この形で母集団を受け取る |
| 6 本・17 本の内訳、`$repoRoot` の語 0 件、`-NoLaunch` の枝の位置、import による名前の衝突が無いこと、他文書に既定パスの前提が無いこと | 壊せなかった | — |
| ⚠️ 属性を前置した書き換えでカナリアが緑のまま、は正規表現単体の実測だけ | ⚠️ | 補集合の検査に変えるので、変異 M-attr（属性を前置した直書きを足す）で、赤になることを実装時に測る |
| ⚠️ measure-memory-stages 相当の文脈での import は未実測 | ⚠️ | 実装時に `pwsh -NoProfile -Command` で import と導出だけを実行して確かめる（アプリは起動しない） |
| ⚠️ `CARGO_TARGET_DIR` 追随の枝は、この環境では未検証 | ⚠️ | resolver の Pester（`Resolve-SnotraCargoExecutable` の 2 本の It）が、env を設定して測っている。今回の変更は resolver を変えない |

### glob の範囲（反対側の論拠を含めて決めた）

- **直下（`scripts/*.ps1`）にする。** 起動ハーネスは `package.json` から `scripts/<name>.ps1` として呼ばれる入口スクリプトであり、現に直下にしか無い
- 再帰にすると `lib/*.Tests.ps1` が入る。fixture や説明の行が行頭で `[string]$ExePath =` を含むと、自分自身に当たる。避けるには除外の機構が要る
- 反対側の論拠（`scripts/governance/` のようなサブディレクトリの先例）は残る死角として、ヘッダに宣言する。サブディレクトリに `.ps1` の起動ハーネスを置くとき、この検査には映らない

### ADR を起こすか

- **起こさない。** 却下されるのは「数え上げて止める」だけで、これは `ADR-harness-executable-path-derivation` の判断の反転である。反転の理由（#1292）と、この巡で却下した 2 案（再帰の glob・既知の対象だけのカナリア）は、検知器のヘッダに書く。ヘッダは検査の射程の正本なので、そこに置けばかぶらない
- 凍結された ADR の「宣言して止める」が現役と読まれる懸念は残る。ただし ADR は凍結された歴史として読む約束であり（`ADR-adr-frozen-history`）、現役の射程はヘッダが持つ
