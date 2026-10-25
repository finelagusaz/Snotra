# plan-review: #1293（既定 ExePath 検知器の母集団を宣言から導き、残る直書き 2 本を揃える）

独立レビュー。`workspace/` 配下の plan.md・research.md・adversarial-*.txt は読んでいない。
基準: HEAD `efc1191`（ブランチ chore/exepath-detector-population・作業ツリーは `workspace/` のみ未追跡）。
アプリを起動するスクリプトは実行していない。実測は scratchpad の probe（`.../scratchpad/p1293/`）と、既存検知器だけを Filter で選んだ Pester 実行。

## 0. 実測した事実（以下の節の根拠）

- **ベースライン**: 既存検知器を `Filter.FullName='*既定 ExePath*'`・`ExcludeTag=Integration` で実行 → 4/4 緑（bench-startup / smoke-egui / smoke-startup / visual-input-metrics）
- PowerShell ファイルは全部 `scripts/` 配下: `git ls-files '*.ps1' '*.psm1' | grep -v '^scripts/'` → 0 件
- `-ExePath` を param に持つ `.ps1`（AST の `ParamBlock.Parameters` と行 regex `^\s*\[string\]\s*\$ExePath\b` が `scripts/` 再帰 17 ファイルで**完全一致**・parse error 0）:
  bench-startup `''` / manual-smoke `"target/debug/snotra.exe"` / measure-memory-stages `"$PSScriptRoot/../target/release/snotra.exe"` / run-pester（既定なし）/ smoke-egui `''` / smoke-startup `''` / visual-input-metrics `''`。`lib/*.Tests.ps1` 5 本は 0（正規表現リテラルは引用内なので当たらない）。入れ子関数に `ExePath` param を持つものも 0（`FindAll` で確認）
- 既存の述語 (1)〜(3) を対象 2 本と run-pester へ当てた結果（`pred.ps1`）:
  - manual-smoke: decl=1 / default=`"target/debug/snotra.exe"`（不合格）/ `$repoRoot` 代入 0 / resolver 呼び出し 0
  - measure-memory-stages: decl=1 / default=`"$PSScriptRoot/../target/release/snotra.exe"`（不合格）/ 0 / 0
  - run-pester: **decl=0**（`[string]$ExePath` に `=` が無いので述語 (1) の regex に当たらない）/ `$repoRoot` 1 / 呼び出し 1
- **Pester 6.0.1**: 既定 `Run.FailOnNullOrEmptyForEach = True`。`-ForEach @()`、および BeforeAll（run 相）で作った変数を `-ForEach` に渡すと **discovery が throw し、コンテナごと失敗**（`FailedContainers=1・Passed=0`）。`run-pester.ps1:52` は `FailedContainersCount` を見るので exit 1（fail-closed だが**同ファイルの他の Describe もすべて走らない**）
- BeforeDiscovery / Describe 本体で作った変数は run 相の It 本体から**見えない**（probe で `Get-Variable` が空）。**`Describe ... -ForEach @(@{ Population = $pop })` で渡すと run 相の It から `$Population` で読める**（probe で `Should -Contain 'smoke-egui.ps1'` 緑）——導出を 2 回書かずに「母集団に X が在る」を run 相で言える
- **既存述語 (3) はブロックコメント `<# #>` を除かない**（`SnotraSmoke.Tests.ps1:238` は `^\s*#` の行だけ除く）。`<#` で始まる `.ps1` は 7 本（measure-memory-stages:1 を含む）。ブロックコメント内に「`Resolve-SnotraCargoExecutable` で導く」と書いた行は呼び出しとして数えられ、`-RepositoryRoot $repoRoot` を持たないので**赤になる**（probe で `calls=1 repoRootArg=False` を確認）

## 1. 変更が要るファイル・シンボル・行

