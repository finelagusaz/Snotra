# plan-review Step 2 — issue #1179 独立レビュー

対象: `workspace/plan.md`（`fix/bench-startup-exepath-default`）
観点: A（共有ヘルパー変更の回帰）/ B（ガバナンス文書の変更の十分性）
方法: コード・doc を実読、かつ `Resolve-SnotraCargoExecutable` の patched/unpatched 版を pwsh で直接実行して実測（Pester 本体は未キャッシュのため npm run test:powershell は指示どおり未実行）。

## 観点 A — 共有ヘルパー変更の回帰

### 呼び出し元 3 件（grep で列挙済み・計画の宣言と一致）

`grep -rn "Resolve-SnotraCargoExecutable" scripts/` の結果は 4 箇所のみ:
`SnotraSmoke.psm1:385`（定義）/ `SnotraSmoke.Tests.ps1:172,178`（既存 It）/
`run-pester.ps1:37` / `visual-check-colors.ps1:261` / `visual-input-metrics.ps1:55`。

3 呼び出し元すべてを読んだ。**いずれも `Resolve-SnotraCargoExecutable` の呼び出し前後で
cwd 依存の別処理を挟んでいない**——Push-Location は関数内で完結し `finally { Pop-Location }`
で呼び出し前の cwd へ戻ってから return するため、呼び出し元からは cwd が変化しないのと
区別が付かない。

- `run-pester.ps1:11,37`: `$repoRoot` を `Resolve-Path` で先に固定し、そのまま
  `-RepositoryRoot $repoRoot` へ渡すだけ。呼び出し前後に cwd 相対の I/O は無い
- `visual-input-metrics.ps1:41,55`: 同型。`$repoRoot` は `Import-Module` 前に確定済み
- `visual-check-colors.ps1:256,260`: `cargo build -p snotra --manifest-path $manifestPath`
  （`$repositoryRoot` から導いた絶対パス）を**先に**実行してから `Resolve-SnotraCargoExecutable`
  を呼ぶ。`--manifest-path` 自体は絶対だが、**`cargo build` は `Resolve-SnotraCargoExecutable` の
  外で呼ばれており Push-Location の対象外**——ここは訂正が要る（下記）。

**訂正: 1 件、挙動が変わる呼び出し元がある（fail-closed・軽微）。**
`visual-check-colors.ps1` の `cargo build`（:258）は呼び出し元の実際の cwd で走る。一方
Phase 1 適用後の `Resolve-SnotraCargoExecutable`（:261）は cwd を `$repositoryRoot` へ固定して
`cargo metadata` を呼ぶ。**相対値の `CARGO_TARGET_DIR` が設定されており、かつスクリプト起動時の
cwd が `$repositoryRoot` と異なる**という交差では、両者が異なるディレクトリへ解決する:

- `cargo build` は `<起動時 cwd>/<相対 CARGO_TARGET_DIR>/debug/snotra.exe` へ本体を置く
  （§4.2 と同じ「相対値は cwd 起点」の cargo の一般的挙動——`cargo metadata` に限らない）
- Push-Location 済みの `Resolve-SnotraCargoExecutable` は
  `<repositoryRoot>/<相対 CARGO_TARGET_DIR>/debug/snotra.exe` を返す

**パッチ適用前はこの 2 つが同じ cwd 基準で揃っていた**（未パッチの `Resolve-SnotraCargoExecutable`
も呼び出し元の cwd をそのまま使うため）。**パッチ適用後は不一致になり、`Test-Path -LiteralPath $exe`
（:262）が `$false` を返して「実行ファイルが在りません」で `throw` する**——サイレントに別本体を
測るのではなく**大きな音で落ちる**（issue が禁じる「失敗が緑と同じ見た目をする」形ではない）。

