BeforeAll {
    # 走査するテストが同じパスを写さないよう、モジュールの所在は 1 つの変数に持つ。
    $script:TraceModulePath = Join-Path $PSScriptRoot 'SnotraTraceInvariants.psm1'

    Import-Module (Join-Path $PSScriptRoot 'SnotraSmoke.psm1') -Force
    Import-Module $script:TraceModulePath -Force

    # trace 1 行ぶんの合成。`Read-SnotraTraceEvents` が返す形（`ConvertFrom-Json` の産物）に
    # 合わせる——判定器が受け取るのは常にその形である。
    function New-TraceEvent {
        param([long]$Seq, [string]$Name, [hashtable]$Data = @{})
        [pscustomobject]@{
            seq    = $Seq
            ts_ms  = 1000 + $Seq
            event  = $Name
            data   = [pscustomobject]$Data
        }
    }

    function Get-Verdict {
        param($Result, [int]$SectionId, [string]$Invariant)
        # 区間行の引き方はモジュールのアクセサに任せる（結果ハッシュの形をテストが写さない）。
        $section = Get-SnotraTraceSectionVerdict -Result $Result -SectionId $SectionId
        if ($null -eq $section) { throw "区間 $SectionId が 1 件で見つからない" }
        return $section[$Invariant]
    }

    # 全区間を 1 つに収める既定。区間の帰属そのものを測るテストだけが複数を渡す。
    $script:OneSection = @( @{ Id = 1; Title = '単一区間'; StartSeq = 0 } )
}

Describe 'Get-SnotraTraceInvariantNames' {
    It '返す名前が Overall のキーと過不足なく一致する（呼び出し側の写しを不要にする）' {
        # **判定を 1 つ足したとき、記録・集計・exit code から黙って落ちないための輪。**
        # 呼び出し側（manual-smoke.ps1）はこの一覧を写さずに引く。
        $names = @(Get-SnotraTraceInvariantNames)
        $result = Test-SnotraTraceInvariants -Events @() -Sections $script:OneSection
        $keys = @($result.Overall.Keys)

        @($names | Where-Object { $keys -notcontains $_ }) | Should -BeNullOrEmpty
        @($keys | Where-Object { $names -notcontains $_ }) | Should -BeNullOrEmpty
    }

    It '判定本体が名指しする不変条件と一覧が過不足なく一致する（モジュールのソースを走査する）' {
        # **母集団はこのモジュールのソーステキストそのものである。** 上の It のように
        # 判定結果（`Overall` のキー）を母集団に取る形では足し忘れを検出できない——キーは
        # `$script:Invariants` から作られるので、一覧自身が母集団になる。実際、判定本体へ
        # H6 を足して一覧へ足さない変異を当てても、上の It を含む 41 本すべてが緑のまま
        # 通った（#1008 で実測）。そのとき違反は現に積まれるのに `Get-SnotraTraceFailureCount`
        # は 0 を返し、**スモークが exit 0 で終わって製品の回帰を素通りさせる**。
        # ソースを読めば、判定本体だけが H6 を知っている状態が左右の食い違いとして現れる。
        #
        # **この検査が守らないもの**（`startup.rs` の `count_matches_the_enum_declaration` が
        # 「改名したらこの検査も直す」と自分の脆さを書くのと同じ性質のものを、ここへ列挙する）:
        #
        # - **単一引用符のリテラルで書かれた名前しか見えない。** `Invariant = $invariant` の
        #   ように変数から組む箇所は素通りする（degrade の分岐が現にその形である）。
        # - **見るのは `Invariant = '…'` と `-Invariant '…'` の 2 形だけである。** 違反や
        #   PASS の記録先を別の書き方で増やしたら、この検査も一緒に直す必要がある。
        # - **走査するのはこのモジュール 1 枚だけである。** 判定を別ファイルへ移したら届かない。
        # - **一覧の順序は守らない。** 順序は表示・集計の列順を決めるが、ソースの出現順とは
        #   独立に決めてよい（`Sort-Object -Unique` で集合として比べている）。
        # - **名前と判定の対応は守らない。** H4 の判定が H5 の名前で違反を積んでいても、
        #   両方が一覧に在れば気づかない。守るのは名前の集合の一致だけである。
        # - **コードとコメントを区別しない**（取り逃す側ではなく、誤って拾う側の穴である）。
        #   走査はファイル全文を見るので、`.psm1` のコメントへ `Invariant = 'H6'` の形の
        #   **例示**を書いた瞬間、判定本体に無い名前として偽の FAIL になる。安全側なので
        #   沈黙よりは軽いが、直し方は「例示を別の書き方にする」であってこの検査ではない。
        $source = Get-Content -LiteralPath $script:TraceModulePath -Raw
        # `'*'` は fail-safe が「不変条件を特定できない」ことを表す印であり、名前ではない。
        $judged = @([regex]::Matches($source, "(?:-Invariant\s+|Invariant\s*=\s*)'([^']*)'") |
            ForEach-Object { $_.Groups[1].Value } |
            Where-Object { $_ -ne '*' } |
            Sort-Object -Unique)
        $listed = @(Get-SnotraTraceInvariantNames)

        # 走査が空振りしたら、以下の 2 つの比較は自明に通る（母集団ゼロの偽の緑）。
        $judged.Count | Should -BeGreaterThan 0 -Because 'ソース走査が 1 件も拾わないなら検査そのものが壊れている'
        @($judged | Where-Object { $listed -notcontains $_ }) |
            Should -BeNullOrEmpty -Because '判定本体だけが知る不変条件は FailureCount に数えられず exit 0 になる'
        @($listed | Where-Object { $judged -notcontains $_ }) |
            Should -BeNullOrEmpty -Because '判定本体が名指ししない不変条件は、表で永久に SKIP のまま残る'
    }
}

