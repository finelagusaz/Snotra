#Requires -Version 7

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:dpiAwarenessInitialized = $false

function Initialize-SnotraNativeInterop {
    if ($null -eq ('SnotraSmokeInterop.Native' -as [type])) {
        Add-Type -Namespace SnotraSmokeInterop -Name Native -MemberDefinition @'
[DllImport("user32.dll")]
public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, UIntPtr dwExtraInfo);
[DllImport("user32.dll", CharSet = CharSet.Unicode)]
public static extern IntPtr FindWindowW(string cls, string title);
[DllImport("user32.dll")]
public static extern bool GetWindowRect(IntPtr hWnd, out RECT value);
[DllImport("user32.dll")]
public static extern bool IsWindowVisible(IntPtr hWnd);
[DllImport("user32.dll")]
public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
[DllImport("user32.dll")]
public static extern bool SetForegroundWindow(IntPtr hWnd);
[DllImport("user32.dll")]
public static extern IntPtr GetForegroundWindow();
[DllImport("user32.dll", CharSet = CharSet.Unicode)]
public static extern int GetWindowTextW(IntPtr hWnd, System.Text.StringBuilder text, int count);
[DllImport("user32.dll")]
public static extern bool SetProcessDpiAwarenessContext(IntPtr value);
[DllImport("user32.dll")]
public static extern bool SetProcessDPIAware();
[DllImport("dwmapi.dll")]
public static extern int DwmGetWindowAttribute(IntPtr hWnd, int attr, out RECT value, int size);
[DllImport("wtsapi32.dll", SetLastError = true)]
public static extern bool WTSQuerySessionInformationW(IntPtr server, int sessionId, int infoClass, out IntPtr buffer, out int bytesReturned);
[DllImport("wtsapi32.dll")]
public static extern void WTSFreeMemory(IntPtr memory);
public struct RECT { public int Left, Top, Right, Bottom; }
'@
    }
}

# WTSQuerySessionInformationW の引数（`WTSSessionInfoEx` = 25・`WTS_CURRENT_SESSION` = -1）と
# `WTSINFOEX_LEVEL1_W.SessionFlags` の値。**Windows 7 では LOCK/UNLOCK の意味が逆**だが、
# 本リポジトリの対象は Windows 10/11 ゆえ documented のまま扱う。
$script:WtsSessionInfoEx = 25
$script:WtsCurrentSession = -1
$script:WtsSessionStateLock = 0
$script:WtsSessionStateUnlock = 1

<#
.SYNOPSIS
`WTSINFOEXW` のバイト列から SessionFlags を読む純関数。

.DESCRIPTION
**構造体のオフセットを自前で仮定するため、同じバッファから読める SessionId で検算する。**
`WTSINFOEXW` は `Level`(DWORD) の後に union が続き、union の中身（`WTSINFOEX_LEVEL1_W`）は
`LARGE_INTEGER` を含むので 8 バイト境界へ揃う。ゆえに SessionId は 8、SessionState は 12、
SessionFlags は 16 に来る。**この仮定が外れれば SessionId が呼び出し元のセッションと合わなくなる**
ので、合わなければ「読めた」と言わずに落とす（誤ったオフセットから読んだ 0 が
「ロック中」に化けるのを防ぐ・fail-closed）。
#>
function ConvertFrom-SnotraWtsInfoEx {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [byte[]]$Buffer,
        [Parameter(Mandatory)]
        [int]$ExpectedSessionId
    )

    if ($Buffer.Length -lt 20) {
        throw "WTSINFOEXW が短すぎます（$($Buffer.Length) バイト・20 バイト以上が要る）。"
    }
    $level = [BitConverter]::ToInt32($Buffer, 0)
    if ($level -ne 1) {
        throw "WTSINFOEXW.Level が 1 ではありません（$level）。SessionFlags の位置を仮定できません。"
    }
    $sessionId = [BitConverter]::ToInt32($Buffer, 8)
    if ($sessionId -ne $ExpectedSessionId) {
        throw "WTSINFOEXW の SessionId が呼び出し元と一致しません（構造体 $sessionId / 呼び出し元 $ExpectedSessionId）。オフセットの仮定が崩れています。"
    }
    $flags = [BitConverter]::ToInt32($Buffer, 16)
    if ($flags -ne $script:WtsSessionStateLock -and $flags -ne $script:WtsSessionStateUnlock) {
        throw "SessionFlags が LOCK(0)/UNLOCK(1) のいずれでもありません（$flags）。ロック状態を判定できません。"
    }

    [pscustomobject]@{
        SessionId = $sessionId
        SessionState = [BitConverter]::ToInt32($Buffer, 12)
        SessionFlags = $flags
        Locked = ($flags -eq $script:WtsSessionStateLock)
    }
}