### コード
- `scripts/lib/SnotraSmoke.Tests.ps1`
  - `:233-235` の `-ForEach @('bench-startup.ps1', ...)` 手書き一覧 → 宣言からの導出（BeforeDiscovery か Describe 本体）+ 理由つき許可リスト
  - `:238` `Join-Path $PSScriptRoot "../$_"` — 再帰にするなら `$_` は `scripts/` 相対パス（`lib\...` を含みうる）になるので結合を合わせる
  - `:222-228` ヘッダ段落「**対象は数え上げているので、一覧に無いスクリプトには黙る。**」以下と、run-pester / visual-check-colors の除外理由 → **偽になる**。新しい母集団の定義・許可リストの理由・残る死角（§2）へ書き換える。visual-check-colors は `-ExePath` param を持たないので宣言由来の母集団には最初から入らない（除外理由が不要になる）
  - `:233` テスト名 `（#1179）`、`:207` 「この性質を守る検査が他に無い」は真のまま（確認のみ）
  - 新設: 「母集団に守りたい対象が現れ、対象外が混じらない」It、許可リストの各項目が母集団に実在すること、run-pester の理由（既定なし）を機械で確かめる It（§3）
- `scripts/manual-smoke.ps1`
  - `:2-3` コメント「既定は debug ビルド」と `[string]$ExePath = "target/debug/snotra.exe"` → `''` + 導出
  - `:55-56` の `Import-Module` の後に `$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path`（述語 (2) は `^\s*\$repoRoot\s*=` が 1 行・`$PSScriptRoot` を含むことを要求）
  - `:202-207` `if (-not $NoLaunch)` の中: `Test-Path $ExePath` / `Resolve-Path $ExePath` → 導出は**この枝の中だけ**で行う（§3 -NoLaunch）。`-LiteralPath` へ揃える（他ハーネスは smoke-egui:68・smoke-startup:27・bench-startup:55 が `-LiteralPath`）
  - `:376` `| バイナリ | ``$ExePath`` |` → **PR へ投稿される記録**。絶対パスを載せない形へ（§3）
  - `:180-182` `git rev-parse` / `git status` は cwd 起点（⚠️ 範囲の判断・§5）
  - `:31-38` 使い方コメント（`cargo build -p snotra` → `npm run smoke:manual`）は真のまま
- `scripts/measure-memory-stages.ps1`
  - `:44` 既定 → `''`
  - `Import-Module (Join-Path $PSScriptRoot 'lib/SnotraSmoke.psm1') -Force` が**無い**ので足す。`$repoRoot` 代入も無いので足す
  - `:52` `Test-Path $ExePath` → 導出の後・`-LiteralPath`・メッセージを `cargo build --release -p snotra` へ。**`:97` 相当の `Stop-Process -Name snotra` より前で落ちる順序を保つ**
  - 散文: `:17` 「release ビルド済み（既定 `target/release/snotra.exe`）」→ 偽になる（CARGO_TARGET_DIR に追随する）。`:34-35` `.PARAMETER ExePath 計測対象の実行ファイル。` → 既定の導出を書くなら resolver doc を正本として指す（smoke-egui:2 と同形）。**このブロックコメントに `Resolve-SnotraCargoExecutable` の名を書くと既存述語 (3) が偽の赤を出す**（§0 最終項）——行コメント `#` に書くか、述語 (3) をトークン化に直す
  - 解決後に絶対形へ正規化して表示（他ハーネスの作法・`SnotraSmoke.psm1:398-399`）

