# 実装計画: issue #1179 — `-ExePath` 既定値を呼び出し元のリポジトリから導く

ブランチ: `fix/bench-startup-exepath-default` / 調査: `workspace/research.md`

## 目的

`bench-startup.ps1` / `smoke-startup.ps1` を**既定のまま**回したとき、そのスクリプトが住むコピーの
本体を測るようにする。現在は両方ともメイン作業コピーの絶対パスを直書きしており、worktree から回すと
**別のコピーの本体を黙って測る**（本体は実在するので完走し、緑になる）。

## 受け入れ条件

1. worktree のスクリプトを**絶対パスで・引数なしで・cwd をメイン作業コピーに置いたまま**起動したとき、
   その worktree の `target/<profile>/snotra.exe` を測る
2. 測った本体のパスが**出力に現れる**（両スクリプトとも）
3. 本体が導けない／存在しないときは現行どおり `throw`（沈黙して別の本体へ落ちない）
4. `CARGO_TARGET_DIR` が**相対値**でも、cwd ではなく対象リポジトリを起点に解決する
5. CI 3 経路（`e2e.yml:124,137` / `release.yml:83`）の挙動が変わらない
6. `docs/build-commands.md` の既定値の記述が実装と一致する

## 変更ファイル一覧と対象シンボル

| ファイル | 対象 | 変更 |
|---|---|---|
| `scripts/lib/SnotraSmoke.psm1` | `Resolve-SnotraCargoExecutable`（385-413） | `cargo metadata` の呼び出しを `Push-Location $RepositoryRoot` … `finally { Pop-Location }` で囲む |
| `scripts/lib/SnotraSmoke.Tests.ps1` | `Describe 'Resolve-SnotraCargoExecutable'`（172-184） | 相対値 `CARGO_TARGET_DIR` × cwd 不一致の `It` を 1 件追加 |
| `scripts/bench-startup.ps1` | `param()` の `$ExePath`（28）／import 直後の検査（47-49）／`Write-Host "Exe:"`（85） | 既定を `''` にし、import 後に `-Profile release` で導出、絶対形へ正規化して表示 |
| `scripts/smoke-startup.ps1` | `param()` の `$ExePath`（9）／`Test-Path` 検査（22-24） | 同上（`-Profile debug`）＋本体パスの表示行を新設 |
| `docs/build-commands.md` | 218 行の `npm run bench:startup` の段落 | 既定の導出元を `cargo metadata` の `target_directory` へ書き換える（`:51` の `test:powershell` の書き方が語彙の先例） |

**触らない**: `smoke-egui.ps1` / `manual-smoke.ps1` / `measure-memory-stages.ps1`（`research.md` §2.2 の
「弱い形」。別作業コピーを黙って測る欠陥クラスに当たらない）。`.github/workflows/`（3 経路とも
`-ExePath` 明示ゆえ変更不要）。`.superpowers/sdd/` の 2 本（追跡外・死骸・`research.md` §2.3）。
`.claude/worktrees/agent-*` 配下の `docs/build-commands.md` の写し（別ブランチの作業コピーであり、
このブランチが更新する責務を持たない）。

## 実装順序

**Phase 1 → 2 → 3 の順に行う。** Phase 1 が先なのは、Phase 2 の 2 スクリプトが Phase 1 の関数へ
依存するためである（先に配線すると、cwd 依存の残る版へ 2 件の呼び出し元が増える）。

### Phase 1 — 共有ヘルパーの cwd 固定（`SnotraSmoke.psm1` + Pester）

`SnotraSmoke.psm1:399` 付近を次の形にする。`$LASTEXITCODE` は `Pop-Location` より**前に**捕まえる。

```powershell
    # **cargo の cwd を対象リポジトリへ固定する**（#1179）。相対値の `CARGO_TARGET_DIR` は
    # manifest ではなく **cargo プロセスの cwd** を起点に解決されるため、固定しないと
    # 「worktree の本体を導いたつもりでメイン作業コピーの target を指す」形が残る（実測）。
    # **manifest の存在検査より後に置く。** 根が不在ならそちらが先に落ち、`Push-Location` へ
    # 到達しない（＝push されないので Pop も要らない）。
    # ※ コメントに行番号を書かない（`.claude/rules/governance-docs.md`「序数で他を指してはならない」。
    #    スクリプトのコメントも `governance:check` の走査元に載る）
    Push-Location -LiteralPath $RepositoryRoot
    try {
        $metadataOutput = & cargo metadata --no-deps --format-version 1 --manifest-path $manifestPath
        $cargoExit = $LASTEXITCODE
    } finally {
        Pop-Location
    }
    if ($cargoExit -ne 0) {
        throw "cargo metadata に失敗しました（exit=$cargoExit）。"
    }
```