<#
.SYNOPSIS
現在のセッションがロックされているかを返す（判定不能なら throw）。

.DESCRIPTION
**デスクトップ名（`OpenInputDesktop` + `GetUserObjectInformation`）では判定できない。**
近年の Windows はロック画面（LockApp）を `Default` デスクトップ上で動かすため、ロック中でも
`Default` が返る（2026-08-01 実測の false negative・#866）。
#>
function Get-SnotraSessionLockState {
    [CmdletBinding()]
    param()

    Initialize-SnotraNativeInterop
    $buffer = [IntPtr]::Zero
    $bytes = 0
    if (-not [SnotraSmokeInterop.Native]::WTSQuerySessionInformationW(
            [IntPtr]::Zero, $script:WtsCurrentSession, $script:WtsSessionInfoEx, [ref]$buffer, [ref]$bytes)) {
        $code = [System.Runtime.InteropServices.Marshal]::GetLastWin32Error()
        throw "WTSQuerySessionInformationW に失敗しました（Win32 error $code）。ロック状態を判定できません。"
    }
    try {
        $bytesArray = New-Object byte[] $bytes
        [System.Runtime.InteropServices.Marshal]::Copy($buffer, $bytesArray, 0, $bytes)
    } finally {
        [SnotraSmokeInterop.Native]::WTSFreeMemory($buffer)
    }
    ConvertFrom-SnotraWtsInfoEx -Buffer $bytesArray -ExpectedSessionId (Get-Process -Id $PID).SessionId
}

<#
.SYNOPSIS
画面がロックされていたら、何をすればよいかを名指しして止める。

.DESCRIPTION
**ロック中は窓が Win32 の視点では生きたままである**（`IsWindowVisible` は真・矩形も妥当・
DPI も正しい）ため、既存のガードはすべて通り、`CopyFromScreen` は**ロック画面の中身を持つ
有効な Bitmap** を返す。判定側が落ちたとしてもメッセージは色や描画経路を指すので、読んだ人は
レンダリングを疑い始める（#866 実測）。ここで名指しして止める。

**倒す向きを 2 つに分ける。** 「ロック中と判定できた」は throw、「**判定できなかった**」は警告のみで
続行する。この関数の仕事は*誤った結果*を防ぐことであり、状態を読めないホストで実行そのものを
拒めば、情報を足さずに道具を失う。加えて `Get-SnotraWindowCapture` は Pester の Integration
テスト経由で **CI（GitHub Actions の Windows runner）でも走る**——判定不能を throw に倒すと、
runner の WTS の振る舞いが未知のまま CI を壊しうる。**ロックという確定した事実にだけ強く倒す。**
#>
function Assert-SnotraSessionUnlocked {
    [CmdletBinding()]
    param(
        [string]$Operation = 'この操作'
    )

    $state = $null
    try {
        $state = Get-SnotraSessionLockState
    } catch {
        Write-Warning "画面のロック状態を判定できませんでした（$($_.Exception.Message)）。ロック中であれば、以降の結果は画面の中身を反映しません（#866）。"
        return
    }
    if ($state.Locked) {
        throw "画面がロックされているため $Operation を実行できません。ロック中は窓が可視のままでも画面に合成されず、キャプチャはロック画面の中身を返します（#866）。画面のロックを解除してから実行してください。"
    }
}

function Initialize-SnotraDpiAwareness {
    Initialize-SnotraNativeInterop
    if ($script:dpiAwarenessInitialized) { return }

    # DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 = -4。既に awareness が設定済みなら
    # Win32 は false を返すため、成否を検査の合否には使わない。
    if (-not [SnotraSmokeInterop.Native]::SetProcessDpiAwarenessContext([IntPtr]::new(-4))) {
        [void][SnotraSmokeInterop.Native]::SetProcessDPIAware()
    }
    $script:dpiAwarenessInitialized = $true
}

