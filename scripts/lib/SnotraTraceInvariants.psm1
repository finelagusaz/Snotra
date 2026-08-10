#Requires -Version 7

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# results 窓の trace から**不変条件**を判定する純関数群（#757）。
#
# **presence 検査とは別物である。** 「イベントが出たか」ではなく `src-tauri/CLAUDE.md`
# 「Win32 / Tauri 注意事項」が求める**「起きてはならないことが起きていないか」**を見る。
# #671 PR A′ は `egui_results:hide` が出たのに窓が残った回帰で、presence を見る smoke は
# 緑のまま通した。
#
# | # | 判定 | 何を捕まえるか |
# |---|---|---|
# | H1 | hidden な窓の中に `egui_results:show` が現れたら異常 | main が hidden なのに results が最前面に残る（#671 PR A′） |
# | H4 | `egui_results:show` の `rows` が 0 なら異常 | 「件数 0 ⇒ hide」の契約違反（`layout::present_results` の連言②） |
# | H5 | hide を挟まない連続 `egui_results:show` は異常 | 二重発火抑止（`ResultsWindow.visible` の `swap`）の破れ |
# | H7 | `egui_search:settled` が `dispatch_seq < pending_seq` で現れたら異常 | 失効した検索結果が行を汚す（#1004） |
#
# **判定不能を PASS へ化けさせない**のがこのモジュールの要石である。「該当イベントが無い」
# 「`rows` が読めない」「main の可視状態が未観測」「窓が閉じていない」「parse できなかった
# 行がある」はすべて SKIP であって合格ではない。
#
# 入力は `SnotraSmoke.psm1` の `Read-SnotraTraceEvents` が返す parse 済みオブジェクト列。
# trace 行の書式は `src-tauri/src/trace.rs`。

$script:EventHideDone = 'egui_hide:done'
$script:EventShowDone = 'egui_show:done'
$script:EventResultsShow = 'egui_results:show'
$script:EventResultsHide = 'egui_results:hide'
$script:EventSearchSettled = 'egui_search:settled'
$script:Invariants = @('H1', 'H4', 'H5', 'H7')
$script:PseudoSectionTitle = '(最初の項目より前)'

<#
.SYNOPSIS
判定する不変条件の名前。表示・集計の列順もこれが決める。

.DESCRIPTION
**呼び出し側はこの一覧を写さない**（`/symmetric-check`）。写しを持つと、判定を 1 つ足したとき
モジュール側だけが直り、記録・集計・exit code から新しい不変条件が**黙って落ちる**。

**逆向き——判定本体へ足してこの一覧へ足し忘れた場合——には検査がある。**
`SnotraTraceInvariants.Tests.ps1` の同名 `Describe` にあるソース走査テストが、このモジュールの
ソーステキストから `Invariant` のリテラルを拾って一覧と突き合わせる（#1008）。
**ただし守る範囲は狭い**——名前を変数から組めば見えないなど、その正本は同テストのコメントである。
#>
function Get-SnotraTraceInvariantNames {
    [CmdletBinding()]
    param()
    return $script:Invariants
}

<#
.SYNOPSIS
オブジェクトのプロパティを、存在しなくても落ちずに読む。

.DESCRIPTION
**StrictMode 下で欠落プロパティへ直接触ると `PropertyNotFoundException` になる**（実測）。
`.PSObject.Properties.Name -contains` も空オブジェクトでは例外を出すため、
**indexer（`.PSObject.Properties[$Name]`）だけが安全**である。trace 行のスキーマが
ドリフトしても判定器が落ちないように、プロパティの読みは必ずここを通す。
#>
function Get-SnotraTraceProperty {
    [CmdletBinding()]
    param(
        $InputObject,
        [Parameter(Mandatory)]
        [string]$Name
    )

    if ($null -eq $InputObject) { return $null }
    $property = $InputObject.PSObject.Properties[$Name]
    if ($null -eq $property) { return $null }
    return $property.Value
}

