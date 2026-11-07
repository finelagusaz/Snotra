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
public static extern IntPtr FindWindowExW(IntPtr parent, IntPtr childAfter, string cls, string title);
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
[DllImport("user32.dll", SetLastError = true)]
public static extern uint SendInput(uint count, INPUT[] inputs, int size);
[DllImport("user32.dll", SetLastError = true)]
public static extern bool PostMessageW(IntPtr hWnd, uint msg, UIntPtr wParam, IntPtr lParam);
[StructLayout(LayoutKind.Sequential)]
public struct MOUSEINPUT { public int dx, dy; public uint mouseData, dwFlags, time; public UIntPtr dwExtraInfo; }
[StructLayout(LayoutKind.Sequential)]
public struct KEYBDINPUT { public ushort wVk, wScan; public uint dwFlags, time; public UIntPtr dwExtraInfo; }
[StructLayout(LayoutKind.Explicit)]
public struct INPUTUNION { [FieldOffset(0)] public MOUSEINPUT mi; [FieldOffset(0)] public KEYBDINPUT ki; }
[StructLayout(LayoutKind.Sequential)]
public struct INPUT { public uint type; public INPUTUNION u; }
'@
    }
}

# WTSQuerySessionInformationW の引数（`WTSSessionInfoEx` = 25・`WTS_CURRENT_SESSION` = -1）と
# `WTSINFOEX_LEVEL1_W.SessionFlags` の値。Windows 7 では LOCK/UNLOCK の意味が逆になる。
# 対象は Windows 10/11 なので、ドキュメントどおりに扱う。
$script:WtsSessionInfoEx = 25
$script:WtsCurrentSession = -1
$script:WtsSessionStateLock = 0
$script:WtsSessionStateUnlock = 1

<#
.SYNOPSIS
`WTSINFOEXW` のバイト列から SessionFlags を読む純関数。

.DESCRIPTION
**オフセットを自前で仮定しているので、同じバッファの SessionId で検算する。**
`WTSINFOEXW` は `Level`(DWORD) の後に union が続く。union の中身（`WTSINFOEX_LEVEL1_W`）は
`LARGE_INTEGER` を含むので、8 バイト境界へ揃う。そのため SessionId は 8、SessionState は 12、
SessionFlags は 16 に来る。仮定が外れると、SessionId が呼び出し元のセッションと合わなくなる。
合わなければ throw する（誤ったオフセットから読んだ 0 を「ロック中」と読まないため）。
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
**ロック中も、ウィンドウは Win32 から見て生きたままである**（`IsWindowVisible` は真で、矩形も
DPI も正しい）。そのため他のガードはすべて通り、`CopyFromScreen` はロック画面を写した有効な
Bitmap を返す。判定が落ちても、メッセージは色や描画経路を指すので、読んだ人はレンダリングを
疑う（#866 実測）。ここでロックを名指しして止める。

- ロック中と判定できた: throw する
- 判定できなかった: 警告だけ出して続ける。この関数の役目は誤った結果を防ぐことで、状態を読めない
  ホストで実行を拒んでも得るものが無い。`Get-SnotraWindowCapture` は Pester の Integration テスト
  から CI（GitHub Actions の Windows runner）でも走るので、throw にすると、runner の WTS の振る舞い
  しだいで CI が壊れうる
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
        throw "画面がロックされているため $Operation を実行できません。ロック中はウィンドウが可視のままでも画面に合成されず、キャプチャはロック画面の中身を返します（#866）。画面のロックを解除してから実行してください。"
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
ウィンドウが載っているモニターの実効 DPI を返す（96 が 100%）。

.DESCRIPTION
**awareness を設定せずに DPI を読むと、エラーにならず 96（100%）が返る。** DPI 非対応の
プロセスには、Win32 が仮想化した値を見せるためである。2026-08-16 に 125% の機体で実測すると、
`GetDeviceCaps(LOGPIXELSX)` は 96 を、`System.Windows.Forms.Screen` は物理 1920x1080 を
1536x864 と返した。そのため、先に `Initialize-SnotraDpiAwareness` を通す。

