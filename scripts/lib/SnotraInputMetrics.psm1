#Requires -Version 7
<#
.SYNOPSIS
  main 窓の入力欄の**縦の幾何**を、スクリーンショットの実ピクセルから導く純関数群。

.DESCRIPTION
  Rust 側の検査（`view.rs` の `input_text_sits_vertically_centered_for_both_body_and_hint`）は
  egui の**論理座標**で galley の位置を縛る。そこを通らないものが 3 つある: 実機のフォント解決・
  softbuffer のラスタライズ（`raster.rs` はカバレッジ AA を持たない）・DPI。**その 3 つを通した
  結果は、実ピクセルを数えるしか見る方法が無い。**

  2026-08-11 の実測がこの道具の由来である。`vertical_align(Center)` を入れた後、論理座標では
  8 フォントすべてがぴたりと対称だったのに、実機ではキャレットが 8 種中 4 種で 1px ずれていた
  （Windows 既定の Yu Gothic UI と Segoe UI が両方ずれる側）。**乖離そのものが発見**であり、
  代理（論理座標）をいくら精密にしても届かない領域だった。

  **判定（exit code）を持たない。** フォント依存の 1px は構造的に消せず（#1045 に経緯と、判定式が
  作れないことの実測がある）、閾値を置けば「常に緑」か「常に赤」のどちらかに倒れる——
  **永久に赤いゲートはゲートが無いのと機能的に同じである**（`SnotraWindowColors.psm1` が #953 で
  同じ理由から旧述語を捨てたのと同型）。ここが出すのは表であり、読むのは人間である。

  ゆえに**毎作業では走らせない**。使う契機は次のいずれか:

  - egui / epaint を上げたとき（丸めの挙動が変われば全フォントの値が動く）
  - 入力欄の描画・行高・フォント登録に触ったとき
  - 「見え方が変わった気がする」と報告を受けたとき（前後で撮って数値で突き合わせる）

  **I/O は呼び出し側が持つ**（`scripts/visual-input-metrics.ps1`）。ここに置くのは、集計済みの
  数値配列から幾何を導く部分だけである——`SnotraWindowColors.psm1` と同じ分離（#953）。
#>

Set-StrictMode -Version Latest

<#
.SYNOPSIS
  行ごとの「入力欄の塗り色の画素数」から、欄の内側の行範囲を導く。
.DESCRIPTION
  **窓が宣言した色で探す**——`input_background_color` は config が宣言する値であり、欄の内側は
  その塗りが横一列に長く並ぶ。枠線（focus リング）は塗り色ではないので自然に外れ、**egui 側の
  既定色が変わっても壊れない**。`SnotraWindowColors.psm1` が「宣言された色が届いているか」を
  見るのと同じ思想である（#953）。

  ゆえに返すのは**内側**そのものであって、枠線の位置ではない。中身（文字・キャレット・IME 帯）
  の余白はこの内側に対して測る。

  塗りの行が 1 つも無いときは `$null` を返す。**窓が出ていない・色の指定が食い違っている**の
  どちらかであり、呼び出し側はそれを「余白 0」と混同してはならない。

  **文字やキャレットが乗る行も塗りを含む**——それらは欄の一部を覆うだけで、行全体を潰しはしない。
  `MinHits` は「欄の幅のうち何 px が塗りであれば欄の行と見なすか」であり、欄幅よりずっと小さく
  取ってよい（文字で埋まった行を落とさないため）。
#>
function Find-SnotraFieldBounds {
    param(
        [Parameter(Mandatory)][AllowEmptyCollection()][int[]]$FillHits,
        [Parameter(Mandatory)][int]$MinHits
    )

    $rows = @()
    for ($i = 0; $i -lt $FillHits.Count; $i++) {
        if ($FillHits[$i] -ge $MinHits) { $rows += $i }
    }
    if ($rows.Count -lt 1) { return $null }

    [pscustomobject]@{
        InnerTop    = $rows[0]
        InnerBottom = $rows[-1]
    }
}

<#
.SYNOPSIS
  「その行に対象の画素が在るか」の真偽列から、対象の縦範囲を導く。
.DESCRIPTION
  返すのは**最初と最後**であって、連続する最初の塊ではない。測る対象（文字列の galley・
  IME の帯）は途中に隙間を持つ——文字と文字の間、字形の凹み——ので、塊で採ると範囲が縮む。

  `Offset` は真偽列の先頭が画像の何行目に当たるかである（内側だけを走査した配列を渡す想定）。

  1 つも真が無ければ `$null` を返す。**キャレットは点滅する**ので、これは「消えている瞬間を
  撮った」ことを意味しうる。呼び出し側は撮り直しで対処すること（不在を結論にしない）。
#>
function Find-SnotraSpan {
    param(
        [Parameter(Mandatory)][AllowEmptyCollection()][bool[]]$Hits,
        [int]$Offset = 0
    )

    $start = -1
    $end = -1
    for ($i = 0; $i -lt $Hits.Count; $i++) {
        if ($Hits[$i]) {
            if ($start -lt 0) { $start = $i }
            $end = $i
        }
    }
    if ($start -lt 0) { return $null }

    [pscustomobject]@{
        Top    = $start + $Offset
        Bottom = $end + $Offset
    }
}

<#
.SYNOPSIS
  入力欄の内側に対する、中身の上下余白を求める。
.DESCRIPTION
  `Skew` は「上余白 − 下余白」である。**符号を保つ**——0 との距離だけを見ると、上に寄ったのか
  下に寄ったのかが消える。2026-08-11 の実測では、修正前が上 0 / 下 8（上に貼り付き）、修正後が
  上 3 / 下 2（わずかに下寄り）で、**同じ「ずれ 1px」でも向きが逆**だった。

  `InnerHeight` と `Height` の偶奇が食い違うと、余りを等分できないため `Skew` は 0 になれない。
  これはフォントに依存する構造的な残余であって欠陥ではない（#1045）。
#>
function Get-SnotraVerticalMargins {
    param(
        [Parameter(Mandatory)]$Field,
        [Parameter(Mandatory)]$Span
    )

    $top = $Span.Top - $Field.InnerTop
    $bottom = $Field.InnerBottom - $Span.Bottom

    [pscustomobject]@{
        Top         = $top
        Bottom      = $bottom
        Skew        = $top - $bottom
        Height      = $Span.Bottom - $Span.Top + 1
        InnerHeight = $Field.InnerBottom - $Field.InnerTop + 1
    }
}

Export-ModuleMember -Function @(
    'Find-SnotraFieldBounds'
    'Find-SnotraSpan'
    'Get-SnotraVerticalMargins'
)
