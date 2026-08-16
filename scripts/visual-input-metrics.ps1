#Requires -Version 7
<#
.SYNOPSIS
  main 窓の入力欄の縦の幾何を、フォントを差し替えながら実ピクセルで測って**表に出す**（判定しない）。

.DESCRIPTION
  Rust 側の検査は egui の論理座標で galley の位置を縛るが、**実機のフォント解決・softbuffer の
  ラスタライズ・DPI を通らない**。そこを通した結果を見る道具である。由来・使う契機・判定を
  持たない理由は `scripts/lib/SnotraInputMetrics.psm1` の doc が正本。

  **合否を出さない。** 出力は「欄の内側 / 中身の高さ / 上下余白 / Skew」の表であり、読むのは
  人間である。前後で撮って突き合わせる使い方を想定している（例: egui を上げる前後）。

  **突き合わせが成り立つのは実効 DPI が同じときだけである。** ゆえに `Dpi` を表の列に持たせて
  ある——見出しではなく列なのは、**表を抜粋して貼ると見出しが落ちる**からである（#1049 の
  PR 本文が 8 行のうち 4 行だけを貼っており、そこには倍率が残っていない）。

.EXAMPLE
  npm run check:input-metrics
  npm run check:input-metrics -- -Fonts 'Yu Gothic UI','Segoe UI'
