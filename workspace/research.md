# 調査: #1292 smoke:egui の既定 ExePath が release で、カテゴリ C の手順では直前のビルドを測らない

## issue の要約

- `docs/build-commands.md` カテゴリ C は「実バイナリを起動する検査の前に `cargo build -p snotra` を打つ」と書く。このコマンドが更新するのは `target/debug/snotra.exe` である
- ところが `scripts/smoke-egui.ps1:2` の既定は `[string]$ExePath = "target/release/snotra.exe"` で、cwd 相対の直書きになっている
- 手順どおりに打つと、release が無ければ exit 1 で止まる（PR #1291 で実測）。release が古いまま残っていれば、それを測って緑を返す
- 受け入れ条件: (1) カテゴリ C の手順をそのまま打つと、直前に建てたバイナリを `smoke:egui` が測る。(2) CI の smoke-egui job が測るバイナリは変わらない
- ユーザーの判断（2026-10-25）: 範囲は「最小＋follow-up issue」とする。検知器の母集団を変える件と、残り 2 本の直書きは別の issue に起こす

## 関連ファイル・シンボル（grep で実在を確認済み）

- `scripts/smoke-egui.ps1`
  - `param()` の `$ExePath`（2 行目）
  - `Import-Module (Join-Path $PSScriptRoot 'lib/SnotraSmoke.psm1')`
  - 存在の確認 `if (-not (Test-Path $ExePath)) { throw "Executable not found: $ExePath" }`（63〜64 行目）
  - 使用点は `Start-SnotraProcess -FilePath $ExePath` の 2 か所（151 行目と、toast の 731 行目）
  - `$repoRoot` の変数は現状どこにも無い
- `scripts/smoke-startup.ps1:9-33` が、揃える先の形である
  - 既定は `''` にする
  - `Import-Module` の後に `$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path` を置く
  - `Resolve-SnotraCargoExecutable -RepositoryRoot $repoRoot -Profile debug` で導く
  - 存在の確認は `Test-Path -LiteralPath` で行い、`throw` の文言に復旧手順「先に cargo build -p snotra」を持たせる
  - `Resolve-Path` で絶対形にしてから `Write-Host "本体: $ExePath"` で出力する
- `scripts/lib/SnotraSmoke.psm1` の `Resolve-SnotraCargoExecutable`
  - 呼び出し側の作法の正本は、この関数の doc である。各スクリプトへ写しを置かない
- `scripts/lib/SnotraSmoke.Tests.ps1` の Describe「起動ハーネスの既定 ExePath はスクリプトの住むコピーから導かれる」
  - `-ForEach` に `'bench-startup.ps1', 'smoke-startup.ps1', 'visual-input-metrics.ps1'` を手で並べている
  - 述語は 3 つある。(1) `param()` の既定が空リテラルであること。(2) `$repoRoot =` が 1 行だけ在り、`$PSScriptRoot` を含むこと。(3) `Resolve-SnotraCargoExecutable` の呼び出しすべてが `-RepositoryRoot $repoRoot` を渡すこと
  - コメント行（`^\s*#`）は判定から除かれる
- `.github/workflows/smoke.yml:116`: `npm run smoke:egui -- -ExePath target/release/snotra.exe`
  - CI は明示して渡すので、既定を変えても CI が測るバイナリは変わらない（issue のコメントでも確認済み）
- `docs/build-commands.md`
  - 210 行目: `smoke:egui` の行に「既定 ExePath = target/release」と書いてある。直す必要がある
  - 229〜234 行目: 「`bench:startup` と `smoke:startup` も…」「この 2 つでは」の段落。ここに `smoke:egui` が入るので、数で言う書き方をやめる
  - 279 行目の注: `smoke:startup` の既定は debug、と書いてある。今回の変更でも真のままである
- `docs/adr/ADR-harness-executable-path-derivation.md`
  - 凍結された歴史なので触らない（`ADR-adr-frozen-history`）。「残る 2 本」という記述は古くなるが、それで構わない

## 再利用できる既存パターン

- `smoke-startup.ps1` の導出の形を、行単位でそのまま写せる。既存の Pester 述語は、この形なら新しい assertion を足さずに通る
- `-ForEach` の一覧へ `'smoke-egui.ps1'` を 1 行足すだけで、退行の検知器に載る

## 技術的制約

- 既定値の導出は `param()` の中に書けない。束縛が `Import-Module` より前に起きるためである（resolver の doc）。そこで既定は `''` にし、import の後で解決する
- 明示された `-ExePath` の意味は変えない。相対パスは cwd 相対のままにする（ADR の決定 3）。CI の明示渡しも、これまでどおり効く
- **debug が古いまま残る型は、逆向きに起こりうる。** 2026-10-25 時点のこの作業コピーでは、`target/debug/snotra.exe` が 2026-10-09、`target/release/snotra.exe` が 2026-10-24 の更新だった（mtime を実測）。既定を debug に変えたあと `cargo build -p snotra` を飛ばすと、古い debug を測ることになる
  - これはカテゴリ C の手順（ビルドしてから測る）に従えば起きない。#835 の警告と同じ扱いになる
  - 測った本体の絶対パスが出力の先頭に出るので、少なくともどの本体を測ったかは見える
- debug ビルドで smoke:egui の観測時間（`StartupObserveTimeoutMs` 25000 / `ObserveTimeoutMs` 8000）に収まるかについて
  - PR #1291 の検証で `-ExePath target/debug/snotra.exe` を渡して緑だった（issue の本文）。実績はこの 1 回である
