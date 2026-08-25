# ADR-harness-executable-path-derivation: 起動ハーネスの本体パスを、スクリプトが住むリポジトリから導く

## 文脈

`scripts/bench-startup.ps1` と `scripts/smoke-startup.ps1` の `-ExePath` 既定値が、メイン作業コピーの
絶対パスで直書きされていた（#1179）。worktree から既定のまま回すと**別の作業コピーの本体を測る**が、
本体は実在するのでスクリプトは完走し、payload も契約検査も通る——**失敗が緑と同じ見た目をする**。
`/implement` は検証を worktree のエージェントへ委ねる設計なので、この経路は日常的に踏まれる。

導出の先例は既にあった。`npm run test:powershell`（`scripts/run-pester.ps1`）が
`Resolve-SnotraCargoExecutable` 経由で `cargo metadata` の `target_directory` から本体を導いており、
`docs/build-commands.md` は「`CARGO_TARGET_DIR` に追随する」と約束している。

## 決定

1. 既定値を `''` にし、`Import-Module` の後で `Resolve-SnotraCargoExecutable` から導く。
   **導出の起点はスクリプトが住むリポジトリ（`$PSScriptRoot` 起点）であって cwd ではない。**
2. `Resolve-SnotraCargoExecutable` の `cargo metadata` 呼び出しを `-RepositoryRoot` へ cwd 固定する。
3. 明示された `-ExePath` の意味は変えない（相対パスは cwd 相対のまま）。
4. 導出の形をソース述語で守る（`SnotraSmoke.Tests.ps1`）。射程は**下限主張**として宣言する。

## 検討した代替案と却下理由

- **`cargo metadata --target-dir` で target を明示する**: 却下。`Push-Location` より構造的に強く見える
  （周囲の cwd に依存しない）が、**`CARGO_TARGET_DIR` への追随という既存の約束を壊す**。
  `docs/build-commands.md` は `test:powershell` について既にこれを約束しており、明示指定は
  ユーザーが設定した `CARGO_TARGET_DIR` を無視することになる。**約束を守るには ambient な env を
  読ませる必要があり、そのとき相対値の解決基準は cargo プロセスの cwd になる**——ゆえに
  cwd の固定は迂回できない。相対値が cwd 起点で解決されることは実測で確かめた
  （cwd をメイン作業コピーに置いて worktree の manifest を指すと、target がメイン側を指す）。

- **明示された相対 `-ExePath` を `$repoRoot` へ re-root する（`run-pester.ps1` に揃える）**: 却下。
  あちらの `-ExePath` は「別の本体を検査するときだけ」の override であり意味が違う。PowerShell の
  慣行では相対パスは cwd 相対であり、**明示引数の意味を黙って変えるのは #1179 の要求の外**である。
  CI 3 経路は cwd = repo root ゆえどちらでも解決先は同じで、変える利得が無い。
  結果として「明示相対は cwd 相対のまま」は受容する残余になり、`docs/build-commands.md` に明記した。

- **ソース述語を「`Resolve-SnotraCargoExecutable` を呼ぶ `.ps1` すべて」へ広げる**: 却下。
  `run-pester.ps1` / `visual-input-metrics.ps1` / `visual-check-colors.ps1` は変数名も導出の形も違い、
  **正当な差異を誤検出する**。対象を数え上げる死角（3 本目の起動ハーネスに黙る）は宣言して止める。

- **述語を足して同義の別構文まで閉じる**: 却下。同じ意味を書ける構文が複数ある以上、`Should` を
  足すたびに「その形以外」が残る（実測: 代入構文を使わない差し替えが通る）。**足し続けても収束しない**
  ——偽の全称を直した宣言がまた全称で偽になるのと同じ形である。閉包を主張せず下限主張として宣言する。

## 帰結

- 起動ハーネスを worktree から既定のまま回すと、その worktree の本体を測る。測った本体の絶対パスは
  両者とも出力の先頭に出る。
- `Resolve-SnotraCargoExecutable` の呼び出し元すべてが cwd 非依存になる。副作用として
  `visual-check-colors.ps1` は「相対 `CARGO_TARGET_DIR` × cwd ≠ repoRoot」の交差で `throw` しうるが、
  これは fail-closed であって「別の本体を黙って測る」形ではない。
- ソース述語は入口・導出・使用点の 3 点を各 1 つの代表的な書き方で覆う。同義の別構文と、
  導出後の値の上書きは射程外である。