Pester を `SnotraSmoke.Tests.ps1` の既存 `Describe` へ 1 件足す。**期待値は実装と同じ `Join-Path` の
形で組む**（実装は `Join-Path $target "$Profile/snotra.exe"` の 2 引数 1 回）。

```powershell
    It '相対値の CARGO_TARGET_DIR でも RepositoryRoot を起点に解決する（cwd に依存しない・#1179）' {
        $repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
        $elsewhere = Join-Path $TestDrive 'elsewhere'
        New-Item -ItemType Directory -Force -Path $elsewhere | Out-Null

        Push-Location -LiteralPath $elsewhere
        try {
            $resolved = Invoke-SnotraEnvironment -Variables @{ CARGO_TARGET_DIR = 'relative-target' } -ScriptBlock {
                Resolve-SnotraCargoExecutable -RepositoryRoot $repositoryRoot
            }
        } finally { Pop-Location }

        $resolved | Should -Be (Join-Path (Join-Path $repositoryRoot 'relative-target') 'debug/snotra.exe')
    }
```

### Phase 2 — 2 スクリプトの配線

**共通の形**（`visual-input-metrics.ps1:55` に倣う）。`param()` の中では導出できないため
（`research.md` §4.3 で実測）、既定は `''` にし、`Import-Module` より後で解決する。

```powershell
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $ExePath) { $ExePath = Resolve-SnotraCargoExecutable -RepositoryRoot $repoRoot -Profile <release|debug> }
if (-not (Test-Path -LiteralPath $ExePath)) { throw <既存の文言> }
$ExePath = (Resolve-Path -LiteralPath $ExePath).Path   # 出力へ載せるパスを絶対形へ揃える
```

- `bench-startup.ps1`: `-Profile release`。既存の `throw` 文言（「release を測るなら先に
  `cargo build --release -p snotra`」）と `Write-Host "Exe:        $ExePath"` はそのまま活かす
- `smoke-startup.ps1`: `-Profile debug`。`Test-Path` は `-LiteralPath` を付ける。**本体パスの表示行が
  無いので新設する**（受け入れ条件 2）——既存の「検証用プロファイル: …」行の直前へ `Write-Host "本体: $ExePath"`
- 既定値を消す代わりに、`param()` へ「なぜ直書きしないか」を 2〜3 行のコメントで残す（#1179 を名指す）

### Phase 3 — 文書と実測

`docs/build-commands.md:218` の一文を、導出元を名指す形へ書き換える。**既定パスの文字列を写さない**
（写しになる）——`:51` の `test:powershell` の書き方（「`cargo metadata` の `target_directory` から
導くため `CARGO_TARGET_DIR` に追随する」）を語彙の先例にする。

## 不変条件と異常系

| # | 不変条件 | 破れたときの検知手段 |
|---|---|---|
| 1 | 導出も存在検査も失敗したら `throw`（沈黙して別の本体へ落ちない） | `Resolve-SnotraCargoExecutable` の 4 つの `throw` ＋ 呼び出し側の `Test-Path`/`throw`。Phase 4 の Gate 1 で実測 |
| 2 | 明示された `-ExePath` の意味を変えない（相対は cwd 相対のまま） | CI 3 経路が明示引数で回る（`e2e.yml` は両スクリプトを paths トリガに載せている） |
| 3 | 相対 `CARGO_TARGET_DIR` でも cwd に依存しない | Phase 1 の新 Pester。**変異注入で赤くなることまで確かめる**（下記） |
| 4 | `Push-Location` の後始末が例外経路でも走る | `finally`。`cargo metadata` が throw する経路を Pester で踏む必要は無い（`finally` は構造で保証） |
| 5 | **`-Profile` の取り違えが起きていない**（bench=release / smoke=debug） | 表示されるパスに `release` / `debug` の一片が現れることを Phase 4 で**明示的に確認する**。`/symmetric-check` 2c の所見: 2 つの対称な呼び出し点へ同型の文字列を配線しており、**入れ替えても ValidateSet・`Test-Path`・起動契約・CI（bench は `continue-on-error: true`）がすべて通る**。区別できる観測はこの一片だけである |

