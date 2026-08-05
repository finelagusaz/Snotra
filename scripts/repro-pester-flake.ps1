#Requires -Version 7
<#
.SYNOPSIS
Pester スイートを同一 runner 上で N 回繰り返し、失敗の標本を一度に集める（#872 / #936）。

.DESCRIPTION
**これは検出器ではない。測定器である。** 反復のうち何回落ちても、このスクリプト自身は 0 で
終わる——赤くする責務は `ci.yml` の `rust-check` が持ち、ここは「12.5% の事象を 1 日 1 標本で
待つ」という調査の律速を外すためだけに在る。**このスクリプトの緑を「検査が通った」と読んで
はならない。** ハーネス自身が壊れたとき（1 反復も完走しない）だけ 2 で終わる。

**各反復は `scripts/run-pester.ps1` を子プロセスとして呼ぶ**——CI の `rust-check` が呼ぶのと
同じ入口である。Pester の設定・タグ・実行ファイルの解決を写し取ると、写しの側だけが陳腐化して
「CI とは違うものを N 回測った」になる。子プロセスの起動コスト（数秒/反復）はその対価である。

**成功した反復の trace も残す。** `SNOTRA_PESTER_TRACE_DIR` を反復ごとに立てるので、キャレット
統合検査は成否によらず stderr の写しをそこへ残す（`SnotraSmoke.Tests.ps1` の `finally`）。
失敗時にしか証拠が無い現状では、fail の 936ms が異常なのか平常なのかを比べる対照群が無い。

**失敗時の猶予**（`-FailureGraceMs`）を渡すと、待ちが不成立で終わった検査が本体を即座に
kill せず、指定 ms だけ待ってから trace を読み直す。**「予算後の遅着」と「恒久的な喪失」を
分ける唯一の実験**であり（#936 受け入れ条件 1）、既定は 0＝現行の挙動である。

.EXAMPLE
pwsh -NoProfile -File scripts/repro-pester-flake.ps1 -Iterations 30