### 文書
- `docs/build-commands.md:229-235`「`bench:startup`・`smoke:startup`・`smoke:egui` も既定の本体を…導く（`bench:startup` は release、他は debug）」→ 列挙が不完全になる（既に `check:input-metrics` が抜けており、今回 `smoke:manual`・`measure:memory:stages` も導出側へ入る）。**列挙を足すのではなく検知器の母集団を正本として指す**（`.claude/rules/governance-docs.md`「機構の実装の詳細を散文へ写さない」・`AGENTS.md`「数え上げは偽になる時点が確定している」）。⚠️「も」なので厳密には下限主張で偽ではないが、括弧内「他は debug」は measure:memory:stages（release）が入ると偽
- `docs/build-commands.md:213` `measure:memory:stages` 行・`:63-65` `smoke:manual` 行・`:280` 「既定 ExePath = debug」→ 真のまま
- `scripts/lib/SnotraSmoke.psm1:385-401` resolver doc → 真のまま（`:401` 「射程と死角はあちらの宣言が正本」が新ヘッダを指し続ける）。⚠️ manual-smoke の記録形式（相対化）を作法として足すなら、ここが正本候補（「解決後は絶対形へ正規化して出力へ載せる」と**公開記録では載せない**の関係を書き分ける）
- `docs/adr/ADR-harness-executable-path-derivation.md:38-45`「対象を数え上げる死角（4 本目に黙る）は宣言して止める」→ 今回で状況は変わるが、**ADR は凍結された歴史**（`.claude/rules/governance-docs.md`「ADR を消すときは…」・`ADR-adr-frozen-history`）なので触らない
- `AGENTS.md` / ルート `CLAUDE.md` / `.claude/rules/safety-nets.md` → この変更で偽になる記述は無い（grep: `ExePath|既定 ExePath|数え上げ` を当てて該当なし。safety-nets「検査の入力集合を、具体対象で検算する」は今回が従う側）
- `PERFORMANCE.md:3103` は CI の明示 `-ExePath` の話で無関係。`.github/workflows/*.yml` は明示 `-ExePath target/release/snotra.exe`（cwd=repo root）で無関係
- `.ps1` のコメントは governance:check の G-heading-refs 走査元に入る（`.claude/rules/governance-docs.md` 冒頭）→ カテゴリ F（`npm run governance:check`）を実行対象に含める

## 2. 母集団の導出案と死角

### 案 A（推奨）: 「param に `ExePath` を宣言する `.ps1`」を AST で導く
- `Get-ChildItem -LiteralPath <scripts> -Recurse -File | Where-Object Extension -eq '.ps1'` → `[Parser]::ParseFile` → `ParamBlock.Parameters | Where Name.VariablePath.UserPath -eq 'ExePath'`。先例: `scripts/occupy-hotkey.ps1:64-65` が既に ParamBlock を AST で読んでいる
- parse error が 1 件でもあれば赤（黙って母集団から落ちるのを防ぐ）
- **選択条件を `[string]$ExePath =`（`=` の有無）にしない。** issue 案は「`[string]$ExePath =` を宣言するもの」だが、これは述語 (1) の形から母集団を作り直すことになり、#1292 と同じ型の穴が残る: 既定なしで宣言し本体で `if (-not $ExePath) { $ExePath = 'target/...' }` と直書きするスクリプトが入らない。**「param を持つ」で選び、run-pester を理由つきで許可リストに置く**——issue の例示（run-pester は既定を持たない明示 override 専用）とも一致する形
- 許可リストは `@{ 'run-pester.ps1' = '<理由>' }` の形。理由を機械で確かめる（run-pester の `DefaultValue` が `$null`）
- 死角:
  - **param 名が違う**（`-Exe`・`-BinaryPath`・`-Path`）ハーネス
  - **param を持たず本体で起動する**ハーネス（`visual-check-colors.ps1:257-261` がこの形——ただし導出済み。直書きで同じ形が足されると黙る）
  - `$PSDefaultParameterValues`・dot-source・別ファイルから既定を注ぎ込む形
  - `scripts/` の外の `.ps1`（現在 0 件・§0）。`.psm1` の関数 param（現在 0 件）
  - 導出の後での上書き・`Set-Variable`（既存ヘッダ `:216-220` が宣言済みの天井。変わらない）
