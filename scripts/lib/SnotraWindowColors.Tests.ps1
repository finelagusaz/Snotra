BeforeAll {
    Import-Module (Join-Path $PSScriptRoot 'SnotraWindowColors.psm1') -Force

    # 色キーはスクリプト側と同じ形（R<<16 | G<<8 | B）。テストが独自の形を持たないよう
    # モジュールの変換器を通す。
    function Key([string]$Hex) { ConvertTo-SnotraColorKey -Hex $Hex }

    # 宣言 1 件。`Label` は失敗メッセージが色を**名指し**するために要る
    # （`docs/development-principles.md`「構造的設計原則と強制の階梯」の「守りたい要素は名指しする」）。
    function Decl([string]$Label, [string]$Hex, [double]$Floor) {
        @{ Label = $Label; Key = (Key $Hex); Floor = $Floor }
    }
}

Describe 'Test-SnotraWindowColors' {
    Context 'main 窓の形（2 色を宣言する）' {
        # #953 の実測分布そのもの: 入力欄 43.1% が最頻で、背景 33.3% は 2 位。
        # **旧述語（最頻色 == 期待色）はこの分布で必ず赤になる。**
        BeforeEach {
            $script:MainHistogram = @{
                (Key '#383838') = 18159   # 入力欄（最頻）
                (Key '#4A2B5C') = 14044   # 背景
                (Key '#373737') = 4776    # 入力欄の縁
                (Key '#5A6169') = 1388
                (Key '#C0DEFF') = 1359
                (Key '#101010') = 2386    # 端数（列挙 5 色の残余。合計を 42112 = 752x56 に合わせる）
            }
            $script:MainTotal = 42112
            $script:MainDeclared = @(
                (Decl 'background' '#4A2B5C' 0.15)
                (Decl 'input_bg'   '#383838' 0.15)
            )
        }

        It '宣言色が最頻でなくとも下限以上なら緑になる（#953 が直ったことの証明）' {
            $r = Test-SnotraWindowColors -Histogram $script:MainHistogram `
                -TotalPixels $script:MainTotal -Declared $script:MainDeclared
            $r.Ok | Should -BeTrue
        }

        It '最頻色を診断として返す（判定には使わないが、赤のときの手がかりになる）' {
            $r = Test-SnotraWindowColors -Histogram $script:MainHistogram `
                -TotalPixels $script:MainTotal -Declared $script:MainDeclared
            $r.Mode | Should -Be (Key '#383838')
            $r.ModeShare | Should -BeGreaterThan 0.43
        }

        It '宣言色が不在（0 px）なら赤になり、その色を名指しする' {
            $histogram = $script:MainHistogram.Clone()
            $histogram.Remove((Key '#4A2B5C'))       # 背景が届かなかった
            $histogram[(Key '#282828')] = 14044      # 代わりに runtime 既定色が出る

            $r = Test-SnotraWindowColors -Histogram $histogram `
                -TotalPixels $script:MainTotal -Declared $script:MainDeclared

            $r.Ok | Should -BeFalse
            $failed = @($r.Colors | Where-Object { -not $_.Ok })
            $failed.Count | Should -Be 1
            $failed[0].Label | Should -Be 'background'
            $failed[0].Share | Should -Be 0        # 「不在」は 0 として読める
        }

        It '宣言色が在るが下限未満なら赤になり、実測占有率を返す' {
            $histogram = $script:MainHistogram.Clone()
            $histogram[(Key '#4A2B5C')] = 2000      # 42112 の約 4.7%（下限 15% 未満）

            $r = Test-SnotraWindowColors -Histogram $histogram `
                -TotalPixels $script:MainTotal -Declared $script:MainDeclared

            $r.Ok | Should -BeFalse
            $failed = @($r.Colors | Where-Object { -not $_.Ok })
            $failed[0].Label | Should -Be 'background'
            # **不在（0）と「在るが足りない」を数値で読み分けられること**が、
            # 理由分類器を置かずに済ませている根拠である。
            $failed[0].Share | Should -BeGreaterThan 0
            $failed[0].Share | Should -BeLessThan 0.15
        }
    }

    Context 'results 窓の形（1 色を宣言する）' {
        # **旧述語より弱くなっていないことを固定する。** 「最頻である」は「下限以上」を含意するが
        # 逆は成り立たないため、下限をグローバル固定にすると現に動いている検出器が緩む。
        It '宣言色が最頻であっても下限を割れば赤になる（旧述語より強いことの証明）' {
            # **この形だけが新旧の強度差を discriminate する。** 旧述語（最頻色 == 期待色）なら
            # 宣言色が最頻なので**緑**になり、新述語は下限 50% を割るので**赤**になる。
            # 「非最頻かつ下限未満」の形では旧述語も赤なので、弱めていないことの証明にならない。
            $histogram = @{
                (Key '#4A2B5C') = 400    # 最頻。だが 40% で下限 50% に届かない
                (Key '#111111') = 300
                (Key '#222222') = 300
            }
            $declared = @( (Decl 'background' '#4A2B5C' 0.50) )

            $r = Test-SnotraWindowColors -Histogram $histogram -TotalPixels 1000 -Declared $declared

            $r.Mode | Should -Be (Key '#4A2B5C')   # 最頻であることを固定してから
            $r.Ok | Should -BeFalse                 # 下限で落ちることを見る
        }

        It '宣言色が非最頻かつ下限未満でも赤になる（下限が尊重されること）' {
            $histogram = @{
                (Key '#4A2B5C') = 100    # 10%
                (Key '#111111') = 90
                (Key '#222222') = 90
                (Key '#333333') = 720    # 最頻
            }
            $declared = @( (Decl 'background' '#4A2B5C' 0.50) )

            $r = Test-SnotraWindowColors -Histogram $histogram -TotalPixels 1000 -Declared $declared

            $r.Mode | Should -Be (Key '#333333')
            $r.Ok | Should -BeFalse
        }

        It '下限を超えていれば緑になる' {
            $histogram = @{ (Key '#4A2B5C') = 761; (Key '#333333') = 239 }
            $declared = @( (Decl 'background' '#4A2B5C' 0.50) )

            (Test-SnotraWindowColors -Histogram $histogram -TotalPixels 1000 -Declared $declared).Ok |
                Should -BeTrue
        }
    }

    Context '自明に緑になる抜け道を塞ぐ' {
        It '宣言が空なら throw する（空集合は ∀ を自明に満たす）' {
            { Test-SnotraWindowColors -Histogram @{ 1 = 1 } -TotalPixels 1 -Declared @() } |
                Should -Throw -ExpectedMessage '*宣言*'
        }

        It '宣言色が重複していれば throw する（2 色が 1 色へ潰れた状態を通さない）' {
            # `-Color` に入力欄色と同じ値を渡すと起きる。**規範ではなく機構で止める。**
            $declared = @(
                (Decl 'background' '#7A1F1F' 0.15)
                (Decl 'input_bg'   '#7A1F1F' 0.15)
            )
            { Test-SnotraWindowColors -Histogram @{ 1 = 1 } -TotalPixels 1 -Declared $declared } |
                Should -Throw -ExpectedMessage '*重複*'
        }

        It '総画素が 0 なら throw する（占有率が定義できない）' {
            $declared = @( (Decl 'background' '#4A2B5C' 0.50) )
            { Test-SnotraWindowColors -Histogram @{} -TotalPixels 0 -Declared $declared } |
                Should -Throw -ExpectedMessage '*総画素*'
        }

        It '下限が 0 なら throw する（測るが決して落ちない宣言を作らない）' {
            # 「宣言 0 件」「宣言色の重複」と同じ型の抜け道。**3 つとも機構で塞ぐ。**
            $declared = @( @{ Label = 'background'; Key = (Key '#4A2B5C'); Floor = 0.0 } )
            { Test-SnotraWindowColors -Histogram @{ 1 = 1 } -TotalPixels 1 -Declared $declared } |
                Should -Throw -ExpectedMessage '*下限*'
        }

        It '下限が 1 を超えるなら throw する（決して満たせない宣言も飾りである）' {
            $declared = @( @{ Label = 'background'; Key = (Key '#4A2B5C'); Floor = 1.5 } )
            { Test-SnotraWindowColors -Histogram @{ 1 = 1 } -TotalPixels 1 -Declared $declared } |
                Should -Throw -ExpectedMessage '*下限*'
        }
    }
}

