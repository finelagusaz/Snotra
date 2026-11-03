<#
.SYNOPSIS
  起動 → 表示 → 検索 → hide の各段階で Snotra のメモリを実測する（段階別・前景計測）。

.DESCRIPTION
  `measure-memory.ps1` が「今この瞬間」の 1 点を測るのに対し、本スクリプトは
  ホットキーとクエリを注入しながら**段階列**を測る。フォント設定など「表示・描画で
  初めて実体化する」コストは 1 点計測では見えないため（2026-07-25 実測: user_font は
  アイドル 20.6 MiB → 検索後 40.9 MiB と段階とともに差が広がる）。

  **サンプリングは `measure-memory.ps1` を `&` で呼んで再利用する（重複実装しない）。**
  `&` は同一プロセスで .ps1 を実行するのでコンソールが増えない。ここが本質で、
  `pwsh -File measure-memory.ps1` と書くと**新しいコンソールがフォーカスを奪い**、
  以後の SendKeys が Snotra ではなく別のウィンドウへ流れて標本が無言で無効になる（実際に踏んだ）。

  前提:
  - release ビルド済み（既定は cargo の target から release 本体を導く。`CARGO_TARGET_DIR` に追随する）
  - `[hotkey]` の値は問わない。段階 B で注入するキーは、起動時の `hotkey:registered` trace から
    **アプリが実際に登録した VK 列**を読む（`smoke-egui.ps1` と同じ。理由はあちらの注入箇所のコメント）。
    そのため本体を `SNOTRA_TRACE=1` で起動する。trace が無かった頃の段階列（PERFORMANCE.md）とは、
    この 1 点で計測の条件が違う
  - 索引キャッシュに当たる起動で測る。外れていたら（`startup:ready` の `cache_hit=false`）1 度だけ
    再起動し、それでも外れれば止める
  - trace の stderr は `$env:TEMP` へ書く（実 config の trace は利用者の実パスを載せるので、リポジトリへ置かない）
  - `[general] auto_hide_on_focus_lost = false` は**推奨であって必須ではない**。上記の
    同一プロセス実行でフォーカスを保つため、`true` のままでも段階列は取れる（実測で
    B/C/D とも成立）。ただし外的要因でフォーカスが逸れるとウィンドウが hide して PrivWS が
    trim 後の値へ落ち、段階列が無言で壊れる。長時間の計測や無人実行では false にする
  - **実行中の snotra を kill する**（`smoke-egui.ps1` と同じくローカル実行時は注意）

  読み方: PrivWS は hide 経路の `EmptyWorkingSet` で trim された後の値になりうるため、
  確保需要の軸は **PrivComm** を見る（詳細は PERFORMANCE.md「egui 期のメモリ実測」）。

.PARAMETER Label
  出力に付ける見出し（A/B 比較時の変種名。例 "font=HackGen" / "font=解決不能"）。

.PARAMETER Query
  段階 C で注入する検索クエリ。開発機の索引に一致する文字列を指定する。

.PARAMETER ExePath
  計測対象の実行ファイル。空ならこのスクリプトが住むリポジトリの release 本体を導く。
  明示した相対パスは cwd 相対のまま解釈する。

.PARAMETER HotkeyVks
  段階 B で注入する VK 列の明示 override（例 "18,81" = Alt+Q）。**通常は指定しない**——
  未指定なら `hotkey:registered` trace から導く。

.EXAMPLE
  npm run measure:memory:stages
  pwsh -NoProfile -File scripts/measure-memory-stages.ps1 -Label "font=解決不能" -Query notepad
#>
# 位置引数を拒む（#1296）。理由と検査は `lib/SnotraSmoke.Tests.ps1` の PositionalBinding の Describe
[CmdletBinding(PositionalBinding = $false)]
param(
    [string]$Label = '',
    [string]$Query = 'code',
    # 空なら release 本体を導く（#1179・#1293）。**理由の正本は `Resolve-SnotraCargoExecutable` の doc**。
    [string]$ExePath = '',
    [string]$HotkeyVks = '',
    # 起動の終端を待つ上限。索引キャッシュが外れた起動は全走査を含む（実 config で約 83 秒）。
    [int]$StartupTimeoutSeconds = 240
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Import-Module (Join-Path $PSScriptRoot 'lib/SnotraSmoke.psm1') -Force

$measure = Join-Path $PSScriptRoot 'measure-memory.ps1'
if (-not (Test-Path $measure)) { throw "測定器が見つかりません: $measure" }
# 導出と存在の確認は、下の `Stop-Process -Name snotra` より前に済ませる——本体が無いのに
# 実行中の snotra を落としてから止まる、という形を作らない。
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $ExePath) { $ExePath = Resolve-SnotraCargoExecutable -RepositoryRoot $repoRoot -Profile release }
if (-not (Test-Path -LiteralPath $ExePath)) { throw "実行ファイルが見つかりません: $ExePath（先に cargo build --release -p snotra）" }
$ExePath = (Resolve-Path -LiteralPath $ExePath).Path   # 出力へ載せる形（理由は resolver の doc）
Write-Host "本体: $ExePath"

