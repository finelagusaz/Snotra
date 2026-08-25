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
[DllImport("user32.dll", CharSet = CharSet.Unicode)]
public static extern int GetClassNameW(IntPtr hWnd, System.Text.StringBuilder text, int count);
[DllImport("user32.dll")]
public static extern bool SetProcessDpiAwarenessContext(IntPtr value);
[DllImport("user32.dll")]
public static extern bool SetProcessDPIAware();
[DllImport("user32.dll")]
public static extern uint GetDpiForWindow(IntPtr hWnd);
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

<#
.SYNOPSIS
窓が載っているモニタの実効 DPI を返す（96 が 100%）。

.DESCRIPTION
**awareness を通さずに DPI を読むと 96 が返る。** DPI 非対応のプロセスには Win32 が仮想化した
値を見せるためで、**エラーにならず「100% である」と読める値**が返る。2026-08-16 に実測した
125% の機体では、`GetDeviceCaps(LOGPIXELSX)` が 96 を、`System.Windows.Forms.Screen` が
物理 1920x1080 を 1536x864 と報告した——どちらも真っ当な見た目で、倍率を 1 段読み違えさせる。
ゆえに `Initialize-SnotraDpiAwareness` を先に通す。

**プライマリモニタではなく窓の載るモニタを見る。** 本体は PER_MONITOR_AWARE_V2 で動くため、
多画面では窓ごとに倍率が違いうる。ピクセルを数える検査は窓の倍率でしか意味を持たない。
#>
function Get-SnotraWindowDpi {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [IntPtr]$Handle
    )

    Initialize-SnotraDpiAwareness
    [int][SnotraSmokeInterop.Native]::GetDpiForWindow($Handle)
}