この交差が起きるのは「相対値の `CARGO_TARGET_DIR` を設定した環境で、`visual-check-colors.ps1`
を repo root 以外の cwd から実行する」場合に限られる。`research.md §4.3`「本セッションの env に
`CARGO_TARGET_DIR` は設定されていない」・CI 3 経路は `visual-check-colors.ps1` を呼ばない
（このスクリプトは手動 GUI smoke 系）ことから、**現状では踏まれていない**が、計画の
「不変条件と異常系」表・「受容する残余」節はこの交差を明示的には宣言していない。
**観点 A の問い「挙動が変わる呼び出し元が無いか」への正確な答えは「無い」ではなく
「1 件あるが fail-closed」である。**

### 既存 Pester It（172-183）は cwd 固定で壊れないか

`SnotraSmoke.Tests.ps1:172-183` の既存 It は `CARGO_TARGET_DIR` に **`$TestDrive` 配下の絶対パス**
を渡している（`$customTarget = Join-Path $TestDrive 'custom-cargo-target'`）。絶対値は cwd に
依存しないため、Push-Location の有無で結果は変わらない。**この It が実行される時点の cwd 自体は
変更しない**（`Describe`/`It` は Pester の実行時 cwd をそのまま使う。今回のパッチはあくまで
`Resolve-SnotraCargoExecutable` 呼び出しの間だけ一時的に cwd を固定し `finally` で戻す）。

**実測で検算した**（Pester 本体が未キャッシュのため、psm1 の関数を直接 dot-source して
patched/unpatched を pwsh で比較。cargo metadata 自体は実行——禁止されているのは
`npm run test:powershell` の実バイナリ起動テストのみ）:

- 未パッチ版: cwd を `%TEMP%\snotra-plan-review-elsewhere` に置き
  `CARGO_TARGET_DIR=relative-target`（相対）で呼ぶと `...\elsewhere\relative-target\debug\snotra.exe`
  を返した（cwd 起点＝バグの再現。research.md §4.2 と一致）
- 計画どおりに `Push-Location -LiteralPath $RepositoryRoot … finally { Pop-Location }` を足した版を
  同条件で呼ぶと `C:\workspace\Snotra\relative-target\debug\snotra.exe`
  （＝ `Join-Path (Join-Path $repositoryRoot 'relative-target') 'debug/snotra.exe'`）を返した。
  **計画の新 It が書く期待値と完全一致**（`MATCH = True` を実測）

### 新 `It` の `-ScriptBlock` は呼び出し元スコープの `$repositoryRoot` を見えるか

`Invoke-SnotraEnvironment`（`SnotraSmoke.psm1:284-319`）は `& $ScriptBlock` で呼ぶ
（dot-source ではなく call 演算子）。それでも既存 It（172-183）が同じ形
（It 内で定義した `$repositoryRoot` を `-ScriptBlock { … -RepositoryRoot $repositoryRoot }` の中で
参照）で成立しており、上の実測（未パッチ版の呼び出し）でも変数未定義エラーは出ず
`$repositoryRoot` が正しく解決された。**計画の新 It も同じ形なので同様に動く**——
これは既存 It のコピーではなく実測で確認した事実である。

**Join-Path の形も実装と一致**を確認: 実装は
`Join-Path ([string]$metadata.target_directory) "$Profile/snotra.exe"`
（`SnotraSmoke.psm1:412`）。計画の期待値
`Join-Path (Join-Path $repositoryRoot 'relative-target') 'debug/snotra.exe'` は、
`$metadata.target_directory` が `Join-Path $repositoryRoot 'relative-target'` に等しくなる
（cwd 固定後の cargo の相対解決）ことを前提にしており、実測はこれを裏付けた。

**判定: 観点 A に要対処なし。** ただし `visual-check-colors.ps1` の 1 件は「回帰が無い」ではなく
「回帰はあるが fail-closed（軽微）」に訂正する（下記「結論」参照）。

## 観点 B — ガバナンス文書の変更の十分性

### 走査（ripgrep 系 vs 生 grep の意味論差を突き合わせ済み）

`既定.*ExePath|既定.*target/release|既定.*snotra\.exe` 系のパターンで**生 `grep -rn`**
（`.gitignore` 非考慮）をリポジトリ全体に当てた。ヒットは:

