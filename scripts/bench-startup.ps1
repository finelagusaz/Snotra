<#
.SYNOPSIS
起動の端から端まで（プロセス作成 → ホットキー登録完了）を N 回測り、区間ごとの
min / p50 / max を出す（issue #1000）。

.DESCRIPTION
**時間の内訳が主、メモリは従である。** 旧版は `SNOTRA_TRACE=1` を立てて stderr を
ファイルへ落としながら**そのファイルを一度も読まず**、プロセスツリーの WorkingSet だけを
測っていた。加えて WebView2 期の子孫プロセス走査を抱えていた（現構成のプロセスツリーは
1 件・#532 SU7）。

計器の本体は `src-tauri/src/startup.rs`（`startup:ready` / `startup:failed`）。
**このスクリプトは判定規則を持たず、契約の検査だけを行う**——区間の意味・`null` の
規則・恒等式はすべて Rust 側の doc が正本である。

.NOTES
**最小値だけに畳まない。** このハーネスが答えるべき問いは「起動が何 ms か」ではなく
「`smoke-startup.ps1` が記録した 0.6〜8s の分散がどの区間に住むか」であり、
**分散そのものが観測対象**である。ゆえに min / p50 / max を並べて出す。
#>
param(
  [int]$Iterations = 5,
  # 終端（`startup:ready` / `startup:failed`）が出るまでの予算。`smoke-startup.ps1` が
  # 実測した分散（0.6s〜8s超）に対する余裕として 20s を既定にする。
  [int]$TerminalTimeoutMs = 20000,
  # 終端が出た後、メモリを測るまでの落ち着き待ち。
  [int]$SettleMs = 1500,
  [string]$ExePath = "C:/workspace/Snotra/target/release/snotra.exe",
  # **検証用プロファイルを使う**（CI 等、実 config が無い環境向け）。既定は実 config で、
  # 開発機の実運用点をそのまま測る。CI では実 config が無く first-run へ落ちるため、
  # smoke 群と同じ形（`New-SnotraVerificationProfile` + `SNOTRA_CONFIG_DIR`）で
  # 非 first-run を再現する。**枝は出力の `first_run` / `cache_hit` に現れる**ので、
  # どちらで測ったかは読み手が毎回確かめられる。
  [switch]$UseVerificationProfile,
  [string]$ProfileDir = "target/bench-startup/profile"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
Import-Module (Join-Path $PSScriptRoot 'lib/SnotraSmoke.psm1') -Force

if ($Iterations -lt 1) { throw "Iterations must be >= 1" }
if ($TerminalTimeoutMs -lt 1000) { throw "TerminalTimeoutMs must be >= 1000" }
if (-not (Test-Path -LiteralPath $ExePath)) {
  throw "Executable not found: $ExePath（release を測るなら先に cargo build --release -p snotra）"
}

# **区間の一覧は Rust 側の `Phase` が正本である。** ここは表示順を決めるだけで、
# 過不足はキー検査（`Test-StartupPayload`）がペイロード側と突き合わせて捕まえる。
$PhaseKeys = @(
  'pre_main', 'config_load', 'index_load', 'path_merge', 'history_load',
  'engine_build', 'tauri_init', 'windows_create', 'setup_rest', 'hotkey_register'
)

<#
.SYNOPSIS
1 回ぶんのペイロードが契約を満たすか検査し、破れの一覧を返す（空なら合格）。

.DESCRIPTION
**検査は 3 つある**（`workspace/plan.md` 受け入れ 3）:

1. **キーの過不足** — 全区間の `*_ns` / `*_ms` が在ること。`Set-StrictMode` 下で欠落キーは
   `$null` ではなく `PropertyNotFoundException` を投げる（実測）ので、存在判定は
   `PSObject.Properties` で行う。**`null` と「キーが無い」は別である。**
2. **`null` の規則（双方向）** — 通らなかった区間は `null` になる。説明者は 3 つ:
   `first_run` / `include_path_env` / `reached_phase` より後ろ（**`ok` の真偽では免除しない**
   ——一律免除は失敗経路の取り落としを見えなくする）。

   **両向きを検査する。** 「説明されない `null`」だけを見る形は片手落ちで、**スキップした
   区間に `0` を書く誤りが素通りする**（変異 (f) を書いてみて気づいた——`null` と `0` を
   区別する設計の要なのに、検査が片方向だった）。枝フラグが「通らない」と言っている区間に
   値が在るのも同じ重さの破れである。
3. **恒等式** — `post_main_ns == sum_phase_ns + unmarked_tail_ns`。**ms 表示値の和は検査
   しない**（丸めは表示境界でだけ行うので、正しくても境界で合わない）。

   **この検査は弱い。** `unmarked_tail_ns` は `post_main - sum_phase` として計算されるので、
   等式は**構成上ほぼ常に真**である——実際に捕まえるのは `sum_phase > post_main`（飽和が
   起きる側）だけで、変異「`post_main` を部分和から作る」（同語反復化）は素通りする（実測）。
4. **外部の壁時計との突き合わせ** — 上の弱さを埋める。ハーネスは `Start-Process` から終端の
   trace が届くまでを**独立に**測っており、`pre_main + post_main` がそれを超えることはない。
   **超えたら、計器が内側で辻褄を合わせている**（同語反復化した実装は区間の実測を捨てるので、
   外から見た経過と食い違う）。下限は置かない——trace の到着はポーリング間隔ぶん遅れる。
#>
function Test-StartupPayload {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)]$Data,
    [Parameter(Mandatory)][string[]]$PhaseKey,
    # ハーネスが独立に測った「起動〜終端の trace を読むまで」の壁時計（ms）。
    [Parameter(Mandatory)][double]$ObservedWallClockMs
  )

  $failures = @()
  $has = { param($name) $null -ne $Data.PSObject.Properties[$name] }

  # --- 1. キーの過不足 ---
  foreach ($k in $PhaseKey) {
    foreach ($suffix in @('_ns', '_ms')) {
      if (-not (& $has "$k$suffix")) { $failures += "キー欠落: $k$suffix" }
    }
  }
  foreach ($k in @('post_main_ns', 'post_main_ms', 'sum_phase_ns', 'unmarked_tail_ns',
      'index_load_unattributed_ms', 'reached_phase', 'first_run', 'cache_hit',
      'include_path_env', 'ok', 'reason')) {
    if (-not (& $has $k)) { $failures += "キー欠落: $k" }
  }
  if ($failures.Count -gt 0) { return $failures }  # 以降は全キーの存在に依存する

  # --- 2. 説明されない null ---
  # `reached_phase` より後ろは「そこへ到達していない」。到達済みの区間だけを検査対象にする。
  $reached = $Data.reached_phase
  $reachedIndex = if ($null -eq $reached) { -1 } else { $PhaseKey.IndexOf([string]$reached) }
  if ($null -ne $reached -and $reachedIndex -lt 0) {
    $failures += "reached_phase が未知の区間名: $reached"
  }
  foreach ($k in $PhaseKey) {
    # pre_main は Phase ではない（壁時計。取得失敗なら null で正しく、値が在っても正しい）。
    if ($k -eq 'pre_main') { continue }

    # この区間が `null` であるべきか。**枝フラグと到達位置だけで決まる。**
    $i = $PhaseKey.IndexOf($k)
    $skippedByBranch =
      ($k -eq 'index_load' -and $Data.first_run) -or
      ($k -eq 'path_merge' -and -not $Data.include_path_env)
    $notReached = ($reachedIndex -lt 0) -or ($i -gt $reachedIndex)
    $shouldBeNull = $skippedByBranch -or $notReached

    $isNull = ($null -eq $Data."${k}_ns")
    $ctx = "reached_phase=$reached / first_run=$($Data.first_run) / include_path_env=$($Data.include_path_env)"

    if ($isNull -and -not $shouldBeNull) {
      $failures += "説明されない null: $k（$ctx）"
    } elseif (-not $isNull -and $skippedByBranch) {
      # **逆向き。** スキップした区間に値が在る＝`null` と `0` の区別が壊れている
      # （`notReached` 側は逆向きに検査しない——`reached_phase` は「刻んだ最後」なので
      # 定義上そこまでは値が在り、矛盾しようがない）。
      $failures += "null であるべき区間に値がある: $k = $($Data."${k}_ns") ns（$ctx）"
    }
  }

  # --- 3. 恒等式（生 ns のみ。ms 表示値の和は検査しない） ---
  $lhs = [long]$Data.post_main_ns
  $rhs = [long]$Data.sum_phase_ns + [long]$Data.unmarked_tail_ns
  if ($lhs -ne $rhs) {
    $failures += "恒等式の破れ: post_main_ns=$lhs != sum_phase_ns + unmarked_tail_ns = $rhs"
  }

  # --- 4. 外部の壁時計との突き合わせ ---
  # **内側の申告が外から見た経過を超えることはない。** 超えたら計器が内側で辻褄を合わせて
  # いる（同語反復化した実装は区間の実測を捨てるので、外から見た経過と食い違う）。
  # 下限は置かない——trace の到着はポーリング間隔ぶん遅れるため、内側 < 外側が正常である。
  $claimedMs = [double]$Data.post_main_ms + $(if ($null -eq $Data.pre_main_ms) { 0 } else { [double]$Data.pre_main_ms })
  if ($claimedMs -gt $ObservedWallClockMs) {
    $failures += ("内側の申告が外から見た経過を超えた: pre_main + post_main = ${claimedMs}ms > " +
      "観測 ${ObservedWallClockMs}ms（計器が内側で辻褄を合わせている疑い）")
  }

  return $failures
}

