#Requires -Version 7
<#
.SYNOPSIS
  非既定の `[visual]` 色で本体を起動し、色が実際に画面へ届いているかを確認する。

.DESCRIPTION
  config の既定色 `#282828` は `snotra-egui-runtime` の `CLEAR_COLOR` と一致するため、
  **既定のまま起動しても「色が届いていない」欠陥は観測できない**
  （`docs/development-principles.md`「config の値は到達性の検出器を持たない」）。

  本スクリプトは `SNOTRA_CONFIG_DIR`（#803）で**使い捨てのプロファイル**を指し、そこへ検証用の
  config を 1 枚書いて起動する。**ユーザーの実 config は読みも書きもしない**——退避も復元も
  存在しないので、異常終了しても実 config が検証色のまま残る経路が構造的に無い。

  既定は自動判定である。main 窓を実際にキャプチャして背景ピクセルを読み、期待色と一致するかを
  **exit code で** 返す。`-Interactive` では起動するだけで判定せず、目視項目を読み上げる。

  **自動判定するのは main と results の定常背景である。** 次の 2 点はタイミング依存なので目視に残す:
  - **show の一瞬のフラッシュ**: softbuffer の present 前で 1 フレーム未満。連写しても
    捉えられる保証がない
  - **results のリサイズ時のちらつき**: 入力と描画のタイミングに依存し、単一キャプチャでは
    不在を証明できない

  **trace は判定に使わない。** 「`set_clear_color` を呼んだ」というログは、その色が画面へ
  出たことを意味しない（`src-tauri/CLAUDE.md`「trace の presence 検査は状態の検査ではない」・
  #671 PR A′ で `egui_results:hide` が出るのに窓が残る回帰を smoke が緑のまま通した）。
  ここで判定の根拠にするのは**描かれた結果のピクセル**だけである。

.PARAMETER Color
  適用する背景色。既定は `#4A2B5C`（既定色とも `CLEAR_COLOR` とも明確に違う紫）。
  `-Color '#FFF'` を渡すと 3 桁 hex の受理を確認できる（#680 の 1・パーサ統合の回帰）。

.PARAMETER Interactive
  自動判定せず、起動して目視項目を読み上げる。人が終了するまで待つ。

.PARAMETER KeepShot
  判定が緑でもスクリーンショットを残す（既定では赤のときだけ残す）。

.EXAMPLE
  npm run check:colors
.EXAMPLE
  npm run check:colors -- -Color '#FFF'
.EXAMPLE
  npm run check:colors -- -Interactive
#>
[CmdletBinding()]
param(
    [string]$Color = '#4A2B5C',
    [switch]$Interactive,
    [switch]$KeepShot
)

$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'lib/SnotraSmoke.psm1') -Force
$shotDir = Join-Path $PSScriptRoot '..\target\visual-check'
# 検証用プロファイル。**`$env:TEMP` ではなく `target/` の下に置く**——スクリーンショットと
# 同じ場所に集まり、`cargo clean` が config.toml も *.bin も掃く（新しい後始末機構を足さない）。
# ただし `CARGO_TARGET_DIR` を設定している環境では cargo の掃除対象から外れる（受容する残余）。
$profileDir = Join-Path $shotDir 'profile'
$stderrLog = Join-Path $shotDir 'snotra-stderr.log'

# --- 期待色を RGB へ（`#RGB` / `#RRGGBB` の両方を受ける。判定側でも 3 桁を展開する必要がある） ---
$hex = $Color.TrimStart('#')
if ($hex.Length -eq 3) { $hex = "$($hex[0])$($hex[0])$($hex[1])$($hex[1])$($hex[2])$($hex[2])" }
if ($hex.Length -ne 6) { throw "-Color は #RGB か #RRGGBB で指定してください: $Color" }
$expected = [pscustomobject]@{
    R = [Convert]::ToInt32($hex.Substring(0, 2), 16)
    G = [Convert]::ToInt32($hex.Substring(2, 2), 16)
    B = [Convert]::ToInt32($hex.Substring(4, 2), 16)
}

# **単一インスタンス衝突は沈黙する**ため、この検査は kill でなく Reject を選ぶ。
Resolve-SnotraExistingProcess -Policy Reject

# --- 検証用プロファイルを作り直す ---
Remove-Item -Path $stderrLog -Force -ErrorAction SilentlyContinue