**受容する残余（宣言・2 件）**

1. 明示された**相対** `-ExePath` は cwd 相対のままなので、cwd がスクリプトの住むコピーと違えば
   別の本体を指しうる。これは呼び出し側が明示した意図として扱い、導出する既定の枝だけを直す。
   CI 3 経路は cwd = repo root なので影響しない
2. **`visual-check-colors.ps1` は「挙動が変わらない呼び出し元」ではない**（`/plan-review` の所見・
   機序を独立検算済み）。`:259` の `cargo build` は呼び出し元の cwd で走り、`:261` の
   `Resolve-SnotraCargoExecutable` はパッチ後 `$repositoryRoot` で走る。**相対値の
   `CARGO_TARGET_DIR` × cwd ≠ repoRoot** の交差で両者の指す先がずれ、`:262` の `Test-Path` が
   `throw` する。**これは fail-closed であって fail-silent ではない**——issue が問題視する
   「別の本体を黙って測る」形にはならないため、実装は変えず残余として宣言する
   （パッチ前は両者が同じ cwd 基準で偶然揃っていたので通っていた）

## テスト方針と検証コマンド

```bash
cargo build -p snotra                 # Pester の統合テストが debug 本体を要求する
npm run test:powershell               # Pester（Phase 1 の新 It を含む）
npm test                              # Vitest（.claude/hooks + .githooks + scripts）
npm run governance:check              # docs/build-commands.md を触るため（AGENTS.md 条件別チェック）
```

**`scripts/*.ps1` と `scripts/lib/*.psm1` には PostToolUse hook の検査が割り当てられていない**
（`selectChecks` の対象外）。編集後の沈黙は「何も走らなかった」であり、合格ではない。上のコマンドを
自分で打つ。

### 変異注入（`.claude/rules/safety-nets.md`「効いていることは一度は実測する」）

Phase 1 の新 Pester が**実際に守りたい退行へ届く**ことを測る。

**ライブの `SnotraSmoke.psm1` を書き換えてはならない**（`.claude/rules/safety-nets.md`
「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」）。
`git checkout --` で戻す前提の一時編集も採らない——`#489`「検査対象を変更しながら検査を走らせない」に触れる。

1. `scripts/lib/SnotraSmoke.psm1` を scratchpad へ**複製し、複製から** `Push-Location`/`Pop-Location`
   の 2 行を外す
2. pwsh で複製を `Import-Module` し、新 `It` と**同じ入力**（cwd = 別ディレクトリ・
   `CARGO_TARGET_DIR` = 相対値）で `Resolve-SnotraCargoExecutable` を呼ぶ
3. 返り値が新 `It` の期待値と**一致しない**ことを確認する（＝この変異なら `Should -Be` が落ちる）。
   `/plan-review` が同じ形で実測済み: unpatched は `...\elsewhere\relative-target\debug\snotra.exe` を返す
4. **変異の強さを確かめる**: 既存 `It`（**絶対値**の `CARGO_TARGET_DIR`）は複製でも同じ値を返す
   ことを確認する——赤くなるのが新 `It` だけであり、射程が正しいことの根拠になる
5. ライブの木で `npm run test:powershell` が緑

### Phase 4 — 直したことの実測（issue の 3 項目め）

**対照（機構なし / あり）の差が証拠であって、緑になったこと自体は証拠ではない。**

観測量は `research.md` §4.4 のバイトサイズ差（メイン release 10,890,752 B ／
`agent-a5b0f5810c7344357` release 10,879,488 B）で、パス表示に頼らず同一性を裏取りできる。

- **A 側（現状・機構なし）**: `agent-a5b0f5810c7344357` の既存 `bench-startup.ps1` を
  **絶対パスで・引数なしで・cwd をメイン作業コピーに置いたまま** `-Iterations 1` で回し、
  `Exe:` 行がメイン作業コピーを名指すことを記録する（＝ issue の症状の再現）
- **検証用 worktree の名前**: `.claude/worktrees/agent-verify-1179` にする。issue が報告した
  「メイン作業ツリーの内側」という geometry を再現でき、かつ **`clean-worktrees.mjs:34-37` が掃く
  母集団（`.claude/worktrees/` 配下・basename が `agent-` 始まり）に入る**ので、Gate が途中で
  throw して撤去の作業項目に届かなくても `npm run clean:worktrees` が後始末の受け皿になる
