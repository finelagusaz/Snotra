#Requires -Version 7
<#
.SYNOPSIS
  `scripts/measure-memory.ps1` が合算するプロセスを、プロセス一覧から選ぶ純関数。

.DESCRIPTION
  **合算するのは、ルート（Snotra 本体）と、その直接の子の設定サイドカーだけである**（#1461）。
  Snotra の子孫には、Snotra から起動したアプリも入る（`commands/launch.rs` の
  `launch_item_core` は `ShellExecuteW` を Snotra のプロセス内で呼ぶので、起動したアプリの親は
  通常 Snotra になる）。サイドカーが `open::that` で開いたブラウザ等も、サイドカーの子になる。
  子孫をすべて合算すると、それらの分まで「Snotra のメモリ」として数えてしまう。

  対象は、hide の trim（`src-tauri/src/working_set.rs`・#1444）と同じ 2 つに絞る。ただし、
  外からは製品が持つサイドカーの `Child` を区別できないので、「ルートの直接の子で、イメージ名が
  サイドカー名に一致する（大小無視の完全一致）もの」を選ぶ。サイドカー名を固定するのは、
  `commands/window.rs` の `launch_settings_process` が固定名 `snotra-settings.exe` で spawn する
  からである。

  ルートから辿れたが選ばなかった子孫は、`Excluded` として返す。呼び出し側はそれを件数と名前で
  出力し、合算に入らなかったものを読み手に見せる。

  - **PID と ppid は uint32 に揃えて照合する**（#532 SU2: CIM の `ParentProcessId` は uint32 で返り、
    int の PID 集合との比較が一致せず、辿りが黙って 0 件になった）
  - ppid の循環（PID の再利用で起きうる）でも、訪問済みの PID を辿り直さないので止まる
  - 既知の限界: ユーザーが `snotra-settings.exe` 自体を Snotra から起動すると、合算に入る。
    親の死後に PID が再利用されてできる偽の親子は扱わない

  I/O（`Get-CimInstance` と perf カウンタの読み）は呼び出し側が持つ。
#>

function Select-SnotraMemoryScope {
    <#
    .SYNOPSIS
      合算する PID（`Included`）と、辿れたが合算しない子孫（`Excluded`: Name / ProcessId）を返す。
    .PARAMETER Processes
      `ProcessId` / `ParentProcessId` / `Name` を持つオブジェクト列（`Win32_Process` をそのまま渡せる）。
    .PARAMETER RootIds
      ルートの PID。
    .PARAMETER SidecarName
      ルートの直接の子のうち、合算に入れるもののイメージ名。
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [object[]]$Processes,
        [Parameter(Mandatory)] [object[]]$RootIds,
        [string]$SidecarName = 'snotra-settings.exe'
    )

    # (ppid -> 子) の索引。**型は uint32 に揃える**（SU2）。
    $byPpid = @{}
    foreach ($p in $Processes) {
        $parent = [uint32]$p.ParentProcessId
        if (-not $byPpid.ContainsKey($parent)) {
            $byPpid[$parent] = [System.Collections.Generic.List[object]]::new()
        }
        $byPpid[$parent].Add($p)
    }

    $roots = @($RootIds | ForEach-Object { [uint32]$_ })
    $included = [System.Collections.Generic.List[uint32]]::new()
    foreach ($r in $roots) {
        if (-not $included.Contains($r)) { $included.Add($r) }
    }
    foreach ($r in $roots) {
        if (-not $byPpid.ContainsKey($r)) { continue }
        foreach ($c in $byPpid[$r]) {
            $cid = [uint32]$c.ProcessId
            if ($c.Name -eq $SidecarName -and -not $included.Contains($cid)) { $included.Add($cid) }
        }
    }

    # ルートから子孫を BFS で辿り、合算しないものを集める。
    $visited = @{}
    # 同じ PID が複数の親の下に現れても（ppid の循環）、除外へは 1 回だけ足す。
    $queued = @{}
    $excluded = [System.Collections.Generic.List[object]]::new()
    $queue = [System.Collections.Generic.Queue[uint32]]::new()
    foreach ($r in $roots) { $queue.Enqueue($r) }
    while ($queue.Count -gt 0) {
        $cur = $queue.Dequeue()
        if ($visited.ContainsKey($cur)) { continue }
        $visited[$cur] = $true
        if (-not $byPpid.ContainsKey($cur)) { continue }
        foreach ($c in $byPpid[$cur]) {
            $cid = [uint32]$c.ProcessId
            if ($visited.ContainsKey($cid) -or $queued.ContainsKey($cid)) { continue }
            $queued[$cid] = $true
            if (-not $included.Contains($cid)) {
                $excluded.Add([pscustomobject]@{ Name = [string]$c.Name; ProcessId = $cid })
            }
            $queue.Enqueue($cid)
        }
    }

    [pscustomobject]@{
        Included = @($included | Sort-Object)
        Excluded = @($excluded | Sort-Object ProcessId)
    }
}

Export-ModuleMember -Function @('Select-SnotraMemoryScope')
