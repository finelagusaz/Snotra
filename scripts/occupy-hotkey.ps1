<#
.SYNOPSIS
Snotra と同じ引数で Alt+Q を握り、Snotra 側の `RegisterHotKey` を実際に失敗させる（issue #1009）。

.DESCRIPTION
`bench-startup.ps1 -UseVerificationProfile` が毎回作り直す検証用プロファイルの既定ホットキーは
Alt+Q である（`SnotraSmoke.psm1` の `New-SnotraVerificationProfile`）。このスクリプトが先に
握っておくと Snotra 側の登録が実際に失敗し、`startup:failed` / `reason=hotkey-registration`
が出る。

**代理では測らない。** `platform/mod.rs` の `SNOTRA_FAKE_INITIAL_HOTKEY_FAILURE` は
**登録を成功させたまま失敗イベントだけを流す**（機序の正本は同ファイルの当該 arm）。ゆえに
それで測れるのは配送経路であって登録の失敗ではない（`AGENTS.md`「検証の作法」の「主張は代理では
なく対象そのもので測ってから書く」）。

引数は `platform/hotkey.rs` の `register_prepared` と同じにする——
`RegisterHotKey(NULL, 1, MOD_NOREPEAT | MOD_ALT, VK_Q)`。**同じ引数で握れば Snotra 側が
失敗することは実測した**（どの引数までが「同じ」と見なされるか——`MOD_NOREPEAT` の有無が
排他に効くか等——は測っていない）。ゆえに `register_prepared` を変えたらここも揃えること。

キー自体は `New-SnotraVerificationProfile` の既定から読む。**リテラルで書くと既定が変わった
ときに沈黙で no-op 化する**——占有できていない対照の起動は普通に成功し、「素通りしなかった」と
いう誤った結論になる。

.NOTES
**このスクリプトは常設である。** `AGENTS.md`「条件別チェック（トリガー → 参照先）」が撤去条件の
明記を要求する「調査・測定のための一時的な足場」ではない——理由は、**実際の `RegisterHotKey`
失敗を起こせる唯一の手段**だからである。`SnotraStartupContract.psm1` の
「`event` と `ok` / `reason` の整合」検査を、**実際に壊れた起動**に対して検算できる。
**当該検査が消えたら、このスクリプトも用済みになる。**

**解放し損ねても後を引かない**——プロセスが終われば OS が登録を解放する。`-DurationSeconds` を
渡さない場合は Ctrl+C で止める（対話用）。エージェントから使うときは秒数を渡すか、
`Stop-Process` で殺してよい。

.EXAMPLE
pwsh -NoProfile -File scripts/occupy-hotkey.ps1 -DurationSeconds 120
#>
param(
  # 0 なら無期限（Ctrl+C で止める）。エージェントから使うときは秒数を渡す。
  [int]$DurationSeconds = 0
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
Import-Module (Join-Path $PSScriptRoot 'lib/SnotraSmoke.psm1') -Force

# **型の再定義を避ける**（`Initialize-SnotraNativeInterop` と同じ流儀）。同一セッションで
# 2 度読み込むと `Add-Type` が落ちる。
if (-not ('Snotra.HotkeyOccupier' -as [type])) {
  Add-Type -Namespace Snotra -Name HotkeyOccupier -MemberDefinition @'
[DllImport("user32.dll", SetLastError = true)]
public static extern bool RegisterHotKey(IntPtr hWnd, int id, uint fsModifiers, uint vk);

[DllImport("user32.dll", SetLastError = true)]
public static extern bool UnregisterHotKey(IntPtr hWnd, int id);
'@
}

# **キーは検証用プロファイルの既定から読む**（リテラルで書くと既定が変わったとき沈黙で
# no-op 化する）。`bench-startup.ps1 -UseVerificationProfile` はこの既定で config を作り直す。
function Get-ParameterDefault {
  param([string]$Name)
  $params = (Get-Command New-SnotraVerificationProfile).ScriptBlock.Ast.Body.ParamBlock.Parameters
  return ($params | Where-Object { $_.Name.VariablePath.UserPath -eq $Name }).DefaultValue.Value
}
$hotkeyModifier = Get-ParameterDefault -Name 'HotkeyModifier'
$hotkeyKey = Get-ParameterDefault -Name 'HotkeyKey'
if ($hotkeyModifier -ne 'Alt' -or $hotkeyKey.Length -ne 1) {
  # **既定が動いたらここで止める。** 対応する Win32 定数の写像を持たないまま握ると、
  # 別のキーを占有して「握れた」と報告し、対照の起動が普通に成功する。
  throw "検証用プロファイルの既定ホットキーが $hotkeyModifier+$hotkeyKey へ変わりました。このスクリプトの Win32 定数を揃えてください"
}

# `platform/hotkey.rs` の `register_prepared` と同じ値。**片方だけ変えると占有にならない。**
$HOTKEY_ID = 1
$MOD_ALT = 0x0001
$MOD_NOREPEAT = 0x4000
$vk = [int][char]$hotkeyKey.ToUpperInvariant()   # 'Q' → 0x51（VK_A〜VK_Z は ASCII 大文字と同値）
$modifiers = $MOD_ALT -bor $MOD_NOREPEAT

if (-not [Snotra.HotkeyOccupier]::RegisterHotKey([IntPtr]::Zero, $HOTKEY_ID, $modifiers, $vk)) {
  $err = [System.Runtime.InteropServices.Marshal]::GetLastWin32Error()
  # **ここで失敗したら占有できていない。** 呼び出し側が「握れた」と誤読すると、
  # 対照の起動が普通に成功して「(e) は素通りしなかった」という誤った結論になる。
  throw "$hotkeyModifier+$hotkeyKey を握れませんでした（GetLastError=$err）。既に誰かが握っているか、Snotra が起動中です"
}

# **`try` は登録の直後から開く**（生成/破棄のペア）。間に置いた行が投げると `finally` を
# 通らない——実害はプロセス終了で OS が解放するので無いが、ペアは構造で守る。
try {
  Write-Host "Alt+Q を握りました（MOD_NOREPEAT|MOD_ALT + VK_Q・id=$HOTKEY_ID）。" -ForegroundColor Green
  if ($DurationSeconds -gt 0) {
    Write-Host "$DurationSeconds 秒後に解放します。" -ForegroundColor Cyan
    Start-Sleep -Seconds $DurationSeconds
  } else {
    Write-Host "Ctrl+C で解放します。" -ForegroundColor Cyan
    # **1 秒ループにしない**——反復ごとに約 27〜44 ms の CPU が乗り、1 コアの 2.7〜4.4% を
    # 恒常的に食う（実測）。長いスリープでも Ctrl+C は 20 ms 以内に届き `finally` が走る（実測）。
    while ($true) { Start-Sleep -Seconds 3600 }
  }
} finally {
  [void][Snotra.HotkeyOccupier]::UnregisterHotKey([IntPtr]::Zero, $HOTKEY_ID)
  Write-Host "Alt+Q を解放しました。" -ForegroundColor Green
}