**プライマリモニターではなく、ウィンドウが載るモニターの DPI を返す。** 本体は PER_MONITOR_AWARE_V2
で動くので、多画面ではウィンドウごとに倍率が違いうる。ピクセルを数える検査には、ウィンドウの倍率が要る。
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

# **検証用プロファイルに config.toml を置く共通の理由は、ここが正本である**（呼び出し側は固有の
# 理由だけを書く）。ファイルが無いと `Config::load` が first-run と判定し、`Config::default()` の
# 探索パスのシード（`default_scan_paths`）が、実マシンに在る既定パスを索引しうる。中身が空でも
# parse は通る（セクションもキーも既定へ落ちる・#824）。骨格を書くのは、seed の意図を読めるようにするためである。
function New-SnotraVerificationProfile {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$ProfileDir,
        [string]$AdditionalSections = '',
        # `[general]` に足すキー（ヘッダ行は書かない）。TOML はテーブルの再定義を許さないので、
        # `[general]` の見出しはこの関数だけが書く。`-AdditionalSections` に `[general]` を
        # 書くと parse が落ちるため、下の guard が止める。
        [string]$GeneralSection = '',
        [string]$PathEntries = '',
        # 既定は分離プロファイルの既定ホットキー Alt+F12（正本は `snotra-core/src/hotkey.rs` の
        # `ISOLATED_PROFILE_HOTKEY_*`）。実プロファイルの Alt+Q と違うので、普段使いの Snotra と
        # 並走しても登録が衝突しない（#1306）。
        [string]$HotkeyModifier = 'Alt',
        [string]$HotkeyKey = 'F12',
        [int]$WindowWidth = 600,
        # アイコンを判定に使わない検査は $false を渡す。$false なら `results_view.rs` の
        # `request_icons_for_results` がすぐ return し、icon worker も `SHGetFileInfoW` も走らない。
        # CI の runner では `SHGetFileInfoW` が `ShellQueryFailed(1008)` を返し、一時的な失敗として
        # 再要求され続けるので、観測時間が延びるだけになる（#872）。
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

    # **`auto_update` は既定で無効にする。** 製品の既定は Full（`snotra-core/src/config.rs` の
    # `#[default]`）なので、何もしなければ起動のたびに実ネットワークへ更新を問い合わせる。
    # 実在の新版が見つかると本物の toast が出て、`SNOTRA_EGUI_FAKE_UPDATE` が届いていなくても
    # 高さの断言が通ってしまう（#755/#801）。ネットワークの状態で smoke が赤くなる原因にもなる。
    # disabled でも fake のハッチは効く。`spawn_update_check`（`src-tauri/src/egui_shell/mod.rs`）は
    # `auto_update` を見る前にハッチで return するので、消えるのは実際の問い合わせだけである。
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
        # 呼び出し側が足す env（`SNOTRA_EGUI_FAKE_UPDATE` などの検査用ハッチ）。名前の文字種は
        # `Invoke-SnotraEnvironment` が検証する。**予約キー（`SNOTRA_CONFIG_DIR` / `SNOTRA_TRACE`）は
        # ここで弾く。** 合流させると後勝ちで `-ConfigDir` が上書きされ、seed したプロファイル以外を
        # 読まないという保証が黙って破れる（#755/#801）。
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
    # **debug ビルドのコンソールウィンドウを隠す**（#872）。debug はコンソールサブシステムで
    # （`main.rs` の `windows_subsystem` は release だけに掛かる）、stdio をリダイレクトして起動すると
    # `ConsoleWindowClass` のウィンドウが作られる。このウィンドウが前面を奪うと、
    # `Set-SnotraForegroundWindow` が本体を前面にできず、注入した打鍵の宛先が定まらない。
    # release にはこのウィンドウが無い。ローカルの既定は debug なので（#1292）、`Hidden` で隠す。
    #
    # `Hidden` は隠すだけで、ウィンドウは visible=False のまま残る（実測）。ウィンドウを列挙する
    # 検査を足すなら、列挙に混じることに注意する。作らせない `ProcessStartInfo.CreateNoWindow` は、
    # `Start-Process` が出しておらず、自前で組み直す手間に見合わない。
    #
    # `-WindowStyle` と `-NoNewWindow` は排他である。`-NoNewWindow` なら子は呼び出し元のコンソールを
    # 使い、新しいウィンドウを作らない。
    if ($NoNewWindow) { $startParameters.NoNewWindow = $true } else { $startParameters.WindowStyle = 'Hidden' }

    Invoke-SnotraEnvironment -Variables $variables -ScriptBlock {
        Start-Process @startParameters
    }
}