- 補助の独立センサ（任意・⚠️ 費用対効果は判断）: (a) `package.json:9-21` の `pwsh -NoProfile -File scripts/*.ps1` 群は独立した入口一覧で、母集団の外にある起動入口を探すのに使える。(b) コメントトークンを除いたコードに `target[/\\](debug|release)[/\\]snotra\.exe` が現れたら赤——修正後は `.ps1` のコード中 0 件のはず（現状はコメント外で manual-smoke:3・measure-memory-stages:44 の 2 件、`<#` 内の measure-memory-stages:17 は**行フィルタでは除けない**のでトークンで除く必要がある）。param 名違い・param 無しの直書きを拾う唯一の足

### 案 B: 行 regex（既存述語 (1) と同じ `^\s*\[string\]\s*\$ExePath\b`）で導く
- 現状の 17 ファイルでは案 A と完全一致（§0）。依存が軽い
- 死角（案 A より広い）: 型なし `$ExePath`・`[Parameter()][string]$ExePath` 同一行（`^\s*\[string\]` の錨で外れる）・型と変数の改行・別の型・入れ子関数の同名 param（偽の対象）・`<# #>` 内の同形の行（偽の対象）

### 案 C: resolver の呼び出し元（現行の選び方）
- 却下。直した後の形から母集団を取るので直し損ねが原理的に入らない（issue「問題」・#1292 の実例）

### 共通の注意（Pester の相）
- 母集団は **discovery 相（BeforeDiscovery か Describe 本体）で導く**。BeforeAll で導くと `-ForEach` は `$null` → コンテナ失敗（実測）
- run 相で母集団を見る It（「smoke-egui.ps1 が在る」等）は、**Describe/Context の `-ForEach @(@{ Population = ... })` で渡す**（実測で可）。BeforeAll で再導出すると導出が 2 か所になり片方だけ退行しうる
- 空の母集団（ルートの綴り違い・Filter の誤り）は既定設定ではコンテナ失敗で赤になる（実測）が、**同ファイル 40 件余の他テストも道連れで走らない**。⚠️ `-AllowNullOrEmptyForEach` を付けて空でも続行し、別 It で `Count -gt 0` を見る形にすると、赤の範囲が局所化する。どちらでも fail-closed なので好み
- 列挙は `-Filter '*.ps1'` ではなく `Where-Object Extension -eq '.ps1'`（`-Filter` は Win32 照合で 3 文字拡張子の短縮名一致がありうる）。⚠️ 現リポジトリで実害は観測していない（`.ps1xml` 等は 0 件）
- 再帰にした場合、テスト名 `<_>` と `Join-Path` は `scripts/` 相対パスにする

## 3. 守るべき不変条件と異常系

### 検知器
- I1 母集団に守りたい対象が現れる: `smoke-egui.ps1`（#1292 で漏れた当の対象）、`manual-smoke.ps1`、`measure-memory-stages.ps1` を名指しで含むことを assert
- I2 対象外が混じらない: `run-pester.ps1` は許可リスト経由で除かれる・`lib/*.Tests.ps1`・`visual-check-colors.ps1`・`prepare-sidecar.ps1` は入らない
- I3 許可リストの各項目は母集団に実在する（古い項目は赤）。理由は機械で検算する（run-pester の既定が `$null`——既定を足すと理由が偽になり赤）
- I4 母集団の導出が空なら赤（上記）
- I5 既存の 3 述語の意味は変えない（入口・導出・使用点）。下限主張の宣言（`:216-220`）は維持
- I6 述語 (3) はコメント（行・ブロック）を数えない。⚠️ 現状はブロックコメントを数える（§0）。対象 2 本の doc を直す実装者が踏みやすいので、トークンで除く形へ直すか、`<# #>` に resolver 名を書かない運用を明記するか、のどちらかが要る