# 数値へ変換できなければ `$null`。**キャストを裸で書かない**——trace のスキーマがドリフト
# して `seq` や `rows` が非数値になったとき、例外が fail-safe まで飛んで**既に確定した違反が
# 捨てられる**（code-review C2）。変換できないことは「判定不能」であって「合格」ではないので、
# 呼び出し側が Unjudgeable として記録する。
function ConvertTo-SnotraTraceInt64 {
    [CmdletBinding()]
    param($Value)

    if ($null -eq $Value) { return $null }
    try { return [long]$Value } catch { return $null }
}

<#
.SYNOPSIS
区間マーカー: これまでに観測した最大の `seq`。事象が無ければ 0。

.DESCRIPTION
**マーカーは操作の「前」に打つ**（#757）。後に打つと直前の操作が前の区間へ紛れ込む。
`seq` は単一の `AtomicU64`（`src-tauri/src/trace.rs`）ゆえ全順序であり、この値より大きい
`seq` を持つ事象が「これ以降」である。
#>
function Get-SnotraTraceMarker {
    [CmdletBinding()]
    param(
        [psobject[]]$Events = @()
    )

    $max = [long]0
    foreach ($event in $Events) {
        $value = ConvertTo-SnotraTraceInt64 (Get-SnotraTraceProperty -InputObject $event -Name 'seq')
        if ($null -eq $value) { continue }
        if ($value -gt $max) { $max = $value }
    }
    return $max
}

<#
.SYNOPSIS
H1 / H4 / H5 / H7 を判定し、違反を区間へ帰属させる。例外は投げない。

.PARAMETER Events
`Read-SnotraTraceEvents` が返す parse 済みオブジェクト列。順序は問わない（`seq` で整列する）。

.PARAMETER Sections
`@( @{ Id = 1; Title = '...'; StartSeq = 0 }, ... )`。`Get-SnotraTraceMarker` の値を
`StartSeq` に入れる。`Id` / `Title` / `StartSeq` の欠落は許容する。

.PARAMETER DroppedLineCount
`[trace]` で始まるのに parse できず捨てた行の数。**stderr に出る非 trace の診断行
（`[index-load] ...` 等）を数えてはならない**——正常な実行が毎回 degrade して検出器が
無意味になる（実測: 実ログ 25 行のうち 1 行が非 trace の診断行だった）。

