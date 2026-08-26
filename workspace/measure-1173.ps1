<#
.SYNOPSIS
#1173 の測定治具——キーボードによる窓移動（SC_MOVE のモーダルループ）と可視中クランプの相互作用を測る。

.DESCRIPTION
2 つの問いに答える。

- **Q1**: `Alt+Space` はこの窓（`decorations:false` / `skip_taskbar:true`）でシステムメニューを開くか。
  tao 0.35.3 の `keyboard.rs:162-172` が `WM_SYSCHAR` を `ProcResult::Value(0)` で握り潰すため
  「開かない」と予測している。**ソース読みは代理であって対象そのものの測定ではない**ので測る。
- **Q2**: SC_MOVE のモーダル移動ループ中に egui のフレームが回り、
  `clamp_main_into_work_area`（`view.rs:1279` の `!any_down()` が唯一の発火条件）が働くか。
  判別子は「**移動モード中にバー矩形を作業領域の外へ出せるか**」——単一モニターでも測れる。

**`SnotraSmoke.psm1` の関数で組む**（`docs/build-commands.md`「エージェントが目視項目を自分で実施するとき」）。
例外は `PostMessage` の 1 つだけで、Q1 が「開かない」だったときに SC_MOVE へ入る唯一の手段である。
**#866 の画面ロック検出が守るのは打鍵注入と窓キャプチャであり、`SC_MOVE` の post はその射程外**
（撮らない・撃たない）。`SendMessage` は使わない——同期呼び出しゆえモーダルループがその内側で回り、
治具の単一スレッドが戻らず矢印注入に到達できない。

**DPI**: `Get-SnotraWindowDpi` を最初に通して awareness を確立する（`Initialize-SnotraDpiAwareness` は
未 export だが同関数が内部で呼ぶ）。通さないと `GetMonitorInfoW` が仮想化した論理値を返し、
`GetWindowRect`（物理）と土俵が合わない——この調査は一度この罠を踏んで訂正している。

.NOTES
**一時的な足場である。撤去条件**: #1173 を閉じる PR が main へ**マージされた後**、次に
`workspace/` を書き換えるサイクルで削除する（`workspace/` はサイクルごとに書き直される）。
撤去対象はこのファイルと `workspace/measurement-1173-treatment.txt` /
`workspace/measurement-1173-control.txt` / `workspace/adversarial-1173.txt` である。
**「issue が閉じたら」を合図にしない**——閉じるのが当の PR なので自己参照して発火しない。
#>
[CmdletBinding()]
param(
    [string]$ProfileDir = 'C:/tmp/snotra-1173',
    [string]$LogDir = 'C:/tmp/snotra-1173-logs',
    # 既定は treatment 側（クランプ有効）。対照（クランプ無効のローカルパッチ）を測るときは
    # `-OutFile .../measurement-1173-control.txt` を渡す——**既定を上書きさせない**。
    [string]$OutFile = 'C:/workspace/Snotra/workspace/measurement-1173-treatment.txt',
    [int]$Iterations = 5,
    [int]$ArrowPresses = 240
)

$ErrorActionPreference = 'Stop'
Import-Module 'C:/workspace/Snotra/scripts/lib/SnotraSmoke.psm1' -Force