# **検証プロファイルに config.toml を置く共通の理由はここが正本である**（3 本の呼び出し側は
# 固有の理由だけを書く）。ファイルが無いと Config::load が first-run と判定し、
# Config::default() の探索パスシード（default_scan_paths）が実マシンの既定パス（存在する
# ものだけ）を索引しうる。#824 以降、中身が空でも parse 自体は通る——セクションもキーも
# 既定へ落ちるため——ので、骨格を書くのは seed の意図を読めるようにするためである。
function New-SnotraVerificationProfile {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$ProfileDir,
        [string]$AdditionalSections = '',
        # `[general]` 内の追加キー（ヘッダ行なし）。TOML はテーブルの再定義を許さないため、
        # `[general]` 自体はこの関数が唯一発行する——呼び出し側が `-AdditionalSections` に
        # `[general]` を書くと、下の既定ブロックと衝突して parse が落ちる（下の guard が
        # 名指しで止める）。
        [string]$GeneralSection = '',
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

    if ($AdditionalSections -match '(?m)^\s*\[general\]\s*$') {
        throw ('AdditionalSections に [general] を含めないでください——auto_update の既定注入と' +
            'テーブル定義が重複し、TOML の parse が落ちます。[general] のキーは -GeneralSection へ渡してください。')
    }

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

    # **`auto_update` は既定で無効化する。** `GeneralConfig.auto_update` の既定は Full
    # （`snotra-core/src/config.rs` の `#[default]`）で、`[general]` を省略しても既定値が
    # 適用されるため、検証用プロファイルは何もしなければ**起動のたびに実ネットワークの
    # 更新チェックを走らせる**——実在の新版が見つかれば本物の toast が出て、
    # `SNOTRA_EGUI_FAKE_UPDATE` が届いていなくても高さ断言が満たされてしまう（#755/#801 是正 E
    # が閉じたはずの「env が届いていない」と「検査対象が出なかった」の混同が別経路で復活する）。
    # 副作用として smoke がネットワークの状態に依存する（間欠的な赤の源）。
    # **fake ハッチは disabled でも効いたままである**——`spawn_update_check`
    # （`src-tauri/src/egui_shell/mod.rs`）のハッチは `auto_update` の判定より**前**で
    # return するため、実チェックだけが消える。
    $generalLines = @('auto_update = "disabled"')
    if (-not [string]::IsNullOrWhiteSpace($GeneralSection)) { $generalLines += $GeneralSection.Trim() }
    $parts += (@('[general]') + $generalLines) -join "`r`n"

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
        [switch]$Trace,
        # 呼び出し側が足す env（`SNOTRA_EGUI_FAKE_UPDATE` 等の視覚スモーク用ハッチ）。
        # 名前の文字種の検証は `Invoke-SnotraEnvironment` が行う。**予約キー
        # （`SNOTRA_CONFIG_DIR` / `SNOTRA_TRACE`）の重複はここで弾く**——弾かずに合流させると
        # 後勝ちで `-ConfigDir` が無効化され、この関数が構造的に保証していた「seed した
        # プロファイル以外を読まない」が黙って破れる（#755/#801 是正 C）。
        [hashtable]$ExtraVariables = @{}
    )

    $reservedVariableNames = @('SNOTRA_CONFIG_DIR', 'SNOTRA_TRACE')
    foreach ($k in $ExtraVariables.Keys) {
        if ($reservedVariableNames -contains $k) {
            throw ("ExtraVariables に予約済みの環境変数名 '$k' が含まれています。" +
                "-ConfigDir / -Trace 経由でのみ設定してください（黙って上書きを許すと、" +
                "検証用プロファイル以外の実 config を読み書きしたまま検査が緑で終わりえます）。")
        }
    }

    $variables = @{ SNOTRA_CONFIG_DIR = $ConfigDir }
    if ($Trace) { $variables.SNOTRA_TRACE = '1' }
    foreach ($k in $ExtraVariables.Keys) { $variables[$k] = $ExtraVariables[$k] }

    $startParameters = @{
        FilePath = $FilePath
        PassThru = $true
    }
    if ($PSBoundParameters.ContainsKey('ArgumentList')) { $startParameters.ArgumentList = $ArgumentList }
    if ($PSBoundParameters.ContainsKey('StandardErrorPath')) { $startParameters.RedirectStandardError = $StandardErrorPath }
    if ($PSBoundParameters.ContainsKey('StandardOutputPath')) { $startParameters.RedirectStandardOutput = $StandardOutputPath }
    # **debug ビルドはコンソール窓を連れてくる。それを見せない**（#872 の現れ方 1）。
    # `main.rs` の `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` により
    # debug はコンソールサブシステムであり、stdio をリダイレクトして起動すると
    # `ConsoleWindowClass` の窓が**実行ファイルのフルパスを題名にして**作られる。この窓が
    # 前面を奪うと、`Set-SnotraForegroundWindow` は本体の窓を前面にできず、注入した打鍵の
    # 宛先が定まらない——CI で実測した前面窓は
    # `title='D:\a\Snotra\Snotra\target\debug\snotra.exe'` だった（run 1280 / 1299）。
    # **release を使う `smoke-egui` が同じキー注入をしながら一度も踏んでいない**のは、
    # release にこの窓が無いためである。
    #
    # **`Hidden` は隠すだけで、窓は在る**（実測: `ConsoleWindowClass` が visible=False で残る）。
    # 前面は奪わなくなるが、窓を列挙する種類の検査を足すときは母集団に混じる。作らせない形は
    # `ProcessStartInfo.CreateNoWindow`（実測: 窓 0 個）だが、`Start-Process` が露出しておらず
    # `PassThru` 相当・引数のクォート規則・リダイレクトの再導出が要る。この 1 件には見合わない。
    #
    # `-WindowStyle` と `-NoNewWindow` は排他なので分ける。`-NoNewWindow` は子が呼び出し元の
    # コンソールを使う＝新しい窓を作らないため、こちらも競合相手を増やさない。
    if ($NoNewWindow) { $startParameters.NoNewWindow = $true } else { $startParameters.WindowStyle = 'Hidden' }

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

    # **cargo の cwd を対象リポジトリへ固定する**（#1179）。相対値の `CARGO_TARGET_DIR` は
    # manifest ではなく **cargo プロセスの cwd** を起点に解決されるため、固定しないと
    # 「worktree の本体を導いたつもりでメイン作業コピーの target を指す」形が残る（実測）。
    # **manifest の存在検査より後に置く**——根が不在ならそちらが先に落ち、ここへ到達しない。
    Push-Location -LiteralPath $RepositoryRoot
    try {
        $metadataOutput = & cargo metadata --no-deps --format-version 1 --manifest-path $manifestPath
        # `Pop-Location` より前に捕まえる（後続の cmdlet が $LASTEXITCODE を運ぶとは限らない）。
        $cargoExit = $LASTEXITCODE
    } finally {
        Pop-Location
    }
    if ($cargoExit -ne 0) {
        throw "cargo metadata に失敗しました（exit=$cargoExit）。"
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

<#
.SYNOPSIS
プロセスを停止し、**終了を待つ**（#872 単一インスタンス衝突）。

.DESCRIPTION
`Stop-Process -Force` は制御を即返す。`tauri_plugin_single_instance` が登録されているため、
先発がまだ生きたまま後発を起動すると、後発は先発へ通知して即終了する——`scripts/smoke-egui.ps1`
が **#755/#801 是正 B** として同じ機序を既に解いており、**機序の正本はそちらのコメントである**。

**throw しない。** 呼び出し点が `finally` を含み、`finally` からの throw は元の例外を覆い隠す
（元の失敗こそが読みたいもの）。終了を確認できなかったことは戻り値と警告で表し、**赤にする
責務は呼び出し側が持つ**——`SnotraSmoke.Tests.ps1` の `Describe '実機配管'` の `AfterAll` と、
次の It の `Resolve-SnotraExistingProcess -Policy Reject` がその実体である。

**引数に型を付けない。** 既存の単体検査は `Id` だけを持つ偽オブジェクトで方針分岐を固定して
おり、`[System.Diagnostics.Process]` を要求するとパラメータ束縛で落ちる（実測:
`Cannot create object of type "System.Diagnostics.Process". "Id" is a ReadOnly property.`）。
#>
function Stop-SnotraProcessAndWait {
    [CmdletBinding()]
    [OutputType([bool])]
    param(
        [Parameter(Mandatory)]
        [AllowNull()]
        $Process,
        [int]$TimeoutMs = 5000,
        # `Stop-Process` 自体のエラーを黙らせる。**既定は黙らせない**——`Resolve-SnotraExistingProcess`
        # の `Stop` 分岐は #853 以来 `-ErrorAction` を付けておらず、アクセス拒否を呼び出し側へ
        # 上げていた。ヘルパへ畳むときにそのエラーチャネルを黙らせない。
        [switch]$Quiet
    )

    if ($null -eq $Process) { return $true }
    if ($Process.HasExited) { return $true }

    if ($Quiet) {
        Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
    } else {
        Stop-Process -Id $Process.Id -Force
    }

    try {
        if ($Process.WaitForExit($TimeoutMs)) { return $true }
    } catch {
        Write-Warning "pid=$($Process.Id) の終了待ちに失敗しました: $($_.Exception.Message)"
        return $false
    }
    Write-Warning "pid=$($Process.Id) が ${TimeoutMs}ms 以内に終了しませんでした（single-instance 衝突の恐れ）。"
    return $false
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
        # **終了を待つ**（#872）。待たずに返すと、呼び出し側が直後に起動する本体が
        # single-instance で先発へ通知して即終了し、trace を 1 行も書かないまま待ちが
        # 予算を使い切る。**`-Quiet` は付けない**——この分岐は #853 以来
        # `Stop-Process` のエラーを呼び出し側へ上げており、そのチャネルを保つ。
        [void](Stop-SnotraProcessAndWait -Process $process)
    }
}

