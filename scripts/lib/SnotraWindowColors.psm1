#Requires -Version 7
<#
.SYNOPSIS
  窓の色ヒストグラムに対し、**その窓が見せねばならない色がすべて届いているか**を判定する（#953）。

.DESCRIPTION
  旧判定は「窓全体の**最頻色**が期待色と一致するか」だった。その前提——「背景は窓で最も広い色」
  ——は main 窓で**偽**である: **入力欄の塗りが背景より広い**（実測 37.6% > 33.3%）。ゆえに main は
  どんな背景色を指定しても赤になり、**永久に赤いゲートはゲートが無いのと機能的に同じ**だった。

  本モジュールは判定を**存在検査**へ移す: 窓が「見せねばならない色」を**宣言**し、宣言された色が
  それぞれ下限占有率以上あるかを見る。

  **ランキング（上位 N 色）を採らない。** ランキングは presence の proxy であり、判定を 2 つの
  独立した量の**相対**面積へ結びつける——`bar_padding` / `font_size` / status 行の有無は順位を
  覆しうるが、**そのとき両色は完全に届いている**。捕まえたい失敗（色が届かない）は「順位が下がる」
  ではなく**不在（0 px）**として現れる。`docs/development-principles.md`
  「構造的設計原則と強制の階梯」の「**守りたい要素は名指しする**」「∀ 条件には ∃ 条件を組にする」。

  **下限は宣言ごとに持つ（グローバル固定にしない）。** 旧述語「最頻である」は「下限以上」を
  **含意する**が、逆は成り立たない。下限を一律の小さな値にすると、results で背景が 10% しか
  届かない状態が**旧判定では赤・新判定では緑**になり、**現に動いている検出器を弱める**——#953 の
  欠陥のちょうど裏返しである。宣言へ載せれば、データ駆動のまま窓ごとの強度を保てる。

  **判定できるのはここまでである（受容する残余）。** これは位置に紐付かない存在検査なので、
  **背景領域と入力欄領域で色が入れ替わる「クロス配線」を検出できない**——両色とも下限を超えて
  緑になる。位置に紐付ける案は、入力欄の矩形を検査側で導く（＝製品のレイアウト計算の写しを作る）
  ことになるため採らない。詳細は `docs/adr/ADR-declared-colors-over-modal-color.md`。
#>

Set-StrictMode -Version Latest

<#
.SYNOPSIS
  `#RGB` / `#RRGGBB`（先頭の `#` は任意）を `R<<16 | G<<8 | B` の int キーへ変換する。
.DESCRIPTION
  3 桁の展開を持つのは `-Color '#FFF'` が受理されることを固定するためである（#680 の 1・
  hex パーサ 2 本立てで `#FFF` が描画色だけ通り下地は既定色へ落ちていた回帰）。
#>
function ConvertTo-SnotraColorKey {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$Hex)

    $h = $Hex.TrimStart('#')
    if ($h.Length -eq 3) { $h = "$($h[0])$($h[0])$($h[1])$($h[1])$($h[2])$($h[2])" }
    if ($h.Length -ne 6) { throw "色は #RGB か #RRGGBB で指定してください: $Hex" }
    return [Convert]::ToInt32($h, 16)
}

<#
.SYNOPSIS
  宣言された色がすべて下限占有率以上あるかを判定する（純関数・Bitmap を受け取らない）。
.PARAMETER Histogram
  色キー（`ConvertTo-SnotraColorKey` の値）→ 画素数。
.PARAMETER TotalPixels
  窓の総画素数。
.PARAMETER Declared
  `@{ Label = '<名前>'; Key = <int>; Floor = <0..1> }` の配列。**Label は失敗メッセージが色を
  名指しするために要る。**