### manual-smoke.ps1
- 明示 `-ExePath` は cwd 相対のまま（`SnotraSmoke.psm1:397`・ADR 決定 3）。導出は `-not $ExePath` の枝だけ。⚠️ 明示の空文字 `-ExePath ''` は他ハーネス同様「導出」に倒れる（run-pester は明示空を throw・`run-pester.ps1:28-29`）。揃えるかは判断不要と見るが、意図として書く
- **`-NoLaunch` の枝では導出しない**——cargo も本体も要らない枝で `cargo metadata` を呼ぶと、cargo 不在・manifest 不在の環境で「起動しないのに落ちる」新しい失敗が生まれる。述語 (3) は行の形しか見ないので、`if (-not $NoLaunch)` の内側に置いても通る
- `-NoLaunch` のとき記録 `:376` は空セルにせず「未起動（`-NoLaunch`）」と書く（導出しない以上 `$ExePath` は空）
- 本体不在: 起動前（`Read-Host` で人間に作業させる前）に throw し、`cargo build -p snotra` を案内（現 `:204` の文言を維持）
- cargo 不在: resolver の `& cargo`（`SnotraSmoke.psm1:423`）は `$ErrorActionPreference='Stop'` 下で CommandNotFound を投げる。起動前に落ちること
- **PR への投稿内容（public）**: 導出後の `$ExePath` は絶対パス（`C:\Users\<name>\...`、worktree の場所、`CARGO_TARGET_DIR` の場所）。`:376` にそのまま載せ `-PostToPr`（`:460-466`）すると**利用者のディレクトリ構成が公開 PR に残る**。現状は既定の相対 `target/debug/snotra.exe` が載るだけ。推奨: 既定の枝では `$repoRoot` 相対（`[IO.Path]::GetRelativePath`）+ プロファイル名を記録し、`$repoRoot` の外（絶対の `CARGO_TARGET_DIR`）なら `<CARGO_TARGET_DIR>/debug/snotra.exe` のような伏せ字にする。コンソール表示（ローカル）は絶対形でよい。⚠️ 明示の絶対 `-ExePath` を渡したときに載るのは今もそうであり今回の退行ではない——ついでに揃えるかは判断
- 記録の一貫性: 本体はスクリプトのコピーから導くのに、`:180-182` の branch/commit/dirty は cwd の git から取る → **cwd が別コピーのとき、記録の commit と測った本体が食い違う**。変更前は両方 cwd 起点で一致していた（＝この変更が初めて生む不整合）。`git -C $repoRoot` を推奨。`gh pr comment`（`:464-465`）も番号省略時は cwd のブランチを見る（同型）。⚠️ 範囲の判断
- バイナリ目印検査（`:206-214`）は導出後の絶対パスで動く。`Resolve-Path $ExePath` を `-LiteralPath` に（`[`・`]` を含む worktree 名でワイルドカード解釈される）

### measure-memory-stages.ps1
- 既定 release（`-Profile release`）。明示 `-ExePath` は cwd 相対のまま（現状は `$PSScriptRoot` 起点の既定なので、cwd 相対は明示時だけ・変わらない）
- 本体不在・cargo 不在は**実行中の snotra を kill する前**に落ちる（現 `:52` の check が `Stop-Process` より前にある順序を保つ）
- `Import-Module SnotraSmoke.psm1` を新たに足す: psm1 は import 時に `Add-Type`（`:10`）と module スコープの `Set-StrictMode`（`:3`）を持つ。StrictMode は呼び出し側へ漏れない（module スコープ）。⚠️ 計測対象プロセスではないので計測値への影響は無いと見るが、pwsh 側の常駐が少し増える
- `measure-memory.ps1` を `&` で呼ぶ同一プロセス実行（`:11-14` の理由）を壊さない。導出は `cargo` を子プロセスで呼ぶが、計測開始前でありコンソールを新設しない（`& cargo` は同じコンソール）——⚠️ フォーカス奪取は起きないと読むが未実測（実行禁止のため）
- 段階計測は `Get-Process -Name snotra` / `Stop-Process -Name snotra` で名前一致（現状のまま）。別コピーの snotra が動いていれば kill される挙動は変わらない（範囲外）

