# research — #1403 SnotraSmoke.psm1 を役割ごとに分けるか

## issue の要約

`scripts/lib/SnotraSmoke.psm1`（1142 行・関数 29 本）と `SnotraSmoke.Tests.ps1`（965 行）に 5 役割が同居している。役割ごとに分けて変更の影響範囲を読めるようにしたい。分けない結論もありうる。

## 関連ファイル・シンボル（2026-11-07・main 996db798 で実測）

### 役割と関数（定義行）

| 役割 | 関数 | 行 |
|---|---|---|
| Win32 相互運用 | `Initialize-SnotraNativeInterop`（非公開・`SnotraSmokeInterop.Native` を Add-Type）・`ConvertFrom-SnotraWtsInfoEx`・`Get-SnotraSessionLockState`・`Assert-SnotraSessionUnlocked`・`Initialize-SnotraDpiAwareness`（非公開）・`Get-SnotraWindowDpi` | 8–216 |
| プロファイル・env・起動 | `New-SnotraVerificationProfile`・`Invoke-SnotraEnvironment`・`Start-SnotraProcess` | 217–416 |
| 本体の解決・プロセス | `Resolve-SnotraCargoExecutable`・`Resolve-SnotraCargoTargetDirectory`・`Get-SnotraRecordedExePath`・`Stop-SnotraProcessAndWait`・`Resolve-SnotraExistingProcess` | 417–608 |
| trace | `Read-SnotraTraceEvents`・`ConvertFrom-SnotraTraceLine`（非公開）・`Read-SnotraTraceSnapshot`・`Wait-SnotraTraceCondition`・`Wait-SnotraTraceEvent`・`Get-SnotraTraceEventCount` | 609–852 |
| 注入・ウィンドウ・捕捉 | `Send-SnotraKey`・`Send-SnotraUnicodeText`・`Send-SnotraPostedChar`・`Send-SnotraKeyChord`・`Wait-SnotraWindow`・`Get-SnotraForegroundWindow`・`Get-SnotraForegroundWindowLabel`（非公開）・`Set-SnotraForegroundWindow`・`Get-SnotraWindowCapture` | 853–1115 |

ファイル内は既に役割順に並んでいる。

### 役割をまたぐ実呼び出し（コメント内の言及を除く）

- 起動 → env: `Start-SnotraProcess` → `Invoke-SnotraEnvironment`（同じ役割）
- 注入・ウィンドウ・捕捉 → Win32: `Send-*` / `Wait-SnotraWindow` / `*ForegroundWindow` → `Initialize-SnotraNativeInterop`、`Get-SnotraWindowCapture` → `Initialize-SnotraDpiAwareness`・`Assert-SnotraSessionUnlocked`
- trace 群は他の役割を 1 つも呼ばない（Native 型も `$script:` 状態も使わない）。完全な葉である

### 消費者ごとの使用関数

| 消費者 | 使う役割 |
|---|---|
| `smoke-egui.ps1` | 起動・プロセス・trace・注入。**加えて `[SnotraSmokeInterop.Native]` 型を直接使う**（757–758 行。モジュール内の関数呼び出しで型が Add-Type 済みであることに暗黙に依存） |
| `smoke-startup.ps1` / `bench-startup.ps1` | 起動・プロセス・trace |
| `measure-memory-stages.ps1` | env・起動・trace・注入 |
| `visual-check-colors.ps1` / `visual-input-metrics.ps1` | 起動・プロセス・注入・捕捉（後者は Win32 も） |
| `manual-smoke.ps1` | プロセス（`Resolve-SnotraCargoExecutable`・`Get-SnotraRecordedExePath`）・trace |
| `occupy-hotkey.ps1` | 起動（`New-SnotraVerificationProfile`）のみ |
| `run-pester.ps1` | プロセス（`Resolve-SnotraCargoExecutable`）のみ |
| `SnotraTraceInvariants.psm1` | **import しない**。doc で入力の形として `Read-SnotraTraceEvents` を名指すだけ |
| `SnotraTraceInvariants.Tests.ps1` | SnotraSmoke を import し、`Read-SnotraTraceSnapshot` の Describe（474 行〜）を持つ |

