# 調査: issue #1179 — `bench-startup.ps1` の `-ExePath` 既定値がメイン作業コピーへ固定

ブランチ: `fix/bench-startup-exepath-default` / 調査日: 2026-08-25

## 1. issue の要約

`scripts/bench-startup.ps1:28` の `-ExePath` 既定値が `"C:/workspace/Snotra/target/release/snotra.exe"`
というメイン作業コピーの絶対パスである。worktree（`.claude/worktrees/agent-*`）から既定のまま回すと、
その worktree ではなく**メイン作業コピーの本体を測る**。

**失敗が緑と同じ見た目をする。** 本体は実在するのでスクリプトは完走し、payload も出て契約検査も通る。
測った対象が違うことだけが観測されない。`/implement` Step 3 は検証を worktree のエージェントへ委ねる設計
ゆえ、この経路は日常的に踏まれる。

やること（issue の 3 項目）:

1. 既定値を呼び出し元のリポジトリから導く（先例は `cargo metadata` の `target_directory`）
2. 導けなかったときの挙動を決める（現行の「本体が無ければ `throw`」を保てばよい）
3. 直したことを worktree から実測する（測った本体のパスを出力へ載せる）

## 2. 同一パターンの走査

### 2.1 走査の射程（先に宣言する）

下表は **Grep ツール（ripgrep 系）で `scripts/` 配下の `$ExePath` 既定値を列挙した結果**である。
**この道具は `.gitignore` された部分木を見ない**——実測: `.superpowers/` を直接指しても
`No matches found` を返すが、生 `grep -rn` は同じ部分木から 2 件を出す（§2.3）。
ゆえに下表は「生きたスクリプト層で見つかった分」であって、リポジトリ全体の網羅ではない。

### 2.2 生きたスクリプト層（`scripts/`）

| ファイル | 既定値 | 分類 |
|---|---|---|
| `scripts/bench-startup.ps1:28` | `"C:/workspace/Snotra/target/release/snotra.exe"` | **本 issue の欠陥そのもの**（絶対パス・別作業コピーを測る） |
| `scripts/smoke-startup.ps1:9` | `"C:\workspace\Snotra\target\debug\snotra.exe"` | **同一の欠陥クラス**（issue に名指しは無い） |
| `scripts/smoke-egui.ps1:2` | `"target/release/snotra.exe"` | 弱い形（cwd 相対。cwd が当該コピーなら正しい） |
| `scripts/manual-smoke.ps1:3` | `"target/debug/snotra.exe"` | 弱い形（同上） |
| `scripts/measure-memory-stages.ps1:44` | `"$PSScriptRoot/../target/release/snotra.exe"` | worktree では既に正しい（`CARGO_TARGET_DIR` には追随しない） |
| `scripts/run-pester.ps1` | 既定なし → `Resolve-SnotraCargoExecutable` | **先例（issue が名指す `npm run test:powershell`）** |
| `scripts/visual-input-metrics.ps1:38` | `''` → `Resolve-SnotraCargoExecutable` | 先例 |
| `scripts/visual-check-colors.ps1:261` | 引数なし → `Resolve-SnotraCargoExecutable` | 先例 |

### 2.3 追跡外・凍結された層（範囲外・理由つき）

| 位置 | 内容 | 範囲外の理由 |
|---|---|---|
| `.superpowers/sdd/2026-08-04-main-show-height-derivation/measure-fixA-position.ps1:19` | `$repo = "C:/workspace/Snotra"` | `.gitignore:25` で `.superpowers/` ごと追跡外。2026-08-04 の一回限りの調査足場で、`package.json` からも CI からも呼ばれない |
| 同ディレクトリ `verify-fixA-invariant.ps1:12` | 同上 | 同上 |
| `docs/superpowers/plans/2026-08-09-*`, `2026-08-10-*` | 一時計装の手順に現れる絶対パス | 凍結された計画文書（実行される層ではない） |
| `.claude/hooks/pre-bash.test.mjs:603` | `"node C:/workspace/Snotra/x.mjs"` | hook 判定の fixture であり本体パスではない |

