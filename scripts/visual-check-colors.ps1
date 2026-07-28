#Requires -Version 7
<#
.SYNOPSIS
  非既定の `[visual]` 色で本体を起動し、色が実際に画面へ届いているかを確認する。

.DESCRIPTION
  config の既定色 `#282828` は `snotra-egui-runtime` の `CLEAR_COLOR` と一致するため、
  **既定のまま起動しても「色が届いていない」欠陥は観測できない**
  （`docs/development-principles.md`「config の値は到達性の検出器を持たない」）。

  本スクリプトは実 config を退避してから非既定色を書き込み、終了時に必ず戻す。
  異常終了でバックアップが残った場合は `-Restore` で回収する。

  既定は `-Assert`（自動判定）である。main 窓を実際にキャプチャして背景ピクセルを読み、
  期待色と一致するかを **exit code で** 返す。`-Interactive` では起動するだけで判定せず、
  目視項目を読み上げる。

  **自動判定できるのは main の定常背景だけである。** 次の 2 点は目視のまま残る:
  - **show の一瞬のフラッシュ**: softbuffer の present 前で 1 フレーム未満。連写しても
    捉えられる保証がない
  - **results 窓の背景**: 出すには文字入力（`SendInput`）が要り、`smoke-egui.ps1` の
    入力注入機構を複製することになる。費用に見合わないため目視へ残した

  **trace は判定に使わない。** 「`set_clear_color` を呼んだ」というログは、その色が画面へ
  出たことを意味しない（`src-tauri/CLAUDE.md`「trace の presence 検査は状態の検査ではない」・
  #671 PR A′ で `egui_results:hide` が出るのに窓が残る回帰を smoke が緑のまま通した）。
  ここで判定の根拠にするのは**描かれた結果のピクセル**だけである。

.PARAMETER Color
  適用する背景色。既定は `#4A2B5C`（既定色とも `CLEAR_COLOR` とも明確に違う紫）。
  `-Color '#FFF'` を渡すと 3 桁 hex の受理を確認できる（#680 の 1・パーサ統合の回帰）。

.PARAMETER Interactive
  自動判定せず、起動して目視項目を読み上げる。人が終了するまで待つ。

.PARAMETER Restore
  起動せず、退避した config を戻すだけ。異常終了後の回収に使う。

.PARAMETER KeepShot
  判定が緑でもスクリーンショットを残す（既定では赤のときだけ残す）。

.EXAMPLE
  npm run check:colors
.EXAMPLE
  npm run check:colors -- -Color '#FFF'
.EXAMPLE
  npm run check:colors -- -Interactive
.EXAMPLE
  npm run check:colors -- -Restore
#>
[CmdletBinding()]
param(
    [string]$Color = '#4A2B5C',
    [switch]$Interactive,
    [switch]$Restore,
    [switch]$KeepShot
)

$ErrorActionPreference = 'Stop'
$configPath = Join-Path $env:APPDATA 'Snotra\config.toml'
$backupPath = "$configPath.visualcheck-bak"
$shotDir = Join-Path $PSScriptRoot '..\target\visual-check'

function Restore-SnotraConfig {
    if (Test-Path $backupPath) {
        Move-Item -Path $backupPath -Destination $configPath -Force
        Write-Host "config を復元しました: $configPath"
    } else {
        Write-Host '復元するバックアップがありません（既に復元済みか、退避前に終了しています）'
    }
}

if ($Restore) {
    Restore-SnotraConfig
    exit 0
}

# --- 期待色を RGB へ（`#RGB` / `#RRGGBB` の両方を受ける。判定側でも 3 桁を展開する必要がある） ---
$hex = $Color.TrimStart('#')
if ($hex.Length -eq 3) { $hex = "$($hex[0])$($hex[0])$($hex[1])$($hex[1])$($hex[2])$($hex[2])" }
if ($hex.Length -ne 6) { throw "-Color は #RGB か #RRGGBB で指定してください: $Color" }
$expected = [pscustomobject]@{
    R = [Convert]::ToInt32($hex.Substring(0, 2), 16)
    G = [Convert]::ToInt32($hex.Substring(2, 2), 16)
    B = [Convert]::ToInt32($hex.Substring(4, 2), 16)
}