function New-SnotraVerificationProfile {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$ProfileDir,
        [string]$AdditionalSections = '',
        [string]$PathEntries = '',
        [string]$HotkeyModifier = 'Alt',
        [string]$HotkeyKey = 'Q',
        [int]$WindowWidth = 600,
        # $false で `results_view.rs` の `request_icons_for_results` が即 return し、
        # icon worker も `SHGetFileInfoW` も走らなくなる。**runner では `SHGetFileInfoW` が
        # `exists:true` のパスに対して `ShellQueryFailed(1008)` を返し**、失敗は
        # `IconFailure::is_transient` が true ゆえ恒久 latch されず再要求され続ける
        # ——アイコンを判定に使わない検査では、この空回りが観測時間を押し広げるだけになる（#872）。
        [bool]$ShowIcons = $true
    )

    New-Item -ItemType Directory -Force -Path $ProfileDir | Out-Null
    Remove-Item -LiteralPath (Join-Path $ProfileDir 'config.toml.bak') -Force -ErrorAction SilentlyContinue
    Get-ChildItem -LiteralPath $ProfileDir -Filter '*.bin' -ErrorAction SilentlyContinue | Remove-Item -Force

    # TOML の真偽値は小文字であり、PowerShell の $true は "True" へ補間される。
    $showIconsToml = $ShowIcons.ToString().ToLowerInvariant()
    $parts = @(
        @"
[hotkey]
modifier = "$HotkeyModifier"
key = "$HotkeyKey"

[appearance]
window_width = $WindowWidth
show_icons = $showIconsToml
"@.Trim()
    )
    if (-not [string]::IsNullOrWhiteSpace($AdditionalSections)) {
        $parts += $AdditionalSections.Trim()
    }
    $parts += '[paths]'
    if (-not [string]::IsNullOrWhiteSpace($PathEntries)) {
        $parts += $PathEntries.Trim()
    }

    $configPath = Join-Path $ProfileDir 'config.toml'
    Set-Content -LiteralPath $configPath -Value (($parts -join "`r`n`r`n") + "`r`n") -Encoding utf8
    $fullPath = (Resolve-Path -LiteralPath $ProfileDir).Path

    [pscustomobject]@{
        Directory = $ProfileDir
        FullPath = $fullPath
        ConfigPath = (Resolve-Path -LiteralPath $configPath).Path
    }
}

function Invoke-SnotraEnvironment {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [hashtable]$Variables,
        [Parameter(Mandatory)]
        [scriptblock]$ScriptBlock
    )

    foreach ($name in $Variables.Keys) {
        if ($name -notmatch '^[A-Za-z_][A-Za-z0-9_]*$') {
            throw "環境変数名が不正です: $name"
        }
    }

    $saved = @{}
    try {
        foreach ($name in $Variables.Keys) {
            $saved[$name] = [pscustomobject]@{
                Exists = Test-Path -LiteralPath "Env:$name"
                Value = [Environment]::GetEnvironmentVariable($name, 'Process')
            }
            Set-Item -LiteralPath "Env:$name" -Value ([string]$Variables[$name])
        }
        & $ScriptBlock
    } finally {
        foreach ($name in $saved.Keys) {
            $prior = $saved[$name]
            if ($prior.Exists) {
                Set-Item -LiteralPath "Env:$name" -Value $prior.Value
            } else {
                Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue
            }
        }
    }
}

function Start-SnotraProcess {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$ConfigDir,
        [Parameter(Mandatory)]
        [string]$FilePath,
        [string[]]$ArgumentList,
        [string]$StandardErrorPath,
        [string]$StandardOutputPath,
        [switch]$NoNewWindow,
        [switch]$Trace
    )

    $variables = @{ SNOTRA_CONFIG_DIR = $ConfigDir }
    if ($Trace) { $variables.SNOTRA_TRACE = '1' }

    $startParameters = @{
        FilePath = $FilePath
        PassThru = $true
    }
    if ($PSBoundParameters.ContainsKey('ArgumentList')) { $startParameters.ArgumentList = $ArgumentList }
    if ($PSBoundParameters.ContainsKey('StandardErrorPath')) { $startParameters.RedirectStandardError = $StandardErrorPath }
    if ($PSBoundParameters.ContainsKey('StandardOutputPath')) { $startParameters.RedirectStandardOutput = $StandardOutputPath }
    if ($NoNewWindow) { $startParameters.NoNewWindow = $true }

    Invoke-SnotraEnvironment -Variables $variables -ScriptBlock {
        Start-Process @startParameters
    }
}