<#
.SYNOPSIS
`cargo metadata` の `target_directory` から本体のパスを導く（`CARGO_TARGET_DIR` に追随する）。

.DESCRIPTION
**呼び出し側の作法の正本はここである**（#1179）。各スクリプトへ写しを置かない。

- **既定値に絶対パスを直書きしない。** worktree から既定のまま回すと、別の作業コピーの本体を指す。
  その本体も実在するので、スクリプトは完走して緑になり、失敗が緑と見分けられない。
- **導出は `param()` の中に書けない。** 既定値の束縛は `Import-Module` より前に起きる（実測）。
  既定は空リテラルにし、import の後で解決する。
- **導出の起点は、スクリプトが在るリポジトリ（`$PSScriptRoot`）である。** cwd ではない。
- **明示された `-ExePath` の意味は変えない**（相対パスは cwd 相対のまま）。導出するのは既定のときだけである。
- 解決後は絶対形へ正規化して、出力へ載せる。どのコピーを測ったかが出力に見えることが、
  `-Profile`（release/debug）の取り違えに気づく唯一の手がかりである。

この作法は `scripts/lib/SnotraSmoke.Tests.ps1` のソース述語が守っている（適用範囲と死角はあちらの宣言が正本）。
#>
function Resolve-SnotraCargoExecutable {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$RepositoryRoot,
        [ValidateSet('debug', 'release')]
        [string]$Profile = 'debug'
    )

    Join-Path (Resolve-SnotraCargoTargetDirectory -RepositoryRoot $RepositoryRoot) "$Profile/snotra.exe"
}

<#
.SYNOPSIS
`cargo metadata` の `target_directory`（`CARGO_TARGET_DIR` に追随する）を返す。

.DESCRIPTION
導出の作法の正本は `Resolve-SnotraCargoExecutable` の doc である。このリポジトリのビルド成果物の
置き場を知る経路は、この関数 1 つにする（#1306）。
#>
function Resolve-SnotraCargoTargetDirectory {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$RepositoryRoot
    )

    $manifestPath = Join-Path $RepositoryRoot 'Cargo.toml'
    if (-not (Test-Path -LiteralPath $manifestPath)) {
        throw "Cargo workspace の manifest がありません: $manifestPath"
    }

    # **cargo の cwd を対象リポジトリへ固定する**（#1179）。相対値の `CARGO_TARGET_DIR` は、
    # manifest ではなく cargo プロセスの cwd を起点に解決される。固定しないと、worktree の本体を
    # 導いたつもりで、メインの作業コピーの target を指す（実測）。
    # manifest の存在検査の後に置く（根が無ければ、そちらが先に落とす）。
    Push-Location -LiteralPath $RepositoryRoot
    try {
        $metadataOutput = & cargo metadata --no-deps --format-version 1 --manifest-path $manifestPath
        # `Pop-Location` より前に検出する（後続の cmdlet が $LASTEXITCODE を運ぶとは限らない）。
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

    [string]$metadata.target_directory
}

<#
.SYNOPSIS
記録（PR へ投稿されうる）に書く本体のパスを返す（#1293）。

.DESCRIPTION
**このリポジトリは公開されており、`manual-smoke.ps1 -PostToPr` は記録を PR コメントへ投稿する。**
`Resolve-SnotraCargoExecutable` が返すのは絶対パスなので、そのまま載せると利用者のディレクトリ構成
（ユーザー名・worktree の置き場）が公開される。