function Read-SnotraTraceEvents {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$Path
    )

    # **読み実装は `Read-SnotraTraceSnapshot` の 1 つだけにする。** 以前はここが独自に
    # Get-Content していたため、読み取り失敗の扱いが 2 か所にあり、片方だけが沈黙していた。
    $snapshot = Read-SnotraTraceSnapshot -Path $Path
    if ($snapshot.ReadError) {
        Write-Warning "trace の読み取りに失敗しました（$Path）: $($snapshot.ReadError)"
    }
    return $snapshot.Events
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
        return @{ Available = $false; Lines = @(); Events = @(); TraceLines = 0; Dropped = 0; ReadError = $null }
    }
    # **1 度だけ読む。** 行数と parse 成功数を別々の読み取りから取ると、稼働中のアプリが間に
    # 書き足した行で差が壊れる（code-review M1）。**生行も返す**のは同じ理由で——presence の
    # 表示と不変条件の判定が別時点のファイルを見ないようにするため。
    #
    # **読み取りの失敗を空と同じ値へ潰さない**（#872）。`-ErrorAction SilentlyContinue` は
    # 落ちた読みに対しても空を返すため、Available=true / Events 0 件 / Dropped 0 という
    # **正常な空ログと見分けられない値**になり、稼働中のアプリを観測する経路で
    # 「読めなかった」が「まだ出ていない」に化ける。読めなければ Available を倒す
    # （`manual-smoke.ps1` の `Get-TraceVerdict` はこれを見て判定そのものを見送る）。
    # **不在（ReadError=$null）と失敗（ReadError あり）は別の状態である。**
    try {
        $lines = @(Get-Content -LiteralPath $Path -ErrorAction Stop)
    } catch {
        return @{
            Available = $false; Lines = @(); Events = @()
            TraceLines = 0; Dropped = 0; ReadError = $_.Exception.Message
        }
    }
    $traceLineCount = 0
    foreach ($text in $lines) { if ($text -match '^\[trace\]\s+') { $traceLineCount++ } }
    $events = @(ConvertFrom-SnotraTraceLine -Line $lines)
    return @{
        Available  = $true
        Lines      = $lines
        Events     = $events
        TraceLines = $traceLineCount
        Dropped    = [Math]::Max(0, $traceLineCount - $events.Count)
        ReadError  = $null
    }
}