#>
function Test-SnotraWindowColors {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][hashtable]$Histogram,
        [Parameter(Mandatory)][int]$TotalPixels,
        # **`AllowEmptyCollection` を付けて、空を下の throw で弾く。** 付けないと PowerShell が
        # 「Cannot bind argument ... empty collection」で先に落とし、**なぜ空が許されないのか**
        # （空集合は ∀ を自明に満たす）が読者へ届かない。拒否メッセージは理由を持つべきである。
        [Parameter(Mandatory)][AllowEmptyCollection()][array]$Declared
    )

    # **「何も宣言しない窓」を自明な緑にしない。** 空集合は ∀ を自明に満たす
    # （`docs/development-principles.md`「構造的設計原則と強制の階梯」）。
    if ($Declared.Count -eq 0) {
        throw '宣言色が 0 件です。何も宣言しない窓は「何も測らない」ため自明に緑になります。'
    }
    if ($TotalPixels -le 0) {
        throw "総画素が $TotalPixels です。占有率を定義できません。"
    }
    # **2 色が 1 色へ潰れた状態を通さない。** 呼び出し側が `-Color` に入力欄色と同じ値を渡すと
    # 起きる。同値だと「入力欄が自分の色を持たず背景を透過している」誤配線も、背景と入力欄の
    # 取り違えも、ヒストグラム上は「両方届いている」としか読めなくなる。**規範ではなく機構で止める。**
    $dup = @($Declared | Group-Object -Property { $_.Key } | Where-Object { $_.Count -gt 1 })
    if ($dup.Count -gt 0) {
        $hex = ($dup | ForEach-Object { '#{0:X6}' -f [int]$_.Name }) -join ', '
        throw "宣言色が重複しています（$hex）。2 色が 1 色へ潰れた状態では、取り違えも透過も見分けられません。"
    }
    # **決して破れない下限を通さない。** 「宣言 0 件」「宣言色の重複」と同じ型の抜け道であり、
    # `Floor = 0` は「測るが決して落ちない」＝宣言が飾りになる（空集合が ∀ を自明に満たすのと同型）。
    foreach ($d in $Declared) {
        if ($d.Floor -le 0 -or $d.Floor -gt 1) {
            throw "下限が $($d.Floor) です（$($d.Label)）。0 以下の下限は決して破れず、1 超は決して満たせません。"
        }
    }

    $colors = foreach ($d in $Declared) {
        $count = if ($Histogram.ContainsKey($d.Key)) { $Histogram[$d.Key] } else { 0 }
        $share = $count / $TotalPixels
        [pscustomobject]@{
            Label = $d.Label
            Key   = [int]$d.Key
            Hex   = '#{0:X6}' -f [int]$d.Key
            Count = $count
            Share = $share
            Floor = $d.Floor
            Ok    = ($share -ge $d.Floor)
        }
    }

    # 最頻色は**判定に使わない**。赤のとき「代わりに何が広く出ているか」を示す診断である
    # （runtime 既定の `#282828` が出ていれば clear color が届いていない、等）。
    $top = $Histogram.GetEnumerator() | Sort-Object -Property Value -Descending | Select-Object -First 1

    [pscustomobject]@{
        Ok        = (@($colors | Where-Object { -not $_.Ok }).Count -eq 0)
        Colors    = @($colors)
        Mode      = if ($null -ne $top) { [int]$top.Key } else { $null }
        ModeShare = if ($null -ne $top) { $top.Value / $TotalPixels } else { 0.0 }
    }
}

<#
.SYNOPSIS
  窓ごとの宣言（見せねばならない色と、その下限占有率）を返す。
.DESCRIPTION
  **配備される下限をここに置くのは、Pester が測れる場所へ出すためである。** 呼び出し側の
  スクリプトに直書きすると `run-pester.ps1`（`scripts/lib` だけを走査する）の視界の外に出て、
  `0.50` を `0.05` へ書き換えても全テストが緑のまま通る——**現に動いている検出器が静かに緩む**、
  本モジュールが消しに来たのと同じ型の欠陥になる。

  下限の根拠（`npm run check:colors` が実際に印字する値）:
  - main    — 背景と入力欄が窓を分け合う（実測 33.3% / 37.6%）→ それぞれ 15%（2.2〜2.5 倍の余裕）
  - results — 背景が支配的（実測 76.1%・行の塗りは選択行だけで行数に依らない）→ 50%（1.5 倍）
#>
function Get-SnotraDeclaredColors {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][ValidateSet('main', 'results')][string]$Window,
        [Parameter(Mandatory)][int]$BackgroundKey,
        [int]$InputBackgroundKey
    )

    switch ($Window) {
        'main' {
            if (-not $PSBoundParameters.ContainsKey('InputBackgroundKey')) {
                throw 'main の宣言には -InputBackgroundKey が要ります（背景と入力欄の 2 色を測るため）。'
            }
            @(
                @{ Label = 'background'; Key = $BackgroundKey; Floor = 0.15 }
                @{ Label = 'input_bg'; Key = $InputBackgroundKey; Floor = 0.15 }
            )
        }
        'results' {
            @( @{ Label = 'background'; Key = $BackgroundKey; Floor = 0.50 } )
        }
    }
}

Export-ModuleMember -Function @(
    'ConvertTo-SnotraColorKey'
    'Test-SnotraWindowColors'
    'Get-SnotraDeclaredColors'
)