- `RepositoryRoot` の下の絶対パスは、リポジトリ相対にして返す
- それ以外（リポジトリの外の絶対パス・相対パス）は、そのまま返す。呼び出し側が明示した値であり、
  導出が生んだものではない
- 空なら `$null` を返す（起動していないことの表し方は呼び出し側が決める）

「下にある」かは `GetRelativePath` で判定する。**文字列の前方一致では判定しない**（`C:\repo2` は
`C:\repo` に前方一致する）。
#>
function Get-SnotraRecordedExePath {
    [CmdletBinding()]
    param(
        [AllowEmptyString()]
        [string]$ExePath,
        [Parameter(Mandatory)]
        [string]$RepositoryRoot
    )

    if ([string]::IsNullOrEmpty($ExePath)) { return $null }
    if (-not [IO.Path]::IsPathRooted($ExePath)) { return $ExePath }
    # 外なら `..` 始まりか（別ドライブなら）絶対パスが返る（実測）。
    $relative = [IO.Path]::GetRelativePath($RepositoryRoot, $ExePath)
    if ($relative -eq '..' -or $relative.StartsWith("..$([IO.Path]::DirectorySeparatorChar)") -or [IO.Path]::IsPathRooted($relative)) {
        return $ExePath
    }
    $relative
}

<#
.SYNOPSIS
プロセスを停止し、**終了を待つ**（#872 単一インスタンス衝突）。

.DESCRIPTION
`Stop-Process -Force` は終了を待たずに返る。`tauri_plugin_single_instance` があるので、先発が
生きているうちに後発を起動すると、後発は先発へ通知して即終了する（機序の正本は
`scripts/smoke-egui.ps1` のコメント・#755/#801）。

**throw しない。** `finally` から呼ばれるので、throw すると元の例外を覆い隠す。終了を確かめられ
なかったことは、戻り値 `$false` と警告で返す。赤にするのは呼び出し側である。

**引数に型を付けない。** 単体検査は `Id` だけを持つ偽オブジェクトを渡す。`[System.Diagnostics.Process]`
を要求すると、`"Id" is a ReadOnly property.` でパラメータの束縛に落ちる（実測）。
#>
function Stop-SnotraProcessAndWait {
    [CmdletBinding()]
    [OutputType([bool])]
    param(
        [Parameter(Mandatory)]
        [AllowNull()]
        $Process,
        [int]$TimeoutMs = 5000,
        # `Stop-Process` 自体のエラーを黙らせる。既定は黙らせず、アクセス拒否などを呼び出し側へ
        # 上げる（#853）。
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
        [string]$ProcessName = 'snotra',
        # 指定すると、実行ファイルがこのディレクトリの配下にあるプロセスだけを対象にする（#1306）。
        # - 検証用プロファイルの起動を邪魔するのは、このリポジトリのビルド成果物から起動した残留だけである
        # - 普段使いの Snotra（インストール先の exe）は単一インスタンスが別なので、落としも拒みもしない
        # - exe 1 本で絞らないのは、debug / release や `-ExePath` が違う残留も拾うためである。拾い漏れると、
        #   後発が single-instance で黙って終わる（#872）
        # - `-ExePath` にリポジトリ外の exe を渡すと、その残留は拾えない
        [string]$BuildRoot = ''
    )

    $existing = @(Get-Process -Name $ProcessName -ErrorAction SilentlyContinue)
    if (-not [string]::IsNullOrEmpty($BuildRoot)) {
        $root = [System.IO.Path]::GetFullPath($BuildRoot).TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
        $existing = @($existing | Where-Object {
                # `.Path` は権限の無いプロセスで例外を投げうる。
                $path = try { $_.Path } catch { $null }
                if ([string]::IsNullOrEmpty($path)) {
                    # 権限で読めないプロセスは候補に残す。黙って外すと、読めない残留が後発を沈黙させる。
                    Write-Warning "pid=$($_.Id) の実行ファイルを読めません。ビルド成果物の残留として扱います。"
                    return $true
                }
                [System.IO.Path]::GetFullPath($path).StartsWith($root, [System.StringComparison]::OrdinalIgnoreCase)
            })
    }
    if ($existing.Count -eq 0) { return }

    if ($Policy -eq 'Reject') {
        throw "Snotra が既に起動しています（pid=$($existing.Id -join ', ')）。single-instance により検証が空振りするため、終了してから再実行してください。"
    }

    foreach ($process in $existing) {
        # **終了を待つ**（#872）。待たずに返すと、直後に起動する本体が single-instance で即終了し、
        # trace を 1 行も書かないまま待ちが時間切れになる。`-Quiet` は付けず、`Stop-Process` の
        # エラーを呼び出し側へ上げる（#853）。
        [void](Stop-SnotraProcessAndWait -Process $process)
    }
}