<#
.SYNOPSIS
trace に述語を満たす事象が現れるまで待つ（待ちループの唯一の実装）。

.DESCRIPTION
**待ちループの形は `Set-SnotraForegroundWindow` に揃える**——「評価 → 期限判定 → sleep」の順で、
`while (now -lt deadline)` にしない。前者は**期限を跨いだ停止のあとでも必ず 1 度は評価する**が、
後者は最後の sleep 中に停止が起きると、**既に成立していた条件を一度も見ないまま諦める**
（#872 run 1306: 期待した事象は 39.354 に出ており期限は 42.57 以降だったのに観測されなかった）。

**trace ファイルは追記のみの過去の記録である。**ゆえに期限後の 1 度の評価が「待ち時間を
こっそり延ばす」ことにはならない——事象が起きたか否かは時刻に依らず確定している。ただし
**期限後に初めて見つかったことは停止の徴候である**ため、見つけても警告を出して記録に残す。

**却下した案: 期限を跨いだら見つかっても失敗にする。**「予算内に応答したか」を測る検査なら
正しいが、この検査が判定するのはキャレット位置であって応答時間ではない。予算は待つ辛抱の
上限にすぎず、そこへ性能の意味を後付けすると、遅い runner で**実装が正しいまま赤になる**。

読み取りの失敗（`ReadError`）は成立判定に影響させない——その周回が「まだ見えていない」のと
同じ扱いになるのは避けられないが、**件数を数えて必ず報告する**。沈黙させると #872 の 3 つの
機序（期限跨ぎ・未着・読み取り失敗）が事後に区別できない。

**諦める条件を scriptblock で受け取らない。** この関数は module スコープで評価するため、
呼び出し側の変数を読むには閉包が要る。ところが `.GetNewClosure()` は Pester の `It` の中では
**親スコープの変数を捕まえない**（実測: 予算 4000ms に対し中断せず 4031ms 待った）。捕まえ
損ねても**発火しないだけ**なので通った実行では観測されず、「本体が死んでも予算いっぱい待つ」
という退化が黙って入る。プロセスを型付きで受け取れば、この間違いは書けなくなる。

