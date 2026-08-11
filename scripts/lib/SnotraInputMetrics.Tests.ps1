BeforeAll {
    Import-Module (Join-Path $PSScriptRoot 'SnotraInputMetrics.psm1') -Force

    # 2026-08-11 の実測を形にした行ヒット列（欄の内側 27 行・幅 600px）。
    # 塗りは内側の全行に在り、**文字やキャレットが乗る行では画素数が減る**（覆われる分）。
    function FillHits {
        param([int]$InnerTop, [int]$InnerBottom, [int]$Length)
        $hits = @(0) * $Length
        for ($i = $InnerTop; $i -le $InnerBottom; $i++) { $hits[$i] = 580 }
        # 文字とキャレットが乗る帯は、その分だけ塗りが削られる。
        for ($i = $InnerTop + 3; $i -le $InnerBottom - 2; $i++) { $hits[$i] = 500 }
        $hits
    }
}

Describe 'Find-SnotraFieldBounds' {
    It '塗りが並ぶ行の範囲を欄の内側とする' {
        $bounds = Find-SnotraFieldBounds -FillHits (FillHits -InnerTop 481 -InnerBottom 507 -Length 600) -MinHits 100

        $bounds.InnerTop | Should -Be 481
        $bounds.InnerBottom | Should -Be 507
    }

    It '中身で覆われた行が閾値を割っても、範囲は縮まない（最初と最後で決まる）' {
        # 文字とキャレットが乗る行は塗りが 500 まで減る。**間が抜けても外側 2 行が残る限り
        # 範囲は同じ**——これは仕様であって妥協ではない。行ごとに連続性を要求すると、
        # 文字が端から端まで伸びた 1 行で欄が 2 つに割れて見える。
        $loose = Find-SnotraFieldBounds -FillHits (FillHits -InnerTop 481 -InnerBottom 507 -Length 600) -MinHits 100
        $tight = Find-SnotraFieldBounds -FillHits (FillHits -InnerTop 481 -InnerBottom 507 -Length 600) -MinHits 550

        ($loose.InnerBottom - $loose.InnerTop + 1) | Should -Be 27
        ($tight.InnerBottom - $tight.InnerTop + 1) | Should -Be 27
    }

    It '閾値が塗りの実数を超えると、欄そのものを見失う' {
        # 上下端の行すら 580 しかない。601 を要求すれば 1 行も残らない——**「見失った」は
        # `$null` として現れ、余白 0 とは区別される**（呼び出し側が撮り直す契機になる）。
        Find-SnotraFieldBounds -FillHits (FillHits -InnerTop 481 -InnerBottom 507 -Length 600) -MinHits 601 |
            Should -BeNullOrEmpty
    }

    It '塗りが 1 行も無ければ null を返す（窓が出ていない・色の指定が食い違う）' {
        Find-SnotraFieldBounds -FillHits (@(0) * 100) -MinHits 100 | Should -BeNullOrEmpty
    }
}

Describe 'Find-SnotraSpan' {
    It '最初と最後を採る（途中の隙間で範囲を縮めない）' {
        # 文字列の galley は字間で途切れる。塊で採ると範囲が縮む。
        $hits = @($false, $true, $true, $false, $false, $true, $false)

        $span = Find-SnotraSpan -Hits $hits
        $span.Top | Should -Be 1
        $span.Bottom | Should -Be 5
    }

    It 'Offset を画像座標へ足す' {
        $span = Find-SnotraSpan -Hits @($false, $true, $true) -Offset 481

        $span.Top | Should -Be 482
        $span.Bottom | Should -Be 483
    }

    It '1 つも真が無ければ null を返す（キャレットの消灯を余白 0 と混同させない）' {
        Find-SnotraSpan -Hits @($false, $false) | Should -BeNullOrEmpty
    }
}

Describe 'Get-SnotraVerticalMargins' {
    BeforeAll {
        # 実測（Segoe UI・欄の内側 27 行・キャレット 22 行）: 上 3 / 下 2。
        $script:field = [pscustomobject]@{ InnerTop = 466; InnerBottom = 492 }
    }

    It '上下余白と高さを求める' {
        $m = Get-SnotraVerticalMargins -Field $script:field -Span ([pscustomobject]@{ Top = 469; Bottom = 490 })

        $m.Top | Should -Be 3
        $m.Bottom | Should -Be 2
        $m.Height | Should -Be 22
        $m.InnerHeight | Should -Be 27
    }

    It 'Skew は符号を保つ（上に寄ったか下に寄ったかを潰さない）' {
        # 修正前の実測（上に貼り付き）と修正後（わずかに下寄り）は、同じ 1px でも向きが逆である。
        $down = Get-SnotraVerticalMargins -Field $script:field -Span ([pscustomobject]@{ Top = 469; Bottom = 490 })
        $up = Get-SnotraVerticalMargins -Field $script:field -Span ([pscustomobject]@{ Top = 468; Bottom = 489 })

        $down.Skew | Should -Be 1
        $up.Skew | Should -Be -1
    }

    It '偶奇が揃えば Skew は 0 になる（HackGen Console の実測 4/4）' {
        $m = Get-SnotraVerticalMargins -Field $script:field -Span ([pscustomobject]@{ Top = 470; Bottom = 488 })

        $m.Top | Should -Be 4
        $m.Bottom | Should -Be 4
        $m.Skew | Should -Be 0
        $m.Height | Should -Be 19
    }
}