function Read-SnotraTraceEvents {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$Path
    )

    # **読む実装は `Read-SnotraTraceSnapshot` 1 つにする。** 2 つあると、読み取り失敗の扱いが
    # 片方だけ変わる（実際に片方だけが失敗を黙っていた）。
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
`Read-SnotraTraceEvents` と `Read-SnotraTraceSnapshot` が共通に使う parse の本体である。
スナップショットは、行数と parse 成功数の差で捨てた行を数える。そのため両者は同じ 1 回の
読み取りを見なければならない。別々に読むと、稼働中のアプリが間に書き足した行で差が壊れる
（parse 成功数が行数を上回りうる）。
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
**「捨てた行」は、`[trace]` で始まるのに parse できなかった行だけを数える。** stderr には
trace 以外の診断行（`[index-load] ...` など）が混じる。「全行 − parse 成功」で数えると、正常な
実行が毎回 degrade になり、検出器が意味を失う（#757 実測）。

この数え方が `Test-SnotraTraceInvariants -DroppedLineCount` の意味を決める。呼び出し側に写しを
置かない（ずれると、片方の smoke だけが誤って degrade になる）。
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
    # **1 度だけ読む**（理由は `ConvertFrom-SnotraTraceLine` の doc）。生の行も返すのは、
    # presence の表示と不変条件の判定が、別の時点のファイルを見ないようにするためである。
    #
    # **読み取りの失敗を、空のログと同じ値にしない**（#872）。`-ErrorAction SilentlyContinue` だと、
    # 失敗しても正常な空ログ（Available=true・Events 0 件・Dropped 0）と同じ値が返り、
    # 「読めなかった」と「まだ出ていない」を区別できない。読めなければ Available=false を返す。
    # 不在（ReadError=$null）と失敗（ReadError あり）は別の状態である。
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
**ループは「評価 → 期限判定 → sleep」の順にする**（`Set-SnotraForegroundWindow` と同じ形）。
`while (now -lt deadline)` だと、最後の sleep 中にプロセスが止まったとき、既に成立していた条件を
一度も見ないまま諦める（#872 で実測）。

trace は追記だけの記録なので、期限後に 1 度評価しても、待ち時間を延ばしたことにはならない。
ただし、期限後に初めて見つかったのは停止の徴候なので、警告を出す。見つかったものを失敗には
しない。予算は待つ上限であって応答時間の基準ではなく、失敗にすると遅い runner で、実装が正しい
まま赤になる。

読み取りの失敗（`ReadError`）は、成立の判定には使わない。ただし回数を数えて、必ず警告に載せる。
黙ると、期限切れ・事象の未着・読み取り失敗を、後から区別できない（#872）。

**`-AbortIfExited` はプロセスを型付きで受け取る。scriptblock では受け取らない。** この関数は
module スコープで評価するので、呼び出し側の変数を読むにはクロージャが要る。しかし Pester の `It`
の中では、`.GetNewClosure()` が親スコープの変数をキャプチャしない（実測）。キャプチャし損ねても
発火しないだけなので、本体が死んでも予算いっぱい待つ形が黙って入る。