- **Gate 1（導出の確認・cold ビルド不要）**: このブランチから新しい worktree を切り、
  `target/` が空のまま既定で回す。`throw` の文言が `<新 worktree>\target\release\snotra.exe` を
  名指すことを確認する（受け入れ条件 3 と 1 の前半）
- **Gate 2（端から端まで）**: `agent-a5b0f5810c7344357` の release 本体を新 worktree の
  `target/release/` へ複製し、**同じ起動の形**（絶対パス・引数なし・cwd はメイン）で `-Iterations 1`。
  `Exe:` 行が新 worktree を名指し、起動が完走することを確認する。複製した本体は
  **「別の・起動できる本体」という役目のスタンドインである**ことを PR 本文へ明記する
  （退避: `startup:ready` に届かなければ新 worktree で `cargo build --release -p snotra` を払う）
  - **複製元は detached HEAD `7c8a4448` のビルドで、このブランチ（`main` の #1181/#1182
    = `LoadOrScanStats` の `Duration` 化の後）とは版が違う。** trace payload のキーが
    `$PhaseKeys` や `Test-SnotraStartupPayload` の契約とずれて**契約検査が落ちうる**。
    **それは「修正が誤り」ではなくスタンドインのスキーマ不一致であり、退避（cold ビルド）へ進む合図である。**
    なお**パス解決の観測（`Exe:` 行が worktree を名指すこと）は契約検査より前に確定している**ので、
    受け入れ条件 1・2 はこの経路でも先に満たされる
- **後始末**: 検証用に切った worktree は `git worktree remove` で消す

`smoke-startup.ps1` 側は Gate 1 と同じ形（debug・`agent-a802f374e32d2b999` の debug 本体を複製）で
1 回確認する。

## SPEC.md・関連文書の更新要否

- `SPEC.md`: **不要**。製品の挙動・仕様ではなく開発用スクリプトの既定値である
- `docs/build-commands.md`: **要**（Phase 3）
- `docs/architecture.md` / `docs/development-principles.md` / 各 `CLAUDE.md`: **不要**（`research.md` §6 で
  走査済み。敵対枠も独立に確認）
- `RETROSPECTIVE.md`: サイクル末の `/retrospective` が担当（この計画の所有ではない）

## 作業項目

### Phase 1 — 共有ヘルパーの cwd 固定

- [x] `SnotraSmoke.psm1` の `Resolve-SnotraCargoExecutable` を `Push-Location`/`finally Pop-Location` で囲み、`$LASTEXITCODE` を `Pop-Location` の前で捕まえる
- [x] 理由（相対 `CARGO_TARGET_DIR` が cwd 起点に解決される・#1179）をコメントで残す
- [x] `SnotraSmoke.Tests.ps1` へ相対 `CARGO_TARGET_DIR` × cwd 不一致の `It` を 1 件足す
- [x] `cargo build -p snotra` の後 `npm run test:powershell` が緑

### Phase 2 — 2 スクリプトの配線

- [x] `bench-startup.ps1`: 既定を `''` へ、`-Profile release` で導出、絶対形へ正規化、`param()` へ理由コメント
- [x] `smoke-startup.ps1`: 既定を `''` へ、`-Profile debug` で導出、絶対形へ正規化、`param()` へ理由コメント
- [x] `smoke-startup.ps1` へ本体パスの表示行を新設する
- [x] 既存の `throw` 文言が残っていることを確認する（沈黙して落ちない）

### Phase 3 — 文書

- [x] `docs/build-commands.md:218` の既定値の記述を導出元を名指す形へ書き換える（パス文字列の写しを置かない）
- [x] **`:51` と `:205` と `:267` は非該当**であることを確認して触らない（`:51` の `test:powershell` の
      `CARGO_TARGET_DIR` 追随は絶対値では元々真・相対値では cwd 非依存になり**むしろ正確になる**。
      `:205` は `smoke:egui` の既定で今回対象外。`:267` は `-Profile debug` のまま真。
      `/plan-review` が実測で確認済み——PR 本文へ「非該当」と一言残し、後続の重複調査を防ぐ）
- [x] `npm run governance:check` が緑

### Phase 4 — 実測と検証

- [x] 変異注入: psm1 の**複製**から `Push-Location`/`Pop-Location` を外し、新 `It` の入力で返り値が
      期待値と一致しないこと、かつ既存 `It`（絶対値）は変わらないことを実測（ライブの木は書き換えない）