.EXAMPLE
pwsh -NoProfile -File scripts/repro-pester-flake.ps1 -Iterations 40 -FailureGraceMs 15000
#>
[CmdletBinding()]
param(
    # 反復回数。CI の 1 job で回しきれる範囲に置く（1 反復あたり概ね 30〜60 秒）。
    [ValidateRange(1, 500)]
    [int]$Iterations = 30,
    # 実行ファイル。未指定なら `run-pester.ps1` が `Resolve-SnotraCargoExecutable` で解決する。
    [string]$ExePath,
    # 反復ごとの証拠（log / trace）の置き場。
    [string]$OutputDir = 'target/pester-flake-repro',
    # 経過時間の上限。job のタイムアウトで証拠ごと失うより、途中で止めて集計を残す方がよい。
    [ValidateRange(1, 600)]
    [int]$MaxMinutes = 90,
    # 失敗した検査が本体を kill する前に待つ時間（ms）。0 = 現行の挙動。
    [ValidateRange(0, 120000)]
    [int]$FailureGraceMs = 0
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
# **反復の失敗で loop を止めない**。PowerShell 7.3+ の `$PSNativeCommandUseErrorActionPreference`
# は既定 true で、`$ErrorActionPreference = 'Stop'` と組むと**子プロセスの非 0 終了が throw する**
# ——測定器がそれで止まると、最初の 1 件だけ採って残りを捨てることになる。ここは exit code を
# 値として読む場所なので、明示的に倒す。
$PSNativeCommandUseErrorActionPreference = $false

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$runner = Join-Path $PSScriptRoot 'run-pester.ps1'
if (-not (Test-Path -LiteralPath $runner)) {
    throw "反復の入口が見つかりません: $runner"
}

$resolvedOutput = if ([IO.Path]::IsPathRooted($OutputDir)) { $OutputDir } else { Join-Path $repoRoot $OutputDir }
New-Item -ItemType Directory -Force -Path $resolvedOutput | Out-Null

# **注目している検査の名前をここに置く。** 集計で「この検査が落ちた反復」を数えるためだけの
# 文字列であり、判定には使わない（判定は run-pester.ps1 の exit code）。名前が変われば
# 集計欄が 0 になるだけで、失敗そのものは exit code 側に残る。
$watchedTest = 'フォルダ復帰後の次打鍵を復元クエリの末尾へ追加する'

$deadline = [DateTime]::UtcNow.AddMinutes($MaxMinutes)
$records = [System.Collections.Generic.List[object]]::new()

Write-Host "反復再現: $Iterations 回 / 上限 $MaxMinutes 分 / 証拠 → $resolvedOutput"
if ($FailureGraceMs -gt 0) {
    Write-Host "失敗時の猶予: ${FailureGraceMs}ms（kill 前に trace を読み直す）"
}

for ($i = 1; $i -le $Iterations; $i++) {
    if ([DateTime]::UtcNow -ge $deadline) {
        Write-Warning "上限 $MaxMinutes 分に達したので $($i - 1) 反復で打ち切ります（残り $($Iterations - $i + 1) 回は未実行）。"
        break
    }

    $iterationDir = Join-Path $resolvedOutput ('iter-{0:d3}' -f $i)
    New-Item -ItemType Directory -Force -Path $iterationDir | Out-Null
    $log = Join-Path $iterationDir 'pester.log'

    $arguments = @('-NoProfile', '-File', $runner)
    if ($PSBoundParameters.ContainsKey('ExePath')) { $arguments += @('-ExePath', $ExePath) }

    $started = [DateTime]::UtcNow
    # 子プロセスへ渡す env（呼び出し元の環境を汚さないよう、後で必ず戻す）。
    $savedTraceDir = [Environment]::GetEnvironmentVariable('SNOTRA_PESTER_TRACE_DIR', 'Process')
    $savedGrace = [Environment]::GetEnvironmentVariable('SNOTRA_PESTER_FAILURE_GRACE_MS', 'Process')
    try {
        $env:SNOTRA_PESTER_TRACE_DIR = $iterationDir
        if ($FailureGraceMs -gt 0) { $env:SNOTRA_PESTER_FAILURE_GRACE_MS = "$FailureGraceMs" }
        # stderr も証拠に含める（Write-Warning が出す不成立の診断がここに載る）。
        & pwsh @arguments *>&1 | Tee-Object -FilePath $log | Out-Null
        $exitCode = $LASTEXITCODE
    } finally {
        [Environment]::SetEnvironmentVariable('SNOTRA_PESTER_TRACE_DIR', $savedTraceDir, 'Process')
        [Environment]::SetEnvironmentVariable('SNOTRA_PESTER_FAILURE_GRACE_MS', $savedGrace, 'Process')
    }
    $elapsed = [DateTime]::UtcNow - $started

    $logText = if (Test-Path -LiteralPath $log) { (Get-Content -LiteralPath $log -Raw) } else { '' }
    $watchedFailed = $logText -match ([regex]::Escape("[-] $watchedTest"))

    $records.Add([pscustomobject]@{
            Iteration     = $i
            ExitCode      = $exitCode
            Passed        = ($exitCode -eq 0)
            WatchedFailed = $watchedFailed
            Seconds       = [Math]::Round($elapsed.TotalSeconds, 1)
            StartedUtc    = $started.ToString('o')
        })

    $verdict = if ($exitCode -eq 0) { 'pass' } elseif ($watchedFailed) { 'FAIL(watched)' } else { "FAIL(exit=$exitCode)" }
    Write-Host ('  [{0,3}/{1}] {2,-16} {3,6:n1}s' -f $i, $Iterations, $verdict, $elapsed.TotalSeconds)
}

if ($records.Count -eq 0) {
    Write-Error '1 反復も完走しませんでした。測定になっていないので失敗として終了します。'
    exit 2
}

$failed = @($records | Where-Object { -not $_.Passed })
$watched = @($records | Where-Object { $_.WatchedFailed })
$passSeconds = @($records | Where-Object { $_.Passed } | ForEach-Object { $_.Seconds })
$failSeconds = @($failed | ForEach-Object { $_.Seconds })

$summary = [System.Text.StringBuilder]::new()
[void]$summary.AppendLine('## Pester 反復再現')
[void]$summary.AppendLine('')
[void]$summary.AppendLine("- 反復: $($records.Count) 回（要求 $Iterations 回）")
[void]$summary.AppendLine("- 失敗: $($failed.Count) 回（うち注目検査 $($watched.Count) 回）")
if ($records.Count -gt 0) {
    [void]$summary.AppendLine("- 失敗率: {0:p1}" -f ($failed.Count / $records.Count))
}
if ($passSeconds.Count -gt 0) {
    $stat = $passSeconds | Measure-Object -Minimum -Maximum
    [void]$summary.AppendLine("- pass の所要: 最小 $($stat.Minimum)s / 最大 $($stat.Maximum)s")
}
if ($failSeconds.Count -gt 0) {
    $stat = $failSeconds | Measure-Object -Minimum -Maximum
    [void]$summary.AppendLine("- fail の所要: 最小 $($stat.Minimum)s / 最大 $($stat.Maximum)s")
}
[void]$summary.AppendLine('')
[void]$summary.AppendLine('| # | 結果 | 注目検査 | 所要 (s) | 開始 (UTC) |')
[void]$summary.AppendLine('|---|---|---|---|---|')
foreach ($r in $records) {
    $mark = if ($r.Passed) { 'pass' } else { "fail (exit=$($r.ExitCode))" }
    $watchedMark = if ($r.WatchedFailed) { '**落ちた**' } else { '-' }
    [void]$summary.AppendLine("| $($r.Iteration) | $mark | $watchedMark | $($r.Seconds) | $($r.StartedUtc) |")
}

$summaryText = $summary.ToString()
Write-Host ''
Write-Host $summaryText
$records | Export-Csv -LiteralPath (Join-Path $resolvedOutput 'iterations.csv') -NoTypeInformation -Encoding utf8
Set-Content -LiteralPath (Join-Path $resolvedOutput 'summary.md') -Value $summaryText -Encoding utf8

if ($env:GITHUB_STEP_SUMMARY) {
    Add-Content -LiteralPath $env:GITHUB_STEP_SUMMARY -Value $summaryText -Encoding utf8
}

# **反復の失敗では 0 で終わる**（冒頭の doc）。赤にするのは `rust-check` の責務である。
exit 0