### 2.4 範囲の決定

- **ユーザー判断（2026-08-25・その 1）**: `bench-startup.ps1` + `smoke-startup.ps1` の 2 件を直す。
  §2.2 の弱い形 3 件は本 issue の欠陥クラス（別作業コピーを黙って測る）に当たらないため範囲外
- **ユーザー判断（2026-08-25・その 2）**: §4.2 の `Resolve-SnotraCargoExecutable` の cwd 残余も
  この PR で塗る

## 3. 再利用できる既存パターン

### `Resolve-SnotraCargoExecutable`（`scripts/lib/SnotraSmoke.psm1:385-413`）

```powershell
Resolve-SnotraCargoExecutable -RepositoryRoot <root> [-Profile debug|release]
```

- `<root>/Cargo.toml` の不在で `throw`、`cargo metadata` の失敗・JSON 不正・`target_directory` 欠落でも `throw`
- 返すのは `Join-Path $metadata.target_directory "$Profile/snotra.exe"` の**文字列のみ**（存在検査は呼び出し側）
- `Export-ModuleMember` 済み（`SnotraSmoke.psm1:938`）
- Pester あり: `scripts/lib/SnotraSmoke.Tests.ps1:172-184`（`CARGO_TARGET_DIR` を差し替えて debug 本体の導出を検査）

### 呼び出し側の 2 つの形

- `run-pester.ps1:27-38`（issue が名指す先例）: 明示された相対パスを `$repoRoot` へ join し、明示が無ければ導出
- `visual-input-metrics.ps1:55`: `if ($ExePath) { $ExePath } else { Resolve-... }` の 1 行

いずれも `$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path` で自分のコピーの根を取る。

## 4. 実測（一次証拠）

### 4.1 入れ子の worktree でも cargo は自分の target を返す

worktree はメイン作業ツリーの**内側**（`C:/workspace/Snotra/.claude/worktrees/agent-*`）に在るため、
cargo が親 workspace を掴む可能性を疑って測った。

```
$ cargo metadata --no-deps --format-version 1 --manifest-path .claude/worktrees/agent-a3d6dba6825a6087d/Cargo.toml
  workspace_root    = C:\workspace\Snotra\.claude\worktrees\agent-a3d6dba6825a6087d
  target_directory  = C:\workspace\Snotra\.claude\worktrees\agent-a3d6dba6825a6087d\target
$ cargo metadata --no-deps --format-version 1 --manifest-path Cargo.toml
  target_directory  = C:\workspace\Snotra\target
```

worktree の manifest は `[workspace]` を持つ完全な写しなので自分自身が workspace root になる。**機構は効く。**

### 4.2 ただし相対値の `CARGO_TARGET_DIR` は cwd を起点に解決される（敵対枠の所見・独立検算済み）

```
$ CARGO_TARGET_DIR=custom-target cargo metadata ... --manifest-path <worktree>/Cargo.toml   # cwd = メイン作業コピー
  target_directory = C:\workspace\Snotra\custom-target                       ← worktree 配下ではない
$ cd <worktree> && CARGO_TARGET_DIR=custom-target cargo metadata ... --manifest-path Cargo.toml
  target_directory = C:\workspace\Snotra\.claude\worktrees\agent-a3d.../custom-target
```

`Resolve-SnotraCargoExecutable` は `-RepositoryRoot` を受け取りながら **cargo 子プロセスの cwd を
そこへ固定していない**（`SnotraSmoke.psm1:399` は `& cargo metadata` を呼び出し元の cwd のまま起動する）。
ゆえに「worktree のスクリプトを絶対パスで直接叩くが shell の cwd はメイン作業コピーのまま」かつ
「相対値の `CARGO_TARGET_DIR` が設定されている」の**交差で、今回の修正が issue と同じ欠陥を再導入する**。

