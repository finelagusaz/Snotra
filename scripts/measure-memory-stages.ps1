<#
.SYNOPSIS
  起動 → 表示 → 検索 → hide の各段階で Snotra のメモリを実測する（段階別・前景計測）。

.DESCRIPTION
  `measure-memory.ps1` が「今この瞬間」の 1 点を測るのに対し、本スクリプトは
  ホットキーとクエリを注入しながら**段階列**を測る。フォント設定など「表示・描画で
  初めて実体化する」コストは 1 点計測では見えないため（2026-07-25 実測: user_font は
  アイドル 20.6 MiB → 検索後 40.9 MiB と段階とともに差が広がる）。

  **サンプリングは `measure-memory.ps1` を `&` で呼んで再利用する（重複実装しない）。**
  `&` は同一プロセスで .ps1 を実行するのでコンソールが増えない。ここが本質で、
  `pwsh -File measure-memory.ps1` と書くと**新しいコンソールがフォーカスを奪い**、
  以後の SendKeys が Snotra ではなく別窓へ流れて標本が無言で無効になる（実際に踏んだ）。

  前提:
  - release ビルド済み（既定 `target/release/snotra.exe`）
  - `[hotkey]` が既定の `Ctrl+K` 系であること（段階 B の注入キーは下でハードコード）
  - `[general] auto_hide_on_focus_lost = false` は**推奨であって必須ではない**。上記の
    同一プロセス実行でフォーカスを保つため、`true` のままでも段階列は取れる（実測で
    B/C/D とも成立）。ただし外的要因でフォーカスが逸れると窓が hide して PrivWS が
    trim 後の値へ落ち、段階列が無言で壊れる。長時間の計測や無人実行では false にする
  - **実行中の snotra を kill する**（`smoke-egui.ps1` と同じくローカル実行時は注意）

  読み方: PrivWS は hide 経路の `EmptyWorkingSet` で trim された後の値になりうるため、
  確保需要の軸は **PrivComm** を見る（詳細は PERFORMANCE.md「egui 期のメモリ実測」）。

.PARAMETER Label
  出力に付ける見出し（A/B 比較時の変種名。例 "font=HackGen" / "font=解決不能"）。

.PARAMETER Query
  段階 C で注入する検索クエリ。開発機の索引に一致する文字列を指定する。

.PARAMETER ExePath
  計測対象の実行ファイル。

.EXAMPLE
  npm run measure:memory:stages
  pwsh -NoProfile -File scripts/measure-memory-stages.ps1 -Label "font=解決不能" -Query notepad
#>
param(
    [string]$Label = '',
    [string]$Query = 'code',
    [string]$ExePath = "$PSScriptRoot/../target/release/snotra.exe"
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms

$measure = Join-Path $PSScriptRoot 'measure-memory.ps1'
if (-not (Test-Path $measure)) { throw "測定器が見つかりません: $measure" }
if (-not (Test-Path $ExePath)) { throw "実行ファイルが見つかりません: $ExePath（release ビルド済みか確認）" }

# `&` = 同一プロセス実行（新コンソールを作らない）。フォーカスを保つための必須条件。
function Invoke-Stage {
    param([string]$Stage)
    # measure-memory.ps1 は**失敗時のみ** `exit`（1: プロセス不在 / 2: perf 0 件）する。
    # 成功時は最後まで流れて終わるため $LASTEXITCODE は更新されない。直前の値が残ると
    # 成功を失敗と誤判定するので、呼ぶ前に 0 で潰してから判定する（誤爆を実測で確認済み）。
    $global:LASTEXITCODE = 0
    & $measure -Label "$Label / $Stage"
    if ($LASTEXITCODE -ne 0) {
        throw "[$Stage] サンプリング失敗 (exit $LASTEXITCODE)。0 MiB と読んではならない。"
    }
}

function Send-Keys {
    param([string]$Keys, [int]$WaitSeconds)
    [System.Windows.Forms.SendKeys]::SendWait($Keys)
    Start-Sleep -Seconds $WaitSeconds
}

Write-Host ''
Write-Host "########## 段階別メモリ実測: $Label ##########" -ForegroundColor Cyan

Stop-Process -Name snotra -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2

Start-Process -FilePath $ExePath
Start-Sleep -Seconds 14      # 索引ロード + 背景再スキャンの収束待ち
Invoke-Stage 'A. アイドル（一度も表示せず）'

Send-Keys '^k' 4             # ホットキーで表示（config の [hotkey] に合わせて要調整）
Invoke-Stage 'B. 表示（空クエリ）'

Send-Keys $Query 6           # 検索 + アイコン抽出・テクスチャ化の収束待ち
Invoke-Stage "C. 検索実行後（query=$Query）"

Send-Keys '{ESC}' 5          # hide → EmptyWorkingSet trim
Invoke-Stage 'D. hide 後（trim 済み）'

Stop-Process -Name snotra -Force -ErrorAction SilentlyContinue
Write-Host "########## 終了: $Label ##########" -ForegroundColor Cyan
Write-Host '注: PrivWS は trim 後の値になりうる。確保需要は PrivComm を見ること。' -ForegroundColor DarkGray