.DESCRIPTION
**状態機械は trace 全体を 1 パスで舐め、違反イベントを区間へ「帰属」させる。**
区間ごとに独立評価すると境界を跨ぐ違反を落とす——項目 N で hide し項目 N+1 で
`egui_results:show` が出た場合、どちらの区間内にも収まらない。区間マーカーは
**評価の境界ではなく帰属の道具**である。
#>
function Test-SnotraTraceInvariants {
    [CmdletBinding()]
    param(
        [psobject[]]$Events = @(),
        [hashtable[]]$Sections = @(),
        [int]$DroppedLineCount = 0
    )

    try {
        return Invoke-SnotraTraceJudgement -Events $Events -Sections $Sections -DroppedLineCount $DroppedLineCount
    } catch {
        # **判定器が落ちて記録が書けないほうが害が大きい。** ただし黙って消さず、例外の本文を
        # 判定不能の理由として残し、**`JudgeFailed` で呼び出し側に fail-closed させる**——
        # 例外は欠陥であって「問題が無かった」ではない（code-review C2）。
        return New-SnotraTraceFailSafeResult -Sections $Sections -DroppedLineCount $DroppedLineCount `
            -Reason "判定器が例外で停止した: $($_.Exception.Message)"
    }
}

<#
.SYNOPSIS
区間の `Id` / `Title` / `StartSeq` を正規化する（フォールバック規則の SSOT）。

.DESCRIPTION
**fail-safe 経路と判定経路の両方がここを通る。** 片方に写しを置くと、同じ入力から別の
区間表が出るようになり、しかも fail-safe 側を固定するテストが無いので静かに通る。
`StartSeq` が読めない区間は `Attributable = $false`——帰属表から外すが、出力からは外さない。
#>
function ConvertTo-SnotraTraceSectionList {
    [CmdletBinding()]
    param(
        [hashtable[]]$Sections = @()
    )

    $normalized = @()
    $index = 0
    foreach ($section in $Sections) {
        $index++
        $rawId = if ($null -ne $section -and $section.ContainsKey('Id')) { $section['Id'] } else { $null }
        $id = ConvertTo-SnotraTraceInt64 $rawId
        if ($null -eq $id) { $id = $index }
        $title = if ($null -ne $section -and $section.ContainsKey('Title')) { [string]$section['Title'] } else { "項目 $id" }
        $rawStart = if ($null -ne $section -and $section.ContainsKey('StartSeq')) { $section['StartSeq'] } else { $null }
        $start = ConvertTo-SnotraTraceInt64 $rawStart
        $normalized += @{
            Id           = $id
            Title        = $title
            StartSeq     = if ($null -ne $start) { $start } else { [long]0 }
            Attributable = ($null -ne $start)
        }
    }
    return $normalized
}

function New-SnotraTraceFailSafeResult {
    [CmdletBinding()]
    param(
        [hashtable[]]$Sections = @(),
        [int]$DroppedLineCount = 0,
        [Parameter(Mandatory)]
        [string]$Reason
    )

    # **不変条件名も区間の正規化規則も書き並べない**（`$script:Invariants` と
    # `ConvertTo-SnotraTraceSectionList` が SSOT）——写しを置くと、判定を 1 つ足したときや
    # フォールバック規則を変えたときにこの経路だけが取り残される（code-review M2 と同種）。
    function New-SkipRow([int]$Id, [string]$Title) {
        $row = @{ Id = $Id; Title = $Title }
        foreach ($invariant in $script:Invariants) { $row[$invariant] = 'SKIP' }
        return $row
    }

    $rows = @( (New-SkipRow 0 $script:PseudoSectionTitle) )
    foreach ($section in (ConvertTo-SnotraTraceSectionList -Sections $Sections)) {
        $rows += (New-SkipRow $section.Id $section.Title)
    }

    $overall = @{}
    $counts = @{}
    foreach ($invariant in $script:Invariants) {
        $overall[$invariant] = 'SKIP'
        $counts[$invariant] = @{ PASS = 0; FAIL = 0; SKIP = $rows.Count }
    }

    return @{
        Sections         = $rows
        Overall          = $overall
        Counts           = $counts
        Violations       = @()
        # `'*'` は「不変条件を特定できない」印であり名前ではない。**この印はここと
        # `SnotraTraceInvariants.Tests.ps1` のソース走査テスト（除外リテラル）の 2 か所に在る**
        # ——別の文字へ変えるとテスト側だけが古い値を除外し続け、新しい印を不変条件名として
        # 拾って偽の FAIL になる。片方だけが直る写しであり、#1008 が数えて回った型そのものである
        # （安全側に倒れるので沈黙より軽い、という理由で写しのまま残している）。
        Unjudgeable      = @( @{ Invariant = '*'; Seq = 0; SectionId = 0; Reason = $Reason } )
        Observed         = @{ ResultsShow = 0; HideWindow = 0 }
        DroppedLineCount = $DroppedLineCount
        JudgeFailed      = $true
    }
}

function Invoke-SnotraTraceJudgement {
    [CmdletBinding()]
    param(
        [psobject[]]$Events = @(),
        [hashtable[]]$Sections = @(),
        [int]$DroppedLineCount = 0
    )

    # --- 区間の正規化 ---------------------------------------------------------
    # `StartSeq` を持たない区間は帰属表から外すが、**出力からは外さない**（全 SKIP で
    # 現れる）。表から消すと「実施しなかった項目」が記録から蒸発する。
    $normalized = @(ConvertTo-SnotraTraceSectionList -Sections $Sections)

    # 擬似区間 0 = 最初のマーカーより前。ここが無いと起動直後の事象が捨てられる。
    $pseudo = @{ Id = 0; Title = $script:PseudoSectionTitle; StartSeq = [long]::MinValue; Attributable = $true }
    $attributable = @(@($pseudo) + @($normalized | Where-Object { $_.Attributable }) |
        Sort-Object -Property { $_.StartSeq } -Stable)

    # --- 事象の正規化 ---------------------------------------------------------
    # `seq` か `event` が読めない行は順序にも区間にも載せられない。捨てた行と同類として
    # 数え、degrade の入力にする（下の $effectiveDropped）。
    $parsed = @()
    $malformed = 0
    foreach ($event in $Events) {
        $seq = ConvertTo-SnotraTraceInt64 (Get-SnotraTraceProperty -InputObject $event -Name 'seq')
        $name = Get-SnotraTraceProperty -InputObject $event -Name 'event'
        if ($null -eq $seq -or $null -eq $name) {
            $malformed++
            continue
        }
        $parsed += @{ Seq = $seq; Name = [string]$name; Raw = $event }
    }
    $parsed = @($parsed | Sort-Object -Property { $_.Seq } -Stable)
    $effectiveDropped = $DroppedLineCount + $malformed

    # --- 1 パスの状態機械 -----------------------------------------------------
    $mainState = 'unknown'   # unknown / visible / hidden
    $openWindow = $null
    $hideWindows = @()
    $resultsShown = $false
    $violations = @()
    $unjudgeable = @()
    $passCount = @{}
    # **判定器が何を実際に見たか**の帳簿。呼び出し側が生イベントから数え直すと、イベント名の
    # 写しがそちらへ増え、名前がドリフトしたとき「検査が走らなかった」ことを検出する当の
    # assertion が黙る。数えるのは判定した側の責務である。
    $observed = @{ ResultsShow = 0; HideWindow = 0 }

    foreach ($event in $parsed) {
        switch ($event.Name) {
            $script:EventHideDone {
                # **連続する egui_hide:done は窓を打ち直さない——開いている窓を延長する。**
                # `hide_egui_main` は可視性ガードの無い listener からも呼ばれ、遷移を問わず
                # trace を出す。打ち直すと 2 つの hide に挟まれた違反が評価から消える。
                if ($mainState -ne 'hidden') {
                    $openWindow = @{
                        SectionId = (Resolve-SnotraTraceSection -Attributable $attributable -Seq $event.Seq)
                        Seq       = $event.Seq
                        Violated  = $false
                        Closed    = $false
                    }
                    $hideWindows += $openWindow
                    $observed.HideWindow++
                }
                $mainState = 'hidden'
            }
            $script:EventShowDone {
                if ($null -ne $openWindow) {
                    $openWindow.Closed = $true
                    $openWindow = $null
                }
                $mainState = 'visible'
            }
            $script:EventResultsHide {
                # **hide 側は要求レベルゆえ連続してよい**（`hide_egui_main` は遷移して
                # いなくても出す）。H5 の separator としてはどちらの発火源も等価に扱う。
                $resultsShown = $false
            }
            $script:EventResultsShow {
                # 区間の解決はここと hide の 2 分岐でしか要らない（全イベントで呼ばない）。
                $sectionId = Resolve-SnotraTraceSection -Attributable $attributable -Seq $event.Seq
                $observed.ResultsShow++

                # --- H1 ---
                if ($mainState -eq 'hidden') {
                    if ($null -ne $openWindow) {
                        $openWindow.Violated = $true
                        $violations += @{
                            Invariant = 'H1'
                            Seq       = $event.Seq
                            SectionId = $openWindow.SectionId
                            Message   = "main が hidden（seq=$($openWindow.Seq) の $script:EventHideDone 以降）なのに $script:EventResultsShow が出た"
                        }
                    } else {
                        # 到達しない（hidden 遷移が必ず窓を開く）が、**無記録の else を残さない**
                        # ——沈黙経路を作らないと掲げるモジュールの中の抜け穴になる（code-review L1）。
                        $unjudgeable += @{
                            Invariant = 'H1'
                            Seq       = $event.Seq
                            SectionId = $sectionId
                            Reason    = 'main が hidden なのに hide 窓が開いていない（状態機械の不整合）'
                        }
                    }
                } elseif ($mainState -eq 'unknown') {
                    $unjudgeable += @{
                        Invariant = 'H1'
                        Seq       = $event.Seq
                        SectionId = $sectionId
                        Reason    = "main の可視状態が未観測（$script:EventShowDone / $script:EventHideDone をまだ見ていない）"
                    }
                }

                # --- H4 ---
                $data = Get-SnotraTraceProperty -InputObject $event.Raw -Name 'data'
                $rows = ConvertTo-SnotraTraceInt64 (Get-SnotraTraceProperty -InputObject $data -Name 'rows')
                if ($null -eq $rows) {
                    $unjudgeable += @{
                        Invariant = 'H4'
                        Seq       = $event.Seq
                        SectionId = $sectionId
                        Reason    = "$script:EventResultsShow の rows が読めない（欠落か非数値——trace のスキーマが変わった可能性）"
                    }
                } elseif ($rows -le 0) {
                    $violations += @{
                        Invariant = 'H4'
                        Seq       = $event.Seq
                        SectionId = $sectionId
                        Message   = "rows = $rows の $script:EventResultsShow（件数 0 ⇒ hide の契約違反）"
                    }
                } else {
                    Add-SnotraTracePass -PassCount $passCount -Invariant 'H4' -SectionId $sectionId
                }

                # --- H5 ---
                if ($resultsShown) {
                    $violations += @{
                        Invariant = 'H5'
                        Seq       = $event.Seq
                        SectionId = $sectionId
                        Message   = "$script:EventResultsHide を挟まない連続した $script:EventResultsShow（二重発火抑止の破れ）"
                    }
                } else {
                    Add-SnotraTracePass -PassCount $passCount -Invariant 'H5' -SectionId $sectionId
                }
                $resultsShown = $true
            }
            $script:EventSearchSettled {
                $sectionId = Resolve-SnotraTraceSection -Attributable $attributable -Seq $event.Seq

                # --- H7 ---
                # 採り込み時点の pending より古い seq が採られたら、失効の規則が破れている。
                # `pending_seq = 0` は「pending 無し」＝この結果が最新だったことを意味する。
                # `data` の読みは H4 と同じく `Get-SnotraTraceProperty` を経由させる——`$event.Raw.data`
                # を直接ドット参照すると、`data` を持たない行が混じった瞬間 StrictMode で例外になる。
                $data = Get-SnotraTraceProperty -InputObject $event.Raw -Name 'data'
                $dispatchSeq = ConvertTo-SnotraTraceInt64 (Get-SnotraTraceProperty -InputObject $data -Name 'dispatch_seq')
                $pendingSeq = ConvertTo-SnotraTraceInt64 (Get-SnotraTraceProperty -InputObject $data -Name 'pending_seq')
                if ($null -eq $dispatchSeq -or $null -eq $pendingSeq) {
                    $unjudgeable += @{
                        Invariant = 'H7'
                        Seq       = $event.Seq
                        SectionId = $sectionId
                        Reason    = 'dispatch_seq / pending_seq が読めない'
                    }
                } elseif ($pendingSeq -ne 0 -and $dispatchSeq -lt $pendingSeq) {
                    # **`Message` を持つ hashtable で積む**（`[pscustomobject]` ではない）——
                    # `Format-SnotraTraceVerdictTable` は違反を `.Seq` / `.Message` で読む。
                    # StrictMode 下ではその 2 つが無いオブジェクトへアクセスした瞬間に例外になる
                    # （H1/H4/H5 の既存の形に揃えることでこの経路を避ける）。
                    $violations += @{
                        Invariant = 'H7'
                        Seq       = $event.Seq
                        SectionId = $sectionId
                        Message   = "失効した結果を採った: dispatch_seq=$dispatchSeq < pending=$pendingSeq"
                    }
                } else {
                    Add-SnotraTracePass -PassCount $passCount -Invariant 'H7' -SectionId $sectionId
                }
            }
        }
    }

    # --- H1 は窓ごとに締める --------------------------------------------------
    # **違反は窓が閉じていなくても確定する。無違反は「まだ後続が来うる」ので SKIP。**
    # この非対称が無いと、最後まで hidden で終わる実行（smoke-egui）で検出器が丸ごと黙る。
    foreach ($window in $hideWindows) {
        if ($window.Violated) { continue }
        if ($window.Closed) {
            Add-SnotraTracePass -PassCount $passCount -Invariant 'H1' -SectionId $window.SectionId
        } else {
            $unjudgeable += @{
                Invariant = 'H1'
                Seq       = $window.Seq
                SectionId = $window.SectionId
                Reason    = "hide 窓が閉じていない（次の $script:EventShowDone が無い）"
            }
        }
    }

    # --- 区間ごとの判定 -------------------------------------------------------
    $failed = @{}
    foreach ($violation in $violations) {
        $failed["$($violation.Invariant)|$($violation.SectionId)"] = $true
    }

    $rows = @()
    foreach ($section in @(@($pseudo) + $normalized)) {
        $row = @{ Id = $section.Id; Title = $section.Title }
        foreach ($invariant in $script:Invariants) {
            $key = "$invariant|$($section.Id)"
            if ($failed.ContainsKey($key)) {
                $row[$invariant] = 'FAIL'
            } elseif ($passCount.ContainsKey($key) -and $passCount[$key] -gt 0) {
                # **D7: 捨てた行があるなら PASS を名乗らない。** 決定的な違反が捨てられた
                # 行に載っていた可能性が残る。FAIL は落とさない（違反は確定している）。
                if ($effectiveDropped -gt 0) {
                    $row[$invariant] = 'SKIP'
                    $unjudgeable += @{
                        Invariant = $invariant
                        Seq       = 0
                        SectionId = $section.Id
                        Reason    = "parse できなかった行が $effectiveDropped 件あるため PASS を SKIP へ落とした"
                    }
                } else {
                    $row[$invariant] = 'PASS'
                }
            } else {
                $row[$invariant] = 'SKIP'
            }
        }
        $rows += $row
    }

    # **`Overall` だけを読むと SKIP の山が PASS に覆われる**（code-review High-1）——1 区間でも
    # PASS なら PASS を名乗るため。`Counts` を併せて返し、呼び出し側が「何件を実際に判定したか」
    # を出せるようにする（`Overall` は exit code 用の要約であって網羅の主張ではない）。
    # **`Overall` は `Counts` の要約である**——先に数え、そこから導く。`Overall` だけを読むと
    # 1 区間の PASS が SKIP の山を覆うので、両方を返して呼び出し側に件数を出させる
    # （code-review High-1）。
    $counts = @{}
    $overall = @{}
    foreach ($invariant in $script:Invariants) {
        $tally = @{ PASS = 0; FAIL = 0; SKIP = 0 }
        foreach ($row in $rows) { $tally[$row[$invariant]]++ }
        $counts[$invariant] = $tally
        $overall[$invariant] = if ($tally.FAIL -gt 0) { 'FAIL' } elseif ($tally.PASS -gt 0) { 'PASS' } else { 'SKIP' }
    }

    return @{
        Sections         = $rows
        Overall          = $overall
        Counts           = $counts
        Violations       = $violations
        Unjudgeable      = $unjudgeable
        Observed         = $observed
        DroppedLineCount = $effectiveDropped
        JudgeFailed      = $false
    }
}

# 昇順に並んだ帰属表から、`Seq` より小さい `StartSeq` を持つ**最後の**区間を選ぶ。
# 同じ `StartSeq` が並ぶ（事象を挟まずに項目が進んだ）場合は後の項目を採る——
# マーカーは操作の前に打つので、その後の事象は後の項目のものである。
function Resolve-SnotraTraceSection {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [AllowEmptyCollection()]
        [array]$Attributable,
        [Parameter(Mandatory)]
        [long]$Seq
    )

    if ($Attributable.Count -eq 0) { return 0 }
    $chosen = $Attributable[0]
    foreach ($section in $Attributable) {
        if ($section.StartSeq -lt $Seq) { $chosen = $section } else { break }
    }
    return $chosen.Id
}

function Add-SnotraTracePass {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [hashtable]$PassCount,
        [Parameter(Mandatory)]
        [string]$Invariant,
        [Parameter(Mandatory)]
        [int]$SectionId
    )

    $key = "$Invariant|$SectionId"
    if ($PassCount.ContainsKey($key)) { $PassCount[$key] = $PassCount[$key] + 1 }
    else { $PassCount[$key] = 1 }
}

<#
.SYNOPSIS
区間 1 つの判定行を引く。見つからなければ `$null`。

.DESCRIPTION
**結果ハッシュの形を呼び出し側へ漏らさないための唯一の口である。** 素の
`$Result.Sections | Where-Object { $_.Id -eq $id }` を各所に書くと、`Sections` が
hashtable の配列であることに 3 か所が依存する。
#>
function Get-SnotraTraceSectionVerdict {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [hashtable]$Result,
        [Parameter(Mandatory)]
        [int]$SectionId
    )

    $match = @($Result.Sections | Where-Object { $_.Id -eq $SectionId })
    if ($match.Count -ne 1) { return $null }
    return $match[0]
}

<#
.SYNOPSIS
判定表（`Overall` または区間の行）のうち FAIL の不変条件名を返す。

.DESCRIPTION
**「FAIL という値で赤を見分ける」規則を呼び出し側へ写さないための口である。**
素の `Where-Object { $row[$_] -eq 'FAIL' }` を各所に書くと、判定値の集合を変えたとき
（例: FAIL を細分する）に一部だけが追随する。名前の母集団も
`Get-SnotraTraceInvariantNames` から引くので、不変条件を 1 つ足したときに
呼び出し側の写しが黙って取りこぼすことがない。
#>
function Get-SnotraTraceFailedInvariants {
    [CmdletBinding()]
    param(
        [AllowNull()]
        [hashtable]$Verdicts
    )

    if ($null -eq $Verdicts) { return @() }
    return @(Get-SnotraTraceInvariantNames | Where-Object { $Verdicts[$_] -eq 'FAIL' })
}

<#
.SYNOPSIS
trace 側の「赤」の件数を返す。0 なら trace は合否を下げない。

.DESCRIPTION
**赤とみなす状態の定義を 1 か所に置く。** 呼び出し側（`manual-smoke.ps1` の exit code、
記録、端末表示）がそれぞれ数え直すと、状態を 1 つ足したときに一部だけが追随し、
**新しい赤が exit code から黙って落ちる**。

赤は 3 つある。いずれも「問題が無かった」ではない:

- 不変条件の **FAIL**（違反そのもの）
- **判定器の例外**（`JudgeFailed`）——例外は欠陥であって無事ではない（code-review C2）
- **trace を読めなかった**（`ReadError`）——観測できなかったことを成功として終えると、
  権限・共有違反で読みが落ちた実行が緑になる（#872。`Read-SnotraTraceSnapshot` が
  読み取り失敗と不在を区別しているのは、この判断のためである）

**trace の不在（`-NoLaunch` で起動した・まだ 1 行も出ていない）は赤ではない。**
`ReadError` が無いまま `Verdict` が `$null` の場合がそれで、0 を返す——ここを赤に
倒すと、trace を出さない正当な使い方が常に失敗する。
#>
function Get-SnotraTraceFailureCount {
    [CmdletBinding()]
    param(
        # `Test-SnotraTraceInvariants` の結果。判定していなければ $null。
        [AllowNull()]
        [hashtable]$Verdict,
        # `Read-SnotraTraceSnapshot` の ReadError。読めていれば $null。
        [AllowNull()]
        [AllowEmptyString()]
        [string]$ReadError
    )

    $count = 0
    if (-not [string]::IsNullOrEmpty($ReadError)) { $count++ }
    if ($null -ne $Verdict) {
        $count += @(Get-SnotraTraceFailedInvariants -Verdicts $Verdict.Overall).Count
        if ($Verdict.JudgeFailed) { $count++ }
    }
    return $count
}

<#
.SYNOPSIS
不変条件ごとの件数を 1 行へ畳む。`-Compact` は端末向けの短い形。

.DESCRIPTION
**`Overall` だけを出させないための既定の表現である**（code-review High-1）。整形を
呼び出し側に書くと、記録・端末・PR コメントで別々の形へドリフトする。
#>
function Format-SnotraTraceCountSummary {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [hashtable]$Result,
        [switch]$Compact
    )

    $parts = foreach ($invariant in $script:Invariants) {
        $c = $Result.Counts[$invariant]
        if ($Compact) { "$invariant P$($c.PASS)/F$($c.FAIL)/S$($c.SKIP)" }
        else { "$invariant PASS $($c.PASS) / FAIL $($c.FAIL) / SKIP $($c.SKIP)" }
    }
    return ($parts -join $(if ($Compact) { ' ' } else { ' 、 ' }))
}

<#
.SYNOPSIS
判定結果を markdown の行列へ整形する（記録・PR コメント用）。

.DESCRIPTION
**SKIP を「合格」と読ませない**ため、凡例と判定不能の理由を必ず併記する。
#>
function Format-SnotraTraceVerdictTable {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [hashtable]$Result
    )

    $lines = @()
    # ヘッダも `$script:Invariants` から作る（写しを置かない・code-review M2）。
    $lines += "| 区間 | 項目 | $($script:Invariants -join ' | ') |"
    $lines += "|---|---|$('---|' * $script:Invariants.Count)"
    foreach ($row in $Result.Sections) {
        $cells = foreach ($invariant in $script:Invariants) {
            if ($row[$invariant] -eq 'FAIL') { "**FAIL**" } else { $row[$invariant] }
        }
        # 縦棒は列区切りゆえ全セルで潰す（片側だけ潰すと題名に `|` が入った瞬間に表が崩れる）。
        $lines += "| $($row.Id) | $($row.Title.Replace('|', '\|')) | $($cells -join ' | ') |"
    }
    $lines += ''
    $lines += 'H1 = hidden な窓に results が現れない / H4 = `rows` が 0 の show が無い / H5 = hide を挟まない連続 show が無い / H7 = pending より古い seq の採り込みが無い'
    $lines += '**SKIP は「判定できなかった」であって合格ではない。** 理由は下の一覧にある。'
    $lines += ''
    $lines += '| 不変条件 | PASS | FAIL | SKIP |'
    $lines += '|---|---|---|---|'
    foreach ($invariant in $script:Invariants) {
        $c = $Result.Counts[$invariant]
        $lines += "| $invariant | $($c.PASS) | $($c.FAIL) | $($c.SKIP) |"
    }
    if ($Result.JudgeFailed) {
        $lines += ''
        $lines += '**判定器そのものが例外で停止した。** この表の SKIP は「調べて問題が無かった」ではなく「調べられなかった」である。'
    }

    if ($Result.Violations.Count -gt 0) {
        $lines += ''
        $lines += '### trace が検出した違反'
        $lines += ''
        foreach ($violation in $Result.Violations) {
            $lines += "- **$($violation.Invariant)** 区間 $($violation.SectionId) / seq $($violation.Seq) — $($violation.Message)"
        }
    }

    if ($Result.Unjudgeable.Count -gt 0) {
        $lines += ''
        $lines += '### 判定できなかった理由'
        $lines += ''
        foreach ($reason in $Result.Unjudgeable) {
            $lines += "- $($reason.Invariant) 区間 $($reason.SectionId) — $($reason.Reason)"
        }
    }

    return $lines
}

Export-ModuleMember -Function @(
    'Get-SnotraTraceInvariantNames'
    'Get-SnotraTraceProperty'
    'Get-SnotraTraceMarker'
    'Test-SnotraTraceInvariants'
    'Get-SnotraTraceSectionVerdict'
    'Get-SnotraTraceFailedInvariants'
    'Get-SnotraTraceFailureCount'
    'Format-SnotraTraceCountSummary'
    'Format-SnotraTraceVerdictTable'
)