function Get-SnotraPrivateWorkingSetMB {
  param([int]$ProcessId)
  # **子孫の走査は持たない**（現構成のプロセスツリーは 1 件・#532 SU7 で WebView2 は消滅）。
  $p = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
  if (-not $p) { return $null }
  return [math]::Round($p.WorkingSet64 / 1MB, 1)
}

function Get-Percentile {
  param([double[]]$Value, [double]$P)
  if ($Value.Count -eq 0) { return $null }
  $sorted = @($Value | Sort-Object)
  $idx = [int][math]::Floor(($sorted.Count - 1) * $P)
  return $sorted[$idx]
}

$profileFull = $null
if ($UseVerificationProfile) {
  # **seed は 1 回だけ**（ループ内で作り直すと毎回 first-run + cache-miss になり、
  # 測っているものが変わる）。2 回目以降の起動が `index.bin` を読む形が実運用点に近い。
  $profileFull = [System.IO.Path]::GetFullPath((Join-Path (Get-Location) $ProfileDir))
  New-SnotraVerificationProfile -ProfileDir $profileFull -ShowIcons $false | Out-Null
}

Write-Host "=== Snotra 起動計器（時間が主・メモリは従） ===" -ForegroundColor Cyan
Write-Host "Exe:        $ExePath"
Write-Host "Iterations: $Iterations"
if ($null -ne $profileFull) { Write-Host "Profile:    $profileFull（検証用・SNOTRA_CONFIG_DIR）" }
Write-Host ""