# `&` = 同一プロセス実行（新コンソールを作らない）。フォーカスを保つための必須条件。
function Invoke-Stage {
    param([string]$Stage)
    # measure-memory.ps1 は**失敗時のみ** `exit`（1: プロセス不在 / 2: perf 0 件）する。
    # 成功時は最後まで流れて終わるため $LASTEXITCODE は更新されない。直前の値が残ると
    # 成功を失敗と誤判定するので、呼ぶ前に 0 で潰してから判定する（誤爆を実測で確認済み）。
    $global:LASTEXITCODE = 0
    & $measure -Label "$Label / $Stage"
    if ($LASTEXITCODE -ne 0) {
        throw "[$Stage] サンプリング失敗 (exit $LASTEXITCODE)。0 MiB と読んではならない。"
    }
}

function Send-Keys {
    param([string]$Keys, [int]$WaitSeconds)
    [System.Windows.Forms.SendKeys]::SendWait($Keys)
    Start-Sleep -Seconds $WaitSeconds
}

Add-Type -Namespace SnotraMeasure -Name Win32 -MemberDefinition @'
[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
[DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr hWnd, out int lpdwProcessId);
'@

# SendKeys は「前景ウィンドウ」へ届く。Snotra が前景でないまま打鍵すると、キーは
# 別のウィンドウへ流れ、段階 C が「検索していないのに成長ゼロの標本」として**無言で**残る
# （2 回踏んだ）。打つ前に前景の PID を確かめ、違えば止める——
# 「測れなかった」と「増えなかった」を区別できない測定器は使えない。
function Assert-SnotraForeground {
    param([string]$Stage)
    $hwnd = [SnotraMeasure.Win32]::GetForegroundWindow()
    $fgPid = 0
    [void][SnotraMeasure.Win32]::GetWindowThreadProcessId($hwnd, [ref]$fgPid)
    $ours = @(Get-Process -Name snotra -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })
    if ($ours -notcontains $fgPid) {
        $who = (Get-Process -Id $fgPid -ErrorAction SilentlyContinue).ProcessName
        throw "[$Stage] 前景が Snotra ではない（PID $fgPid = $who）。この状態の打鍵は別のウィンドウへ流れ、標本が無言で無効になる。"
    }
}

Write-Host ''
Write-Host "########## 段階別メモリ実測: $Label ##########" -ForegroundColor Cyan

Stop-Process -Name snotra -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2

# **finally で必ず落とす**。段階の途中で throw すると（実際に踏んだ）計測対象が起動したまま
# 残り、次の `cargo build` が exe を掴まれて「アクセスが拒否されました」で落ちる。
# 起動したものを片付ける責務は、成功経路だけに置いてはならない。
$errPaths = @()

# 起動して、起動の終端（`startup:ready` / `startup:failed`）を待つ。終端の data を返す。
# 実 config を読ませるので `Start-SnotraProcess`（検証用プロファイル専用）は使わない。
function Start-TracedSnotra {
    $path = Join-Path $env:TEMP ("snotra-measure-stages-{0}.stderr.log" -f [guid]::NewGuid().ToString('N'))
    $script:errPaths += $path
    Invoke-SnotraEnvironment -Variables @{ SNOTRA_TRACE = '1' } -ScriptBlock {
        Start-Process -FilePath $ExePath -RedirectStandardError $path | Out-Null
    }
    $launchedAt = Get-Date
    Write-Host "trace: $path"
    $terminal = Wait-SnotraTraceCondition -Path $path -TimeoutMs ($StartupTimeoutSeconds * 1000) `
        -Description 'trace の startup:ready / startup:failed' `
        -Predicate { $_.event -eq 'startup:ready' -or $_.event -eq 'startup:failed' }
    if ($null -eq $terminal) { throw "起動の終端を ${StartupTimeoutSeconds} 秒以内に観測できない（$path）。" }
    if (-not $terminal.data.ok) { throw "本体の起動が失敗した（reason=$($terminal.data.reason)）。" }
    [pscustomobject]@{ Path = $path; LaunchedAt = $launchedAt; Data = $terminal.data }
}

