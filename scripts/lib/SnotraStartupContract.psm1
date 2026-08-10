#Requires -Version 7

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# 起動計器（`startup:ready` / `startup:failed`）のペイロードが**契約を満たすか**を判定する
# 純関数（#1000 で `bench-startup.ps1` に置き、#1009 でここへ出した）。
#
# **判定規則を持たない。** 区間の意味・`null` の規則・恒等式はすべて `src-tauri/src/startup.rs`
# の `//!` が正本であり、ここはそれを外から検めるだけである。
#
# **module に置くのは検査自身を測るためである**（#1009）。Pester の探索は `scripts/lib` に
# 限られ（`run-pester.ps1`）、`.ps1` の中の関数はテストを書けない——`SnotraTraceInvariants.psm1`
# が同型の判定を module + Tests + blocking CI で持つのに対し、この検査群だけが
# `e2e.yml` の `continue-on-error: true` なステップからしか走らず、**壊れても誰も気づかない**
# 位置に居た。
#
# **検査を序数で指してはならない**（`.claude/rules/governance-docs.md`「書く約束」——検査 ID は
# 「引用される識別子」に当たる）。名前で指すこと。途中に足すと序数はずれ、ずれても誰も
# 気づかない。

<#
.SYNOPSIS
1 回ぶんのペイロードが契約を満たすか検査し、破れの一覧を返す（空なら合格）。

.DESCRIPTION
**下に並べた各項が検査の正本である**（数は書かない——足すたびに腐る）:

- **キーの過不足** — 全区間の `*_ns` / `*_ms` が在ること。`Set-StrictMode` 下で欠落キーは
  `$null` ではなく `PropertyNotFoundException` を投げる（実測）ので、存在判定は
  `PSObject.Properties` で行う。**`null` と「キーが無い」は別である。**
- **`null` の規則（双方向）** — 通らなかった区間は `null` になる。説明者は
  `first_run` / `include_path_env` / `reached_phase` より後ろ（**`ok` の真偽では免除しない**
  ——一律免除は失敗経路の取り落としを見えなくする）。

  **両向きを検査する。** 「説明されない `null`」だけを見る形は片手落ちで、**スキップした
  区間に `0` を書く誤りが素通りする**（変異 (f) を書いてみて気づいた——`null` と `0` を
  区別する設計の要なのに、検査が片方向だった）。枝フラグが「通らない」と言っている区間に
  値が在るのも同じ重さの破れである。
- **恒等式** — `post_main_ns == sum_phase_ns + unmarked_tail_ns`。**ms 表示値の和は検査
  しない**（丸めは表示境界でだけ行うので、正しくても境界で合わない）。

  **この検査は弱い。** `unmarked_tail_ns` は `post_main - sum_phase` として計算されるので、
  等式は**構成上ほぼ常に真**である——実際に捕まえるのは `sum_phase > post_main`（飽和が
  起きる側）だけで、変異「`post_main` を部分和から作る」（同語反復化）は素通りする（実測）。
- **外部の壁時計との突き合わせ** — 上の弱さを埋める。ハーネスは `Start-Process` から終端の
  trace が届くまでを**独立に**測っており、`pre_main + post_main` がそれを超えることはない。
  **超えたら、計器が内側で辻褄を合わせている**（同語反復化した実装は区間の実測を捨てるので、
  外から見た経過と食い違う）。下限は置かない——trace の到着はポーリング間隔ぶん遅れる。

  **この検査が見ないもの: 内側の申告が実際より小さくなる方向。** 下限が無いので、終端を
  ホットキー登録の完了より手前で打ち切る変異は**原理的に素通りする**（#1009 で (j) として
  実測）。下限を置けば捕まるが、**trace の到着遅れと区別できない**ため置いていない。
- **`event` と `ok` / `reason` の整合** — イベント名が意味を運ぶ設計
  （`ADR-startup-instrument-contract-shape`）ゆえ、**名前と中身が食い違ったら壊れている**。
  #1009 で実測: ホットキー登録が実際に失敗した起動で `event` だけを `startup:ready` に偽ると、
  `ok=false` / `reason=hotkey-registration` が正直に載ったまま**他の検査は全部通った**
  ——キーの存在しか見ておらず、値を一度も読んでいなかったためである。

  **この検査が見ないもの: `outcome` そのものの誤り。** `event` と `ok` は同じ `outcome` から
  導かれるので、`outcome` を取り違える変異は両方が揃って動き素通りする。捕まえるのは
  `to_json`（`ok`）と `finish`（`event`）という**別の場所の導出が食い違うこと**だけである。
