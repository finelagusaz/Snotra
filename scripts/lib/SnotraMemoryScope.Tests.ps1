BeforeAll {
    Import-Module (Join-Path $PSScriptRoot 'SnotraMemoryScope.psm1') -Force

    # `Win32_Process` の行の写し。CIM は `ProcessId` / `ParentProcessId` を uint32 で返す。
    function P {
        param($Id, $Parent, [string]$Name)
        [pscustomobject]@{ ProcessId = [uint32]$Id; ParentProcessId = [uint32]$Parent; Name = $Name }
    }

    function ExcludedPairs($Scope) {
        @($Scope.Excluded | ForEach-Object { '{0}:{1}' -f $_.Name, $_.ProcessId })
    }

    # 典型の形: ルート 100 の子にサイドカー 200 と起動アプリ 300。サイドカーが開いたブラウザ 210、
    # 起動アプリの子 310。無関係な 900 はルートの子孫でない。
    $script:typical = @(
        (P 4 0 'System')
        (P 100 4 'snotra.exe')
        (P 200 100 'snotra-settings.exe')
        (P 210 200 'msedge.exe')
        (P 300 100 'probe1461.exe')
        (P 310 300 'conhost.exe')
        (P 900 4 'explorer.exe')
    )
}

Describe 'Select-SnotraMemoryScope' {
    It 'a. 合算はルートと直接の子のサイドカーだけ。起動アプリとサイドカーの子は除外に出る' {
        $r = Select-SnotraMemoryScope -Processes $script:typical -RootIds @([uint32]100)

        @($r.Included) | Sort-Object | Should -Be @(100, 200)
        @($r.Sidecars) | Should -Be @(200)
        ExcludedPairs $r | Should -Be @('msedge.exe:210', 'probe1461.exe:300', 'conhost.exe:310')
    }

    It 'b. 起動アプリの子に居るサイドカー名の孫は、直接の子でないので除外に出る' {
        $procs = $script:typical + @((P 320 300 'snotra-settings.exe'))
        $r = Select-SnotraMemoryScope -Processes $procs -RootIds @([uint32]100)

        @($r.Included) | Sort-Object | Should -Be @(100, 200)
        ExcludedPairs $r | Should -Contain 'snotra-settings.exe:320'
    }

    It 'c. サイドカー名の大文字小文字は区別しない' {
        $procs = @((P 100 4 'snotra.exe'), (P 200 100 'SNOTRA-SETTINGS.EXE'))
        $r = Select-SnotraMemoryScope -Processes $procs -RootIds @([uint32]100)

        @($r.Included) | Sort-Object | Should -Be @(100, 200)
        @($r.Excluded).Count | Should -Be 0
    }

    It 'd. ppid が int でも uint32 のルートと照合できる（#532 SU2）' {
        $procs = @($script:typical | ForEach-Object {
            [pscustomobject]@{ ProcessId = [uint32]$_.ProcessId; ParentProcessId = [int]$_.ParentProcessId; Name = $_.Name }
        })
        $r = Select-SnotraMemoryScope -Processes $procs -RootIds @([int]100)

        @($r.Included) | Sort-Object | Should -Be @(100, 200)
        ExcludedPairs $r | Should -Be @('msedge.exe:210', 'probe1461.exe:300', 'conhost.exe:310')
    }

    It 'e. 子孫が無ければ、合算はルートだけで、除外は空' {
        $procs = @((P 100 4 'snotra.exe'), (P 900 4 'explorer.exe'))
        $r = Select-SnotraMemoryScope -Processes $procs -RootIds @([uint32]100)

        @($r.Included) | Should -Be @(100)
        @($r.Excluded).Count | Should -Be 0
    }

    It 'f. ルートが 2 件なら、それぞれのサイドカーも合算に入る' {
        $procs = @(
            (P 100 4 'snotra.exe'), (P 200 100 'snotra-settings.exe'),
            (P 500 4 'snotra.exe'), (P 600 500 'snotra-settings.exe'), (P 700 500 'probe1461.exe')
        )
        $r = Select-SnotraMemoryScope -Processes $procs -RootIds @([uint32]100, [uint32]500)

        @($r.Included) | Sort-Object | Should -Be @(100, 200, 500, 600)
        ExcludedPairs $r | Should -Be @('probe1461.exe:700')
    }

    It 'g. ルートの子孫でないプロセスは、合算にも除外にも出ない' {
        $r = Select-SnotraMemoryScope -Processes $script:typical -RootIds @([uint32]100)

        @($r.Included) | Should -Not -Contain 900
        @($r.Excluded.ProcessId) | Should -Not -Contain 900
    }

    It 'h. ルートがルートの子でも、除外には出ない' {
        $procs = @((P 100 4 'snotra.exe'), (P 500 100 'snotra.exe'))
        $r = Select-SnotraMemoryScope -Processes $procs -RootIds @([uint32]100, [uint32]500)

        @($r.Included) | Sort-Object | Should -Be @(100, 500)
        @($r.Excluded).Count | Should -Be 0
    }

    It 'i. ppid が循環しても止まり、各 PID は高々 1 回だけ出る' {
        # PID の再利用で 300 → 310 → 300 の循環ができた形。
        $procs = @((P 100 4 'snotra.exe'), (P 300 100 'a.exe'), (P 310 300 'b.exe'), (P 300 310 'a.exe'))
        $r = Select-SnotraMemoryScope -Processes $procs -RootIds @([uint32]100)

        @($r.Included) | Should -Be @(100)
        ExcludedPairs $r | Should -Be @('a.exe:300', 'b.exe:310')
    }

    It 'j. サイドカー名に似た名前は、直接の子でも除外に出る（完全一致）' {
        # 前方・後方・中間の一致と、`.` を任意文字と読む正規表現の一致を、それぞれ 1 件ずつ置く。
        $procs = @(
            (P 100 4 'snotra.exe'),
            (P 200 100 'snotra-settings-helper.exe'),
            (P 201 100 'snotra-settings.exe.old'),
            (P 202 100 'old-snotra-settings.exe'),
            (P 203 100 'snotra-settingsXexe')
        )
        $r = Select-SnotraMemoryScope -Processes $procs -RootIds @([uint32]100)

        @($r.Included) | Should -Be @(100)
        @($r.Sidecars).Count | Should -Be 0
        ExcludedPairs $r | Should -Be @(
            'snotra-settings-helper.exe:200', 'snotra-settings.exe.old:201',
            'old-snotra-settings.exe:202', 'snotra-settingsXexe:203'
        )
    }
}