# 最小の有効 TOML。config.toml を置く共通の理由は `New-SnotraVerificationProfile` の上の
# コメントが正本で、visual 固有の帰結は「first-run で既定値起動すると背景が `CLEAR_COLOR` に
# なり、『色が届いていない』と誤読される」ことである。ここで値を書く理由は `[visual]` の色
# （これが検証対象そのものである）、下で理由を説明する `auto_hide_on_focus_lost` と
# `[[paths.scan]]`、そして `-Interactive` から導く `show_on_startup` である。
# 共通セクションの骨格は共有モジュールが持つ（#843）。visual 固有の general/visual と scan は
# 呼び出し側から渡し、3 本の seed が同型でないことを保つ。
# **`auto_hide_on_focus_lost = false` は自動判定の前提である**（既定は true）。スクリプトを走らせた
# 端末がフォーカスを保つため、既定のままだと窓は可視判定を通った直後に隠れ、**キャプチャは窓が
# 在った座標の「下にあるもの」を撮る**——実測でエディタが写った。プロファイルを所有する今、
# この前提は config で表現できる（旧版は実 config を触るため、こう書く選択肢が無かった）。
$showOnStartup = if ($Interactive) { 'false' } else { 'true' }
# results を決定的に出すため、専用の索引対象を 3 件置く。1 件では選択行の塗りが窓全体を
# 占め、clear color が最頻色にならない。未選択行を 2 件含めて下地を十分露出させる。
# リポジトリ内の実ファイル名へ依存させないため、名前と検索文字 "z" をここで対にする。
$scanDir = Join-Path $shotDir 'scan'
New-Item -ItemType Directory -Force -Path $scanDir | Out-Null
foreach ($name in @('zsnotracolor-a.snotra-color-fixture', 'zsnotracolor-b.snotra-color-fixture', 'zsnotracolor-c.snotra-color-fixture')) {
    $dummy = Join-Path $scanDir $name
    if (-not (Test-Path $dummy)) { New-Item -ItemType File -Path $dummy | Out-Null }
}
$scanDirToml = (Resolve-Path $scanDir).Path -replace '\\', '/'

