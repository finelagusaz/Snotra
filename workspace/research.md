# 調査: issue #1429 — manual-smoke.ps1 の -Only に複数の番号を渡せない

## issue の要約

- `scripts/manual-smoke.ps1` の使い方は `npm run smoke:manual '--' -Only 2,5,10` と書く。この形では複数の番号を渡せない
- あるべき姿: 使い方に書いた形で複数の項目を指定できる。`,` で分割して `int` に変換し、数でなければ throw する。変換の扱いを Pester で固定する
- 判定: バグ（文書＝使い方コメントと `docs/build-commands.md` の記述に、コードを合わせる）。SPEC.md に `-Only` の記述は無い

## 実測（2026-11-15・pwsh 7.6.10・npm 12.2.2・npm は `C:/nvm4w/nodejs/npm.ps1`）

スクラッチの最小プローブで測った。`[CmdletBinding(PositionalBinding = $false)] param([int[]]$I, [string[]]$S, [string]$T)`、npm script は `pwsh -NoProfile -File p.ps1`（`package.json` の `smoke:manual` と同じ形）。

| 呼び方 | `[int[]]` | `[string[]]` | `[string]` |
|---|---|---|---|
| Bash: `npm run p -- -X 2,5,10` | **count=1 `[2510]`（黙って化ける）** | count=1 `['2,5,10']` | `'2,5,10'` |
| Bash: `pwsh -File p.ps1 -X 2,5,10` | count=1 `[2510]` | count=1 `['2,5,10']` | — |
| PowerShell: `npm run p '--' -X 2,5,10` | **変換エラー（値 "2 5 10"）** | count=1 `['2 5 10']` | `'2 5 10'` |
| PowerShell: `npm run p '--' -X '2,5,10'` | — | count=1 `['2,5,10']` | `'2,5,10'` |
| PowerShell: `npm run p -- -X 2,5`（`--` を引用しない） | — | 位置引数エラー（`'2 5'`）＝ #1296 の防壁 | — |
| PowerShell 直接: `./p.ps1 -X 1,2` | — | count=2 `['1','2']` | **変換エラー**（配列を string へ変換できない） |
| PowerShell 直接: `./p.ps1 -X '1,2'` | — | count=1 `['1,2']` | `'1,2'` |
| PowerShell 直接: `./p.ps1 -X 2` | — | count=1 `['2']` | — |

結論:

- issue の「Bash から打つと `2510` に化けうる（未実測）」は**実測で真**。エラーにならず、`-Only` に一致する項目が無いとして throw する（`2510` は id に無い）か、偶然一致すれば別の項目を走らせる（`-Only 1,3` → `13`、`-Only 1,2` → `12`。いずれも実在 id・3b で発見し主が再測定）
- `[int]` キャストは寛容: `'2.5'`→2・`'0x10'`→16・`'1e1'`→10・`' 2 '`→2・`'-1'`→-1 を黙って通す。`'99999999999'` と全角 `'１'` は例外。`\d` は全角数字に当たる（`'１' -match '^\d+$'` が True）ので、検証は `^[0-9]+$` で行う（2026-11-15 実測）
- PowerShell から npm 経由で `2,5,10` を渡すと、npm.ps1 の `$args` を経て**1 つの文字列 `"2 5 10"`（空白区切り）**として届く。カンマではない
- `[string]` にすると、issue の回避策（直接呼び出しの `-Only 1,2`）が変換エラーで壊れる
- `[string[]]` なら上の全経路で値が届く。各要素を `,` と空白で分割すれば、すべて同じ id 列になる

## 関連するファイル・シンボル

- `scripts/manual-smoke.ps1`
  - 9–10 行: `-Only` の宣言（`[int[]]$Only = @()`）と例のコメント
  - 43 行: 使い方コメント `npm run smoke:manual '--' -Only 2,5,10`
  - 181–187 行: `$allIds` の導出と `-Only` による絞り込み。全件不一致のときだけ throw する（一部不一致は黙って落ちる）
- `package.json` 18 行: `"smoke:manual": "pwsh -NoProfile -File scripts/manual-smoke.ps1"`
- `docs/build-commands.md` 95 行: `npm run smoke:manual '--' -Only 2,5 # 一部だけ再実施`
- `scripts/lib/SnotraSmoke.psm1`: smoke 共通の純関数。`Export-ModuleMember -Function @(...)`（1077 行）で公開する
- `scripts/lib/SnotraSmoke.Tests.ps1`: Pester。`Describe 'npm script の入口は PositionalBinding = $false で位置引数を拒む（#1296）'`（351 行〜）が `manual-smoke.ps1` の param() を複製して束縛を検査する
- `scripts/run-pester.ps1`: `npm run test:powershell` の本体。`scripts/lib` 配下の Tests を走らせる