- [x] A 側の再現: `agent-a5b0f5810c7344357` の現行スクリプトを cwd=メインのまま引数なしで回し、`Exe:` がメインを名指すことを記録
- [x] Gate 1: 新 worktree（`target/` 空）で既定のまま回し、`throw` が新 worktree のパスを名指す
- [x] Gate 2: release 本体を複製して既定のまま `-Iterations 1`、`Exe:` が新 worktree を名指して完走
- [x] **取り違えの確認**: `bench` の表示パスに `release`、`smoke` の表示パスに `debug` が含まれることを実測（不変条件 5）
- [x] `smoke-startup.ps1` を同じ形で 1 回確認する
- [x] 検証用 worktree（`agent-verify-1179`）を `git worktree remove` で消す
- [x] `npm test` が緑
- [x] 実装差分を確定させる（A/B の観測値を PR 本文へ載せられる形で控える → 下記）

### Phase 4 の実測結果（2026-08-25・PR 本文へ載せる分）

**変異注入**（psm1 の複製へ当てた。ライブの木は無改変）

| 版 | 新 `It`（相対値 `CARGO_TARGET_DIR` × cwd 不一致） | 既存 `It`（絶対値） |
|---|---|---|
| 変異体（Push/Pop 剥がし） | **不一致 → 赤**（`...\Temp\snotra-1179-elsewhere\relative-target\debug\snotra.exe`） | 一致 → 緑 |
| 正版（パッチ済み） | 一致 → 緑（`C:\workspace\Snotra\relative-target\debug\snotra.exe`） | 一致 → 緑 |

検知器は発火し、**変異の強さも正しい**（赤くなるのは新 `It` だけ＝射程が既存検査を巻き込まない）。

**A/B 対照** — いずれも **cwd = メイン作業コピー**、スクリプトを絶対パスで・引数なしで起動

| | スクリプトの所在 | 測った本体 | 終了 |
|---|---|---|---|
| **A（修正前）** | `agent-a5b0f5810c7344357` | `C:/workspace/Snotra/target/release/snotra.exe`（**メイン**） | exit 0・「起動計器 passed」 |
| **B（修正後）** | `agent-verify-1179` | `...\agent-verify-1179\target\release\snotra.exe`（**自分のコピー**） | exit 0・「起動計器 passed」 |

**両方とも緑である。** 違うのは測った対象だけであり、これが issue の主張（失敗が緑と同じ見た目をする）の実物である。

- **Gate 1**（`target/` 空・ビルド無し）: `throw` が
  `C:\workspace\Snotra\.claude\worktrees\agent-verify-1179\target\release\snotra.exe` を名指した（exit 1）
- **Gate 2**（release 本体を複製して 1 回）: `Exe:` が worktree の絶対パス・`pre_main=43ms post_main=220ms
  mem=70.3MB cache_hit=True first_run=False`・「起動計器 passed（1 runs）」。**スタンドインの
  スキーマ不一致は起きず、退避（cold ビルド）は不要だった**
- **`smoke-startup.ps1`**: `本体: ...\agent-verify-1179\target\debug\snotra.exe`・
  「Startup smoke passed (1 runs)」。本体サイズはメイン `53,621,248` に対し worktree `53,613,568` で別物
- **取り違えの確認**: bench の表示パスに `release`、smoke の表示パスに `debug` を実測（不変条件 5）

### Phase 5 — 委譲レビューへの対応（実装中に判明・計画外）

`workspace/verify-1179.txt`（worktree の検証・レビュー委譲・アンカー `37b49ad7`）の所見。

- [x] **High-1**: `$repoRoot` の導出を cwd 起点へ戻すと **Pester 129・vitest 901・smoke 自身のすべてが
      緑のまま** #1179 の欠陥が復活する（委譲先が変異 (D) で実測）。計画は `-Profile` の渡し忘れだけを
      残余として宣言しており、**この足は宣言の外だった**。`.claude/rules/safety-nets.md`「壊れたとき
      緑が緑のまま推移するか」に照らすと**推移する**ので、機構を置く条件を満たす
      → `SnotraSmoke.Tests.ps1` へソース述語の `Describe` を 1 件追加（`$repoRoot` の導出が
      `$PSScriptRoot` 起点であること・2 スクリプト分）。`test:powershell` は `ci.yml:239` で通常 PR CI に載る