if (-not (Test-Path $configPath)) {
    throw "config.toml が見つかりません: $configPath`n  本体を一度起動して生成してから再実行してください。"
}
# 二重退避の防止: 前回のバックアップを上書きすると、ユーザーの実 config が検証用の色で固定される
if (Test-Path $backupPath) {
    throw "前回のバックアップが残っています: $backupPath`n  -Restore で戻してから再実行してください。"
}
# **単一インスタンス衝突は沈黙する**: 本体は tauri_plugin_single_instance を使うため、既に起動して
# いると 2 つ目のプロセスは既存インスタンスの窓を show して即終了する。スクリプトは何事もなく
# 復元まで走り、操作者には「検証した」ように見える——検証補助の失敗モードとして最悪の形なので
# **退避より前に**弾く。
$existing = @(Get-Process -Name 'snotra' -ErrorAction SilentlyContinue)
if ($existing.Count -gt 0) {
    throw "Snotra が既に起動しています（pid=$($existing.Id -join ', ')）。`n  single-instance により 2 つ目のプロセスは即終了し、検証は空振りします。終了してから再実行してください。"
}

# 退避から復元までを 1 つの try で包む——退避と書き換えの間で中断されると自動復元が効かない
Copy-Item -Path $configPath -Destination $backupPath
$proc = $null
try {
$patched = Get-Content -Path $configPath -Raw

# **同名キーは全セクションで置換される**（`-replace` は全一致に効く）。`background_color` は
# `[visual]` と `[visual.custom_theme]` の両方に在りうるため、後者も検証色になる（実測）。
# 判定は `[visual]` の値が効くので影響せず、config は退避から復元されるが、**復元前に強制終了され
# `-Restore` も打たれなかった場合はカスタム配色の保存値が検証色のまま残る**（受容する残余）。
function Set-TomlKey {
    param([string]$Text, [string]$Section, [string]$Key, [string]$Value)
    if ($Text -match "(?m)^\s*$Key\s*=") {
        return $Text -replace "(?m)^\s*$Key\s*=.*$", "$Key = $Value"
    }
    if ($Text -match "(?m)^\[$Section\]") {
        return $Text -replace "(?m)^\[$Section\]", "[$Section]`n$Key = $Value"
    }
    return $Text + "`n[$Section]`n$Key = $Value`n"
}

$patched = Set-TomlKey -Text $patched -Section 'visual' -Key 'background_color' -Value "`"$Color`""
# 自動判定では hotkey 注入なしで窓を出したい。config を書き換えている以上、これは追加コストゼロで
# 済む——`smoke-egui.ps1` の keybd_event 機構を複製しないための選択である。
if (-not $Interactive) {
    $patched = Set-TomlKey -Text $patched -Section 'general' -Key 'show_on_startup' -Value 'true'
}
Set-Content -Path $configPath -Value $patched -NoNewline

Write-Host ''
Write-Host "背景色を $Color にしました（退避元: $backupPath）"

if ($Interactive) {
    Write-Host ''
    Write-Host '目視する 4 点:'
    Write-Host "  1. メインウィンドウの定常の背景が $Color である"
    Write-Host '     → 暗いままなら clear color が届いていない（runtime の CLEAR_COLOR が出ている）'
    Write-Host '  2. ホットキーで出した瞬間も同色である'
    Write-Host '     → 別の色が一瞬見えてから変わるなら、下地（ネイティブ背景ブラシ）がずれている'
    Write-Host '  3. 結果リスト窓（何か入力して出す）の背景も同色である'
    Write-Host '     → main だけ変わって results が暗いなら、results の経路が届いていない'
    Write-Host '  4. 文字を打って件数を変え続けたとき、results がちらつかないこと'
    Write-Host '     → リサイズごとに下地を撃つため全クライアント領域の erase を誘発する。'
    Write-Host '       目に見えるちらつきになるかはタイミング依存で、ソースからは判定できない'
    Write-Host ''
    Write-Host '本体を終了すると config を自動で復元します（異常終了時は -Restore）。'
    Write-Host ''
    cargo run -p snotra
    return
}

# --- 自動判定 ---
Add-Type -Namespace VisualCheck -Name Native -MemberDefinition @'
[DllImport("user32.dll", CharSet = CharSet.Unicode)]
public static extern IntPtr FindWindowW(string cls, string title);
[DllImport("user32.dll")]
public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
[DllImport("user32.dll")]
public static extern bool IsWindowVisible(IntPtr hWnd);
public struct RECT { public int Left, Top, Right, Bottom; }
'@
Add-Type -AssemblyName System.Drawing

    $proc = Start-Process -FilePath 'cargo' -ArgumentList 'run', '-p', 'snotra' -PassThru -NoNewWindow
    Write-Host "本体を起動しました（pid=$($proc.Id)）。窓の出現を待ちます…"

    # cold build を含むので長めに待つ。`show_on_startup = true` ゆえ hotkey 注入は要らない
    $deadline = (Get-Date).AddSeconds(300)
    $hwnd = [IntPtr]::Zero
    while ((Get-Date) -lt $deadline) {
        $hwnd = [VisualCheck.Native]::FindWindowW($null, 'Snotra')
        if ($hwnd -ne [IntPtr]::Zero -and [VisualCheck.Native]::IsWindowVisible($hwnd)) { break }
        if ($proc.HasExited) { throw "本体が終了しました（exit=$($proc.ExitCode)）。ビルドに失敗している可能性があります。" }
        Start-Sleep -Milliseconds 500
    }
    if ($hwnd -eq [IntPtr]::Zero) { throw '窓 "Snotra" が現れませんでした（300 秒）。' }

    # 最初の present を確実に跨ぐための待ち。ここで待たないと「まだ何も描かれていない窓」を
    # 撮り、下地（ネイティブブラシ）を clear color と誤って判定しうる——**両者は今や同色なので、
    # この取り違えは判定を緑にする向きに効く**（沈黙経路）。
    Start-Sleep -Seconds 2

    $r = New-Object VisualCheck.Native+RECT
    if (-not [VisualCheck.Native]::GetWindowRect($hwnd, [ref]$r)) { throw 'GetWindowRect に失敗しました。' }
    $w = $r.Right - $r.Left
    $h = $r.Bottom - $r.Top
    if ($w -le 0 -or $h -le 0) { throw "窓の矩形が不正です: ${w}x${h}" }

    $bmp = New-Object System.Drawing.Bitmap($w, $h)
    $gfx = [System.Drawing.Graphics]::FromImage($bmp)
    $gfx.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size)
    $gfx.Dispose()

    # 観測点: 左端の中央付近。**入力欄は内側マージンの中にあり、DWM の角丸は隅を欠く**ため、
    # 隅ではなく左端中央を採る。外れる場合はスクリーンショットを見て調整すること。
    $px = 3
    $py = [int]($h / 2)
    $c = $bmp.GetPixel($px, $py)

    $ok = ($c.R -eq $expected.R) -and ($c.G -eq $expected.G) -and ($c.B -eq $expected.B)
    $actual = '#{0:X2}{1:X2}{2:X2}' -f $c.R, $c.G, $c.B

    if (-not $ok -or $KeepShot) {
        New-Item -ItemType Directory -Force -Path $shotDir | Out-Null
        $shot = Join-Path $shotDir "main-$($hex).png"
        $bmp.Save($shot, [System.Drawing.Imaging.ImageFormat]::Png)
        Write-Host "スクリーンショット: $shot"
    }
    $bmp.Dispose()

    Write-Host ''
    Write-Host "観測点 (${px},${py}) / 窓 ${w}x${h}: 期待 $Color / 実測 $actual"
    if ($ok) {
        Write-Host '判定: 緑 — main の定常背景に config の色が届いています。'
    } else {
        Write-Host '判定: 赤 — 色が届いていません。次のどれかです:'
        Write-Host '  - clear color が view から渡っていない（runtime 既定の #282828 が出る）'
        Write-Host '  - 観測点が背景でない場所を指している（スクリーンショットを見て $px/$py を調整）'
    }
    Write-Host ''
    Write-Host '自動判定はここまでです。次の 2 点は目視で確認してください（-Interactive）:'
    Write-Host '  - show の一瞬に別の色が見えないか（下地のずれ）'
    Write-Host '  - 結果リスト窓の背景も同色か'
    Write-Host ''

    if (-not $ok) { exit 1 }
} finally {
    if ($proc -and -not $proc.HasExited) {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
        Write-Host "本体を終了しました（pid=$($proc.Id)）"
    }
    Restore-SnotraConfig
}