# **finally で必ず落とす**。段階の途中で throw すると（実際に踏んだ）計測対象が起動したまま
# 残り、次の `cargo build` が exe を掴まれて「アクセスが拒否されました」で落ちる。
# 起動したものを片付ける責務は、成功経路だけに置いてはならない。
try {
    # **段階 A は、索引キャッシュに当たった起動だけで測る。** 外れると起動の中で全走査が走り
    # （実 config で約 83 秒・2026-11-03）、14 秒後の段階 A は走査の途中を測って何も言わずに
    # 出力していた。外れたら、走査が保存したキャッシュに当てるため 1 度だけ再起動する。
    $started = Start-TracedSnotra
    if (-not $started.Data.cache_hit) {
        Write-Host ("索引キャッシュが外れていた（first_run={0}・index_load={1} ms）。キャッシュに当てるため再起動する。" -f `
            $started.Data.first_run, $started.Data.index_load_ms) -ForegroundColor Yellow
        Stop-Process -Name snotra -Force -ErrorAction SilentlyContinue
        Start-Sleep -Seconds 2
        $started = Start-TracedSnotra
        if (-not $started.Data.cache_hit) {
            throw "再起動しても索引キャッシュに当たらない（first_run=$($started.Data.first_run)）。段階 A を測れない。"
        }
    }
    $errPath = $started.Path
    Write-Host ("索引キャッシュに当たった起動を測る（index_load={0} ms）。" -f $started.Data.index_load_ms)
    # 起動から 14 秒後に測る。**この 14 秒は背景再スキャン（#1001 で撤去済み）が走っていた頃の
    # 見積もりであり、今は余裕が過大である。** 縮めると段階 A が測る対象そのものが変わる
    # （まだ動いている初期化を含む値になる）ので、**縮めるなら新しい値を実測してから**——
    # 過去の A/B とも比較できなくなる。起点は上の終端の待ちの前（起動の時刻）に置き、従来と揃える。
    $remaining = 14 - ((Get-Date) - $started.LaunchedAt).TotalSeconds
    if ($remaining -gt 0) { Start-Sleep -Milliseconds ([int]($remaining * 1000)) }
    Invoke-Stage 'A. アイドル（一度も表示せず）'

    # 注入する VK 列は段階 A の後で読む（14 秒の待機の前後で段階 A の条件を変えない）。
    # 待ちの形・失敗時の止め方は smoke-egui.ps1 と同じだが、共有関数へは寄せていない:
    # あちらは起動レイテンシの予算と報告を同じ箇所に持ち、こちらは要らない。
    if ($PSBoundParameters.ContainsKey('HotkeyVks')) {
        $vks = @($HotkeyVks -split ',' | ForEach-Object { [byte]([int]$_.Trim()) })
        Write-Host "Hotkey VKs (explicit): $($vks -join ',')"
    } else {
        $hkEvent = Wait-SnotraTraceEvent -Path $errPath -EventName 'hotkey:registered' -TimeoutMs 10000
        if ($null -eq $hkEvent) { throw "[B] hotkey:registered を trace で観測できない（$errPath）。注入するキーを決められない。" }
        $hk = $hkEvent.data
        if (-not $hk.ok) { throw "[B] アプリがホットキーの登録に失敗している（modifier=$($hk.modifier) key=$($hk.key)）。" }
        $vks = @($hk.vks | ForEach-Object { [byte][int]$_ })
        Write-Host "Hotkey VKs (from trace): $($vks -join ',') (modifier=$($hk.modifier) key=$($hk.key))"
    }
    if ($vks.Count -lt 1) { throw "[B] 注入する VK 列が空" }
    # keybd_event で押下順に押し、逆順で離す。Alt を含むと Alt の解放で表示が確定する
    # （ShowAfterAltRelease）ので、SendKeys の '%' ではなくこちらを使う。
    Send-SnotraKeyChord -VirtualKeys $vks
    Start-Sleep -Seconds 4
    Assert-SnotraForeground 'B'  # ここで前景を取れていないと以後の打鍵が全部無駄になる
    Invoke-Stage 'B. 表示（空クエリ）'

    Assert-SnotraForeground 'C'
    Send-Keys $Query 6           # 検索 + アイコン抽出・テクスチャ化の収束待ち
    Invoke-Stage "C. 検索実行後（query=$Query）"

    Send-Keys '{ESC}' 5          # hide → EmptyWorkingSet trim
    Invoke-Stage 'D. hide 後（trim 済み）'
}
finally {
    Stop-Process -Name snotra -Force -ErrorAction SilentlyContinue
}
Write-Host "trace は $($errPaths -join ' / ') に残した（実パスを含む。不要なら消す）" -ForegroundColor DarkGray
Write-Host "########## 終了: $Label ##########" -ForegroundColor Cyan
Write-Host '注: PrivWS は trim 後の値になりうる。確保需要は PrivComm を見ること。' -ForegroundColor DarkGray
