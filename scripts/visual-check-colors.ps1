#Requires -Version 7
<#
.SYNOPSIS
  非既定の `[visual]` 色で本体を起動し、色が実際に画面へ届いているかを目視確認する。

.DESCRIPTION
  config の既定色 `#282828` は `snotra-egui-runtime` の `CLEAR_COLOR` と一致するため、
  **既定のまま起動しても「色が届いていない」欠陥は観測できない**
  （`docs/development-principles.md`「config の値は到達性の検出器を持たない」）。

  本スクリプトは実 config を退避してから非既定色を書き込み、終了時に必ず戻す。
  異常終了でバックアップが残った場合は `-Restore` で回収する。

.PARAMETER Color
  適用する背景色。既定は `#4A2B5C`（既定色とも `CLEAR_COLOR` とも明確に違う紫）。
  `-Color '#FFF'` を渡すと 3 桁 hex の受理を確認できる（#680 の 1・パーサ統合の回帰）。

.PARAMETER Restore
  起動せず、退避した config を戻すだけ。異常終了後の回収に使う。

.EXAMPLE
  npm run check:colors
.EXAMPLE
  pwsh -NoProfile -File scripts/visual-check-colors.ps1 -Color '#FFF'
.EXAMPLE
  pwsh -NoProfile -File scripts/visual-check-colors.ps1 -Restore
#>
[CmdletBinding()]
param(
    [string]$Color = '#4A2B5C',
    [switch]$Restore
)

$ErrorActionPreference = 'Stop'
$configPath = Join-Path $env:APPDATA 'Snotra\config.toml'
$backupPath = "$configPath.visualcheck-bak"

function Restore-SnotraConfig {
    if (Test-Path $backupPath) {
        Move-Item -Path $backupPath -Destination $configPath -Force
        Write-Host "config を復元しました: $configPath"
    } else {
        Write-Host "復元するバックアップがありません（既に復元済みか、退避前に終了しています）"
    }
}

if ($Restore) {
    Restore-SnotraConfig
    exit 0
}

if (-not (Test-Path $configPath)) {
    throw "config.toml が見つかりません: $configPath`n  本体を一度起動して生成してから再実行してください。"
}
# 二重退避の防止: 前回のバックアップを上書きすると、ユーザーの実 config が検証用の色で固定される
if (Test-Path $backupPath) {
    throw "前回のバックアップが残っています: $backupPath`n  -Restore で戻してから再実行してください。"
}

Copy-Item -Path $configPath -Destination $backupPath
$original = Get-Content -Path $configPath -Raw

if ($original -match '(?m)^\s*background_color\s*=') {
    $patched = $original -replace '(?m)^\s*background_color\s*=.*$', "background_color = `"$Color`""
} elseif ($original -match '(?m)^\[visual\]') {
    $patched = $original -replace '(?m)^\[visual\]', "[visual]`nbackground_color = `"$Color`""
} else {
    $patched = $original + "`n[visual]`nbackground_color = `"$Color`"`n"
}
Set-Content -Path $configPath -Value $patched -NoNewline

Write-Host ''
Write-Host "背景色を $Color にしました（退避元: $backupPath）"
Write-Host ''
Write-Host '目視する 3 点:'
Write-Host "  1. メインウィンドウの定常の背景が $Color である"
Write-Host '     → 暗いままなら clear color が届いていない（runtime の CLEAR_COLOR が出ている）'
Write-Host '  2. ホットキーで出した瞬間も同色である'
Write-Host '     → 別の色が一瞬見えてから変わるなら、下地（ネイティブ背景ブラシ）がずれている'
Write-Host "  3. 結果リスト窓（何か入力して出す）の背景も同色である"
Write-Host '     → main だけ変わって results が暗いなら、results の経路が届いていない'
Write-Host ''
Write-Host '本体を終了すると config を自動で復元します（異常終了時は -Restore）。'
Write-Host ''

try {
    cargo run -p snotra
} finally {
    Restore-SnotraConfig
}