function Resolve-SnotraCargoExecutable {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$RepositoryRoot,
        [ValidateSet('debug', 'release')]
        [string]$Profile = 'debug'
    )

    $manifestPath = Join-Path $RepositoryRoot 'Cargo.toml'
    if (-not (Test-Path -LiteralPath $manifestPath)) {
        throw "Cargo workspace の manifest がありません: $manifestPath"
    }

    $metadataOutput = & cargo metadata --no-deps --format-version 1 --manifest-path $manifestPath
    if ($LASTEXITCODE -ne 0) {
        throw "cargo metadata に失敗しました（exit=$LASTEXITCODE）。"
    }
    try {
        $metadata = ($metadataOutput -join [Environment]::NewLine) | ConvertFrom-Json
    } catch {
        throw "cargo metadata の JSON を解釈できません: $($_.Exception.Message)"
    }
    if ([string]::IsNullOrWhiteSpace([string]$metadata.target_directory)) {
        throw 'cargo metadata に target_directory がありません。'
    }

    Join-Path ([string]$metadata.target_directory) "$Profile/snotra.exe"
}

function Resolve-SnotraExistingProcess {
    [CmdletBinding()]
    param(
        [ValidateSet('Stop', 'Reject')]
        [string]$Policy,
        [string]$ProcessName = 'snotra'
    )

    $existing = @(Get-Process -Name $ProcessName -ErrorAction SilentlyContinue)
    if ($existing.Count -eq 0) { return }

    if ($Policy -eq 'Reject') {
        throw "Snotra が既に起動しています（pid=$($existing.Id -join ', ')）。single-instance により検証が空振りするため、終了してから再実行してください。"
    }

    foreach ($process in $existing) {
        Stop-Process -Id $process.Id -Force
    }
}

function Read-SnotraTraceEvents {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path)) { return }
    ConvertFrom-SnotraTraceLine -Line (Get-Content -LiteralPath $Path -ErrorAction SilentlyContinue)
}

<#
.SYNOPSIS
既に読み込んだ行から trace イベントを取り出す（parse の唯一の実装）。

.DESCRIPTION
`Read-SnotraTraceEvents` と `Read-SnotraTraceSnapshot` の**共通の parse 本体**である。
スナップショットは「行数」と「parse 成功数」の差で捨てた行を数えるので、**両者が同じ 1 回の
読み取りを見なければならない**——別々に読むと、稼働中のアプリが間に書き足した行で差が
壊れる（parse 成功が行数を上回りうる・code-review M1）。
#>
function ConvertFrom-SnotraTraceLine {
    [CmdletBinding()]
    param(
        [AllowEmptyCollection()]
        [string[]]$Line = @()
    )

    foreach ($text in $Line) {
        if ($text -notmatch '^\[trace\]\s+(.+)$') { continue }
        try {
            $Matches[1] | ConvertFrom-Json
        } catch {
            # 書き込み途中の末尾行や壊れた診断行は、次回の読取りで再評価する。
        }
    }
}

<#
.SYNOPSIS
trace ファイルを 1 度読み、判定器へ渡す形（イベント列と「捨てた行」の数）にして返す。

.DESCRIPTION
**「捨てた行」は `[trace]` で始まるのに `Read-SnotraTraceEvents` が返さなかった行だけを数える。**
stderr には非 trace の診断行（`[index-load] ...` 等）が混じるため、素朴に「全行 − parse 成功」
で数えると**正常な実行が毎回 degrade して検出器が無意味になる**（#757 で実測: 実ログ 25 行の
うち 1 行が非 trace の診断行だった）。

この数え方は `Test-SnotraTraceInvariants -DroppedLineCount` の意味を決める判定規則であり、
**呼び出し側に写しを置かない**——片方がドリフトすると片方の smoke だけが誤って degrade する。
#>
function Read-SnotraTraceSnapshot {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [AllowEmptyString()]
        [AllowNull()]
        [string]$Path
    )

    if ([string]::IsNullOrEmpty($Path) -or -not (Test-Path -LiteralPath $Path)) {
        return @{ Available = $false; Lines = @(); Events = @(); TraceLines = 0; Dropped = 0 }
    }
    # **1 度だけ読む。** 行数と parse 成功数を別々の読み取りから取ると、稼働中のアプリが間に
    # 書き足した行で差が壊れる（code-review M1）。**生行も返す**のは同じ理由で——presence の
    # 表示と不変条件の判定が別時点のファイルを見ないようにするため。
    $lines = @(Get-Content -LiteralPath $Path -ErrorAction SilentlyContinue)
    $traceLineCount = 0
    foreach ($text in $lines) { if ($text -match '^\[trace\]\s+') { $traceLineCount++ } }
    $events = @(ConvertFrom-SnotraTraceLine -Line $lines)
    return @{
        Available  = $true
        Lines      = $lines
        Events     = $events
        TraceLines = $traceLineCount
        Dropped    = [Math]::Max(0, $traceLineCount - $events.Count)
    }
}