`docs/build-commands.md:51` は `test:powershell` について「`CARGO_TARGET_DIR` に追随する」と**既に約束
している**ので、これは珍しい辺縁ではなく**破れた約束**である。ユーザー判断によりこの PR で塞ぐ。

### 4.3 測定環境の裏取り

- リポジトリ直下・`.cargo/` に `config.toml` は無い（`build.target-dir` の上書きは無い）
- 本セッションの env に `CARGO_TARGET_DIR` は設定されていない（空）
- `git worktree list` は main + `agent-*` 11 本
- **`param()` の既定値から module の関数は呼べない**（scratchpad で最小再現を実行し
  `The term '...' is not recognized` / exit 1 を実測。敵対枠も pwsh 7.6.5 で独立に再現）

### 4.4 実測に使える観測量（cold ビルドを払わずに済む）

| コピー | release | debug |
|---|---|---|
| メイン `C:/workspace/Snotra` | 10,890,752 B（08-16） | 53,621,248 B（08-25） |
| `.claude/worktrees/agent-a5b0f5810c7344357` | 10,879,488 B（08-25） | 53,721,088 B（08-25） |

**バイトサイズが違う**ので、「どちらのコピーの本体を測ったか」は出力のパス表示に頼らず
ファイル同一性でも裏が取れる。

## 5. CI からの呼び出し（変更の射程）

| 呼び出し | 引数 |
|---|---|
| `.github/workflows/e2e.yml:137` | `scripts/bench-startup.ps1 -ExePath target/release/snotra.exe -UseVerificationProfile -Iterations 7` |
| `.github/workflows/e2e.yml:124` | `scripts/smoke-startup.ps1 -ExePath target/release/snotra.exe` |
| `.github/workflows/release.yml:83` | `scripts/smoke-startup.ps1 -ExePath target/release/snotra.exe` |

**3 経路とも `-ExePath` を明示する**ため、既定値の変更は CI の挙動を変えない。cwd は repo root なので、
明示された相対パスを `$repoRoot` へ join する形を採っても採らなくても解決先は同じである。

加えて `e2e.yml:23,25` は両スクリプトを paths トリガに載せているので、**この PR の CI run が変更後の
スクリプトを実際に実行する**（ただし明示 `-ExePath` 経路のみで、既定値の枝は CI では走らない）。

`package.json` の `smoke:startup` / `bench:startup` は引数を渡さないので、**開発者がローカルで
`npm run` を打つ経路が、既定値の枝を通る唯一の生きた経路である。**

## 6. 更新が要る文書

| 位置 | 現在の記述 | 要否 |
|---|---|---|
| `docs/build-commands.md:218` | 「`npm run bench:startup` は **release 本体を測る**（既定 `target/release/snotra.exe`）」 | **要**（既定の導出元が変わる。`:51` の `test:powershell` の書き方が語彙の先例） |
| `docs/build-commands.md:267` | 「`npm run smoke:startup`（既定 ExePath = debug）ではなく…」 | 不要（`-Profile debug` のままゆえ修正後も真。敵対枠も独立に確認） |
| `PERFORMANCE.md:2722` / `CONTRIBUTING.md:92` / `.claude/rules/src-tauri.md:28` | コマンド名の参照のみ | 不要（既定パスの導出元に言及していない。敵対枠が独立に確認） |

`docs/build-commands.md` は `*.md` ゆえ PostToolUse hook の検査対象外で、`governance:check` が別途要る
（`AGENTS.md` 条件別チェックのガバナンス文書行）。

## 7. 技術的制約

- **`param()` の既定値の中で `Resolve-SnotraCargoExecutable` を呼べない**（§4.3 で実測）。
  既定は `''` にし、解決は import 後（現行の `Test-Path`/`throw` が在る位置）へ置く。これは
  `visual-input-metrics.ps1` / `run-pester.ps1` が実際に採っている形である
- **`scripts/*.ps1` には PostToolUse hook の検査が割り当てられていない**（`selectChecks` の対象外）。
  編集時の沈黙は「何も走らなかった」であり、合格ではない