**presence ではなく件数の下限（`MinMatchCount`）で待つ。** 既定の 1 は従来どおり「1 件でも
あれば成立」で、複数回同じイベント名を待つ呼び出し側（例: show → hide → show の 2 周目）が
**1 周目の行に毎回一致して即座に戻る**事故を防ぐのはこの下限だけである（#755/#801 是正 A）。
呼び出し側は行為（打鍵等）の**前**に `Get-SnotraTraceEventCount` で件数を数え、
`件数 + 1` を渡すことで「これから起きる新しい 1 件」を待てる。マーカーを行為の前に打つ考え方は
`SnotraTraceInvariants.psm1` の `Get-SnotraTraceMarker` と同じである。
#>
function Wait-SnotraTraceCondition {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$Path,
        [Parameter(Mandatory)]
        [scriptblock]$Predicate,
        [Parameter(Mandatory)]
        [int]$TimeoutMs,
        [int]$PollMs = 100,
        # 与えると、このプロセスが終了した時点で期限を待たずに諦める（待っても変わらないため）。
        [System.Diagnostics.Process]$AbortIfExited,
        [string]$Description = 'trace の条件',
        # 一致件数がこれ以上になるまで待つ。既定 1 は「presence」（従来の挙動）と同じ。
        [int]$MinMatchCount = 1
    )

    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    $started = [DateTime]::UtcNow
    $rounds = 0
    $readErrors = 0
    $lastReadError = $null
    $lastSnapshot = $null
    $aborted = $false

    while ($true) {
        $rounds++
        $snapshot = Read-SnotraTraceSnapshot -Path $Path
        $lastSnapshot = $snapshot
        if ($snapshot.ReadError) {
            $readErrors++
            $lastReadError = $snapshot.ReadError
        }
        $matched = @($snapshot.Events | Where-Object -FilterScript $Predicate)
        if ($matched.Count -ge $MinMatchCount) {
            $elapsed = [int]([DateTime]::UtcNow - $started).TotalMilliseconds
            if ([DateTime]::UtcNow -ge $deadline) {
                Write-Warning ("$Description は予算 ${TimeoutMs}ms を過ぎた評価で成立しました" +
                    "（${elapsed}ms・$rounds 周）。停止の徴候として記録します。")
            }
            if ($readErrors -gt 0) {
                Write-Warning "$Description の待ちで trace の読み取りに失敗した周回が $readErrors 回ありました: $lastReadError"
            }
            return ($matched | Select-Object -Last 1)
        }
        if ($null -ne $AbortIfExited -and $AbortIfExited.HasExited) { $aborted = $true; break }
        if ([DateTime]::UtcNow -ge $deadline) { break }
        Start-Sleep -Milliseconds $PollMs
    }

    # **不成立の理由を区別できる形で残す。** 何も出さないと、期限切れ・事象未着・読み取り失敗が
    # 呼び出し側からは同じ $null にしか見えない（#872 で 7 件目まで機序を割れなかった原因）。
    $elapsed = [int]([DateTime]::UtcNow - $started).TotalMilliseconds
    $reason = if ($aborted) { "本体が終了（exit=$($AbortIfExited.ExitCode)）" } else { '予算切れ' }
    $seen = if ($null -ne $lastSnapshot -and $lastSnapshot.Available) {
        "trace 行 $($lastSnapshot.TraceLines) / 事象 $($lastSnapshot.Events.Count) / 捨てた行 $($lastSnapshot.Dropped)"
    } else {
        'trace を読めていない'
    }
    Write-Warning ("$Description が成立しませんでした（$reason・${elapsed}ms・$rounds 周・" +
        "読み取り失敗 $readErrors 回・最後に見たもの: $seen）。" +
        $(if ($lastReadError) { " 最後の読み取りエラー: $lastReadError" } else { '' }))
    return $null
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
        [int]$PollMs = 200,
        # 同名イベントを周回のたびに待つ呼び出し側向け（#755/#801 是正 A）。行為の前に
        # `Get-SnotraTraceEventCount` で数えた件数 + 1 を渡すと、既に出ている古い 1 件に
        # 即一致して戻ることがなくなる。既定 1 は presence のままの従来の挙動。
        [int]$MinMatchCount = 1
    )

    # 待ちループの形（期限跨ぎ・読み取り失敗の扱い）は `Wait-SnotraTraceCondition` が単独で持つ。
    # ここに写しを置くと、穴を塞ぐ変更が片方だけに入る（#872 では実際に 2 か所へ分かれていた）。
    return Wait-SnotraTraceCondition -Path $Path -TimeoutMs $TimeoutMs -PollMs $PollMs `
        -MinMatchCount $MinMatchCount `
        -Description "trace の $EventName" -Predicate { $_.event -eq $EventName }.GetNewClosure()
}

