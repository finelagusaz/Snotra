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

# **単一インスタンス衝突は沈黙する**: 本体は tauri_plugin_single_instance を使うため、既に起動して
# いると 2 つ目のプロセスは既存インスタンスの窓を show して即終了する。スクリプトは何事もなく
# 完走し、操作者には「検証した」ように見える——検証補助の失敗モードとして最悪の形なので弾く。
# **プロファイルを分けてもこれは消せない**: single-instance の識別子は app identity であって
# config dir ではない。`SNOTRA_CONFIG_DIR` はデータを分けるが、同時起動を許すわけではない。
$existing = @(Get-Process -Name 'snotra' -ErrorAction SilentlyContinue)
if ($existing.Count -gt 0) {
    throw "Snotra が既に起動しています（pid=$($existing.Id -join ', ')）。`n  single-instance により 2 つ目のプロセスは即終了し、検証は空振りします。終了してから再実行してください。"
}

# --- 検証用プロファイルを作り直す ---
New-Item -ItemType Directory -Force -Path $profileDir | Out-Null
# **前回実行の残骸を消す。** プロファイルは実行間で再利用するので、残したままだと後段の 2 つの
# 判定（seed の健全性・env が効いたことの証拠）が**どちらも古いファイルで空振りに合格する**。
Remove-Item -Path (Join-Path $profileDir 'config.toml.bak') -Force -ErrorAction SilentlyContinue
Remove-Item -Path (Join-Path $profileDir '*.bin') -Force -ErrorAction SilentlyContinue
Remove-Item -Path $stderrLog -Force -ErrorAction SilentlyContinue

# 最小の有効 TOML。**`[hotkey]` / `[appearance]` / `[paths]` は `#[serde(default)]` を持たない
# 必須セクション**（`snotra-core/src/config.rs` の `Config`）で、欠けると parse が落ちて
# 破損復旧経路（`.bak` 退避 + 既定値起動）を踏む。既定値で起動すると背景は `CLEAR_COLOR` に
# なり、「色が届いていない」と誤読される。
# `scripts/smoke-egui.ps1` と `scripts/smoke-startup.ps1` も同型の seed を持つ（必須セクションの根拠は共通・
# 片方だけ直さないこと）。**あちらは `[[paths.scan]]` にダミーを 1 件置くが、こちらは置かない**
# ——results 窓を出す必要が無く、scan 0 件なら索引構築が即終了するためである。
# `[paths]` は空ヘッダで置く（`PathsConfig.scan` は `#[serde(default)]` ゆえ空 Vec になり、
# `Config::default_scan_paths()` には落ちない）。
# **`auto_hide_on_focus_lost = false` は自動判定の前提である**（既定は true）。スクリプトを走らせた
# 端末がフォーカスを保つため、既定のままだと窓は可視判定を通った直後に隠れ、**キャプチャは窓が
# 在った座標の「下にあるもの」を撮る**——実測でエディタが写った。プロファイルを所有する今、
# この前提は config で表現できる（旧版は実 config を触るため、こう書く選択肢が無かった）。
$showOnStartup = if ($Interactive) { 'false' } else { 'true' }
# **索引対象を 1 つ与える。** 空の索引では結果が 1 件も出ず、results 窓は原理的に開かない
# （SPEC §8.6「results 可視 ⇔ 結果が空でない」）——**目視項目 3・4 が観測不能になる**。
# 旧版は実 config を書き換えていたので実ユーザーの索引が使え、この問題が無かった。
# 対象はこのリポジトリの `scripts/`（常に在る・十数件で索引構築は一瞬・打鍵で件数が変わる）。
$scanDirToml = (Resolve-Path $PSScriptRoot).Path -replace '\\', '/'
$seedToml = @"
[hotkey]
modifier = "Alt"
key = "Q"

[appearance]
window_width = 600

[general]
show_on_startup = $showOnStartup
auto_hide_on_focus_lost = false

[visual]
background_color = "$Color"

[paths]

[[paths.scan]]
path = "$scanDirToml"
extensions = [".ps1", ".mjs"]
include_folders = false
"@
Set-Content -Path (Join-Path $profileDir 'config.toml') -Value $seedToml -Encoding utf8

$profileFull = (Resolve-Path $profileDir).Path
$env:SNOTRA_CONFIG_DIR = $profileFull

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
    try { cargo run -p snotra } finally { Remove-Item Env:SNOTRA_CONFIG_DIR -ErrorAction SilentlyContinue }
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
[DllImport("user32.dll")]
public static extern bool SetProcessDpiAwarenessContext(IntPtr value);
[DllImport("user32.dll")]
public static extern bool SetProcessDPIAware();
[DllImport("dwmapi.dll")]
public static extern int DwmGetWindowAttribute(IntPtr hWnd, int attr, out RECT value, int size);
public struct RECT { public int Left, Top, Right, Bottom; }
'@
Add-Type -AssemblyName System.Drawing