# PostMessage だけはモジュールの interop に無い（上の .DESCRIPTION が理由を持つ）。
if (-not ('Snotra1173.Post' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace Snotra1173 {
  public class Post {
    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool PostMessageW(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")]
    public static extern IntPtr MonitorFromWindow(IntPtr hWnd, uint flags);
    [DllImport("user32.dll")]
    public static extern bool GetMonitorInfoW(IntPtr hMonitor, ref MONITORINFO info);
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int left, top, right, bottom; }
    [StructLayout(LayoutKind.Sequential)]
    public struct MONITORINFO { public int cbSize; public RECT rcMonitor; public RECT rcWork; public uint dwFlags; }
  }
}
'@
}

$VK_MENU = 0x12; $VK_SPACE = 0x20; $VK_DOWN = 0x28; $VK_ESCAPE = 0x1B; $VK_RETURN = 0x0D
$VK_CONTROL = 0x11; $VK_K = 0x4B; $VK_M = 0x4D
$WM_SYSCOMMAND = 0x0112; $SC_MOVE = 0xF010

New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
$lines = New-Object System.Collections.Generic.List[string]
function Emit([string]$s) {
    $lines.Add($s) | Out-Null
    Write-Host $s
    # **測る前に、いま持っている分を出力先へ書く**（中断に耐えるため・#878）。
    Set-Content -LiteralPath $OutFile -Value $lines -Encoding UTF8
}

function Get-Rect([IntPtr]$h) {
    $r = New-Object SnotraSmokeInterop.Native+RECT
    if (-not [SnotraSmokeInterop.Native]::GetWindowRect($h, [ref]$r)) { throw 'GetWindowRect に失敗しました。' }
    return $r
}

Emit "# #1173 測定結果（生の観測）"
Emit ""
Emit "実行日時: $([DateTimeOffset]::Now.ToString('yyyy-MM-dd HH:mm:ss zzz'))"
Emit "バイナリ: target/release/snotra.exe（release）"
Emit "プロファイル: $ProfileDir（使い捨て・hotkey は実 config へ揃えて Ctrl+K）"
Emit ""

Assert-SnotraSessionUnlocked

$exe = 'C:/workspace/Snotra/target/release/snotra.exe'
if (-not (Test-Path $exe)) { throw "release バイナリがありません: $exe" }

New-SnotraVerificationProfile -ProfileDir $ProfileDir -HotkeyModifier 'Ctrl' -HotkeyKey 'K' -WindowWidth 300 | Out-Null

$proc = Start-SnotraProcess -ConfigDir $ProfileDir -FilePath $exe -Trace `
    -StandardErrorPath (Join-Path $LogDir 'stderr.log') -StandardOutputPath (Join-Path $LogDir 'stdout.log')

try {
    Start-Sleep -Milliseconds 2500
    # hotkey で表示（Ctrl+K・Alt を含まないので ShowAfterAltRelease に当たらない）
    Send-SnotraKeyChord -VirtualKeys @($VK_CONTROL, $VK_K)
    $h = Wait-SnotraWindow -Title 'Snotra' -Process $proc -TimeoutMs 20000

    # **DPI awareness をここで確立する**（以降の GetMonitorInfoW / GetWindowRect が物理座標で揃う）
    $dpi = Get-SnotraWindowDpi -Handle $h
    $mi = New-Object Snotra1173.Post+MONITORINFO
    $mi.cbSize = 40
    [void][Snotra1173.Post]::GetMonitorInfoW([Snotra1173.Post]::MonitorFromWindow($h, 2), [ref]$mi)
    Emit "窓 DPI: $dpi（96 = 100%）"
    Emit "rcMonitor: $($mi.rcMonitor.left),$($mi.rcMonitor.top),$($mi.rcMonitor.right),$($mi.rcMonitor.bottom)"
    Emit "rcWork:    $($mi.rcWork.left),$($mi.rcWork.top),$($mi.rcWork.right),$($mi.rcWork.bottom)"
    $workBottom = $mi.rcWork.bottom
    Emit ""

    [void](Set-SnotraForegroundWindow -Handle $h)
    Start-Sleep -Milliseconds 300
    $r0 = Get-Rect $h
    Emit "## Q1: Alt+Space はシステムメニューを開くか"
    Emit ""
    Emit "打鍵前の窓矩形: L=$($r0.left) T=$($r0.top) R=$($r0.right) B=$($r0.bottom)"

    Send-SnotraKeyChord -VirtualKeys @($VK_MENU, $VK_SPACE) -InterKeyDelayMs 80
    Start-Sleep -Milliseconds 600

    $menu = [SnotraSmokeInterop.Native]::FindWindowW('#32768', [NullString]::Value)
    $menuVisible = ($menu -ne [IntPtr]::Zero) -and [SnotraSmokeInterop.Native]::IsWindowVisible($menu)
    $fg = [SnotraSmokeInterop.Native]::GetForegroundWindow()
    $cls = New-Object System.Text.StringBuilder 256
    [void][SnotraSmokeInterop.Native]::GetClassNameW($fg, $cls, 256)
    Emit "FindWindowW('#32768'): handle=$menu visible=$menuVisible"
    Emit "前面窓: handle=$fg class='$($cls.ToString())'（本体窓 handle=$h）"
    Emit "Q1 判定: $(if ($menuVisible) { 'システムメニューが開いた' } else { '開かなかった' })"
    Emit ""

    if ($menuVisible) { Send-SnotraKey -VirtualKey $VK_ESCAPE; Start-Sleep -Milliseconds 200; Send-SnotraKey -VirtualKey $VK_ESCAPE -Up }

    Emit "## Q2: モーダル移動ループ中にクランプが発火するか"
    Emit ""
    Emit "入口: $(if ($menuVisible) { 'Alt+Space → M' } else { 'PostMessage(WM_SYSCOMMAND, SC_MOVE)' })"
    Emit "判別子: 移動モード中に窓の bottom が rcWork.bottom（$workBottom）を越えられるか"
    Emit ""

    for ($i = 1; $i -le $Iterations; $i++) {
        # Escape / 確定後に窓が隠れていれば hotkey で出し直す（隠れた窓へ前面化しても打鍵の宛先が定まらない）
        if (-not [SnotraSmokeInterop.Native]::IsWindowVisible($h)) {
            Send-SnotraKeyChord -VirtualKeys @($VK_CONTROL, $VK_K)
            Start-Sleep -Milliseconds 800
        }
        [void](Set-SnotraForegroundWindow -Handle $h)
        Start-Sleep -Milliseconds 250
        $rs = Get-Rect $h
        Emit "### 反復 $i"
        Emit "開始:   L=$($rs.left) T=$($rs.top) B=$($rs.bottom)"

        if ($menuVisible) {
            Send-SnotraKeyChord -VirtualKeys @($VK_MENU, $VK_SPACE) -InterKeyDelayMs 80
            Start-Sleep -Milliseconds 400
            Send-SnotraKeyChord -VirtualKeys @($VK_M) -InterKeyDelayMs 60
        } else {
            [void][Snotra1173.Post]::PostMessageW($h, $WM_SYSCOMMAND, [IntPtr]$SC_MOVE, [IntPtr]0)
        }
        Start-Sleep -Milliseconds 500

        $series = New-Object System.Collections.Generic.List[string]
        $maxBottom = $rs.bottom
        for ($k = 1; $k -le $ArrowPresses; $k++) {
            Send-SnotraKey -VirtualKey $VK_DOWN
            Send-SnotraKey -VirtualKey $VK_DOWN -Up
            if ($k % 20 -eq 0) {
                Start-Sleep -Milliseconds 60
                $rk = Get-Rect $h
                if ($rk.bottom -gt $maxBottom) { $maxBottom = $rk.bottom }
                $series.Add("k=$k B=$($rk.bottom)") | Out-Null
            }
        }
        Start-Sleep -Milliseconds 300
        $rMid = Get-Rect $h
        Emit "移動中: $($series -join ' / ')"
        Emit "移動中の最大 bottom: $maxBottom（rcWork.bottom=$workBottom・越えた=$($maxBottom -gt $workBottom)）"
        Emit "確定前: L=$($rMid.left) T=$($rMid.top) B=$($rMid.bottom)"

        # 確定（Enter）→ 最初のフレームで戻るか
        Send-SnotraKey -VirtualKey $VK_RETURN; Start-Sleep -Milliseconds 60; Send-SnotraKey -VirtualKey $VK_RETURN -Up
        Start-Sleep -Milliseconds 700
        $rEnd = Get-Rect $h
        Emit "確定後: L=$($rEnd.left) T=$($rEnd.top) B=$($rEnd.bottom)"
        Emit ""
        Start-Sleep -Milliseconds 400
    }
} finally {
    # **必ずモーダルループから抜ける**——入ったまま終了すると窓がループに残る
    try { Send-SnotraKey -VirtualKey $VK_ESCAPE; Start-Sleep -Milliseconds 80; Send-SnotraKey -VirtualKey $VK_ESCAPE -Up } catch {}
    try { Stop-SnotraProcessAndWait -Process $proc } catch { try { $proc | Stop-Process -Force } catch {} }
    Set-Content -LiteralPath $OutFile -Value $lines -Encoding UTF8
}

Emit "（測定終了）"