<#
.SYNOPSIS
指定イベント名の現在の一致件数を数える（`Wait-SnotraTraceEvent -MinMatchCount` のマーカー用）。

.DESCRIPTION
**行為（打鍵等）の前に呼ぶこと。** 後に呼ぶと、行為が引き起こした 1 件が自分自身の
ベースラインへ紛れ込み、`MinMatchCount = 件数 + 1` が実際には「もう 1 件」を要求してしまう
（#755/#801 是正 A）。
#>
function Get-SnotraTraceEventCount {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$Path,
        [Parameter(Mandatory)]
        [string]$EventName
    )

    $snapshot = Read-SnotraTraceSnapshot -Path $Path
    return @($snapshot.Events | Where-Object { $_.event -eq $EventName }).Count
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
    # **注入の時刻を残す**（`SNOTRA_EGUI_INPUT_TRACE` を立てた実行だけ・#872/#936）。
    # `keybd_event` は戻り値が void で、注入の成否を返す経路が無い——ゆえにここで残せるのは
    # 「呼んだ」ことだけである。それでも本体側の `SNOTRA_EGUI_INPUT rx_key` と突き合わせれば、
    # 「注入 〜 アプリへの配送」という**どちらの側もログを持たなかった唯一の区間**が測れる。
    # 時計は epoch ms で、本体の `ts_ms` と同じ土俵に載る。**打鍵の実装はこの 1 か所**なので、
    # 呼び出し側（smoke / Pester）に写しを置かずに全注入点を覆える。
    if ($env:SNOTRA_EGUI_INPUT_TRACE) {
        $ts = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        $state = if ($Up) { 'up' } else { 'down' }
        Write-Host ("SNOTRA_SMOKE_INJECT ts_ms={0} vk=0x{1:X2} state={2}" -f $ts, $VirtualKey, $state)
    }
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
    # 形は `Set-SnotraForegroundWindow` / `Wait-SnotraTraceCondition` に揃える（#872）——
    # 「評価 → 期限判定 → sleep」。`while (now -lt deadline)` だと、最後の sleep を跨ぐ停止で
    # **窓が現れていても一度も見ないまま**「現れませんでした」と落ちる。
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    while ($true) {
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
        if ([DateTime]::UtcNow -ge $deadline) { break }
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
    # **クラス名も出す**（#872）。題名だけでは、前面を握っていたのが本体の窓なのか
    # debug ビルドが連れてくるコンソール窓（`ConsoleWindowClass`）なのかを事後に確定できない。
    $class = New-Object System.Text.StringBuilder 256
    [void][SnotraSmokeInterop.Native]::GetClassNameW($handle, $class, $class.Capacity)
    [uint32]$ownerPid = 0
    [void][SnotraSmokeInterop.Native]::GetWindowThreadProcessId($handle, [ref]$ownerPid)
    return "0x$($handle.ToString('X')) pid=$ownerPid class='$($class.ToString())' title='$($title.ToString())'"
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
    'Stop-SnotraProcessAndWait'
    'Read-SnotraTraceEvents'
    'Read-SnotraTraceSnapshot'
    'Wait-SnotraTraceCondition'
    'Wait-SnotraTraceEvent'
    'Get-SnotraTraceEventCount'
    'Send-SnotraKey'
    'Send-SnotraKeyChord'
    'Wait-SnotraWindow'
    'Get-SnotraForegroundWindow'
    'Set-SnotraForegroundWindow'
    'Get-SnotraWindowCapture'
    'Get-SnotraWindowDpi'
    'ConvertFrom-SnotraWtsInfoEx'
    'Get-SnotraSessionLockState'
    'Assert-SnotraSessionUnlocked'
)