**`-MinMatchCount`（件数の下限）で待つ。** 同じイベント名を何度も待つ呼び出し側（例: show →
hide → show の 2 周目）は、下限が無いと 1 周目の行に一致してすぐ戻る（#755/#801）。行為（打鍵
など）の**前に** `Get-SnotraTraceEventCount` で数え、`件数 + 1` を渡すと、これから起きる 1 件を
待てる。
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
        # 一致件数がこれ以上になるまで待つ。既定の 1 は「1 件でもあれば成立」である。
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

    # **成立しなかった理由を、区別できる形で残す。** 何も出さないと、期限切れ・事象の未着・
    # 読み取り失敗が、呼び出し側からは同じ $null にしか見えない（#872）。
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
        # 同じイベントを何度も待つときに使う（意味は `Wait-SnotraTraceCondition` の doc）。
        [int]$MinMatchCount = 1
    )

    # 待ちループは `Wait-SnotraTraceCondition` だけが持つ。写しを置くと、直しが片方にしか入らない（#872）。
    return Wait-SnotraTraceCondition -Path $Path -TimeoutMs $TimeoutMs -PollMs $PollMs `
        -MinMatchCount $MinMatchCount `
        -Description "trace の $EventName" -Predicate { $_.event -eq $EventName }.GetNewClosure()
}

<#
.SYNOPSIS
指定イベント名の現在の一致件数を数える（`Wait-SnotraTraceEvent -MinMatchCount` のマーカー用）。

.DESCRIPTION
**行為（打鍵など）の前に呼ぶ。** 後に呼ぶと、行為が起こした 1 件が数に入り、
`MinMatchCount = 件数 + 1` がさらにもう 1 件を待ってしまう（#755/#801）。
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
    # `keybd_event` は void を返すので、残せるのは「呼んだ」ことだけである。本体側の
    # `SNOTRA_EGUI_INPUT rx_key` と突き合わせると、注入からアプリへ届くまでの区間を測れる
    # （時計は epoch ms で、本体の `ts_ms` と比べられる）。`keybd_event` で打鍵するのはこの関数
    # だけなので、ここで記録すれば打鍵の注入をすべて拾える。文字の注入
    # （`Send-SnotraUnicodeText` / `Send-SnotraPostedChar`）は記録しない。
    if ($env:SNOTRA_EGUI_INPUT_TRACE) {
        $ts = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        $state = if ($Up) { 'up' } else { 'down' }
        Write-Host ("SNOTRA_SMOKE_INJECT ts_ms={0} vk=0x{1:X2} state={2}" -f $ts, $VirtualKey, $state)
    }
}

function Send-SnotraUnicodeText {
    # `SendInput` の `KEYEVENTF_UNICODE` で文字を打つ（#1289）。打鍵（`keybd_event`）とは別の
    # 注入経路である。前面ウィンドウには `VK_PACKET` の down/up と `WM_CHAR` が届く。
    # **BMP 外の文字は、サロゲートの 2 単位を別々の down/up で送る。** 受け手には
    # 「down・CHAR(high)・up・down・CHAR(low)・up」の順で届くので、キーのメッセージをまたいで
    # 連結しないと入力されない。
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$Text
    )

    Initialize-SnotraNativeInterop
    $KEYEVENTF_UNICODE = 0x4
    $KEYEVENTF_KEYUP = 0x2
    $inputs = foreach ($unit in $Text.ToCharArray()) {
        foreach ($flags in @($KEYEVENTF_UNICODE, ($KEYEVENTF_UNICODE -bor $KEYEVENTF_KEYUP))) {
            # **入れ子の構造体は内側から組んで代入する。** PowerShell の `$ev.u.ki.wScan = …` は
            # 値型のコピーに書くだけで、元の `INPUT` は 0 のまま送られる。
            $ki = New-Object SnotraSmokeInterop.Native+KEYBDINPUT
            $ki.wVk = 0
            $ki.wScan = [uint16][char]$unit
            $ki.dwFlags = $flags
            $union = New-Object SnotraSmokeInterop.Native+INPUTUNION
            $union.ki = $ki
            $ev = New-Object SnotraSmokeInterop.Native+INPUT
            $ev.type = 1   # INPUT_KEYBOARD
            $ev.u = $union
            $ev
        }
    }
    $inputs = [SnotraSmokeInterop.Native+INPUT[]]@($inputs)
    $size = [System.Runtime.InteropServices.Marshal]::SizeOf([type][SnotraSmokeInterop.Native+INPUT])
    $sent = [SnotraSmokeInterop.Native]::SendInput([uint32]$inputs.Count, $inputs, $size)
    if ($sent -ne $inputs.Count) {
        throw ("SendInput injected {0}/{1} events (GetLastError={2})" -f $sent, $inputs.Count,
            [System.Runtime.InteropServices.Marshal]::GetLastWin32Error())
    }
}