- [x] **High-1 の検知器を、置く前に変異で測る**（複製へ変異 (D) を当て、正版=緑 / 変異体=赤を実測）
- [x] **Medium-1**: `bench-startup.ps1` の `-ProfileDir` 既定だけが cwd 起点で、修正後は `Exe:` が
      worktree・`Profile:` がメイン作業コピーへ割れる（実測: メイン側に `config.toml` と `index.bin` が
      生成された）。**差分が触っていない行だが、割れは今回の変更で初めて生きた組み合わせである**
      → `-ExePath` と同じ判断（既定の枝だけ導出・明示値は cwd 相対のまま）へ揃える
- [x] **Low-1**: `smoke-startup.ps1` の `throw` へ復旧手順（`cargo build -p snotra`）を足す
      ——既定が自分のコピーを指すぶん「本体が無い」が worktree で起きやすくなった
- [x] **Low-4**: `docs/build-commands.md` の行折れを整える
- [x] **⚠️-2（部分採用）**: 新設段落から「`CARGO_TARGET_DIR` に追随する」の重複を外し、
      `test:powershell` への参照へ寄せた（`.claude/rules/governance-docs.md`「かぶりなく」）
- [x] **Low-2 / Low-3 は却下**（理由は下記「却下したレビュー所見」）
- [x] Phase 5 の差分で `test:powershell` / `npm test` / `governance:check` が緑
- [x] 委譲先へ新しい sha を渡して再実行させる（`/implement` 3c: 自分が出した指摘の解消は本人が検算する）

### Phase 6 — 委譲レビュー 2 巡目への対応

2 巡目（`workspace/verify-1179-round2.txt`・アンカー `0f0c7af8`）は High-1 / Medium-1 の解消を
独自に実測したうえで、**検知器自身の欠陥**を 1 件出した。

- [x] **High-2**: 検知器は `$repoRoot` の**導出**を縛るが**使用点**を縛らない。導出行を無傷のまま
      `-RepositoryRoot` の引数を cwd 起点へ差し替えると、**Pester 131・vitest 901・smoke 自身が
      すべて緑のまま** #1179 が復活する（委譲先が変異 G で実測）。宣言していた死角は「導出の中身／
      プロファイルの置き場／3 本目のハーネス」の 3 つで、**使用点は名指されていなかった**
      → **除外リストへ書き足すのではなく穴を塞いだ。** 書き足す直し方は「偽の全称を直した文が
      また全称で偽になる」形に嵌まる。使用点を見る `Should` を 2 件足し、宣言を
      「保証は導出と使用点の両方に掛かる」という肯定形へ書き換えた
- [x] **High-2 の検知器を足ごとに測る**——変異 D（導出）／G（使用点）／M-add（再導出を足して後勝ち）の
      3 本すべてで Pester の配線ごと赤、正版のみ緑を実測
- [x] **Low-2**: 明示 `-ProfileDir` の絶対値が `Join-Path` で `<cwd>\C:\…` になり壊れていた
      （**修正前から在った欠陥**。委譲先が `git show 37b49ad7:` の一時展開で同一エラーを確認）。
      報告に値するのは挙動ではなく**新設した param コメント**で、「`-ExePath` と同じ判断」と
      謳いながら `-ExePath` は絶対値を正しく扱い `-ProfileDir` は扱えなかった——**コメントが
      挙動より一段強い**。ネイティブの `GetFullPath(path, basePath)` へ寄せて挙動側を合わせた
      （実測: 相対値は cwd 相対のまま exit 0／絶対値は exit 1 → exit 0）
- [x] **Low-1**: 検知器の `-Because` が原因を取り違えていた（意味不変の整形でも「cwd 起点にすると」と
      言う）→「1 行で書くこと」という**形の要求**へ書き換え
- [x] **⚠️-A**: 新段落末尾の「明示した相対パスは cwd 相対のまま」は bench/smoke では真だが
      `test:powershell` では偽（`run-pester.ps1:34` が re-root する）→「（`test:powershell` と違い）」を補った
- [x] Phase 6 の差分で `test:powershell` 131 / `npm test` 901 / `governance:check` が緑
- [x] **Low-3 は却下**（ハーネスの構文エラーがどの自動検査にも届かない件。委譲先自身が「実害小・
      対処不要」と判断。ソース述語型検知器一般の性質であり、この差分に固有の欠陥ではない）