- `scripts/*.ps1` を編集しても、編集時に自動で走る検査は無い。検証は `npm run test:powershell` で手で行う

## 作業の流れについての所見（ユーザーの依頼「今の作業の流れ含めて」への回答）

- **#1179 が直した範囲の外に、この 1 本が残った。** 検知器の母集団は「`Resolve-SnotraCargoExecutable` を呼ぶ `.ps1`」から取られた（ADR の代替案の 3 つ目）。`smoke-egui.ps1` はこれを呼ばないので、候補に入っていなかった。`480aa1d` に `smoke-egui` の語は 0 件で、検討されたうえで外されたという記録も無い（3b の B3）。検知器のヘッダ自身も「対象は数え上げているので、4 本目が増えても黙る」と宣言している
  - 守りたい性質は「`-ExePath` の既定を持つスクリプトが、直書きをせず導出を通ること」である。母集団は、その性質を持ちうる側（`[string]$ExePath =` を宣言するスクリプト）から取るべきだった。直した後の形（resolver を呼ぶスクリプト）から取ると、直し損ねたものが原理的に入らない
- **同じ型の直書きが、少なくとも 2 本残っている**（`git grep -n 'ExePath =' scripts/` で確認）
  - `scripts/manual-smoke.ps1:3`: `"target/debug/snotra.exe"` の cwd 相対。cwd がリポジトリルートでないときだけずれる
  - `scripts/measure-memory-stages.ps1:44`: `"$PSScriptRoot/../target/release/snotra.exe"`。`CARGO_TARGET_DIR` に追随しないだけで、別の作業コピーは測らない
  - どちらも #1179 の「絶対パスで別コピーを測る」ほど重くはない
- 扱い: follow-up issue に起こす（ユーザー判断）。内容は 2 つある。(1) Pester の母集団を `[string]$ExePath =` の宣言から導き、除外は理由つきの許可リストにする。(2) 残り 2 本を導出の形へ揃える。検知器の母集団の変更はセーフティネットの変更なので、この PR では行わない

## 未解決の疑問

- なし（3b の所見で解消した。下の表を参照）

## 3b 敵対的調査の所見と採否（`workspace/adversarial-1292.txt`）

測定環境は前提どおりだった。`CARGO_TARGET_DIR` は未設定、`cargo metadata` の `target_directory` は `C:\workspace\Snotra\target`、resolver が返すのは `target\{debug,release}\snotra.exe`。mtime も上の記述どおりだった。

| 所見 | 判定 | 採否と理由 |
|---|---|---|
| B1: 「smoke-egui は release」と読めるコメントが `SnotraSmoke.Tests.ps1:572-574` と `SnotraSmoke.psm1:367-369` に残る | 壊せた | **採る。** 自分で読んで確かめた。後者の「release を使う `smoke-egui` が…一度も踏んでいない」は、変更後はローカルの既定で偽になる。どちらも「CI では release」と限定する形に直す。計画の変更ファイルへ加える |
| B2: `prepare-sidecar.ps1:11` にも `target/release` と `target/debug` の直書きがある | 壊せた | **記録だけにする。** 対象は snotra.exe ではなく snotra-settings のサイドカーで、ビルド構成を選ぶ用途であり、検査が測る本体の取り違えとは別の型である。follow-up issue に「名前を変えた直書きも母集団の候補」として書き添える |
| B3: 「#1179 の母集団が理由で候補に入らなかった」の因果は記録から決められない | 壊せた | **採る。** `git show 480aa1d` に `smoke-egui` は 0 件だった（実測）。言えるのは「#1179 の対象と検知器の母集団に smoke-egui は入っておらず、検討された記録も無い」までである。上の所見の書き方をそう読み替える |
| K1: 既存の述語 3 つは、計画どおりの複製を新しい assertion なしで通す（末尾カンマも正しく剥がす。`$repoRoot` の語は元のファイルに 0 件） | 壊せなかった | 計画どおりに進める。resolver の呼び出しは 1 行で書く（折ると述語 (3) の行一致から外れる） |
| K2: 既定のまま smoke-egui を呼ぶ経路は無い。CI は `smoke.yml:116` で明示して渡す | 壊せなかった | 受け入れ条件 2 の根拠にする。既定の枝は CI では走らないので、ローカルの実機確認が唯一の観測になる |
| K3: `$ExePath` の使用点に、cwd 相対や release を前提にしたものは無い | 壊せなかった | — |
| K4: debug 固有の差（stderr の非 JSON 行・コンソール窓）は既存の機構で塞がれている | 壊せなかった | — |
| K5/K6: 他スクリプトの分類と、build-commands.md の該当箇所は正しい | 壊せなかった | — |
| ⚠️W1: debug での smoke-egui の実績は #1291 の 1 回だけで、固定 Sleep（Z を押しっぱなしにするブロック・#1274）が debug のフレーム遅延に耐えるかは未測 | 未確定 | **採る。** 実機の確認を 1 回ではなく 3 回連続で回す。1 回でも赤なら、既定を debug にする判断を見直す（未確定欄） |
| ⚠️W2: 実際の Pester は未実行 | — | 実装の Phase 1/2 で回す |
| ⚠️W3: 歴史の計画書（`docs/superpowers/plans/` 配下）の「release の後に smoke:egui」 | — | 触らない。凍結された記録である |
| ⚠️W4: prepare-sidecar を同じ型に数えるか | — | B2 のとおり |
| ⚠️W5: B3 の因果 | — | B3 のとおり |