$savedTrace = $env:SNOTRA_TRACE
$runs = @()
$failures = @()

try {
  for ($run = 1; $run -le $Iterations; $run++) {
    Write-Host "Run $run/$Iterations ... " -NoNewline

    Get-Process snotra -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Milliseconds 300

    $errPath = Join-Path $env:TEMP ("snotra_bench_{0}.err" -f $run)
    $outPath = Join-Path $env:TEMP ("snotra_bench_{0}.out" -f $run)
    foreach ($p in @($errPath, $outPath)) { if (Test-Path -LiteralPath $p) { Remove-Item -LiteralPath $p -Force } }

    # **外から独立に測る壁時計。** 内側の申告と突き合わせる相手であり、計器が自分の
    # 出力だけで辻褄を合わせる形（同語反復化）を外から捕まえる唯一の材料である。
    $wall = [System.Diagnostics.Stopwatch]::StartNew()
    if ($null -ne $profileFull) {
      # **env の設定と復元は共有モジュールが持つ**（成功・例外の両経路で戻す）。
      # `SNOTRA_CONFIG_DIR` / `SNOTRA_TRACE` は予約キーで、ここから上書きできない。
      $proc = Start-SnotraProcess -ConfigDir $profileFull -Trace -FilePath $ExePath `
        -StandardErrorPath $errPath -StandardOutputPath $outPath
    } else {
      $env:SNOTRA_TRACE = "1"
      $proc = Start-Process -FilePath $ExePath -PassThru `
        -RedirectStandardError $errPath -RedirectStandardOutput $outPath
    }

    # **終端は 2 つある。** `startup:ready` だけを待つと、失敗した起動では期限切れになり
    # 「終端が出なかった」という**誤った理由**で落ちる——起きたこと（登録失敗・bridge の
    # 初期化失敗）が読めない。両方を待って、`startup:failed` は理由つきの失敗として扱う。
    $terminal = Wait-SnotraTraceCondition -Path $errPath -TimeoutMs $TerminalTimeoutMs -PollMs 100 `
      -AbortIfExited $proc -Description '起動の終端（startup:ready / startup:failed）' `
      -Predicate { $_.event -eq 'startup:ready' -or $_.event -eq 'startup:failed' }
    $wall.Stop()
    $observedMs = $wall.Elapsed.TotalMilliseconds

    $memMB = $null
    if ($null -ne $terminal) {
      Start-Sleep -Milliseconds $SettleMs
      if (-not $proc.HasExited) { $memMB = Get-SnotraPrivateWorkingSetMB -ProcessId $proc.Id }
    }

    if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force }
    Start-Sleep -Milliseconds 200

    if ($null -eq $terminal) {
      # **沈黙を合格と読ませない**（#471 / #690 の型）。
      $failures += "run=$run 終端が出なかった（予算 ${TerminalTimeoutMs}ms）"
      Write-Host "終端なし" -ForegroundColor Red
      continue
    }

    $data = $terminal.data
    if ($terminal.event -eq 'startup:failed') {
      # **`reason` はそのまま載せる**（ハーネス側で分類名を書き起こさない——写しが 2 部になる）。
      $failures += "run=$run 起動が失敗した: reason=$($data.reason) / reached_phase=$($data.reached_phase)"
      Write-Host "startup:failed reason=$($data.reason)" -ForegroundColor Red
      continue
    }

    $contractFailures = Test-StartupPayload -Data $data -PhaseKey $PhaseKeys -ObservedWallClockMs $observedMs
    foreach ($f in $contractFailures) { $failures += "run=$run $f" }

    $row = [ordered]@{ run = $run; memory_MB = $memMB }
    foreach ($k in $PhaseKeys) { $row[$k] = $data."${k}_ms" }
    $row['post_main'] = $data.post_main_ms
    $row['unattributed'] = $data.index_load_unattributed_ms
    $runs += [pscustomobject]$row

    $branch = "cache_hit=$($data.cache_hit) first_run=$($data.first_run) path_env=$($data.include_path_env)"
    Write-Host ("pre_main={0}ms post_main={1}ms mem={2}MB {3}" -f `
        $data.pre_main_ms, $data.post_main_ms, $memMB, $branch)
  }
} finally {
  if ($null -eq $savedTrace) {
    Remove-Item Env:SNOTRA_TRACE -ErrorAction SilentlyContinue
  } else {
    # **空文字を作らない**（#872: PowerShell の env 復元が空文字を作り、測定ハーネスの
    # 全反復が黙って計器つきで走っていた）。`SNOTRA_TRACE` は `env_flag` ゆえ空文字は
    # 「無効」に落ちるが、復元の形は smoke 群と揃える。
    $env:SNOTRA_TRACE = $savedTrace
  }
}

Write-Host ""
if ($runs.Count -gt 0) {
  Write-Host "=== 区間ごとの min / p50 / max（ms） ===" -ForegroundColor Cyan
  # **最小値だけに畳まない。** 分散こそが観測対象である（この計器の存在理由）。
  $summary = foreach ($k in @($PhaseKeys + @('post_main'))) {
    $values = @($runs | ForEach-Object { $_.$k } | Where-Object { $null -ne $_ } | ForEach-Object { [double]$_ })
    if ($values.Count -eq 0) {
      [pscustomobject]@{ phase = $k; min = 'n/a'; p50 = 'n/a'; max = 'n/a'; samples = 0 }
    } else {
      [pscustomobject]@{
        phase   = $k
        min     = ($values | Measure-Object -Minimum).Minimum
        p50     = Get-Percentile -Value $values -P 0.5
        max     = ($values | Measure-Object -Maximum).Maximum
        samples = $values.Count
      }
    }
  }
  $summary | Format-Table -AutoSize

  Write-Host "=== 各 run ===" -ForegroundColor Cyan
  $runs | Format-Table -AutoSize
}

if ($failures.Count -gt 0) {
  Write-Host "=== 失敗 ===" -ForegroundColor Red
  $failures | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
  throw "起動計器の検査が失敗しました（$($failures.Count) 件）"
}

if ($runs.Count -eq 0) { throw "有効な標本が 1 つも取れませんでした" }

Write-Host "起動計器 passed（$($runs.Count) runs）。" -ForegroundColor Green