- [x] **⚠️-B は対応不要**（数え上げの死角は宣言済み。広げると既存 3 本で誤検出になることを 2 巡とも実読で裏づけ）

### 却下したレビュー所見（理由つき）

| 所見 | 却下の理由 |
|---|---|
| **Low-2**: 導出が抜けたとき `throw` がパスを表示できない | 到達経路が**変異注入だけ**である。導出行が在る限り `$ExePath` はこの地点で非空であり、守る退行が実在しない |
| **Low-3**: `smoke-startup.ps1` の `Test-Path` で `-LiteralPath` が不揃い（`:71-72` / `:120`） | 挙動上の差が無く、差分が触っていない行である。ここで直すと「ついでの整形」が差分に混じり、レビューの焦点がぼやける |
| **⚠️-1**: `Push-Location` が native 子プロセスへ効くのは実装依存では | **実リポジトリで効くことを実測済み**（相対 CTD が RepositoryRoot 配下へ解決）。委譲先自身も「この差分の妥当性を否定するものではない」と述べている。`--target-dir` 案は `CARGO_TARGET_DIR` の追随という既存の約束を壊すので採らない |
| **⚠️-3** | 委譲先自身が「射程を `visual-check-colors.ps1` に限った判断は正しい」と結論。対応不要 |

## 未確定（実装前に潰す）

*（なし——下記 3 件は調査中に解消済み。判断と根拠を残す）*

- [x] **明示された相対 `-ExePath` を repoRoot へ join するか** — **却下**。`run-pester.ps1` は join するが、
  あちらの `-ExePath` は「別の本体を検査するときだけ」の override であり意味が違う。PowerShell の慣行では
  相対パスは cwd 相対であり、明示引数の意味を黙って変えるのは issue の要求外。CI 3 経路は cwd = repo root
  ゆえどちらでも同一。→ `visual-input-metrics.ps1` 型を採り、残余を「不変条件と異常系」で宣言した
- [x] **実測にどこまで払うか** — cold ビルドは不要と判定。`agent-a5b0f5810c7344357` に**バイトサイズの違う**
  release/debug 本体が既に在り（`research.md` §4.4）、これを複製すれば「別の・起動できる本体」として
  Gate 2 が成立する。退避経路（cold ビルド）も Phase 4 に書いた
- [x] **`-Profile release` の枝に Pester を足すか** — **却下**。`Join-Path $target "release/snotra.exe"` は
  ValidateSet 済み文字列の補間であり、テストは実装の同語反復になる。守りたい退行は「`.ps1` が
  `-Profile` を渡し忘れる」ほうだが、それは psm1 のテストからは原理的に見えない。検知は Phase 4 の
  `Exe:` 行（`release` / `debug` が読める）に委ね、これを**受容する残余**として宣言する

## セルフレビュー

- リスク: **高**（`docs/build-commands.md` = ガバナンス文書を変更するため。`/plan-review`「リスク判定」の
  「hook、CI、rules、skills、ガバナンス文書を変更する」に該当）
- plan-review: 独立レビュー1体（Step 2 = 計画準拠）
- エージェント数: 2（Step 3b の敵対枠 1 + plan-review 1）
- 要対処: **0 件**
- 軽微: 2 件・**両方反映済み**（① `visual-check-colors.ps1` の fail-closed な交差を「受容する残余」2 へ
  追加 ② `docs/build-commands.md` の `:51`/`:205`/`:267` が非該当であることを Phase 3 の作業項目へ明記）
- 未検証（`/plan-review` が申告・そのまま引き継ぐ）: 実 Pester の完走（実バイナリ起動のため委譲側で
  禁止した——**Phase 1 の作業項目が実行時に測る**）／`SnotraSmoke.Tests.ps1` の他 `Describe` への波及
  （`Push-Location` は当該関数に閉じるため理論上無関係だが実 run では未確認——同じく Phase 1 で測る）

### `/plan-review` 結果（Step 2 = 計画準拠の独立レビュー 1 体）

- 観点 A（共有ヘルパー変更の回帰）: 呼び出し元 3 件（`run-pester.ps1:37` / `visual-input-metrics.ps1:55` /
  `visual-check-colors.ps1:261`）を全読。回帰は上記の 1 件のみ。既存 Pester（`SnotraSmoke.Tests.ps1:172-183`）は
  **絶対値**の `CARGO_TARGET_DIR` を使うため cwd 固定の影響を受けない
