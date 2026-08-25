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
  # 空なら**このスクリプトが住むリポジトリ**の release 本体を `cargo metadata` から導く（#1179）。
  # **絶対パスを直書きしない**——worktree から既定のまま回すと別の作業コピーの本体を測り、本体は
  # 実在するので完走して緑になる（失敗が緑と同じ見た目をする）。導出は `param()` の中に書けない
  # ——既定値の束縛は `Import-Module` より前に起きる（実測）ので、解決は下の import 後で行う。
  [string]$ExePath = '',
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
# **契約の検査は module 側が持つ**（#1009）。`.ps1` の中に置くと Pester の探索
# （`scripts/lib` のみ）から外れ、検査自身を測れない。
Import-Module (Join-Path $PSScriptRoot 'lib/SnotraStartupContract.psm1') -Force

if ($Iterations -lt 1) { throw "Iterations must be >= 1" }
if ($TerminalTimeoutMs -lt 1000) { throw "TerminalTimeoutMs must be >= 1000" }
# 明示された `-ExePath` の意味は変えない（相対パスは cwd 相対のまま・PowerShell の慣行）。
# 導出するのは既定の枝だけである。
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $ExePath) {
  $ExePath = Resolve-SnotraCargoExecutable -RepositoryRoot $repoRoot -Profile release
}
if (-not (Test-Path -LiteralPath $ExePath)) {
  throw "Executable not found: $ExePath（release を測るなら先に cargo build --release -p snotra）"
}
# **測った本体のパスを絶対形で出力へ載せる**（#1179）。どのコピーを測ったかが読み手に見える形が、
# `-Profile` の取り違え（release/debug）に対する唯一の観測でもある。
$ExePath = (Resolve-Path -LiteralPath $ExePath).Path

# **区間の一覧は Rust 側の `Phase` が正本である。** ここは表示順を決めるだけで、
# 過不足はキー検査（`Test-SnotraStartupPayload`）がペイロード側と突き合わせて捕まえる。
$PhaseKeys = @(
  'pre_main', 'config_load', 'index_load', 'path_merge', 'history_load',
  'engine_build', 'tauri_init', 'windows_create', 'setup_rest', 'hotkey_register'
)

function Get-SnotraWorkingSetMB {
  param([int]$ProcessId)
  # **子孫の走査は持たない**（現構成のプロセスツリーは 1 件・#532 SU7 で WebView2 は消滅）。
  # **返すのは `WorkingSet64`（プロセス全体）であって private working set ではない。**
  # 旧名 `Get-SnotraPrivateWorkingSetMB` は在りもしない private を名乗っていた（#1009 で改名）。
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

    # **終了を待つ**（#872）。待たずに次を起動すると single-instance で即終了し、trace を
    # 1 行も書かないまま待ちが予算を使い切る。共有ヘルパーが待ちと警告を持つ
    # （`smoke-egui.ps1` / `smoke-startup.ps1` と同じ経路）。
    Resolve-SnotraExistingProcess -Policy Stop

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
      if (-not $proc.HasExited) { $memMB = Get-SnotraWorkingSetMB -ProcessId $proc.Id }
    }

    # ここも待つ（#872）。ループ先頭の `Resolve-SnotraExistingProcess` が拾い直すが、
    # **落とした直後に次を起動する形を残さない**のが共有ヘルパーの趣旨である。
    [void](Stop-SnotraProcessAndWait -Process $proc)

    if ($null -eq $terminal) {
      # **沈黙を合格と読ませない**（#471 / #690 の型）。
      $failures += "run=$run 終端が出なかった（予算 ${TerminalTimeoutMs}ms）"
      Write-Host "終端なし" -ForegroundColor Red
      continue
    }

    $data = $terminal.data

    # **契約の検査は成功・失敗のどちらの終端でも走らせる。** 失敗した起動でも payload は契約を
    # 守るべきであり、**とくに `event` と `ok` の整合はここを通らないと `startup:ready` を騙る
    # 変異に届かない**——騙られた run は下の失敗分岐へ入らないためである。
    $contractFailures = Test-SnotraStartupPayload -Data $data -PhaseKey $PhaseKeys `
      -ObservedWallClockMs $observedMs -EventName $terminal.event
    foreach ($f in $contractFailures) { $failures += "run=$run $f" }

    if ($terminal.event -eq 'startup:failed') {
      # **`reason` はそのまま載せる**（ハーネス側で分類名を書き起こさない——写しが 2 部になる）。
      $failures += "run=$run 起動が失敗した: reason=$($data.reason) / reached_phase=$($data.reached_phase)"
      Write-Host "startup:failed reason=$($data.reason)" -ForegroundColor Red
      continue
    }

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