function Wait-SnotraTraceEvent {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$Path,
        [Parameter(Mandatory)]
        [string]$EventName,
        [Parameter(Mandatory)]
        [int]$TimeoutMs,
        [int]$PollMs = 200
    )

    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    while ([DateTime]::UtcNow -lt $deadline) {
        $match = @(Read-SnotraTraceEvents -Path $Path | Where-Object { $_.event -eq $EventName } | Select-Object -Last 1)
        if ($match.Count -gt 0) { return $match[0] }
        Start-Sleep -Milliseconds $PollMs
    }
    return $null
}

function Send-SnotraKey {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [byte]$VirtualKey,
        [switch]$Up
    )

    Initialize-SnotraNativeInterop
    $flags = if ($Up) { 0x2 } else { 0 }
    [SnotraSmokeInterop.Native]::keybd_event($VirtualKey, 0, $flags, [UIntPtr]::Zero)
}

function Send-SnotraKeyChord {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [byte[]]$VirtualKeys,
        [int]$InterKeyDelayMs = 50
    )

    foreach ($vk in $VirtualKeys) {
        Send-SnotraKey -VirtualKey $vk
        if ($InterKeyDelayMs -gt 0) { Start-Sleep -Milliseconds $InterKeyDelayMs }
    }
    for ($i = $VirtualKeys.Count - 1; $i -ge 0; $i--) {
        Send-SnotraKey -VirtualKey $VirtualKeys[$i] -Up
        if ($InterKeyDelayMs -gt 0) { Start-Sleep -Milliseconds $InterKeyDelayMs }
    }
}

function Wait-SnotraWindow {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$Title,
        [System.Diagnostics.Process]$Process,
        [int]$TimeoutMs = 30000,
        [int]$PollMs = 200
    )

    Initialize-SnotraNativeInterop
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    while ([DateTime]::UtcNow -lt $deadline) {
        $handle = [SnotraSmokeInterop.Native]::FindWindowW([NullString]::Value, $Title)
        $ownerMatches = $true
        if ($handle -ne [IntPtr]::Zero -and $null -ne $Process) {
            [uint32]$ownerPid = 0
            [void][SnotraSmokeInterop.Native]::GetWindowThreadProcessId($handle, [ref]$ownerPid)
            $ownerMatches = ($ownerPid -eq $Process.Id)
        }
        if ($handle -ne [IntPtr]::Zero -and $ownerMatches -and [SnotraSmokeInterop.Native]::IsWindowVisible($handle)) {
            return $handle
        }
        if ($null -ne $Process -and $Process.HasExited) {
            throw "本体が終了しました（exit=$($Process.ExitCode)）。窓 '$Title' を観測できません。"
        }
        Start-Sleep -Milliseconds $PollMs
    }
    throw "窓 '$Title' が ${TimeoutMs}ms 以内に現れませんでした。"
}

function Get-SnotraForegroundWindow {
    [CmdletBinding()]
    param()

    Initialize-SnotraNativeInterop
    return [SnotraSmokeInterop.Native]::GetForegroundWindow()
}

# 前面を握っている窓を人が読める形にする。**失敗したときの一次証拠**（誰に打鍵が届いていたか）
# はその場でしか採れないため、Set-SnotraForegroundWindow の警告へ載せる。
function Get-SnotraForegroundWindowLabel {
    [CmdletBinding()]
    param()

    $handle = Get-SnotraForegroundWindow
    if ($handle -eq [IntPtr]::Zero) { return '前面窓なし' }

    $title = New-Object System.Text.StringBuilder 256
    [void][SnotraSmokeInterop.Native]::GetWindowTextW($handle, $title, $title.Capacity)
    [uint32]$ownerPid = 0
    [void][SnotraSmokeInterop.Native]::GetWindowThreadProcessId($handle, [ref]$ownerPid)
    return "0x$($handle.ToString('X')) pid=$ownerPid title='$($title.ToString())'"
}