function Send-SnotraPostedChar {
    # `PostMessageW(WM_CHAR)` で、直前のキー押下を持たない文字を 1 つ送る（#1269 の形）。
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [IntPtr]$WindowHandle,
        [Parameter(Mandatory)]
        [char]$Char
    )

    Initialize-SnotraNativeInterop
    $WM_CHAR = 0x0102
    if (-not [SnotraSmokeInterop.Native]::PostMessageW($WindowHandle, $WM_CHAR, [UIntPtr][uint32][char]$Char, [IntPtr]1)) {
        throw ("PostMessageW(WM_CHAR) failed (GetLastError={0})" -f [System.Runtime.InteropServices.Marshal]::GetLastWin32Error())
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
    # ループの順は「評価 → 期限判定 → sleep」（理由は `Wait-SnotraTraceCondition` の doc・#872）。
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    while ($true) {
        # 同じタイトルのウィンドウを順にたどり、`-Process` のものを探す。**最初の 1 つで止めない。**
        # 普段使いの Snotra と並走すると、そちらのウィンドウが先に見つかりうる（#1306）。
        $handle = [IntPtr]::Zero
        while ($true) {
            $handle = [SnotraSmokeInterop.Native]::FindWindowExW([IntPtr]::Zero, $handle, [NullString]::Value, $Title)
            if ($handle -eq [IntPtr]::Zero) { break }
            $ownerMatches = $true
            if ($null -ne $Process) {
                [uint32]$ownerPid = 0
                [void][SnotraSmokeInterop.Native]::GetWindowThreadProcessId($handle, [ref]$ownerPid)
                $ownerMatches = ($ownerPid -eq $Process.Id)
            }
            if ($ownerMatches -and [SnotraSmokeInterop.Native]::IsWindowVisible($handle)) {
                return $handle
            }
        }
        if ($null -ne $Process -and $Process.HasExited) {
            throw "本体が終了しました（exit=$($Process.ExitCode)）。ウィンドウ '$Title' を観測できません。"
        }
        if ([DateTime]::UtcNow -ge $deadline) { break }
        Start-Sleep -Milliseconds $PollMs
    }
    throw "ウィンドウ '$Title' が ${TimeoutMs}ms 以内に現れませんでした。"
}

function Get-SnotraForegroundWindow {
    [CmdletBinding()]
    param()

    Initialize-SnotraNativeInterop
    return [SnotraSmokeInterop.Native]::GetForegroundWindow()
}

# 前面のウィンドウを、人が読める形にする。前面化に失敗したとき、打鍵が誰に届いていたかは
# その場でしか採れないので、`Set-SnotraForegroundWindow` の警告に載せる。
function Get-SnotraForegroundWindowLabel {
    [CmdletBinding()]
    param()

    $handle = Get-SnotraForegroundWindow
    if ($handle -eq [IntPtr]::Zero) { return '前面ウィンドウなし' }

    $title = New-Object System.Text.StringBuilder 256
    [void][SnotraSmokeInterop.Native]::GetWindowTextW($handle, $title, $title.Capacity)
    # **クラス名も出す**（#872）。題名だけでは、本体のウィンドウか、debug ビルドのコンソール
    # ウィンドウ（`ConsoleWindowClass`）かを後から見分けられない。
    $class = New-Object System.Text.StringBuilder 256
    [void][SnotraSmokeInterop.Native]::GetClassNameW($handle, $class, $class.Capacity)
    [uint32]$ownerPid = 0
    [void][SnotraSmokeInterop.Native]::GetWindowThreadProcessId($handle, [ref]$ownerPid)
    return "0x$($handle.ToString('X')) pid=$ownerPid class='$($class.ToString())' title='$($title.ToString())'"
}

