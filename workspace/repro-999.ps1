#Requires -Version 7
<#
.SYNOPSIS
  #999 の再現ハーネス。results 窓の表示直後に打鍵を注入し、**生ログを出すだけ**（判定しない）。

.DESCRIPTION
  `egui_results:show` の直後から注入打鍵が沈黙する現象（#999）を、`SNOTRA_EGUI_INPUT_TRACE` の
  計器つき／なしで交互に測るための道具である。**合否を出さない**——出力は各実行の生ログと、
  注入時刻を並べた突き合わせ材料であり、読むのは人間である（`scripts/visual-input-metrics.ps1` と同じ流儀）。

  設計の根拠は `workspace/research.md` と `workspace/plan.md`。要点だけ再掲する:

  - **`SNOTRA_TRACE` は両方の回で常時 ON**、トグルするのは `SNOTRA_EGUI_INPUT_TRACE` だけである。
    両方切ると計器なしの回が盲目になり、`egui_results:show` で止まったことすら見えない
  - **計器は系を乱す**（`PERFORMANCE.md`・`snotra-egui-runtime/src/env.rs` の doc）。ゆえに
    OFF → ON を 1 組として**交互に**回す。まとめて OFF を先に回すとドリフトが A/B の差へ紛れる
  - **`SNOTRA_EGUI_INPUT_TRACE` へ空文字を渡さない**（空文字は未設定として扱われる・
    `docs/adr/ADR-egui-trace-hatch-empty-only.md`）。ON の回だけ `-ExtraVariables` に `'1'` を載せる
  - 注入時刻（`SNOTRA_SMOKE_INJECT`）は `Send-SnotraKey` が `Write-Host`＝**情報ストリーム**へ出し、
    本体の `rx_key` は子プロセスの **stderr** へ出る。別ストリームなので `6>>` で拾う（実測済み）

.NOTES
  **この足場は `workspace/` に住む。** `/retrospective` のサイクル終了処理が `workspace/` ごと
  撤去する既存の機構に載るため、**撤去の合図を自分で持つ必要がない**——`scripts/` へ置くと
  「#999 が閉じたら消す」という自己参照の撤去条件になり、閉じるのが当の PR のとき発火しない。

.EXAMPLE
  pwsh -NoProfile -File workspace/repro-999.ps1 -Pairs 1
  pwsh -NoProfile -File workspace/repro-999.ps1 -Pairs 6 -DownCount 200