# **このプロセスを DPI 対応にする。** 既定の PowerShell は DPI 非対応で、`GetWindowRect` が返す
# 矩形と `CopyFromScreen` が使う座標空間が**スケール比だけずれる**（実測: 125% 環境で約 1.25 倍。
# 窓は正しく描かれているのに、その左上を撮って「色が届いていない」と判定していた）。
# **これは沈黙する誤りである**——撮れた画像は有効な PNG で、判定も exit code も普通に返る。
# DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 = -4。古い OS 向けに SetProcessDPIAware へ落ちる。
if (-not [VisualCheck.Native]::SetProcessDpiAwarenessContext([IntPtr]::new(-4))) {
    [void][VisualCheck.Native]::SetProcessDPIAware()
}

$proc = $null
try {
    $proc = Start-Process -FilePath 'cargo' -ArgumentList 'run', '-p', 'snotra' -PassThru -NoNewWindow -RedirectStandardError $stderrLog
    Write-Host "本体を起動しました（pid=$($proc.Id)）。窓の出現を待ちます…"

    # cold build を含むので長めに待つ。`show_on_startup = true` ゆえ hotkey 注入は要らない
    $deadline = (Get-Date).AddSeconds(300)
    $hwnd = [IntPtr]::Zero
    while ((Get-Date) -lt $deadline) {
        # **`$null` を渡してはならない。** PowerShell は `$null` を `[string]` 引数へ渡すとき
        # **空文字へ変換する**ため `FindWindowW("", "Snotra")` になり、クラス名 `""` に一致せず
        # 常に 0 を返す。実測（同一プロセス・同一時刻）: `$null` → 0 / `[NullString]::Value` →
        # 有効な HWND。この誤りで自動判定は導入以来一度も動かず、毎回 300 秒待って throw していた。
        $hwnd = [VisualCheck.Native]::FindWindowW([NullString]::Value, 'Snotra')
        if ($hwnd -ne [IntPtr]::Zero -and [VisualCheck.Native]::IsWindowVisible($hwnd)) { break }
        if ($proc.HasExited) { throw "本体が終了しました（exit=$($proc.ExitCode)）。ビルドに失敗している可能性があります。詳細: $stderrLog" }
        Start-Sleep -Milliseconds 500
    }
    if ($hwnd -eq [IntPtr]::Zero) { throw "窓 `"Snotra`" が現れませんでした（300 秒）。詳細: $stderrLog" }

    # 最初の present を確実に跨ぐための待ち。ここで待たないと「まだ何も描かれていない窓」を
    # 撮り、下地（ネイティブブラシ）を clear color と誤って判定しうる——**両者は今や同色なので、
    # この取り違えは判定を緑にする向きに効く**（沈黙経路）。
    Start-Sleep -Seconds 2

    # **待ちの後にもう一度可視性を見る。** 窓が隠れても `GetWindowRect` は最後の矩形を返し続け、
    # `CopyFromScreen` は**その座標に今あるもの**を撮る——判定は「窓でないもの」の色で下される。
    # 実測で踏んだ（`auto_hide_on_focus_lost` の既定 true で隠れ、下のエディタが写った）。
    if (-not [VisualCheck.Native]::IsWindowVisible($hwnd)) {
        throw '窓が可視でなくなりました（キャプチャ直前）。フォーカス喪失で隠れた可能性があります——seed の auto_hide_on_focus_lost を確認してください。'
    }

    # **`GetWindowRect` は DWM のドロップシャドウを含む**ため、そのまま撮ると上下左右に窓外が
    # 混じる（実測: 118px の矩形のうち窓は約 96px で、残りに背後のエディタが写った）。
    # `DWMWA_EXTENDED_FRAME_BOUNDS`（= 9）は**目に見える**枠を返す。取れない環境では
    # `GetWindowRect` へ落ちる（下の最頻色判定が数 px のずれを吸収する）。
    $r = New-Object VisualCheck.Native+RECT
    if ([VisualCheck.Native]::DwmGetWindowAttribute($hwnd, 9, [ref]$r, 16) -ne 0) {
        if (-not [VisualCheck.Native]::GetWindowRect($hwnd, [ref]$r)) { throw 'GetWindowRect に失敗しました。' }
    }
    $w = $r.Right - $r.Left
    $h = $r.Bottom - $r.Top
    if ($w -le 0 -or $h -le 0) { throw "窓の矩形が不正です: ${w}x${h}" }

    $bmp = New-Object System.Drawing.Bitmap($w, $h)
    $gfx = [System.Drawing.Graphics]::FromImage($bmp)
    $gfx.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size)
    $gfx.Dispose()

    # **1 点ではなく最頻色で判定する。** 1 点サンプルは「どこが背景か」を人が当てる必要があり、
    # 入力欄・トースト・角丸・DWM の枠の 1 px ずれのどれでも外れる（実測で 3 通りの外し方をした）。
    # 背景は定義上その窓で最も広い色なので、**最頻色が背景である**——観測点の位置を当てる問題が
    # 消える。占有率も出す（低ければ「背景を撮れていない」ことが数字で見える）。
    $counts = @{}
    for ($y = 0; $y -lt $h; $y++) {
        for ($x = 0; $x -lt $w; $x++) {
            $q = $bmp.GetPixel($x, $y)
            # **`[int]` へ明示的に上げる。** `$q.R` は `[byte]` で、`-shl` は左辺の型幅で
            # 切り詰めるため `[byte] -shl 16` は 0 になる（実測: 期待 #4A2B5C に対し #00005C）。
            $k = ([int]$q.R -shl 16) -bor ([int]$q.G -shl 8) -bor [int]$q.B
            $counts[$k] = 1 + ($counts[$k] ?? 0)
        }
    }
    $top = $counts.GetEnumerator() | Sort-Object -Property Value -Descending | Select-Object -First 1
    $mode = [int]$top.Key
    $share = $top.Value / ($w * $h)
    $expectedKey = ($expected.R -shl 16) -bor ($expected.G -shl 8) -bor $expected.B

    $ok = ($mode -eq $expectedKey)
    $actual = '#{0:X6}' -f $mode

    if (-not $ok -or $KeepShot) {
        $shot = Join-Path $shotDir "main-$($hex).png"
        $bmp.Save($shot, [System.Drawing.Imaging.ImageFormat]::Png)
        Write-Host "スクリーンショット: $shot"
    }
    $bmp.Dispose()

    # env が効いたことの**肯定的証拠**。効いていなければ本体は実 config を読み、実プロファイルへ
    # 書くので、ここには seed した config.toml しか無い。ピクセルが赤いとき「色が届いていない」と
    # 「env が効いていない」を切り分けるのはこの 1 行である。
    # **`scripts/smoke-egui.ps1` と `scripts/smoke-startup.ps1` が同型の判定を持つ**（#804・
    # 片方だけ直さないこと。共有ヘルパーにしない理由と共有化の送り先は seed 側と同じ・#843）。
    # **実測（#803）**: 出るのは `index.bin` である（索引 0 件でも書かれる）。`window.bin` と
    # 履歴は正常終了で書かれるものなので、下の `finally` が `Stop-Process -Force` する以上出ない。
    $generated = @(Get-ChildItem -Path $profileDir -Filter '*.bin' -ErrorAction SilentlyContinue)

    Write-Host ''
    Write-Host ("窓 ${w}x${h} の最頻色: 期待 $Color / 実測 $actual（占有率 {0:P1}）" -f $share)
    if (-not (Test-SeedHealth -LogPath $stderrLog)) { exit 1 }
    if ($generated.Count -eq 0) {
        Write-Host ''
        Write-Host "判定: 赤 — 検証用プロファイルに *.bin が生成されていません（$profileFull）。"
        Write-Host '  SNOTRA_CONFIG_DIR が効いておらず、本体が実 config を読んでいる可能性があります。'
        exit 1
    }
    Write-Host "プロファイルへの書き込みを確認: $($generated.Name -join ', ')（SNOTRA_CONFIG_DIR は効いています）"

    if ($ok) {
        Write-Host '判定: 緑 — main の定常背景に config の色が届いています。'
    } else {
        Write-Host '判定: 赤 — 色が届いていません。次のどれかです:'
        Write-Host '  - clear color が view から渡っていない（runtime 既定の #282828 が出る）'
        Write-Host '  - 撮れているのが窓でない（スクリーンショットを見る。占有率が低いときは特に疑う）'
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
    # **set したものは戻す。** `npm run check:colors` は別プロセスなので実害は無いが、
    # 開いている pwsh から直に叩くと以後の `cargo run` と memory_footprint の「実運用点」が
    # 使い捨てプロファイルを指し続ける（`config_dir_is_wired_to_dirs_config_dir_with_snotra_suffix`
    # も上書き側の分岐を assert するようになり、既定の結線を見なくなる）。
    Remove-Item Env:SNOTRA_CONFIG_DIR -ErrorAction SilentlyContinue
}