## 再利用できる既存パターン

- `scripts/visual-input-metrics.ps1` 158–161 行: `[string[]]$Fonts` を `;` で分割する。**同じ不具合の類型（`pwsh -File` が配列を 1 文字列で渡す）を、区切り文字の分割で既に受けている**
- 判定を `scripts/lib/*.psm1` の純関数へ出し、対話スクリプトを走らせずに Pester で測る（`manual-smoke.ps1` 冒頭コメント「判定を純関数として外へ出してあるので…」）
- 例外の検査は ErrorId／メッセージの一部で見る（`Should -Throw -ExpectedMessage '*…*'`、`SnotraWindowColors.Tests.ps1` 206 行）

## 同じパターンの他の経路（npm 入口の配列 param）

`package.json` の `-File scripts/*.ps1` の入口で、配列型の param を持つのは次の 2 つだけ（`grep '\[int\[\]\]\|\[string\[\]\]' scripts/*.ps1`）。

- `manual-smoke.ps1` `-Only`（今回の対象）
- `visual-input-metrics.ps1` `-Fonts`（`[string[]]`・`;` 区切りで対処済み・`docs/build-commands.md` 147 行が `;` 区切りを文書化）。PowerShell から npm 経由で `'A','B'` を渡すと `"A B"` という 1 フォント名に化けうるが、文書は `;` の形しか案内していない。今回は触らない

## 技術的な制約

- `[CmdletBinding(PositionalBinding = $false)]` を保つ（#1296）。`ValueFromRemainingArguments` で `2 5 10` を受ける案は、位置引数の拒否を壊すので採れない
- `manual-smoke.ps1` は対話入力を要し、エージェントも CI も実行できない（stdin リダイレクトで throw）。変換は純関数として `SnotraSmoke.psm1` に置き、Pester で測る
- `scripts/**` はセーフティネットの範囲（`.claude/rules/safety-nets.md` の `paths`）。マージはユーザーと合意してから。PR 本文の冒頭に「マージは合意してから」と書く
- `Set-StrictMode -Version Latest` と `$ErrorActionPreference = 'Stop'` の下で動く

## 敵対的調査（3b）の所見と採否

出典: `workspace/adversarial-1429.txt`（sonnet 1 体）。6 命題とも壊せなかった。

| 命題 | 判定 | 採否 |
|---|---|---|
| 1. Bash-npm で `[int[]]` が `2510` | 壊せず。**追加事実: `1,3`→`[13]`・`1,2`→`[12]`** | 採る。主が再測定で確認（2026-11-15）。12・13 は実在 id なので、**throw されず別の項目が黙って走る**。issue の「どの項目にも当たらない」より悪い |
| 2. PS-npm で `"2 5 10"` の 1 文字列 | 壊せず（npm.ps1・npm 12.2.2）。⚠️ Windows PowerShell 5.1 ホストは未測 | 採る。5.1 は未測のまま残す——実装は空白でも `,` でも分割するので、5.1 がどちらで渡しても受かる |
| 3. `[string[]]` + `,`・空白分割で全経路一致、`[string]` は直接 `-Only 1,2` で壊れる | 壊せず。npm.cmd（cmd 経由）でも `['2,5,10']`。⚠️ 空要素未測 | 採る。空要素 `2,,5` は主が測定: `['2,,5']` で届く → 計画で「空トークンは読み飛ばす」 |
| 4. VFRA は #1296 と両立しない | 壊せず（`foo` が受理された） | 採る |
| 5. 配列 param の入口は 2 つ | 壊せず（下限の主張）。⚠️ grep パターン依存 | 採る。本文は既に「grep で見た範囲」と書いてある。今回の修正は `-Only` 単体で閉じ、この数に依存しない |
| 6. 一部不一致は黙って落ちる | 壊せず（コード読み） | 採る。計画の未確定欄で扱う |

## 未解決の疑問

- 一部だけ一致しない id（例 `-Only 2,99`）を throw にするか。現状は黙って 2 だけ走る。issue は「数に変換できなければ throw」までしか言っていない → 計画で決める
- 区切りに空白を含めるか → 実測で PowerShell-npm 経路が空白区切りで届くため、含める必要がある