- **`index_load_unattributed_ms` の非負性** — 外側の区間と内側の `LoadOrScanStats.total_ms` の
  差である。**非負性が乗る前提と、破れたときに負値がそのまま出力へ現れることは
  `startup.rs` の `to_json` が正本**。ここはその前提が破れたことを外から捕まえる。
#>
function Test-SnotraStartupPayload {
    [CmdletBinding()]
    [OutputType([string[]])]
    param(
        [Parameter(Mandatory)]$Data,
        [Parameter(Mandatory)][string[]]$PhaseKey,
        # ハーネスが独立に測った「起動〜終端の trace を読むまで」の壁時計（ms）。
        [Parameter(Mandatory)][double]$ObservedWallClockMs,
        # trace 行の**イベント名**。ペイロードの外側に在るので別で渡す。
        [Parameter(Mandatory)][string]$EventName
    )

    $failures = @()
    $has = { param($name) $null -ne $Data.PSObject.Properties[$name] }

    # --- キーの過不足 ---
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

    # --- 説明されない null ---
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

    # --- 恒等式（生 ns のみ。ms 表示値の和は検査しない） ---
    $lhs = [long]$Data.post_main_ns
    $rhs = [long]$Data.sum_phase_ns + [long]$Data.unmarked_tail_ns
    if ($lhs -ne $rhs) {
        $failures += "恒等式の破れ: post_main_ns=$lhs != sum_phase_ns + unmarked_tail_ns = $rhs"
    }

    # --- 外部の壁時計との突き合わせ ---
    # **内側の申告が外から見た経過を超えることはない。** 超えたら計器が内側で辻褄を合わせて
    # いる（同語反復化した実装は区間の実測を捨てるので、外から見た経過と食い違う）。
    # 下限は置かない——trace の到着はポーリング間隔ぶん遅れるため、内側 < 外側が正常である。
    $claimedMs = [double]$Data.post_main_ms + $(if ($null -eq $Data.pre_main_ms) { 0 } else { [double]$Data.pre_main_ms })
    if ($claimedMs -gt $ObservedWallClockMs) {
        $failures += ("内側の申告が外から見た経過を超えた: pre_main + post_main = ${claimedMs}ms > " +
            "観測 ${ObservedWallClockMs}ms（計器が内側で辻褄を合わせている疑い）")
    }

    # --- event と ok / reason の整合 ---
    # **名前が意味を運ぶなら、名前と中身は一致しなければならない。** 他の検査はキーの存在と
    # 数値しか見ないので、`event` を偽った起動を 1 つも捕まえない（#1009 で実測）。
    $expectedEvent = if ($Data.ok) { 'startup:ready' } else { 'startup:failed' }
    if ($EventName -ne $expectedEvent) {
        $failures += "event が ok と食い違う: event=$EventName / ok=$($Data.ok) / reason=$($Data.reason)（期待 $expectedEvent）"
    }
    # `reason` は失敗のときだけ在る。**成功時に理由が載るのも同じ重さの破れである**
    # （どちらかが嘘をついている）。
    if ($Data.ok -and $null -ne $Data.reason) {
        $failures += "ok=true なのに reason がある: reason=$($Data.reason)"
    } elseif (-not $Data.ok -and $null -eq $Data.reason) {
        $failures += "ok=false なのに reason が null（失敗の理由が読めない）"
    }

    # --- index_load_unattributed_ms の非負性 ---
    # `null` は正常（first-run 枝では `LoadOrScanStats` 自体が無い）。値が在るときだけ検める。
    if ($null -ne $Data.index_load_unattributed_ms -and [long]$Data.index_load_unattributed_ms -lt 0) {
        $failures += ("index_load_unattributed_ms が負: $($Data.index_load_unattributed_ms)" +
            "（外側の index_load が内側の LoadOrScanStats.total_ms を下回った——" +
            "正本は startup.rs の to_json）")
    }

    return $failures
}

Export-ModuleMember -Function Test-SnotraStartupPayload