- `docs/build-commands.md:205`（`smoke:egui` の既定 ExePath = target/release）— **本 issue の
  スコープ外**（`smoke-egui.ps1` は計画・研究とも明示的に対象外と宣言済み。挙動も変わらない
  ので偽にならない）
- `docs/build-commands.md:218`（`bench:startup` の既定パス直書き）— **計画が直す対象そのもの**
- `docs/build-commands.md:267`（`smoke:startup` の「既定 ExePath = debug」の注記）—
  **直さなくてよい**。修正後も `-Profile debug` のまま導出するため、この文言（profile 名としての
  「debug」であってパス文字列ではない）は真であり続ける
- **`.claude/worktrees/agent-*/docs/build-commands.md`**（10 件前後）— 別 worktree 内の**写し**
  であり、このリポジトリの `docs/build-commands.md` そのものではない。今回のブランチが触るのは
  main 側の 1 枚のみで、各 worktree は自分のマージ時に追随する（追跡対象外の走査ノイズ）
- `PERFORMANCE.md:2722` / `CONTRIBUTING.md:92` / `.claude/rules/src-tauri.md:28` — コマンド名の
  参照のみで既定値の記述を持たない（実読して確認。研究 §6 の判定と一致）

`.superpowers/sdd/` 配下にも `bench-startup` / `smoke-startup` の文字列が複数あるが、いずれも
凍結された過去の作業ログ（`task-*-report.md` 等）であり、当時の実行結果の記録であって
「既定値がこうだ」という現在形の主張ではない。**Grep ツール（ripgrep 系）はこの部分木を見落とす
ことを研究段階で実測済み**——今回は生 `grep -rn` で直接突き合わせ、この部分木を含めても
新規の対処要 1 件を追加しない結果を得た。

**2 回目の走査（ブリーフが名指した `ExePath` / `CARGO_TARGET_DIR` の形そのもので再走査）**:
`grep -rn "CARGO_TARGET_DIR\|ExePath" --include="*.md" .`（`.claude/worktrees` 除外）を追加で実行。
新規ヒットは `docs/build-commands.md:92`（`visual-check-colors` の `cargo clean` 対象外の注記。
既定パスの主張ではない）・`docs/adr/ADR-config-dir-env-seam-rejected-alternatives.md:68-69`（凍結
された ADR 本文・却下理由の記録）・`PERFORMANCE.md:2755`（「`e2e.yml` は `-ExePath` を渡す」という
事実記述で既定値の主張ではない）・`.superpowers/sdd/`・`docs/superpowers/plans/`（いずれも凍結済み
作業ログ）のみ。**対処要は増えない。**

**判定: `docs/build-commands.md:218` の 1 段落のみで十分。** 計画の走査（研究 §6・敵対枠の
独立検算）を、走査ツールとパターンを変えて（ripgrep→生 grep、既定値パターン→ブリーフが名指した
生パターン）2 通りで再現し、同じ結論に至った。

### `docs/build-commands.md:51`（`test:powershell` の `CARGO_TARGET_DIR` 追随の記述）は偽にならないか

51 行目: 「統合テストの本体は `cargo metadata` の `target_directory` から導くため
`CARGO_TARGET_DIR` に追随する」。これは `run-pester.ps1` 経由で `Resolve-SnotraCargoExecutable`
を呼ぶ経路の記述である。Phase 1 の cwd 固定は、**相対値の `CARGO_TARGET_DIR` の解決基準を
「呼び出し元の cwd」から「`-RepositoryRoot`」へ変えるだけ**で、「`CARGO_TARGET_DIR` に追随する」
という主張自体は保たれる（絶対値では元々真、相対値では今回の修正で cwd 非依存になり
**むしろより正確に**真になる）。`run-pester.ps1` は `npm run` 経由なら cwd が repo root に
一致するため、実運用上もこの行の観測結果は変わらない。**偽にも不正確にもならない。
計画がこの行を触らない判断は妥当。**

