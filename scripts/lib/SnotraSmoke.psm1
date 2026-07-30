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
public static extern bool SetProcessDpiAwarenessContext(IntPtr value);
[DllImport("user32.dll")]
public static extern bool SetProcessDPIAware();
[DllImport("dwmapi.dll")]
public static extern int DwmGetWindowAttribute(IntPtr hWnd, int attr, out RECT value, int size);
public struct RECT { public int Left, Top, Right, Bottom; }
'@
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
        [int]$WindowWidth = 600
    )

    New-Item -ItemType Directory -Force -Path $ProfileDir | Out-Null
    Remove-Item -LiteralPath (Join-Path $ProfileDir 'config.toml.bak') -Force -ErrorAction SilentlyContinue
    Get-ChildItem -LiteralPath $ProfileDir -Filter '*.bin' -ErrorAction SilentlyContinue | Remove-Item -Force

    $parts = @(
        @"
[hotkey]
modifier = "$HotkeyModifier"
key = "$HotkeyKey"

[appearance]
window_width = $WindowWidth
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
    foreach ($line in Get-Content -LiteralPath $Path -ErrorAction SilentlyContinue) {
        if ($line -notmatch '^\[trace\]\s+(.+)$') { continue }
        try {
            $Matches[1] | ConvertFrom-Json
        } catch {
            # 書き込み途中の末尾行や壊れた診断行は、次回の読取りで再評価する。
        }
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

function Set-SnotraForegroundWindow {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [IntPtr]$Handle
    )

    Initialize-SnotraNativeInterop
    [SnotraSmokeInterop.Native]::SetForegroundWindow($Handle)
}

function Get-SnotraWindowCapture {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [IntPtr]$Handle
    )

    Initialize-SnotraDpiAwareness
    Add-Type -AssemblyName System.Drawing

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
    'Wait-SnotraTraceEvent'
    'Send-SnotraKey'
    'Send-SnotraKeyChord'
    'Wait-SnotraWindow'
    'Set-SnotraForegroundWindow'
    'Get-SnotraWindowCapture'
)