#>
[CmdletBinding()]
param(
    # OFF → ON を 1 組として何組回すか。**組の中で交互に**なるのが要点である。
    [int]$Pairs = 1,
    # results 窓を出すための 1 文字クエリ。**A-Z のみ**（`Send-SnotraKey` が VK で送るため）。
    [ValidatePattern('^[A-Za-z]$')]
    [string]$Query = 'A',
    # `egui_results:show` の観測後に撃つ Down の回数。#996 の測定は 200 行／10 行の双方で沈黙した。
    [int]$DownCount = 10,
    # Down の押下時間と間隔。#996 の diag（通った側）は 40ms down→up / 120ms 間隔だった。
    [int]$DownHoldMs = 40,
    [int]$DownIntervalMs = 120,
    # **既定は 0 である。** #996 の diag は `egui_results:show` の観測後に 800ms 置いて通り、
    # 沈黙した測定スクリプトは置かずに本番操作へ入った。既定は**沈黙した側**に合わせる。
    [int]$PostShowDelayMs = 0,
    [int]$StartupWaitMs = 1500,
    [int]$ObserveTimeoutMs = 10000,
    [int]$StartupObserveTimeoutMs = 30000,
    [string]$ExePath = '',
    # 複製元。既定は実 config（`%APPDATA%\Snotra`）。**読むだけで、書き換えない。**
    [string]$SourceConfigDir = (Join-Path $env:APPDATA 'Snotra'),
    [string]$EvidenceRoot = '',
    # **D-2 の逸脱を戻す枝。** 既定は `auto_update = "disabled"` へ倒すが、再現しなかったときは
    # まずここを実 config の値へ戻して測り直す（`workspace/plan.md` D-1 の指示）。
    [switch]$KeepAutoUpdate,
    # **probe: Down を「拡張キー」として撃つ。**
    # `Send-SnotraKey` は `keybd_event(vk, bScan=0, flags=0)` で撃つため
    # `KEYEVENTF_EXTENDEDKEY (0x1)` が立たず、VK_DOWN が **`Numpad2` として配送される**（実測）。
    # このスイッチはその 1 点だけを変えて対照を取るためにある。**`Send-SnotraKey` の写しではない**
    # ——写しにならないよう、注入するのは Down だけで、他のキーは今までどおり `Send-SnotraKey` が撃つ。
    [switch]$ExtendedDown
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Import-Module (Join-Path $repoRoot 'scripts/lib/SnotraSmoke.psm1') -Force

# 画面ロック中は窓が描かれず、前面化も打鍵も宛先を失う（#866）。
Assert-SnotraSessionUnlocked -Operation '#999 の再現測定'

$exe = if ($ExePath) { $ExePath } else { Resolve-SnotraCargoExecutable -RepositoryRoot $repoRoot -Profile release }
if (-not (Test-Path -LiteralPath $exe)) {
    throw "実行ファイルがありません: $exe（先に cargo build -p snotra --release）"
}
$exe = (Resolve-Path -LiteralPath $exe).Path

if (-not (Test-Path -LiteralPath $SourceConfigDir)) {
    throw "複製元の config ディレクトリがありません: $SourceConfigDir"
}

# **既定の出力先はリポジトリの外である。** `[trace]` の `icon:extract_failed` は
# **利用者の実ファイルパスを逐語で載せる**（実測: Dropbox 配下・VS Code 拡張配下など）。
# このリポジトリは公開されているので、生ログを `workspace/` へ置くと squash マージで
# **個人のディレクトリ内容が main の履歴へ入る**。生ログはここに置き、
# コミットするのは経路だけを数えた派生表にする。
if (-not $EvidenceRoot) { $EvidenceRoot = Join-Path $env:TEMP 'snotra-evidence-999' }
New-Item -ItemType Directory -Force -Path $EvidenceRoot | Out-Null

Write-Host "本体: $exe"
Write-Host "複製元 config: $SourceConfigDir"
Write-Host "証拠の出力先: $EvidenceRoot"

# `[hotkey]` は trace 由来の VK を使うので読まない。ここで読むのは**窓の高さに効く入力**だけである
# ——丸ごと複製は「何が効いているか」を隠す形なので、高さを動かす入力は明示的に残す
# （ユーザー判断 2026-08-26: 「計測の目的を考えると高さを変える設定は明示的にしたほうがいい」）。
function Write-SnotraProfileFacts {
    param([string]$ConfigPath, [string]$OutPath)

    $lines = @("# 窓の高さに効く入力（複製後の実効値）", "# source: $ConfigPath", "")
    $raw = Get-Content -LiteralPath $ConfigPath -Raw
    foreach ($key in @('auto_update', 'window_width', 'show_icons', 'font_family', 'font_size', 'max_results')) {
        $m = [regex]::Match($raw, "(?m)^\s*$key\s*=\s*(.+?)\s*$")
        $lines += if ($m.Success) { "$key = $($m.Groups[1].Value)" } else { "$key = <未指定・既定値>" }
    }
    Set-Content -LiteralPath $OutPath -Value ($lines -join "`r`n") -Encoding utf8
}

function Invoke-Repro999Run {
    param(
        [Parameter(Mandatory)][string]$RunDir,
        [Parameter(Mandatory)][bool]$Instrument,
        [Parameter(Mandatory)][string]$RunLabel
    )

    New-Item -ItemType Directory -Force -Path $RunDir | Out-Null
    $profileDir = Join-Path $RunDir 'profile'
    $errPath = Join-Path $RunDir 'stderr.log'
    $outPath = Join-Path $RunDir 'stdout.log'
    $injectLog = Join-Path $RunDir 'inject.log'

    # **複製から破棄までを 1 つの try で囲う。** 複製は 16.5 MB の実索引を持ち込むので、
    # ここから `Start-SnotraProcess` までの間で throw すると**削除されないまま %TEMP% に残る**
    # ——生成と破棄の間に早期脱出がある形である（`/symmetric-check` の「生成→登録の間に早期リターン」）。
    # `$proc` は throw の位置によって未束縛でありうるので、`finally` 側で存在を見る。
    $proc = $null
    # **`finally` が読む変数は try より前で束縛する。** `Copy-Item` が途中で失敗すると
    # `$configPath` は未束縛のままで、StrictMode 下の `finally` の読みが throw する
    # ——**元の例外を隠したうえに削除まで飛ばす**（`/symmetric-check` 再実行で発見・実測）。
    $configPath = $null
    # **呼び出し元の env を保存する。** `Invoke-SnotraEnvironment` は保存して戻すが、
    # こちらは `Remove-Item` で消すだけだった——外側で計器を立てている呼び出し元がいると、
    # この関数が黙ってそれを落とす。
    $priorTrace = [Environment]::GetEnvironmentVariable('SNOTRA_EGUI_INPUT_TRACE', 'Process')
    try {

    # **使い捨てプロファイルは実 config の複製である**（#996 と同じ条件・実索引を持ち込む）。
    Copy-Item -LiteralPath $SourceConfigDir -Destination $profileDir -Recurse -Force
    $configPath = Join-Path $profileDir 'config.toml'
    if (-not (Test-Path -LiteralPath $configPath)) { throw "複製先に config.toml がありません: $configPath" }

    # **既知の逸脱 1 件**: `auto_update` を disabled へ倒す。実チェックはネットワーク依存の雑音を
    # 持ち込むうえ、**toast は窓の高さを変え**、**toast 窓それ自体が focus 事象の発生源**であり、
    # H1（`held_since_focus_gain`）の検定を汚す。**再現しなかったときは、まずこの 1 行を戻す。**
    $raw = Get-Content -LiteralPath $configPath -Raw
    $raw = if ($KeepAutoUpdate) {
        $raw   # D-2 の逸脱を戻した回。実 config の値をそのまま使う
    } elseif ($raw -match '(?m)^\s*auto_update\s*=') {
        [regex]::Replace($raw, '(?m)^\s*auto_update\s*=.*$', 'auto_update = "disabled"')
    } elseif ($raw -match '(?m)^\s*\[general\]\s*$') {
        [regex]::Replace($raw, '(?m)^(\s*\[general\]\s*)$', "`$1`r`nauto_update = `"disabled`"")
    } else {
        $raw.TrimEnd() + "`r`n`r`n[general]`r`nauto_update = `"disabled`"`r`n"
    }
    Set-Content -LiteralPath $configPath -Value $raw -Encoding utf8
    Write-SnotraProfileFacts -ConfigPath $configPath -OutPath (Join-Path $RunDir 'profile.txt')

    # **`SNOTRA_TRACE` は両方の回で立てる**（`-Trace`）。トグルするのは計器だけである。
    $extra = @{}
    if ($Instrument) { $extra['SNOTRA_EGUI_INPUT_TRACE'] = '1' }   # 空文字を渡さない（ADR）

    $proc = Start-SnotraProcess -ConfigDir $profileDir -FilePath $exe -Trace `
        -ExtraVariables $extra -StandardErrorPath $errPath -StandardOutputPath $outPath -NoNewWindow

        # **注入側の計器は、この PowerShell プロセスの env が握っている。**
        # `Start-SnotraProcess` は `Invoke-SnotraEnvironment` の中でだけ env を立てて**すぐ戻す**ので、
        # 打鍵を撃つ時刻には消えている——ここで立て直さないと `Send-SnotraKey` の
        # `if ($env:SNOTRA_EGUI_INPUT_TRACE)` が偽になり、**注入時刻が 1 行も残らないまま
        # 本体側だけが計器つきで走る**（沈黙が「注入していない」と見分けられなくなる）。
        # OFF の回では立てない——注入側も計器なしにするのが #996 の条件だからである。
        if ($Instrument) { $env:SNOTRA_EGUI_INPUT_TRACE = '1' }

        Start-Sleep -Milliseconds $StartupWaitMs

        $hkEvent = Wait-SnotraTraceEvent -Path $errPath -EventName 'hotkey:registered' -TimeoutMs $StartupObserveTimeoutMs
        if ($null -eq $hkEvent) { throw 'hotkey:registered を観測できません（本体が起動していない可能性）' }
        if (-not $hkEvent.data.ok) { throw "hotkey 登録に失敗しています（modifier=$($hkEvent.data.modifier) key=$($hkEvent.data.key)）" }
        $vks = @($hkEvent.data.vks | ForEach-Object { [byte][int]$_ })

        Send-SnotraKeyChord -VirtualKeys $vks 6>> $injectLog
        $shown = $null -ne (Wait-SnotraTraceEvent -Path $errPath -EventName 'egui_show:done' -TimeoutMs $ObserveTimeoutMs)
        if (-not $shown) { throw "egui_show:done を ${ObserveTimeoutMs}ms 以内に観測できません" }

        # **前面化は残す**——本体の focus 要求は窓の OS focus に条件づけられており、
        # 前面が取れなければ打鍵の宛先が定まらない。
        $hwnd = Wait-SnotraWindow -Title 'Snotra' -Process $proc -TimeoutMs $ObserveTimeoutMs
        [void](Set-SnotraForegroundWindow -Handle $hwnd)

        Send-SnotraKey -VirtualKey ([byte][char]$Query.ToUpperInvariant()) 6>> $injectLog
        Start-Sleep -Milliseconds 30
        Send-SnotraKey -VirtualKey ([byte][char]$Query.ToUpperInvariant()) -Up 6>> $injectLog

        $resultsEvent = Wait-SnotraTraceEvent -Path $errPath -EventName 'egui_results:show' -TimeoutMs $ObserveTimeoutMs
        if ($null -eq $resultsEvent) { throw "egui_results:show を ${ObserveTimeoutMs}ms 以内に観測できません（クエリ '$Query' の結果が 0 件か）" }
        $rows = $resultsEvent.data.rows
        Write-Host ("  egui_results:show rows=$rows")

        # **既定は 0 である**（#996 で沈黙した側の形）。
        if ($PostShowDelayMs -gt 0) { Start-Sleep -Milliseconds $PostShowDelayMs }

        $VK_DOWN = 0x28
        $VK_ESCAPE = 0x1B
        for ($i = 0; $i -lt $DownCount; $i++) {
            if ($ExtendedDown) {
                # **この枝は `Send-SnotraKey` の写しではないが、副作用を 1 つ欠く**（`/dry-check` 実測）:
                # `SNOTRA_SMOKE_INJECT` の行を出さないので、**probe の回は `inject.log` に Down が載らない**。
                # 意図的である——ここで `Write-Host` を書くと注入ログの生成点が 2 か所になり、
                # 「打鍵の実装は 1 か所」という `Send-SnotraKey` の doc の主張が偽になる。
                # **この枠は identity（physical の内訳）だけを見る**ので、注入時刻は要らない。
                # 恒久的な直し方は `Send-SnotraKey` へ `-Extended` を足すこと（`workspace/followup-issue-draft.md`）。
                #
                # **`[SnotraSmokeInterop.Native]` の初期化に依存する。** それを行う
                # `Initialize-SnotraNativeInterop` は **module から export されていない**ので、
                # ここから呼べない（実測。`Get-SnotraForegroundWindowLabel` と同じ）。依存は満たされている
                # ——この行に来るまでに hotkey chord とクエリ 1 文字を `Send-SnotraKey` が撃っており、
                # あちらが先頭で初期化する。**Down 以外の注入を消すときは、この前提も一緒に消える。**
                #
                # `KEYEVENTF_EXTENDEDKEY = 0x1` / `KEYEVENTF_KEYUP = 0x2`。
                # **`bScan` も渡す**——フラグだけ立てて `bScan=0` のままでは
                # `physical=Numpad2` が変わらなかった（実測 2026-08-26・40/40）。
                # `0x50` は Down の scancode（拡張側は lParam の bit24 で区別される）。
                [SnotraSmokeInterop.Native]::keybd_event($VK_DOWN, 0x50, 0x1, [UIntPtr]::Zero)
                Start-Sleep -Milliseconds $DownHoldMs
                [SnotraSmokeInterop.Native]::keybd_event($VK_DOWN, 0x50, 0x3, [UIntPtr]::Zero)
            } else {
                Send-SnotraKey -VirtualKey $VK_DOWN 6>> $injectLog
                Start-Sleep -Milliseconds $DownHoldMs
                Send-SnotraKey -VirtualKey $VK_DOWN -Up 6>> $injectLog
            }
            Start-Sleep -Milliseconds $DownIntervalMs
        }

        # **`Get-SnotraForegroundWindowLabel` は module から export されていない**（実測・
        # `Export-ModuleMember` の一覧に無い）。export されている `Get-SnotraForegroundWindow` を使い、
        # main のハンドルと一致するかだけを見る——#999 が確かめたのと同じ観測量である。
        $fgAfterDown = Get-SnotraForegroundWindow
        $fgIsMain = ($fgAfterDown -eq $hwnd)

        Send-SnotraKey -VirtualKey $VK_ESCAPE 6>> $injectLog
        Start-Sleep -Milliseconds $DownHoldMs
        Send-SnotraKey -VirtualKey $VK_ESCAPE -Up 6>> $injectLog

        $hidEvent = Wait-SnotraTraceEvent -Path $errPath -EventName 'egui_hide:done' -TimeoutMs $ObserveTimeoutMs

        # **判定しない。観測を並べるだけである。**
        # `Label` / `Error` も**ここで埋める**——呼び出し側で `Add-Member` を重ねる形は、
        # 例外が出た回と出なかった回で列が食い違い、`Add-Member` 自身が失敗した（実測）。
        [pscustomobject]@{
            Label = $RunLabel
            Instrument = $Instrument
            Rows = $rows
            HideObserved = ($null -ne $hidEvent)
            ForegroundIsMain = $fgIsMain
            ForegroundAfterDown = ('0x{0:X}' -f [int64]$fgAfterDown)
            RunDir = $RunDir
            Error = ''
        }
    } finally {
        # **消すのではなく、呼び出し元の値へ戻す。**
        # `$null` を代入すると空文字が残るので、不在は `Remove-Item` で表す
        # （ADR-egui-trace-hatch-empty-only）。
        if ($null -eq $priorTrace) {
            Remove-Item -LiteralPath 'Env:SNOTRA_EGUI_INPUT_TRACE' -ErrorAction SilentlyContinue
        } else {
            Set-Item -LiteralPath 'Env:SNOTRA_EGUI_INPUT_TRACE' -Value $priorTrace
        }
        # **`[void]` で捨てる**——`finally` の中でも出力は関数の出力ストリームへ載るので、
        # 戻り値の `$true` が要約の行に混ざり、`Export-Csv` が空行を書いた（実測）。
        # **`$proc` は未束縛でありうる**（起動より前で throw した場合）。
        if ($null -ne $proc) { [void](Stop-SnotraProcessAndWait -Process $proc) }

        # **複製したプロファイルは残さない。** `index.bin` は実測 16.5 MB で、しかも
        # **利用者の実ファイルパスそのもの**である——証拠として git に載せてよいものではない。
        # 残すのは `config.toml` と `profile.txt`（高さに効く入力の実効値）だけにする。
        if (Test-Path -LiteralPath $profileDir) {
            # **`config.toml` の不在で throw した経路がある**（複製先に config.toml が無い場合）。
            # ここで無条件に `Copy-Item` すると `finally` の中で新しい例外が起き、
            # **元の例外を隠したうえに下の削除まで飛ばす**——16.5 MB が残る。
            if ($null -ne $configPath -and (Test-Path -LiteralPath $configPath)) {
                Copy-Item -LiteralPath $configPath -Destination (Join-Path $RunDir 'config.toml') -Force
            }
            Remove-Item -LiteralPath $profileDir -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

$summary = @()
foreach ($pair in 1..$Pairs) {
    foreach ($instrument in @($false, $true)) {
        $label = "pair{0:D2}-{1}" -f $pair, $(if ($instrument) { 'on' } else { 'off' })
        $runDir = Join-Path $EvidenceRoot $label
        Write-Host "== $label =="
        try {
            $summary += Invoke-Repro999Run -RunDir $runDir -Instrument $instrument -RunLabel $label
        } catch {
            # **落ちた回も標本である。** 例外で打ち切ると、沈黙した回だけが記録から消える。
            # **列は成功時と揃える**（揃えないと `Export-Csv` が先頭行の列だけを書く）。
            Write-Warning "$label : $($_.Exception.Message)"
            $summary += [pscustomobject]@{
                Label = $label; Instrument = $instrument; Rows = $null; HideObserved = $false
                ForegroundIsMain = $null; ForegroundAfterDown = ''; RunDir = $runDir
                Error = $_.Exception.Message
            }
        }
    }
}

$summaryPath = Join-Path $EvidenceRoot 'summary.csv'
$summary | Export-Csv -LiteralPath $summaryPath -NoTypeInformation -Encoding utf8
Write-Host ''
$summary | Format-Table Label, Rows, HideObserved, Error -AutoSize
Write-Host "要約: $summaryPath"
Write-Host '判定はこのスクリプトが行わない。生ログは各 run ディレクトリの stderr.log / inject.log にある。'