### 製品
- 製品コード・`.github/workflows` は触らない。CI の 3 経路は明示 `-ExePath` なので影響なし

## 4. 実装後に当てる変異注入

**ライブの木ではなく複製（worktree か `$TestDrive` のフィクスチャ）へ当てる**（`.claude/rules/safety-nets.md`「複製に変異を当てる」）。赤の理由が assert の `-Because` か、コンテナ失敗（discovery の throw）かを毎回出力で見分ける。

| # | 変異 | 期待 |
|---|---|---|
| M1 | `scripts/new-harness.ps1` に `[string]$ExePath = "target/debug/snotra.exe"` を足す（一覧には何も足さない）| 赤（述語 (1)・テスト名に `new-harness.ps1`）。**issue の要求 3 の本体** |
| M2 | M1 を `scripts/sub/x.ps1` に置く | 再帰なら赤、直下なら緑（＝宣言した死角であることを確認） |
| M3 | 既定なし `[string]$ExePath` + 本体 `if (-not $ExePath) { $ExePath = 'target/debug/snotra.exe' }` | 赤（母集団に入り述語 (1) が Count=0）。案 B/issue 原案の `=` 選択では緑になることも対照で確かめる |
| M4 | 型なし `$ExePath = 'target/...'` / `[Parameter()][string]$ExePath = '...'` 同一行 | 案 A: 母集団に入り述語 (1) Count=0 で赤。案 B: 母集団から落ちて緑 |
| M5 | 許可リストから run-pester を外す | 赤（述語 (1) Count=0） |
| M6 | run-pester に既定 `= 'target/debug/snotra.exe'` を足す | 赤（許可リストの理由の検算） |
| M7 | 許可リストに実在しない名前を足す／run-pester を改名 | 赤（古い項目） |
| M8 | manual-smoke / measure-memory-stages の既定を直書きへ戻す | 各々赤（**修正の回帰検出**） |
| M9 | 対象 2 本の `$repoRoot` を cwd 起点（`(Get-Location).Path`）へ | 赤（述語 (2)） |
| M10 | 対象 2 本の呼び出しを `-RepositoryRoot (Get-Location)` へ | 赤（述語 (3)） |
| M11 | 母集団のルートを綴り違い（空の母集団）にする | 赤（コンテナ失敗か `Count -gt 0`）。`run-pester.ps1` 経由で exit 1 になることも確認 |
| M12 | 母集団を BeforeAll で導く形へ移す | 赤（コンテナ失敗・実測済みの型）|
| M13 | 「母集団に smoke-egui.ps1 が在る」It の期待名を消す／smoke-egui.ps1 の param を改名 | 後者で赤（I1 が効く）|
| M14 | measure-memory-stages の `<# #>` doc に「`Resolve-SnotraCargoExecutable` で導く」を書く | 述語 (3) の偽の赤が出ないこと（I6 を直した場合）。直さない場合は赤になることを確認して運用注記の根拠にする |
| M15 | 宣言済みの通り抜け: 導出の後で上書き・`Set-Variable`・param 名 `-Exe` の直書きハーネス | 緑（死角としてヘッダに宣言されていることの確認。補助センサ (b) を置くなら param 名違いは赤）|
| M16 | manual-smoke `:376` 相当の記録整形へ `C:\Users\x\...\target\debug\snotra.exe` を与える | 記録にユーザーディレクトリが出ない。⚠️ manual-smoke は実行不能（`Read-Host`・`:48` のガード）なので、整形を純関数へ出して Pester で測るか、人間の実機確認へ委ねるかの判断が要る |

## 5. 判断が分かれる点と推奨