### 文書・コメントの参照

`git grep SnotraSmoke` で、scripts 以外に `docs/build-commands.md`（161・282・294–295 行）・`docs/adr/` 4 本（凍結・触らない）・`docs/superpowers/` 3 本（履歴）・`snotra-core/src/hotkey.rs:35`・`snotra-egui-runtime/src/env.rs:12`・`src-tauri/src/egui_shell/mod.rs:241`・`.claude/skills/implement/SKILL.md:101`。

## 技術的な制約

- `SnotraSmoke.Tests.ps1` は `Mock -ModuleName SnotraSmoke` を 24 か所で使う（`Start-Process`・`Stop-Process`・`Get-Process`・`Write-Warning`・`Get-SnotraSessionLockState`）。Mock は「呼ぶ側の関数が居るモジュール」を名指す。関数を別モジュールへ移すと、名指しを移し先へ揃えないと Mock が効かず、実プロセスを止める・起動する実害になりうる
- `docs/build-commands.md:161–163` は「エージェントが目視項目を実施するときは `SnotraSmoke.psm1` の関数だけで組む。この経路だけが画面ロックの検出に守られる」と書く。分けるならこの文の「モジュール」の範囲を言い直す必要がある
- import の実行時コスト: Add-Type は `Initialize-SnotraNativeInterop` が初回呼び出し時に行う遅延型。全体を import しても、trace だけを使う側に Win32 の読み込みは起きない
- PowerShell の nested module / dot-source なら、モジュール名 `SnotraSmoke` と import 経路を保ったまま、ファイルだけを分けられる（Mock の `-ModuleName` は不変）

## 過去の変更の分布（2026-08-01 以降・機能変更 16 コミット）

- 1 役割だけに触れたもの: 10（例: #1289 は注入だけ、#1105 は DPI だけ、#879 は trace だけ）
- 2〜3 役割にまたがるもの: 6（#1306・#904・#889・#939・#871・#890）。#904 は起動と trace、#1306 は起動・プロセス・ウィンドウ
- 変更は役割の単位でおおむね閉じている。ただし、ファイルが 1 本であることが変更を難しくした記録は見当たらない

## issue の前提の裏取り

- 「trace を読むだけの消費者も全体を import している」は**おおむね偽**。`SnotraTraceInvariants.psm1` は import しない。`manual-smoke.ps1` は trace 以外にプロセスの役割も使う。trace だけを使うのは `SnotraTraceInvariants.Tests.ps1` の `Read-SnotraTraceSnapshot` の Describe だけである（これは SnotraSmoke 側のテストが別ファイルに居る、という置き場所のずれ）
- 「5 役割が同居している」は真。ただしファイル内で既に役割順に連続している

## 敵対的調査（3b）の反映

所見の全文は `workspace/adversarial-1403.txt` にある。

- 採った: 外部の指し先の漏れ（`smoke-startup.ps1:56`・`visual-check-colors.ps1:100`・resolver の doc を指す 6 行・`smoke-egui.ps1:723`）。主が grep で実在を確かめた → plan の表へ
- 採った: governance:check は `.psm1` のコメントを読む（`scripts/governance/lib.mjs` の `COMMENT_FAMILY`）。コードスパンの折返しで赤になる → plan の Phase 2 で役割ごとに回す
- 採った: Comment を除いたトークン列の比較は、`#Requires` の変更を見逃し、コメント行の削除を誤って赤にする → NewLine を畳み、`#Requires` を別に比べる形へ（plan Phase 1）
- 採った: 件数の訂正（`docs/superpowers/` は 3 本。`-ModuleName SnotraSmoke` を含む行は `Should -Invoke` を合わせて 36）
- 壊せなかった: trace 群が葉であること。`smoke-egui.ps1` の Native 型への暗黙の依存（使う前に必ず `Initialize-SnotraNativeInterop` を通るので、今は実害なし）。research.md の行範囲

## 未解決の疑問

- [解決] 分けない。ユーザー判断（2026-11-07）: 「Smokeの文言だけ refine-ja 使ってシンプルにしようか。テストに必要な操作などに絞りたい」