- 観点 A（新 `It` の実効性）: psm1 を直接 import して patched / unpatched の両方を実行し、
  unpatched が `...\elsewhere\relative-target\...`（バグ再現）・patched が計画の期待値と `MATCH = True` を実測。
  **主エージェントも独立に同じ期待値の形を測った**（`Join-Path` の二重適用が実装と一致することを確認）
- 観点 B: `governance-check` の 3 チェッカー（`G-build-commands.mjs` / `G-ci-table.mjs` / `G-hook-commands.mjs`）を
  実読し、**いずれも構造検査のみで散文の意味論を見ない**ことを確認。計画は意味論の担保を
  `governance:check` に頼っていない

### `.claude/rules/` の適用（`paths` に `scripts/**` を持つ 2 枚）

本セッションは全ファイル読み取りを `cat`/`sed` で行ったため rules の自動配送が発火しておらず、
Read ツールで手動配送した（`MEMORY.md` の `rules-delivery-needs-read-tool`）。**2 件の違反を捕まえた。**

| rules の条項 | 検出した違反 | 反映 |
|---|---|---|
| `.claude/rules/safety-nets.md`「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」 | 変異注入をライブの psm1 の一時編集として書いていた | 複製へ変異を当てる形へ書き換え、**変異の強さの確認**（既存 `It` は変わらないこと）も追加した |
| `.claude/rules/governance-docs.md`「序数で他を指してはならない」（走査元は `.md` だけでなく**スクリプトのコメントも載る**） | psm1 へ入れるコメントが `:394-397` と行番号で参照していた | 行番号を外し、散文で「manifest の存在検査より後」と書く形へ |

同 rules の他条項との照合: `docs/build-commands.md` の書き換えは「かぶりなく」（パス文字列の写しを
置かない）を守る。「CI の実測は計画の検証項目に置かずPR 本文へ送る」（`safety-nets.md`）は、
Phase 4 がすべてローカル実測ゆえ該当しない。

### `/symmetric-check` の結果（`AGENTS.md` 条件別チェック「対称ペア（生成/破棄）を変更」に該当）

| # | 候補 | 判定 | 根拠 |
|---|---|---|---|
| 1 | `Push-Location`/`Pop-Location` と `Invoke-SnotraEnvironment` の重複 | **不要** | 対象リソースが違う（cwd vs `Env:`・`SnotraSmoke.psm1:284-319`）。`scripts/` 配下に `Push-Location`/`Pop-Location`/`Set-Location` は 0 件で揃える先例が無く、ネイティブ機構を採る |
| 2 | `Push-Location` の位置 | **適用** | 既存の `Test-Path $manifestPath` throw（`:394-397`）より後。Phase 1 のコードへ反映済み |
| 3 | `-Profile release` / `debug` の取り違え（2c 同型ペア） | **適用** | 不変条件 5 と Phase 4 の作業項目へ反映済み |
| 4 | 検証用 worktree の 生成/破棄 | **適用** | 名前を `agent-verify-1179` にして `clean-worktrees.mjs:34-37` の掃除母集団へ入れた |

### Step 1 の自己照合（`/plan-review` Step 1 の 7 項目）

1. issue の 3 要件 → Phase 2（既定の導出）／Phase 2 の `throw` 維持（導けないときの挙動）／Phase 4（実測）に対応
2. 変更ファイル・シンボルは全件 grep で実在確認済み（`research.md` §2.2・§3）
3. 不変条件 4 件に検知手段を割り当て、うち 1 件は変異注入で発火まで測る
4. `SPEC.md` 不要の判断は「製品の挙動ではない」から。関連文書は `research.md` §6 で走査
5. 未確定欄に未チェックなし
6. トリガーを跨ぐ分割をしていない（Phase 1 の関数変更と Phase 2 の呼び出し点移行は**同一 PR**。
   ただし PowerShell に `-D warnings` 相当は無く、`dead_code` の制約は当たらない）
7. 変更で偽になる散文の走査: `bench:startup` / `bench-startup` / `smoke:startup` / `smoke-startup` /
   `ExePath` / `CARGO_TARGET_DIR` の 6 形で走査済み（`research.md` §6。敵対枠も独立に確認）

## 人間レビュー

- [x] 承認済み — 2026-08-25 / 問い: "この計画を承認なさいますか。それとも `workspace/plan.md` へ注釈を追加なさいますか。" / 回答: "承認"