Describe 'Get-SnotraTraceMarker' {
    It '事象が無ければ 0 を返す（マーカーの初期値）' {
        Get-SnotraTraceMarker -Events @() | Should -Be 0
    }

    It '観測済みの最大 seq を返す' {
        $events = @(
            New-TraceEvent 1 'egui_show:done'
            New-TraceEvent 7 'egui_results:show' @{ rows = 2 }
            New-TraceEvent 4 'egui_results:hide'
        )
        Get-SnotraTraceMarker -Events $events | Should -Be 7
    }
}

Describe 'Test-SnotraTraceInvariants — 正常列' {
    It '閉じた hide 窓・rows>0 の show・hide を挟んだ show で H1/H4/H5 がすべて PASS' {
        $events = @(
            New-TraceEvent 1 'egui_show:done'
            New-TraceEvent 2 'egui_results:show' @{ rows = 3 }
            New-TraceEvent 3 'egui_results:hide'
            New-TraceEvent 4 'egui_hide:done'
            New-TraceEvent 5 'egui_show:done'
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection

        Get-Verdict $r 1 'H1' | Should -Be 'PASS'
        Get-Verdict $r 1 'H4' | Should -Be 'PASS'
        Get-Verdict $r 1 'H5' | Should -Be 'PASS'
        $r.Violations.Count | Should -Be 0
    }
}

Describe 'Test-SnotraTraceInvariants — 故意の違反（フォールトインジェクション）' {
    It 'H1: hide 窓の中に egui_results:show が現れたら FAIL' {
        $events = @(
            New-TraceEvent 1 'egui_show:done'
            New-TraceEvent 2 'egui_hide:done'
            New-TraceEvent 3 'egui_results:show' @{ rows = 3 }
            New-TraceEvent 4 'egui_show:done'
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection

        Get-Verdict $r 1 'H1' | Should -Be 'FAIL'
        $r.Overall.H1 | Should -Be 'FAIL'
        @($r.Violations | Where-Object { $_.Invariant -eq 'H1' }).Count | Should -Be 1
    }

    It 'H4: rows = 0 の egui_results:show は FAIL（件数 0 ⇒ hide の契約違反）' {
        $events = @(
            New-TraceEvent 1 'egui_show:done'
            New-TraceEvent 2 'egui_results:show' @{ rows = 0 }
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection

        Get-Verdict $r 1 'H4' | Should -Be 'FAIL'
        @($r.Violations | Where-Object { $_.Invariant -eq 'H4' }).Count | Should -Be 1
    }

    It 'H5: hide を挟まない連続 egui_results:show は FAIL（swap による二重発火抑止の破れ）' {
        $events = @(
            New-TraceEvent 1 'egui_show:done'
            New-TraceEvent 2 'egui_results:show' @{ rows = 3 }
            New-TraceEvent 3 'egui_results:show' @{ rows = 4 }
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection

        Get-Verdict $r 1 'H5' | Should -Be 'FAIL'
        @($r.Violations | Where-Object { $_.Invariant -eq 'H5' }).Count | Should -Be 1
    }
}

Describe 'Test-SnotraTraceInvariants — hide 側の非対称' {
    It 'egui_results:hide が連続しても H5 は FAIL にならない（hide は要求レベルで無条件に出る）' {
        $events = @(
            New-TraceEvent 1 'egui_show:done'
            New-TraceEvent 2 'egui_results:show' @{ rows = 3 }
            New-TraceEvent 3 'egui_results:hide'
            New-TraceEvent 4 'egui_results:hide'
            New-TraceEvent 5 'egui_results:show' @{ rows = 2 }
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection

        Get-Verdict $r 1 'H5' | Should -Be 'PASS'
        $r.Violations.Count | Should -Be 0
    }

    It '連続する egui_hide:done は窓を打ち直さず、2 つの hide に挟まれた違反が消えない' {
        # `hide_egui_main` は可視性ガードの無い listener からも呼ばれ、egui_hide:done を
        # 無条件に出す。hide ごとに窓を開き直す実装だと、この違反が評価から落ちる。
        $events = @(
            New-TraceEvent 1 'egui_show:done'
            New-TraceEvent 2 'egui_hide:done'
            New-TraceEvent 3 'egui_results:show' @{ rows = 3 }
            New-TraceEvent 4 'egui_hide:done'
            New-TraceEvent 5 'egui_show:done'
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection

        Get-Verdict $r 1 'H1' | Should -Be 'FAIL'
    }
}

Describe 'Test-SnotraTraceInvariants — 区間への帰属' {
    It '区間の境界を跨ぐ H1 違反を落とさず、hide のあった区間へ帰属させる' {
        # 項目 1 で hide し、項目 2 の区間で違反 show が出る。区間ごとに独立評価すると
        # どちらの区間にも収まらず消える（計画 D1 の回帰点）。
        $sections = @(
            @{ Id = 1; Title = '項目 1'; StartSeq = 0 }
            @{ Id = 2; Title = '項目 2'; StartSeq = 2 }
        )
        $events = @(
            New-TraceEvent 1 'egui_show:done'
            New-TraceEvent 2 'egui_hide:done'
            New-TraceEvent 3 'egui_results:show' @{ rows = 3 }
            New-TraceEvent 4 'egui_show:done'
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $sections

        $r.Overall.H1 | Should -Be 'FAIL'
        Get-Verdict $r 1 'H1' | Should -Be 'FAIL'
        Get-Verdict $r 2 'H1' | Should -Be 'SKIP'
        @($r.Violations | Where-Object { $_.Invariant -eq 'H1' })[0].SectionId | Should -Be 1
    }

    It '最初のマーカーより前の事象は擬似区間 0 へ寄せる（捨てない）' {
        $sections = @( @{ Id = 1; Title = '項目 1'; StartSeq = 5 } )
        $events = @(
            New-TraceEvent 1 'egui_show:done'
            New-TraceEvent 2 'egui_results:show' @{ rows = 0 }
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $sections

        Get-Verdict $r 0 'H4' | Should -Be 'FAIL'
        Get-Verdict $r 1 'H4' | Should -Be 'SKIP'
    }
}

Describe 'Test-SnotraTraceInvariants — 判定不能は PASS へ化けない' {
    It '事象が 1 件も無ければ全 SKIP（空を合格と読ませない）' {
        $r = Test-SnotraTraceInvariants -Events @() -Sections $script:OneSection

        Get-Verdict $r 1 'H1' | Should -Be 'SKIP'
        Get-Verdict $r 1 'H4' | Should -Be 'SKIP'
        Get-Verdict $r 1 'H5' | Should -Be 'SKIP'
        $r.Overall.H1 | Should -Be 'SKIP'
    }

    It 'rows が読めない egui_results:show では H4 が SKIP で、理由が Unjudgeable に載る' {
        $events = @(
            New-TraceEvent 1 'egui_show:done'
            New-TraceEvent 2 'egui_results:show' @{}
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection

        Get-Verdict $r 1 'H4' | Should -Be 'SKIP'
        @($r.Unjudgeable | Where-Object { $_.Invariant -eq 'H4' }).Count | Should -Be 1
    }

    It 'main の可視状態が未観測なら H1 は SKIP で、理由が Unjudgeable に載る' {
        $events = @(
            New-TraceEvent 1 'egui_results:show' @{ rows = 2 }
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection

        Get-Verdict $r 1 'H1' | Should -Be 'SKIP'
        @($r.Unjudgeable | Where-Object { $_.Invariant -eq 'H1' }).Count | Should -Be 1
    }

    It '閉じていない hide 窓は、違反が無ければ SKIP（まだ後続が来うる）' {
        $events = @(
            New-TraceEvent 1 'egui_show:done'
            New-TraceEvent 2 'egui_hide:done'
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection

        Get-Verdict $r 1 'H1' | Should -Be 'SKIP'
    }

    It '閉じていない hide 窓でも、違反があれば FAIL（違反はそれ自体で確定する）' {
        # smoke-egui は最後まで hidden で終わるため、この非対称が無いと検出器が丸ごと黙る。
        $events = @(
            New-TraceEvent 1 'egui_show:done'
            New-TraceEvent 2 'egui_hide:done'
            New-TraceEvent 3 'egui_results:show' @{ rows = 3 }
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection

        Get-Verdict $r 1 'H1' | Should -Be 'FAIL'
    }
}

Describe 'Test-SnotraTraceInvariants — 捨てた行があるときの degrade' {
    It 'DroppedLineCount が 0 でなければ PASS が SKIP へ落ちる' {
        $events = @(
            New-TraceEvent 1 'egui_show:done'
            New-TraceEvent 2 'egui_results:show' @{ rows = 3 }
            New-TraceEvent 3 'egui_results:hide'
            New-TraceEvent 4 'egui_hide:done'
            New-TraceEvent 5 'egui_show:done'
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection -DroppedLineCount 1

        Get-Verdict $r 1 'H1' | Should -Be 'SKIP'
        Get-Verdict $r 1 'H4' | Should -Be 'SKIP'
        Get-Verdict $r 1 'H5' | Should -Be 'SKIP'
        @($r.Unjudgeable | Where-Object { $_.Reason -match 'parse' }).Count | Should -BeGreaterThan 0
    }

    It 'DroppedLineCount が 0 でなくても FAIL は FAIL のまま（違反は捨てた行に依存しない）' {
        $events = @(
            New-TraceEvent 1 'egui_show:done'
            New-TraceEvent 2 'egui_results:show' @{ rows = 0 }
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection -DroppedLineCount 3

        Get-Verdict $r 1 'H4' | Should -Be 'FAIL'
        $r.Overall.H4 | Should -Be 'FAIL'
    }
}

Describe 'Test-SnotraTraceInvariants — 壊れた入力で例外を投げない' {
    It 'event が無い行・seq が無い行が混じっても落ちない' {
        $events = @(
            [pscustomobject]@{ seq = 1; data = [pscustomobject]@{} }
            [pscustomobject]@{ event = 'egui_show:done'; data = [pscustomobject]@{} }
            New-TraceEvent 3 'egui_results:show' @{ rows = 2 }
        )
        { Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection } | Should -Not -Throw
    }

    It 'Sections が空でも落ちず、擬似区間 0 へ帰属する' {
        # `Should -Not -Throw` のスクリプトブロック内の代入は外へ届かない（子スコープ）ため、
        # 「落ちないこと」と「帰属先」は別々に測る。
        $events = @( New-TraceEvent 1 'egui_results:show' @{ rows = 0 } )
        { Test-SnotraTraceInvariants -Events $events -Sections @() } | Should -Not -Throw

        $r = Test-SnotraTraceInvariants -Events $events -Sections @()
        Get-Verdict $r 0 'H4' | Should -Be 'FAIL'
    }

    It 'Sections の要素に StartSeq / Id が欠けていても落ちない' {
        $sections = @(
            @{ Title = 'StartSeq が無い' }
            @{ StartSeq = 0; Title = 'Id が無い' }
        )
        $events = @( New-TraceEvent 1 'egui_results:show' @{ rows = 2 } )
        { Test-SnotraTraceInvariants -Events $events -Sections $sections } | Should -Not -Throw
    }
}

Describe 'Test-SnotraTraceInvariants — スキーマドリフトで確定済みの違反を捨てない（C2）' {
    It 'rows が非数値の行が後ろに混じっても、先に確定した FAIL が残る' {
        # 裸のキャストだと `[int]'many'` が例外になり、fail-safe が Violations ごと捨てて
        # exit 0 になっていた。**赤が緑へ化ける**形なのでキャストは必ず変換ヘルパーを通す。
        $events = @(
            New-TraceEvent 1 'egui_show:done'
            New-TraceEvent 2 'egui_results:show' @{ rows = 0 }
            New-TraceEvent 3 'egui_results:hide'
            New-TraceEvent 4 'egui_results:show' @{ rows = 'many' }
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection

        $r.JudgeFailed | Should -BeFalse
        Get-Verdict $r 1 'H4' | Should -Be 'FAIL'
        @($r.Violations | Where-Object { $_.Invariant -eq 'H4' }).Count | Should -Be 1
        @($r.Unjudgeable | Where-Object { $_.Invariant -eq 'H4' }).Count | Should -Be 1
    }

    It 'seq や StartSeq が非数値でも例外にならず、判定できた分は残る' {
        $events = @(
            [pscustomobject]@{ seq = 'まる'; event = 'egui_results:show'; data = [pscustomobject]@{ rows = 0 } }
            New-TraceEvent 2 'egui_show:done'
            New-TraceEvent 3 'egui_results:show' @{ rows = 0 }
        )
        $sections = @( @{ Id = 1; Title = '項目 1'; StartSeq = 'ゼロ' } )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $sections

        $r.JudgeFailed | Should -BeFalse
        $r.Overall.H4 | Should -Be 'FAIL'
        # StartSeq が読めない区間は帰属表から外れるので、違反は擬似区間へ落ちる。
        Get-Verdict $r 0 'H4' | Should -Be 'FAIL'
        Get-Verdict $r 1 'H4' | Should -Be 'SKIP'
    }
}

Describe 'Test-SnotraTraceInvariants — Counts が SKIP の山を隠さない（High-1）' {
    It '1 区間だけ判定できたとき、Overall は PASS でも Counts は SKIP の件数を示す' {
        $sections = @(
            @{ Id = 1; Title = '項目 1'; StartSeq = 0 }
            @{ Id = 2; Title = '項目 2'; StartSeq = 100 }
            @{ Id = 3; Title = '項目 3'; StartSeq = 200 }
        )
        $events = @(
            New-TraceEvent 1 'egui_show:done'
            New-TraceEvent 2 'egui_results:show' @{ rows = 3 }
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $sections

        $r.Overall.H4 | Should -Be 'PASS'
        $r.Counts.H4.PASS | Should -Be 1
        $r.Counts.H4.FAIL | Should -Be 0
        # 擬似区間 0 + 判定できなかった項目 2・3。
        $r.Counts.H4.SKIP | Should -Be 3
    }
}

Describe 'Test-SnotraTraceInvariants — Observed（判定器が実際に何を見たか）' {
    It '評価した egui_results:show と hide 窓の件数を返す（呼び出し側が数え直さないため）' {
        $events = @(
            New-TraceEvent 1 'egui_show:done'
            New-TraceEvent 2 'egui_results:show' @{ rows = 3 }
            New-TraceEvent 3 'egui_results:hide'
            New-TraceEvent 4 'egui_hide:done'
            New-TraceEvent 5 'egui_hide:done'
            New-TraceEvent 6 'egui_show:done'
            New-TraceEvent 7 'egui_results:show' @{ rows = 1 }
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection

        $r.Observed.ResultsShow | Should -Be 2
        # 連続する hide は窓を打ち直さないので 1 つ。
        $r.Observed.HideWindow | Should -Be 1
    }

    It 'イベント名が 1 つも一致しなければ Observed が 0 になる（検査が走らなかったことの証拠）' {
        $events = @(
            New-TraceEvent 1 'egui_show:DONE'
            New-TraceEvent 2 'egui_results:shown' @{ rows = 3 }
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection

        $r.Observed.ResultsShow | Should -Be 0
        $r.Overall.H4 | Should -Be 'SKIP'
    }

    It 'fail-safe でも Observed のキーが揃う（呼び出し側が分岐せずに読める）' {
        $r = Test-SnotraTraceInvariants -Events @() -Sections $script:OneSection
        $r.Observed.ResultsShow | Should -Be 0
        $r.Observed.HideWindow | Should -Be 0
    }
}

Describe 'Format-SnotraTraceCountSummary' {
    It '通常形と Compact 形の両方が全不変条件を含む（呼び出し側が整形を写さないため）' {
        $r = Test-SnotraTraceInvariants -Events @() -Sections $script:OneSection
        $full = Format-SnotraTraceCountSummary -Result $r
        $compact = Format-SnotraTraceCountSummary -Result $r -Compact

        foreach ($name in (Get-SnotraTraceInvariantNames)) {
            $full | Should -BeLike "*$name*"
            $compact | Should -BeLike "*$name*"
        }
        $full | Should -BeLike '*SKIP*'
        $compact.Length | Should -BeLessThan $full.Length
    }
}

Describe 'Read-SnotraTraceSnapshot（捨てた行の数え方）' {
    It '非 trace の診断行を「捨てた行」に数えない（数えると正常な実行が毎回 degrade する）' {
        # 実ログ（#757 で実測）にはこの形の行が混じる。素朴に「全行 − parse 成功」で数えると
        # 捨てた行が常に 1 以上になり、PASS が毎回 SKIP へ落ちて検出器が無意味になる。
        $path = Join-Path $TestDrive 'trace.log'
        Set-Content -LiteralPath $path -Encoding UTF8 -Value @(
            '[index-load] cache_hit=true total=785ms hash=0ms'
            '[trace] {"data":{},"event":"egui_show:done","seq":1,"ts_ms":1001}'
            '[trace] {"data":{"rows":2},"event":"egui_results:show","seq":2,"ts_ms":1002}'
        )
        $snapshot = Read-SnotraTraceSnapshot -Path $path

        $snapshot.Available | Should -BeTrue
        $snapshot.TraceLines | Should -Be 2
        $snapshot.Events.Count | Should -Be 2
        $snapshot.Dropped | Should -Be 0
    }

    It '[trace] で始まるのに parse できない行だけを捨てた行として数える' {
        $path = Join-Path $TestDrive 'torn.log'
        Set-Content -LiteralPath $path -Encoding UTF8 -Value @(
            '[trace] {"data":{},"event":"egui_show:done","seq":1,"ts_ms":1001}'
            '[trace] {"data":{"rows":2},"event":"egui_results:sh'
        )
        $snapshot = Read-SnotraTraceSnapshot -Path $path

        $snapshot.TraceLines | Should -Be 2
        $snapshot.Events.Count | Should -Be 1
        $snapshot.Dropped | Should -Be 1
    }

    It 'ファイルが無ければ Available=false で、捨てた行 0 を名乗らない扱いにする' {
        $snapshot = Read-SnotraTraceSnapshot -Path (Join-Path $TestDrive 'missing.log')
        $snapshot.Available | Should -BeFalse
        $snapshot.Events.Count | Should -Be 0
        # 不在は「読み取りの失敗」ではない。両者を同じ値にすると、下の検査が区別を失う。
        $snapshot.ReadError | Should -BeNullOrEmpty
    }

    It '読み取りに失敗したら Available=false と ReadError を立て、「読めた」と言わない（#872）' {
        # ディレクトリは Test-Path が真で Get-Content が落ちる（実測）。`-ErrorAction
        # SilentlyContinue` のままだと Available=true・Events 0 件・Dropped 0 という
        # **正常な空ログと区別できない値**を返し、稼働中のアプリを観測する経路で
        # 「読めなかった」が「まだ出ていない」に化ける。
        $dir = Join-Path $TestDrive 'unreadable-snapshot'
        New-Item -ItemType Directory -Force -Path $dir | Out-Null

        $snapshot = Read-SnotraTraceSnapshot -Path $dir

        $snapshot.Available | Should -BeFalse
        $snapshot.ReadError | Should -Not -BeNullOrEmpty
        $snapshot.Events.Count | Should -Be 0
        $snapshot.Dropped | Should -Be 0
    }
}

Describe 'Get-SnotraTraceFailureCount（赤とみなす状態の定義）' {
    BeforeAll {
        function New-Verdict {
            param([hashtable]$Overall = @{}, [bool]$JudgeFailed = $false)
            $all = @{}
            foreach ($n in (Get-SnotraTraceInvariantNames)) {
                $all[$n] = if ($Overall.ContainsKey($n)) { $Overall[$n] } else { 'SKIP' }
            }
            return @{ Overall = $all; JudgeFailed = $JudgeFailed }
        }
    }

    It 'trace を読めなかったら赤にする（観測不能を成功として終えない）' {
        # #872: `Read-SnotraTraceSnapshot` が読み取り失敗を Available=false へ倒すので、
        # 判定は走らず Verdict は $null になる。**ここで 0 を返すと、権限・共有違反で
        # 読みが落ちた実行が exit 0 で終わる**——この PR が塞いでいる false green そのもの。
        Get-SnotraTraceFailureCount -Verdict $null -ReadError 'Unable to get content' | Should -Be 1
    }

    It 'trace の不在は赤にしない（-NoLaunch や 1 行も出ていない実行）' {
        # 読み取り失敗と同じ扱いにすると、trace を出さない正当な使い方が常に失敗する。
        Get-SnotraTraceFailureCount -Verdict $null -ReadError $null | Should -Be 0
        Get-SnotraTraceFailureCount -Verdict $null -ReadError '' | Should -Be 0
    }

    It '不変条件の FAIL を数える' {
        $names = @(Get-SnotraTraceInvariantNames)
        $verdict = New-Verdict -Overall @{ $names[0] = 'FAIL'; $names[1] = 'FAIL' }
        Get-SnotraTraceFailureCount -Verdict $verdict -ReadError $null | Should -Be 2
    }

    It '判定器の例外も赤にする（例外は欠陥であって無事ではない）' {
        Get-SnotraTraceFailureCount -Verdict (New-Verdict -JudgeFailed $true) -ReadError $null | Should -Be 1
    }

    It '全 SKIP は赤にしない（調べられなかったことを違反として数えない）' {
        Get-SnotraTraceFailureCount -Verdict (New-Verdict) -ReadError $null | Should -Be 0
    }

    It '赤は積み上がる（FAIL と例外と読み取り失敗が互いを隠さない）' {
        $names = @(Get-SnotraTraceInvariantNames)
        $verdict = New-Verdict -Overall @{ $names[0] = 'FAIL' } -JudgeFailed $true
        Get-SnotraTraceFailureCount -Verdict $verdict -ReadError 'boom' | Should -Be 3
    }
}

Describe 'Get-SnotraTraceFailedInvariants' {
    It '母集団を Get-SnotraTraceInvariantNames から引く（呼び出し側が名前を写さない）' {
        # 不変条件を 1 つ足したとき、呼び出し側の写しが黙って取りこぼすのを防ぐ。
        $names = @(Get-SnotraTraceInvariantNames)
        $row = @{}
        foreach ($n in $names) { $row[$n] = 'PASS' }
        $row[$names[-1]] = 'FAIL'

        $failed = @(Get-SnotraTraceFailedInvariants -Verdicts $row)

        $failed.Count | Should -Be 1
        $failed[0] | Should -Be $names[-1]
    }

    It '判定表が無ければ空を返す（呼び出し側に null 分岐を書かせない）' {
        @(Get-SnotraTraceFailedInvariants -Verdicts $null).Count | Should -Be 0
    }
}

Describe 'Format-SnotraTraceVerdictTable' {
    It '区間ごとの判定を markdown の表として返す' {
        $events = @(
            New-TraceEvent 1 'egui_show:done'
            New-TraceEvent 2 'egui_results:show' @{ rows = 0 }
        )
        $r = Test-SnotraTraceInvariants -Events $events -Sections $script:OneSection
        $lines = @(Format-SnotraTraceVerdictTable -Result $r)

        ($lines -join "`n") | Should -Match '\|\s*H1\s*\|'
        ($lines -join "`n") | Should -Match 'FAIL'
    }
}