<#
.SYNOPSIS
ウィンドウを前面へ出し、**実際に前面になったか**を返す。

.DESCRIPTION
`SetForegroundWindow` の戻り値では判定しない。前面ロックの規則により、対象が既に前面でも
FALSE が返る（CI runner で実測）。呼び出し側が知りたいのは、注入した打鍵がそのウィンドウへ
届くかであり、それを表すのは `GetForegroundWindow` との一致である。
一度で前面にならない環境のために `$TimeoutMs` まで再試行する。諦めるときは、前面のウィンドウを
警告に載せて `$false` を返す。
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
    # 前面ウィンドウが無い環境では、`GetForegroundWindow` も Zero を返す。Zero 同士の一致を
    # 「前面にできた」と読まないよう、比較の前に止める。
    if ($Handle -eq [IntPtr]::Zero) {
        throw '前面化するウィンドウのハンドルが Zero です。'
    }

    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    while ($true) {
        [void][SnotraSmokeInterop.Native]::SetForegroundWindow($Handle)
        if ((Get-SnotraForegroundWindow) -eq $Handle) { return $true }
        if ([DateTime]::UtcNow -ge $deadline) { break }
        Start-Sleep -Milliseconds $PollMs
    }

    Write-Warning ("ウィンドウ 0x$($Handle.ToString('X')) を ${TimeoutMs}ms 以内に前面化できません" +
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

    # **可視判定より先にロックを見る。** ロック中は IsWindowVisible も矩形も正常な値を返し、
    # 以下は最後まで成功して、中身だけが違う Bitmap を返す（#866）。
    # 判定できないときに throw しない理由は `Assert-SnotraSessionUnlocked` の doc にある。
    Assert-SnotraSessionUnlocked -Operation 'ウィンドウのキャプチャ'

    if (-not [SnotraSmokeInterop.Native]::IsWindowVisible($Handle)) {
        throw 'キャプチャ対象のウィンドウが可視ではありません。'
    }

    $rect = New-Object SnotraSmokeInterop.Native+RECT
    $usedDwmBounds = ([SnotraSmokeInterop.Native]::DwmGetWindowAttribute($Handle, 9, [ref]$rect, 16) -eq 0)
    if (-not $usedDwmBounds -and -not [SnotraSmokeInterop.Native]::GetWindowRect($Handle, [ref]$rect)) {
        throw 'DwmGetWindowAttribute と GetWindowRect の両方に失敗しました。'
    }

    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    if ($width -le 0 -or $height -le 0) {
        throw "ウィンドウの矩形が不正です: ${width}x${height}"
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

    # Bitmap の Dispose は呼び出し側が行う。判定はモジュールに持たせず、画像と矩形だけを返す。
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
    'Resolve-SnotraCargoTargetDirectory'
    'Get-SnotraRecordedExePath'
    'Resolve-SnotraExistingProcess'
    'Stop-SnotraProcessAndWait'
    'Read-SnotraTraceEvents'
    'Read-SnotraTraceSnapshot'
    'Wait-SnotraTraceCondition'
    'Wait-SnotraTraceEvent'
    'Get-SnotraTraceEventCount'
    'Send-SnotraKey'
    'Send-SnotraKeyChord'
    'Send-SnotraUnicodeText'
    'Send-SnotraPostedChar'
    'Wait-SnotraWindow'
    'Get-SnotraForegroundWindow'
    'Set-SnotraForegroundWindow'
    'Get-SnotraWindowCapture'
    'Get-SnotraWindowDpi'
    'ConvertFrom-SnotraWtsInfoEx'
    'Get-SnotraSessionLockState'
    'Assert-SnotraSessionUnlocked'
)