<#
.SYNOPSIS
窓を前面へ出し、**実際に前面になったか**を返す。

.DESCRIPTION
`SetForegroundWindow` の戻り値を答えにしない。前面ロックの規則により、**対象が既に前面でも
FALSE が返る**（CI runner で実測・run #1280）。呼び出し側が知りたいのは「注入した打鍵の宛先が
その窓に定まったか」であり、それを表すのは `GetForegroundWindow` との一致だけである。
奪取が一度で通らない環境のために $TimeoutMs まで再試行し、諦めるときは前面を握っていた窓を
警告へ残す。
#>
function Set-SnotraForegroundWindow {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [IntPtr]$Handle,
        [int]$TimeoutMs = 3000,
        [int]$PollMs = 150
    )

    Initialize-SnotraNativeInterop
    # 前面窓が無い環境では GetForegroundWindow も Zero を返す。Zero 同士の一致を「前面化できた」
    # と読むと打鍵の宛先が無いまま合格するので、比較の前に落とす（fail-closed）。
    if ($Handle -eq [IntPtr]::Zero) {
        throw '前面化する窓のハンドルが Zero です。'
    }

    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    while ($true) {
        [void][SnotraSmokeInterop.Native]::SetForegroundWindow($Handle)
        if ((Get-SnotraForegroundWindow) -eq $Handle) { return $true }
        if ([DateTime]::UtcNow -ge $deadline) { break }
        Start-Sleep -Milliseconds $PollMs
    }

    Write-Warning ("窓 0x$($Handle.ToString('X')) を ${TimeoutMs}ms 以内に前面化できません" +
        "（前面: $(Get-SnotraForegroundWindowLabel)）。")
    return $false
}

function Get-SnotraWindowCapture {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [IntPtr]$Handle
    )

    Initialize-SnotraDpiAwareness
    Add-Type -AssemblyName System.Drawing

    # **可視判定より先に見る。** ロック中は IsWindowVisible も矩形も真っ当な値を返し、
    # ここから下は最後まで成功して**中身だけが違う Bitmap** を返す（#866）。
    # 判定不能を throw へ倒さない理由は Assert-SnotraSessionUnlocked のコメント。
    Assert-SnotraSessionUnlocked -Operation '窓のキャプチャ'

    if (-not [SnotraSmokeInterop.Native]::IsWindowVisible($Handle)) {
        throw 'キャプチャ対象の窓が可視ではありません。'
    }

    $rect = New-Object SnotraSmokeInterop.Native+RECT
    $usedDwmBounds = ([SnotraSmokeInterop.Native]::DwmGetWindowAttribute($Handle, 9, [ref]$rect, 16) -eq 0)
    if (-not $usedDwmBounds -and -not [SnotraSmokeInterop.Native]::GetWindowRect($Handle, [ref]$rect)) {
        throw 'DwmGetWindowAttribute と GetWindowRect の両方に失敗しました。'
    }

    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    if ($width -le 0 -or $height -le 0) {
        throw "窓の矩形が不正です: ${width}x${height}"
    }

    $bitmap = New-Object System.Drawing.Bitmap($width, $height)
    try {
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
        } finally {
            $graphics.Dispose()
        }
    } catch {
        $bitmap.Dispose()
        throw
    }

    # Bitmap の Dispose は呼び出し側の責務。判定ロジックをモジュールへ持ち込まないため、
    # 画像と矩形だけを返す。
    [pscustomobject]@{
        Bitmap = $bitmap
        Rect = $rect
        Width = $width
        Height = $height
        UsedDwmBounds = $usedDwmBounds
    }
}

Export-ModuleMember -Function @(
    'New-SnotraVerificationProfile'
    'Invoke-SnotraEnvironment'
    'Start-SnotraProcess'
    'Resolve-SnotraCargoExecutable'
    'Resolve-SnotraExistingProcess'
    'Read-SnotraTraceEvents'
    'Read-SnotraTraceSnapshot'
    'Wait-SnotraTraceEvent'
    'Send-SnotraKey'
    'Send-SnotraKeyChord'
    'Wait-SnotraWindow'
    'Get-SnotraForegroundWindow'
    'Set-SnotraForegroundWindow'
    'Get-SnotraWindowCapture'
    'ConvertFrom-SnotraWtsInfoEx'
    'Get-SnotraSessionLockState'
    'Assert-SnotraSessionUnlocked'
)