Describe 'Get-SnotraDeclaredColors' {
    # **配備される下限をここで測る。** 述語のテストが固定するのは「渡された下限を尊重すること」
    # だけであり、**スクリプトが今も 0.50 を渡すこと**は別の命題である。宣言をモジュールへ
    # 置いたのは、この 2 つ目を Pester の視界へ入れるためである（`run-pester.ps1` は
    # `scripts/lib` しか走査しない）。
    It 'results の下限は 50% を下回らない（旧述語 = 最頻 と同等以上の強度を配備値で固定する）' {
        $d = @(Get-SnotraDeclaredColors -Window results -BackgroundKey 1)
        $d.Count | Should -Be 1
        $d[0].Label | Should -Be 'background'
        $d[0].Floor | Should -BeGreaterOrEqual 0.50
    }

    It 'main は背景と入力欄の 2 色を宣言し、下限はどちらも 15% を下回らない' {
        $d = @(Get-SnotraDeclaredColors -Window main -BackgroundKey 1 -InputBackgroundKey 2)
        $d.Count | Should -Be 2
        @($d.Label) | Should -Be @('background', 'input_bg')
        foreach ($c in $d) { $c.Floor | Should -BeGreaterOrEqual 0.15 }
    }

    It 'main の宣言に入力欄キーを渡さなければ throw する' {
        { Get-SnotraDeclaredColors -Window main -BackgroundKey 1 } |
            Should -Throw -ExpectedMessage '*InputBackgroundKey*'
    }

    It '配備される宣言はそのまま述語を通る（配備値と述語の契約が食い違わない）' {
        $d = @(Get-SnotraDeclaredColors -Window main -BackgroundKey 1 -InputBackgroundKey 2)
        { Test-SnotraWindowColors -Histogram @{ 1 = 100 } -TotalPixels 100 -Declared $d } |
            Should -Not -Throw
    }
}

Describe 'ConvertTo-SnotraColorKey' {
    It '#RRGGBB を R<<16|G<<8|B へ変換する' {
        ConvertTo-SnotraColorKey -Hex '#4A2B5C' | Should -Be 0x4A2B5C
    }

    It '3 桁 hex を展開して受理する（#680 の 1・`-Color ''#FFF''` の回帰検査を兼ねる）' {
        ConvertTo-SnotraColorKey -Hex '#FFF' | Should -Be 0xFFFFFF
    }

    It '先頭の # を欠いても受理する' {
        ConvertTo-SnotraColorKey -Hex '4A2B5C' | Should -Be 0x4A2B5C
    }

    It '桁数が不正なら throw する' {
        { ConvertTo-SnotraColorKey -Hex '#12345' } | Should -Throw -ExpectedMessage '*#RGB*'
    }
}