- 判定ロジックは psm1 側（`Resolve-SnotraCargoExecutable`）に在り Pester で測られている。`.ps1` へ
  入るのは配線だけなので、`.ps1` に新しい判定ロジックは増えない
- `smoke-startup.ps1` は `param()` に `[CmdletBinding()]` を持たない素の形。`$PSBoundParameters` は
  使えるが、`if ($ExePath)` の形なら不要
- 両スクリプトとも `Set-StrictMode -Version Latest` 下にある
- `-Profile` パラメータ名と自動変数 `$PROFILE` は衝突しない（敵対枠が実測。関数スコープが分離。
  現に `smoke-startup.ps1:44` は script スコープで `$profile` を再代入したまま動いている）

## 8. 敵対的調査（Step 3b）の所見と採否

サブエージェント 1 体（general-purpose / sonnet）。全文は `workspace/adversarial-1179.txt`。

### 壊せた項目 → 採用

| # | 所見 | 私の独立検算 | 採否 |
|---|---|---|---|
| 争点 2 | 相対 `CARGO_TARGET_DIR` 下で `target_directory` が cwd 起点になる | **再現した**（§4.2 に逐語で記録） | **採用**。ユーザー判断でこの PR に含める |
| 争点 1 | 「リポジトリ全体で走査した」は偽（Grep ツールが `.gitignore` を尊重） | **再現した**（`.superpowers/` 直指しで `No matches`／生 grep は 2 件） | **採用**。§2.1 で射程を先に宣言し、§2.3 で見つかった分を理由つきで範囲外にした |

**機序の裁定**: 争点 2 に添えられた機序（cargo の相対パス解決が cwd 起点）は、私が対照実験
（cwd=main / cwd=worktree）で独立に確かめた。所見・機序とも採る。

### 壊せなかった項目

| # | 命題 | どこまで確かめたか |
|---|---|---|
| 争点 2 コア | `CARGO_TARGET_DIR` 未設定下では worktree 自身の target を返す | 別 4 本の worktree で再現・崩れず |
| 争点 3 | CI 3 経路とも `-ExePath` 明示 | `e2e.yml` / `release.yml` / `.claude/skills/` / `.claude/agents/` を確認・崩れず |
| 争点 4 | `param()` 既定値内で module 関数を呼べない | 最小再現を pwsh 7.6.5 で実行・崩れず（私も独立に実測） |
| 争点 5 | 更新要文書は `build-commands.md:218` のみ | `CONTRIBUTING.md` / `PERFORMANCE.md` / `.claude/rules/src-tauri.md` を確認・崩れず |

### ⚠️ 確信の持てない所見 → 裁定

| 所見 | 裁定 |
|---|---|
| `$Profile` と自動変数 `$PROFILE` の衝突 | **却下**（敵対枠自身が実測で否定。§7 に記録） |
| `CARGO_TARGET_DIR` 残余は PR スコープ外かもしれない | **ユーザーへ問うて解決**——この PR で塗る（§2.4） |
| `.superpowers/sdd/` のスクリプトが生きているか死骸か未判定 | **死骸と裁定**。`.gitignore` 追跡外・2026-08-04 の一回限り・`package.json` と CI から未参照（§2.3） |

### 私自身の測定事故（記録）

敵対枠の争点 1 を検算する途中、Bash ツールの **cwd がコール間で持続する**ことを失念し、
対照実験で `cd <worktree>` した後の `find .superpowers` / `grep` を worktree 側で走らせた。
`.superpowers` はそこに存在しないため、**エラーではなく 0 件**が返り、私は一度「敵対枠の証拠は捏造」と
誤判定した。`pwd` を打って気づき、主リポジトリで測り直して所見が正しいことを確認した。
（`MEMORY.md` の `deleted-cwd-silently-zeroes-measurements` と同型。0 件を見たら `pwd` を先に打つ。）