### `governance:check` への依存度

`scripts/governance/checks/G-build-commands.mjs`・`G-ci-table.mjs`・`G-hook-commands.mjs` を実読。
3 検査とも見るのは**構造**だけである:

- `G-build-commands`: `npm run <name>` が `package.json` の `scripts` に実在するか、
  `cargo test -p <crate>` の crate が workspace member か
- `G-ci-table`: 表の検証コマンドが対応 workflow の `run:` に実在するか（逆方向の欠落も検出）
- `G-hook-commands`: hook が使う cargo コマンドがカテゴリ A の記載と一致するか

**いずれも「既定パスの文字列が実装と一致しているか」という意味論は見ない**——散文の
正確性は構造検査の対象外である。計画の Phase 3 チェックリストは「`npm run governance:check` が
緑」を要求しているが、これは**構造的な壊れ（`npm run bench:startup` というバッククォート表記が
崩れていないか等）のガード**として置かれており、**意味論の正しさは Phase 3 の手動走査
（研究 §6・敵対枠の独立検算・および本レビューの再検算）に委ねている**——計画は
governance:check に意味論の担保を頼っていない。**判定: 過度な依存なし。**

## 結論

### 要対処
*(なし)*

### 軽微
- **`visual-check-colors.ps1` は「挙動が変わらない呼び出し元」ではない。** Phase 1 適用後、
  相対値の `CARGO_TARGET_DIR` を設定した環境からこのスクリプトを `$repositoryRoot` 以外の
  cwd で起動すると、:258 の `cargo build`（cwd 基準のまま）と :261 の
  `Resolve-SnotraCargoExecutable`（`$repositoryRoot` 基準に変わる）が別ディレクトリを指し、
  :262 の `Test-Path` が `throw` する。**現状は誰も踏んでいない**（研究 §4.3: セッション env に
  `CARGO_TARGET_DIR` 未設定・CI 3 経路はこのスクリプトを呼ばない）が、**fail-closed であって
  fail-silent ではない**——issue が問題視する「別の本体を黙って測る」形にはならない。
  計画の「不変条件と異常系」表・「受容する残余」節（`plan.md:116-118`）へ、
  「明示された相対 `-ExePath`」の残余と同じ書式でこの交差を追記すると、次にこの表を読む人が
  同じ調査をやり直さずに済む（実装を変える必要は無い）
- Phase 3 の作業項目（`plan.md:194`）は「パス文字列の写しを置かない」とだけ書いており、
  `docs/build-commands.md:51` を「偽にならないことを確認済み」と明示していない
  （判断は研究 §6 の脚注的な言及に留まる）。実装は変わらないが、Phase 3 完了時に
  この 1 行の非該当を PR 本文か作業項目コメントへ明記すると、後続レビューが同じ確認を
  繰り返さずに済む

### 未検証
- `npm run test:powershell` の実 Pester 実行（禁止指示のため）。ただし本レビューは
  `Resolve-SnotraCargoExecutable` を pwsh で直接叩き、patched/unpatched 両方の戻り値を
  計画の期待値と突き合わせて一致を確認した（Pester のアサーション機構を使っていないだけで、
  検証対象のロジック自体は実測済み）
- `SnotraSmoke.Tests.ps1` 内の他 Describe（`実機配管` 等）への波及——Push-Location は
  `Resolve-SnotraCargoExecutable` 内に閉じており他関数を触らないため理論上無関係だが、
  実 Pester run では確認していない
- `.claude/worktrees/agent-*` 配下の `docs/build-commands.md` 写しの扱い——今回のブランチが
  それらを更新する責務を持たないことは構造的に自明（各 worktree は別ブランチ）だが、
  「触らない」がこのリポジトリの規約上正しいことの確認は計画に明示が無い（軽微よりさらに
  弱い注記であり、対処不要と判断する）

## ⚠️ 確信の持てない所見
*(なし。上記 2 観点の範囲では、独立に反証できる所見は見つからなかった)*