1. **glob: 直下か再帰か** → 再帰（`scripts/**`・Extension 照合）を推奨。根拠: 追加される `lib/*.Tests.ps1` 5 本は母集団に 0 件（実測）で費用が無く、直下だと `scripts/<subdir>/` に置かれたハーネスへ黙る（M2）。PowerShell は全部 `scripts/` 配下（§0）なので `scripts/` の外は今のところ母集団の外で問題ない——外に置かれたら黙ることは死角として宣言する
2. **選択条件: param の有無か、`[string]$ExePath =` か** → param の有無 + 許可リスト（§2 案 A・M3）。issue 原案と違う点なので計画で明示する
3. **AST か行 regex か** → 母集団は AST を推奨（入れ子関数・ブロックコメント・同一行属性に強い。先例 occupy-hotkey:64）。⚠️ 述語 (1) は行 regex のままだと、AST で拾った宣言が 1 行形でないとき Count=0 で赤（fail-closed だが理由の文言がずれる）。(1) も AST の `DefaultValue` で見るか、「宣言は 1 行の `[string]$ExePath =` 形」を前提として書くか。現 7 本はすべて 1 行形（実測）。ADR の「述語を足して同義構文まで閉じる: 却下」は足すことへの却下で、同じ述語の読み方を AST へ替えることは別問題と見る
4. **変異の確かめ方: 恒久テストか一回限りか** → ⚠️ 意見が割れうる。恒久化するなら母集団の導出を `-ScriptsRoot` を取る関数にして `$TestDrive` のフィクスチャ（直書きハーネス 1 本・許可リスト対象 1 本）で「母集団に入る／入らない」を Pester に置く（discovery と run の両相から呼べるよう psm1 か dot-source のヘルパに置く必要がある——BeforeDiscovery の関数は run 相から見えない）。述語そのものまで関数化するのは過剰と見る。推奨: 導出関数 + フィクスチャ 1 件の恒久テスト、M1 以外の変異は worktree で一回測って PR 本文に記録
5. **ADR を起こすか** → 起こさない。否定の知識（「母集団を直した後の形から取ると直し損ねが入らない」「`=` で選ぶと既定なし + 本体直書きが漏れる」）は検知器の内部の判断で、正本は Tests ヘッダ（`SnotraSmoke.psm1:401` が既にそこを射程の正本と指している）。既存 ADR の「数え上げの死角は宣言して止める」は凍結された歴史として残す。⚠️ ADR 運用（否定の知識が生じた決定のみ・#593）に照らすと「選択条件」の却下は ADR 候補になりうる——検知器の外で再利用される知識かで決める
6. **`prepare-sidecar.ps1:11`** → 対象外。`-ExePath` param を持たず母集団に入らない。snotra-settings をビルドしてからコピーする道具で、ビルド構成の選択（`-Release`）であって「別コピーの本体を黙って測る」型ではない。`CARGO_TARGET_DIR` に追随しない別の欠陥はありうる（⚠️ 別 issue 候補）
7. **manual-smoke の git/gh を `-C $repoRoot` に揃えるか** → 揃えることを推奨（この変更が初めて生む記録の不整合・§3）。範囲が隣接するので計画で明示して合意を取る
8. **manual-smoke の記録のパス形式** → 既定の枝は repo 相対 + プロファイル、repo 外は伏せる（§3）。issue の「補足」が名指す論点で、素朴な移植（`$ExePath` をそのまま記録）は public PR へ利用者パスを出すので**出荷を止める論点**
9. **空の母集団の扱い**（コンテナ失敗に任せるか `-AllowNullOrEmptyForEach` + 件数 It か）→ ⚠️ どちらも fail-closed。後者は赤の範囲が局所化し理由が読める。好みの範囲
10. **述語 (3) のブロックコメント問題を今回直すか** → ⚠️ 対象 2 本の doc を触る今回が最も踏みやすいので、少なくとも運用注記か修正のどちらかを入れることを推奨。トークン化（`[Parser]::ParseFile` の tokens から `TokenKind.Comment` を除く）へ替えるなら (1)〜(3) 全体の入力が変わるので、ベースライン 4/4 緑を新しい入力でも再確認する