$generalSection = @"
show_on_startup = $showOnStartup
auto_hide_on_focus_lost = false
"@
$additionalSections = @"
[visual]
background_color = "$Color"
"@
$pathEntries = @"
[[paths.scan]]
path = "$scanDirToml"
extensions = [".snotra-color-fixture"]
include_folders = false
"@
$profile = New-SnotraVerificationProfile -ProfileDir $profileDir `
    -GeneralSection $generalSection -AdditionalSections $additionalSections -PathEntries $pathEntries
$profileFull = $profile.FullPath

Write-Host ''
Write-Host "検証用プロファイル: $profileFull"
Write-Host "背景色: $Color（ユーザーの実 config には触れません）"

# seed が parse されたかを本体の stderr で確かめる。**`config.toml.bak` の不在では証明にならない**
# ——退避は best-effort で、`fs::rename` が失敗すれば parse 失敗でも `.bak` は現れない
# （`config.rs` の `backup_invalid`）。`[config] ` 付きの eprintln は読み込み失敗の全 arm に在り、
# 成功時には出ないので、これが唯一の健全な観測点である。
function Test-SeedHealth {
    param([string]$LogPath)
    # **ログが無いのは「合格」ではなく「観測できなかった」である。** このログはスクリプト自身が
    # `-RedirectStandardError` で作らせるものなので、不在は前提の崩壊を意味する。
    # 「副作用の不在」を成功と読む形は `.bak` 判定で一度退けたので、ここで再発させない。
    if (-not (Test-Path $LogPath)) {
        Write-Host ''
        Write-Host "判定: 赤 — 本体の stderr ログが在りません（$LogPath）。seed が読めたか確かめられません。"
        return $false
    }
    $bad = @(Select-String -Path $LogPath -SimpleMatch '[config] ')
    if ($bad.Count -eq 0) { return $true }
    Write-Host ''
    Write-Host '判定: 赤 — seed した config が読めていません（色の問題ではありません）:'
    $bad | ForEach-Object { Write-Host "  $($_.Line)" }
    return $false
}

function Measure-WindowBackground {
    param(
        [Parameter(Mandatory)]$Capture,
        [Parameter(Mandatory)][int]$ExpectedKey,
        [Parameter(Mandatory)][string]$Label,
        [Parameter(Mandatory)][string]$ShotPrefix
    )

    # **1 点ではなく最頻色で判定する。** 1 点サンプルは入力欄・角丸・DWM 枠の 1 px ずれで
    # 外れる。背景は窓で最も広い色なので、最頻色と占有率を観測する。
    $counts = @{}
    for ($y = 0; $y -lt $Capture.Height; $y++) {
        for ($x = 0; $x -lt $Capture.Width; $x++) {
            $pixel = $Capture.Bitmap.GetPixel($x, $y)
            # `[byte] -shl 16` は型幅で切り詰められるため、先に `[int]` へ上げる。
            $key = ([int]$pixel.R -shl 16) -bor ([int]$pixel.G -shl 8) -bor [int]$pixel.B
            $counts[$key] = 1 + ($counts[$key] ?? 0)
        }
    }

    $top = $counts.GetEnumerator() | Sort-Object -Property Value -Descending | Select-Object -First 1
    $mode = [int]$top.Key
    $share = $top.Value / ($Capture.Width * $Capture.Height)
    $ok = ($mode -eq $ExpectedKey)
    $actual = '#{0:X6}' -f $mode

    if (-not $ok -or $KeepShot) {
        $shot = Join-Path $shotDir "$ShotPrefix-$($hex).png"
        $Capture.Bitmap.Save($shot, [System.Drawing.Imaging.ImageFormat]::Png)
        Write-Host "スクリーンショット: $shot"
    }

    [pscustomobject]@{
        Label = $Label
        Width = $Capture.Width
        Height = $Capture.Height
        Actual = $actual
        Share = $share
        Ok = $ok
    }
}

if ($Interactive) {
    Write-Host ''
    Write-Host '目視する 4 点:'
    Write-Host "  1. メインウィンドウの定常の背景が $Color である"
    Write-Host '     → 暗いままなら clear color が届いていない（runtime の CLEAR_COLOR が出ている）'
    Write-Host '  2. ホットキー（Alt+Q）で出した瞬間も同色である'
    Write-Host '     → 別の色が一瞬見えてから変わるなら、下地（ネイティブ背景ブラシ）がずれている'
    Write-Host '  3. 結果リスト窓（何か入力して出す）の背景も同色である'
    Write-Host '     → main だけ変わって results が暗いなら、results の経路が届いていない'
    Write-Host '  4. 文字を打って件数を変え続けたとき、results がちらつかないこと'
    Write-Host '     → リサイズごとに下地を撃つため全クライアント領域の erase を誘発する。'
    Write-Host '       目に見えるちらつきになるかはタイミング依存で、ソースからは判定できない'
    Write-Host ''
    Write-Host '起動ログに `[config] ` で始まる行が出たら、seed した config が読めていない。'
    Write-Host 'そのときは既定色で起動しているので、上の 4 点はどれも判定にならない。'
    Write-Host ''
    Write-Host "検証用プロファイルは $profileFull に残る（cargo clean が掃く）。"
    Write-Host ''
    Invoke-SnotraEnvironment -Variables @{ SNOTRA_CONFIG_DIR = $profileFull } -ScriptBlock {
        cargo run -p snotra
    }
    return
}

# --- 自動判定 ---
$proc = $null
$mainCapture = $null
$resultsCapture = $null
$succeeded = $false
try {
    # cargo を親プロセスにすると強制終了時に子の Snotra が残り得る。先に build して、所有できる
    # 本体プロセスを直接起動する。
    $repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
    $manifestPath = Join-Path $repositoryRoot 'Cargo.toml'
    cargo build -p snotra --manifest-path $manifestPath
    if ($LASTEXITCODE -ne 0) { throw "cargo build -p snotra に失敗しました（exit=$LASTEXITCODE）。" }
    $exe = Resolve-SnotraCargoExecutable -RepositoryRoot $repositoryRoot
    if (-not (Test-Path -LiteralPath $exe)) { throw "実行ファイルが在りません: $exe" }

    $proc = Start-SnotraProcess -ConfigDir $profileFull -FilePath $exe `
        -StandardErrorPath $stderrLog -NoNewWindow
    Write-Host "本体を起動しました（pid=$($proc.Id)）。窓の出現を待ちます…"

    $mainHwnd = Wait-SnotraWindow -Title 'Snotra' -Process $proc -TimeoutMs 300000 -PollMs 500

    # 最初の present を確実に跨ぐための待ち。ここで待たないと「まだ何も描かれていない窓」を
    # 撮り、下地（ネイティブブラシ）を clear color と誤って判定しうる——**両者は今や同色なので、
    # この取り違えは判定を緑にする向きに効く**（沈黙経路）。
    Start-Sleep -Seconds 2

    $expectedKey = ([int]$expected.R -shl 16) -bor ([int]$expected.G -shl 8) -bor [int]$expected.B
    $mainCapture = Get-SnotraWindowCapture -Handle $mainHwnd
    $mainResult = Measure-WindowBackground -Capture $mainCapture -ExpectedKey $expectedKey `
        -Label 'main' -ShotPrefix 'main'
    $mainCapture.Bitmap.Dispose()
    $mainCapture = $null

    # 専用 scan に置いた 3 件を "z" で検索し、results を決定的に出す。
    if (-not (Set-SnotraForegroundWindow -Handle $mainHwnd)) {
        throw 'main 窓を前面化できず、results を出すキー入力の宛先を確定できませんでした。'
    }
    Start-Sleep -Milliseconds 100
    $queryVk = [byte][int][char]'Z'
    Send-SnotraKey -VirtualKey $queryVk
    Send-SnotraKey -VirtualKey $queryVk -Up
    $resultsHwnd = Wait-SnotraWindow -Title 'Snotra Results' -Process $proc -TimeoutMs 10000
    Start-Sleep -Seconds 1
    $resultsCapture = Get-SnotraWindowCapture -Handle $resultsHwnd
    $resultsResult = Measure-WindowBackground -Capture $resultsCapture -ExpectedKey $expectedKey `
        -Label 'results' -ShotPrefix 'results'
    $resultsCapture.Bitmap.Dispose()
    $resultsCapture = $null

    # env が効いたことの**肯定的証拠**。効いていなければ本体は実 config を読み、実プロファイルへ
    # 書くので、ここには seed した config.toml しか無い。ピクセルが赤いとき「色が届いていない」と
    # 「env が効いていない」を切り分けるのはこの 1 行である。
    # **`scripts/smoke-egui.ps1` と `scripts/smoke-startup.ps1` が同型の判定を持つ**（#804・
    # 片方だけ直さないこと。共有ヘルパーにしない理由と共有化の送り先は seed 側と同じ・#843）。
    # **実測（#803）**: 出るのは `index.bin` である（索引 0 件でも書かれる）。`window.bin` と
    # 履歴は正常終了で書かれるものなので、下の `finally` が `Stop-Process -Force` する以上出ない。
    $generated = @(Get-ChildItem -Path $profileDir -Filter '*.bin' -ErrorAction SilentlyContinue)

    Write-Host ''
    foreach ($result in @($mainResult, $resultsResult)) {
        Write-Host ("$($result.Label) 窓 $($result.Width)x$($result.Height) の最頻色: 期待 $Color / 実測 $($result.Actual)（占有率 {0:P1}）" -f $result.Share)
    }
    $seedHealthy = Test-SeedHealth -LogPath $stderrLog
    $profileWritten = ($generated.Count -gt 0)
    if (-not $profileWritten) {
        Write-Host ''
        Write-Host "判定: 赤 — 検証用プロファイルに *.bin が生成されていません（$profileFull）。"
        Write-Host '  SNOTRA_CONFIG_DIR が効いておらず、本体が実 config を読んでいる可能性があります。'
    } else {
        Write-Host "プロファイルへの書き込みを確認: $($generated.Name -join ', ')（SNOTRA_CONFIG_DIR は効いています）"
    }

    $succeeded = $seedHealthy -and $profileWritten -and $mainResult.Ok -and $resultsResult.Ok
    if ($mainResult.Ok -and $resultsResult.Ok) {
        Write-Host '判定: 緑 — main と results の定常背景に config の色が届いています。'
    } else {
        Write-Host '判定: 赤 — 色が届いていません。次のどれかです:'
        Write-Host '  - clear color が view から渡っていない（runtime 既定の #282828 が出る）'
        Write-Host '  - 撮れているのが窓でない（スクリーンショットを見る。占有率が低いときは特に疑う）'
    }
    Write-Host ''
    Write-Host '自動判定はここまでです。次の 2 点は目視で確認してください（-Interactive）:'
    Write-Host '  - show の一瞬に別の色が見えないか（下地のずれ）'
    Write-Host '  - 結果リスト窓をリサイズさせ続けたときにちらつかないか'
    Write-Host ''
} finally {
    if ($mainCapture) { $mainCapture.Bitmap.Dispose() }
    if ($resultsCapture) { $resultsCapture.Bitmap.Dispose() }
    if ($proc -and -not $proc.HasExited) {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
        Write-Host "本体を終了しました（pid=$($proc.Id)）"
    }
}

if (-not $succeeded) { exit 1 }