#>
[CmdletBinding()]
param(
    # 測るフォント。既定は日本語 4 種 + 英語 4 種で、**DPI 96（100%）で行高の偶奇が割れる代表**
    # として選んである（2026-08-11 実測: Segoe UI と Yu Gothic UI がずれ、Arial と
    # HackGen Console は揃う）。
    #
    # **この割れ方は表示倍率に依存する。** 2026-08-16 に DPI 120（125%）で測ると、同じ本体・
    # 同じ 8 フォントで内側が 32 行（偶数）・キャレットが 8 種とも偶数になり、**割れる代表が
    # 1 つも無くなった**（Skew は全 8 種で 0）。ゆえにこの既定集合が名前どおり働くかは倍率次第
    # であり、**全種が対称に出たことを「非対称が直った」と読んではならない**。
    [string[]]$Fonts = @(
        'Yu Gothic UI', 'Meiryo UI', 'Noto Sans JP', 'HackGen Console',
        'Segoe UI', 'Arial', 'Consolas', 'Verdana'
    ),
    # 入力する文字。**A-Z のみ**（`Send-SnotraKey` が VK で送るため）。
    [string]$Query = 'AGQ',
    [string]$ExePath = '',
    # スクリーンショットを残す（既定は測って捨てる）。
    [switch]$KeepShots
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Import-Module (Join-Path $PSScriptRoot 'lib/SnotraSmoke.psm1') -Force
# 色キーの形（`R<<16 | G<<8 | B`）と 3 桁 hex の展開は**あちらが持つ**。ここへ写しを置かない。
Import-Module (Join-Path $PSScriptRoot 'lib/SnotraWindowColors.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'lib/SnotraInputMetrics.psm1') -Force

# 画面ロック中は窓が描かれず、撮った絵が黒一色になる（#866）。
Assert-SnotraSessionUnlocked -Operation '入力欄の幾何測定'

$exe = if ($ExePath) { $ExePath } else { Resolve-SnotraCargoExecutable -RepositoryRoot $repoRoot }
if (-not (Test-Path $exe)) { throw "実行ファイルがありません: $exe" }
Write-Host "対象: $exe"

$outDir = Join-Path $repoRoot 'target/visual-input-metrics'
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

# **非既定色を使う**（`docs/build-commands.md`「`[visual]` の色を変える変更は非既定色で目視する」）。
# 塗りを宣言色で探すため、背景と入力欄の塗りが明確に違う組を選ぶ。
$backgroundColor = '#1E2430'
$inputBackgroundColor = '#3A2A55'

Add-Type -AssemblyName System.Drawing

# **塗りの判定には許容差が要る。** 入力欄の塗りは領域によって成分が ±1 揺れる
# （実測: `#3A2A55` に対し `392954` / `3A2A54` / `392A55` が混在）。`raster.rs` の重ね塗りで
# 生じる誤差であり、**厳密一致で判定するとこの揺れを「中身」として拾い、16 行の偽の帯を
# 上下余白として報告する**（2026-08-11 に実測）。
#
# `ConvertTo-SnotraColorKey`（厳密一致）を使わないのはこの一点である——あちらは「宣言した色が
# 届いているか」の存在検査に十分だが、**領域の境界を引く**にはこちらが要る。
$fill = @{
    R = [Convert]::ToInt32($inputBackgroundColor.Substring(1, 2), 16)
    G = [Convert]::ToInt32($inputBackgroundColor.Substring(3, 2), 16)
    B = [Convert]::ToInt32($inputBackgroundColor.Substring(5, 2), 16)
}
$fillTolerance = 3

function Test-IsFill {
    param([Parameter(Mandatory)]$Pixel)
    ([Math]::Abs([int]$Pixel.R - $fill.R) -le $fillTolerance) -and
    ([Math]::Abs([int]$Pixel.G - $fill.G) -le $fillTolerance) -and
    ([Math]::Abs([int]$Pixel.B - $fill.B) -le $fillTolerance)
}

# Bitmap → 行ごとの「入力欄の塗り画素数」。**I/O 側**（判定は純関数が持つ）。
#
# **`[int]` へ上げてからシフトする。** PowerShell の `-shl` は左オペランドの型で動くため、
# `[byte] -shl 16` は型幅で切り詰められて 0 になる——R と G が消えた鍵は 1 画素とも一致せず、
# 「塗りが 1 行も見つからない」形で**沈黙する**（2026-08-11 に実測）。この形とインライン化の
# 理由は `scripts/visual-check-colors.ps1` の `Get-WindowColorHistogram` のコメントが正本。
function Get-FillHitsPerRow {
    param([Parameter(Mandatory)]$Capture)
    $hits = @(0) * $Capture.Height
    for ($y = 0; $y -lt $Capture.Height; $y++) {
        $n = 0
        for ($x = 0; $x -lt $Capture.Width; $x++) {
            if (Test-IsFill -Pixel $Capture.Bitmap.GetPixel($x, $y)) { $n++ }
        }
        $hits[$y] = $n
    }
    $hits
}

# 代表行で塗りが占める x の範囲を返す。**枠線を探索から外すために要る**——枠線は塗り色ではなく
# 縦に長く連なるので、これを除かないと「中身」として最初に拾われ、上下余白が 0 に化ける（実測）。
function Get-FillColumnRange {
    param([Parameter(Mandatory)]$Capture, [Parameter(Mandatory)][int]$Row)
    $first = -1
    $last = -1
    for ($x = 0; $x -lt $Capture.Width; $x++) {
        if (Test-IsFill -Pixel $Capture.Bitmap.GetPixel($x, $Row)) {
            if ($first -lt 0) { $first = $x }
            $last = $x
        }
    }
    if ($first -lt 0) { return $null }
    @{ First = $first; Last = $last }
}

# 欄の内側で「塗りでない画素」が縦に長く連なる列を探し、その列の真偽列を返す。
#
# **右から探す。** 入力した文字の縦棒（`A` の左辺など）も縦に長く連なるため、左から探すと
# そちらを先に拾い、**上下余白が文字のインク範囲に化ける**（実測で Top/Bottom が 4/1 になった）。
# キャレットは入力文字の**右**に立つので、右端から最初に見つかる縦線がそれである——色に依存せず
# 分けられるので、egui の既定カーソル色が変わっても壊れない。
#
# **キャレットは点滅する**ので、見つからない＝消灯の瞬間を撮った可能性がある（呼び出し側で再試行）。
function Get-CaretColumnHits {
    param(
        [Parameter(Mandatory)]$Capture,
        [Parameter(Mandatory)]$Bounds,
        [Parameter(Mandatory)][int]$MinRun,
        [Parameter(Mandatory)][int]$FromX,
        [Parameter(Mandatory)][int]$ToX
    )
    for ($x = $ToX; $x -ge $FromX; $x--) {
        $hits = @()
        $n = 0
        for ($y = $Bounds.InnerTop; $y -le $Bounds.InnerBottom; $y++) {
            $isFill = Test-IsFill -Pixel $Capture.Bitmap.GetPixel($x, $y)
            $hits += (-not $isFill)
            if (-not $isFill) { $n++ }
        }
        if ($n -ge $MinRun) { return @{ X = $x; Hits = [bool[]]$hits } }
    }
    $null
}

# **`;` 区切りの単一文字列も受ける。** `pwsh -File` は `-Fonts 'A','B'` を 1 つの文字列として
# 渡すため（`[int[]]` と違い `[string[]]` はカンマで分割されない）、npm 経由の指定が
# 「1 フォントだけ測って黙って終わる」形に化ける（実測）。
$fontList = @($Fonts | ForEach-Object { $_ -split ';' } | Where-Object { $_.Trim() } | ForEach-Object { $_.Trim() })

$rows = @()
foreach ($font in $fontList) {
    $slug = ($font -replace '[^A-Za-z0-9]', '')
    $profileDir = Join-Path $outDir "profile-$slug"
    $additional = @"
[visual]
background_color = "$backgroundColor"
input_background_color = "$inputBackgroundColor"
font_family = "$font"
"@
    $profile = New-SnotraVerificationProfile -ProfileDir $profileDir `
        -GeneralSection 'show_on_startup = true' -AdditionalSections $additional -ShowIcons $false
    $stderr = Join-Path $profileDir 'stderr.log'

    $proc = $null
    $capture = $null
    $row = [ordered]@{ Font = $font; Dpi = '-'; Inner = '-'; Caret = '-'; Top = '-'; Bottom = '-'; Skew = '-' }
    try {
        $proc = Start-SnotraProcess -ConfigDir $profile.FullPath -FilePath $exe `
            -StandardErrorPath $stderr -NoNewWindow
        $hwnd = Wait-SnotraWindow -Title 'Snotra' -Process $proc -TimeoutMs 120000 -PollMs 500
        # **窓が出た時点で読む。** 幾何の測定が失敗した行にも倍率は残したい——「測れなかった」と
        # 「別の倍率で測った」を後から区別できるようにするため。
        $row.Dpi = Get-SnotraWindowDpi -Handle $hwnd
        # 最初の present を跨ぐ（未描画の窓を撮ると塗りが 1 行も見つからない）。
        Start-Sleep -Seconds 2
        if (Set-SnotraForegroundWindow -Handle $hwnd) {
            # VK_A..VK_Z は ASCII 大文字と同値。押下と解放を分けて送る（`Send-SnotraKey` の形）。
            foreach ($ch in $Query.ToUpperInvariant().ToCharArray()) {
                $vk = [byte][int][char]$ch
                Send-SnotraKey -VirtualKey $vk
                Start-Sleep -Milliseconds 30
                Send-SnotraKey -VirtualKey $vk -Up
                Start-Sleep -Milliseconds 30
            }
            Start-Sleep -Milliseconds 400
        }

        # **キャレットの点滅を跨ぐため撮り直す。** 消灯の瞬間は「中身が無い」として現れるので、
        # 1 枚で結論すると `Caret` が欠けた表になる。
        for ($attempt = 0; $attempt -lt 8; $attempt++) {
            $capture = Get-SnotraWindowCapture -Handle $hwnd
            $bounds = Find-SnotraFieldBounds -FillHits (Get-FillHitsPerRow -Capture $capture) -MinHits 100
            if ($bounds) {
                # 塗りの左右端の 1px 内側だけを探す（枠線を外す・`Get-FillColumnRange` の doc）。
                $span_x = Get-FillColumnRange -Capture $capture -Row $bounds.InnerTop
                $column = if ($span_x) {
                    Get-CaretColumnHits -Capture $capture -Bounds $bounds -MinRun 15 `
                        -FromX ($span_x.First + 1) -ToX ($span_x.Last - 1)
                } else { $null }
                if ($column) {
                    Write-Verbose "caret x=$($column.X) fill=$($span_x.First)..$($span_x.Last) inner=$($bounds.InnerTop)..$($bounds.InnerBottom)"
                    $span = Find-SnotraSpan -Hits $column.Hits -Offset $bounds.InnerTop
                    $m = Get-SnotraVerticalMargins -Field $bounds -Span $span
                    $row.Inner = $m.InnerHeight
                    $row.Caret = $m.Height
                    $row.Top = $m.Top
                    $row.Bottom = $m.Bottom
                    $row.Skew = $m.Skew
                    if ($KeepShots) {
                        $capture.Bitmap.Save((Join-Path $outDir "$slug.png"), [System.Drawing.Imaging.ImageFormat]::Png)
                    }
                    break
                }
            }
            $capture.Bitmap.Dispose()
            $capture = $null
            Start-Sleep -Milliseconds 150
        }
    } finally {
        if ($capture) { $capture.Bitmap.Dispose() }
        if ($proc) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
    }
    $rows += [pscustomobject]$row
}

Write-Host ''
Write-Host '入力欄の縦の幾何（Dpi = 窓の載るモニタの実効 DPI・96 が 100%。Skew = 上余白 − 下余白で符号が寄った向きを表す）'
$rows | Format-Table -AutoSize | Out-String -Width 200 | Write-Host
Write-Host '判定は出さない。値の意味と読み方は scripts/lib/SnotraInputMetrics.psm1 の doc を参照。'
if ($KeepShots) { Write-Host "スクリーンショット: $outDir" }
