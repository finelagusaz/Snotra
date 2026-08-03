BeforeAll {
    $modulePath = Join-Path $PSScriptRoot 'SnotraSmoke.psm1'
    Import-Module $modulePath -Force
}

BeforeDiscovery {
    Import-Module (Join-Path $PSScriptRoot 'SnotraSmoke.psm1') -Force
    # 画面がロックされていると Integration は**環境の都合で**落ちる（前面を奪えず打鍵が届かず、
    # キャプチャはロック画面を返す・#866）。それを「赤」で残すと、実装の欠陥と区別できない。
    # **確定してロック中のときだけ** skip し、判定不能なら従来どおり実行する（fail-open——
    # 判定できないことを理由に検査を止めない）。
    $sessionLocked = $false
    try {
        $lockState = Get-SnotraSessionLockState
        $sessionLocked = $lockState.Locked
        Write-Host "[#866] セッションのロック状態: Locked=$($lockState.Locked) SessionFlags=$($lockState.SessionFlags) SessionId=$($lockState.SessionId)"
    } catch {
        Write-Host "[#866] セッションのロック状態: 判定不能（$($_.Exception.Message)）— Integration は実行する"
    }
}

Describe 'New-SnotraVerificationProfile' {
    It '必須セクションと呼び出し側固有の節を持つ seed を作り、古い成果物を除く' {
        $profile = Join-Path $TestDrive 'profile'
        New-Item -ItemType Directory -Force -Path $profile | Out-Null
        Set-Content -Path (Join-Path $profile 'config.toml.bak') -Value 'stale'
        Set-Content -Path (Join-Path $profile 'index.bin') -Value 'stale'

        $created = New-SnotraVerificationProfile -ProfileDir $profile `
            -AdditionalSections "[general]`nshow_on_startup = true" `
            -PathEntries @'
[[paths.scan]]
path = "C:/fixture"
extensions = [".exe"]
include_folders = false
'@

        $toml = Get-Content -Raw $created.ConfigPath
        $toml | Should -Match '(?m)^\[hotkey\]\r?$'
        $toml | Should -Match '(?m)^\[appearance\]\r?$'
        $toml | Should -Match '(?m)^\[general\]\r?$'
        $toml | Should -Match '(?m)^\[paths\]\r?$'
        $toml | Should -Match '(?m)^\[\[paths\.scan\]\]\r?$'
        Test-Path (Join-Path $profile 'config.toml.bak') | Should -BeFalse
        @(Get-ChildItem -Path $profile -Filter '*.bin').Count | Should -Be 0
        [IO.Path]::IsPathRooted($created.FullPath) | Should -BeTrue
    }

    It 'ShowIcons を TOML の真偽値として書く（既定は true・$false で無効化できる）' {
        # このノブが黙って効かなくなると、検査は緑のまま runner への要求だけが戻る（#872）。
        # PowerShell の $true は "True" へ補間されるため、TOML として読めるかを直接見る。
        $on = New-SnotraVerificationProfile -ProfileDir (Join-Path $TestDrive 'icons-on')
        $off = New-SnotraVerificationProfile -ProfileDir (Join-Path $TestDrive 'icons-off') -ShowIcons $false

        (Get-Content -Raw $on.ConfigPath) | Should -Match '(?m)^show_icons = true\r?$'
        (Get-Content -Raw $off.ConfigPath) | Should -Match '(?m)^show_icons = false\r?$'
    }
}

Describe 'Invoke-SnotraEnvironment' {
    It '正常終了後に既存値と未設定状態を復元する' {
        $env:SNOTRA_PESTER_EXISTING = 'before'
        Remove-Item Env:SNOTRA_PESTER_MISSING -ErrorAction SilentlyContinue

        $seen = Invoke-SnotraEnvironment -Variables @{
            SNOTRA_PESTER_EXISTING = 'during'
            SNOTRA_PESTER_MISSING = 'temporary'
        } -ScriptBlock {
            "$env:SNOTRA_PESTER_EXISTING/$env:SNOTRA_PESTER_MISSING"
        }

        $seen | Should -Be 'during/temporary'
        $env:SNOTRA_PESTER_EXISTING | Should -Be 'before'
        Test-Path Env:SNOTRA_PESTER_MISSING | Should -BeFalse
    }

    It '処理が例外でも環境変数を復元する' {
        $env:SNOTRA_PESTER_EXISTING = 'before'

        { Invoke-SnotraEnvironment -Variables @{ SNOTRA_PESTER_EXISTING = 'during' } -ScriptBlock {
            throw 'injected failure'
        } } | Should -Throw '*injected failure*'

        $env:SNOTRA_PESTER_EXISTING | Should -Be 'before'
    }
}

Describe 'Start-SnotraProcess' {
    It '起動自体が失敗しても config/trace 環境変数を元へ戻す' {
        $savedConfigExists = Test-Path Env:SNOTRA_CONFIG_DIR
        $savedTraceExists = Test-Path Env:SNOTRA_TRACE
        $savedConfig = $env:SNOTRA_CONFIG_DIR
        $savedTrace = $env:SNOTRA_TRACE
        try {
            $env:SNOTRA_CONFIG_DIR = 'caller-profile'
            Remove-Item Env:SNOTRA_TRACE -ErrorAction SilentlyContinue
            Mock -ModuleName SnotraSmoke Start-Process { throw 'injected launch failure' }

            { Start-SnotraProcess -ConfigDir 'temporary-profile' -Trace -FilePath 'missing.exe' } |
                Should -Throw '*injected launch failure*'

            $env:SNOTRA_CONFIG_DIR | Should -Be 'caller-profile'
            Test-Path Env:SNOTRA_TRACE | Should -BeFalse
        } finally {
            if ($savedConfigExists) { $env:SNOTRA_CONFIG_DIR = $savedConfig } else { Remove-Item Env:SNOTRA_CONFIG_DIR -ErrorAction SilentlyContinue }
            if ($savedTraceExists) { $env:SNOTRA_TRACE = $savedTrace } else { Remove-Item Env:SNOTRA_TRACE -ErrorAction SilentlyContinue }
        }
    }
}

Describe 'Resolve-SnotraCargoExecutable' {
    It 'CARGO_TARGET_DIR を設定したとき metadata の target_directory から debug 本体を導く' {
        $repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
        $customTarget = Join-Path $TestDrive 'custom-cargo-target'

        $resolved = Invoke-SnotraEnvironment -Variables @{ CARGO_TARGET_DIR = $customTarget } -ScriptBlock {
            Resolve-SnotraCargoExecutable -RepositoryRoot $repositoryRoot
        }

        $resolved | Should -Be (Join-Path $customTarget 'debug/snotra.exe')
    }
}

Describe 'Read-SnotraTraceEvents' {
    It 'trace 以外と壊れた JSON を除き、有効なイベントを順番どおり返す' {
        $tracePath = Join-Path $TestDrive 'trace.log'
        @(
            'noise'
            '[trace] {broken'
            '[trace] {"seq":1,"event":"one","data":{"ok":true}}'
            '[trace] {"seq":2,"event":"two","data":{"value":42}}'
        ) | Set-Content $tracePath

        $events = @(Read-SnotraTraceEvents -Path $tracePath)
        $events.Count | Should -Be 2
        $events[0].event | Should -Be 'one'
        $events[1].data.value | Should -Be 42
    }
}

Describe 'Resolve-SnotraExistingProcess' {
    It 'Reject 方針では既存プロセスを終了せず例外にする' {
        Mock -ModuleName SnotraSmoke Get-Process { @([pscustomobject]@{ Id = 123 }) }
        Mock -ModuleName SnotraSmoke Stop-Process {}

        { Resolve-SnotraExistingProcess -Policy Reject } | Should -Throw '*pid=123*'
        Should -Invoke -ModuleName SnotraSmoke Stop-Process -Times 0
    }

    It 'Stop 方針では列挙した既存プロセスだけを停止する' {
        Mock -ModuleName SnotraSmoke Get-Process { @([pscustomobject]@{ Id = 123 }) }
        Mock -ModuleName SnotraSmoke Stop-Process {}

        Resolve-SnotraExistingProcess -Policy Stop

        Should -Invoke -ModuleName SnotraSmoke Stop-Process -Times 1 -ParameterFilter { $Id -eq 123 -and $Force }
    }
}

Describe 'ConvertFrom-SnotraWtsInfoEx（#866 ロック検出の解釈）' {
    # **実際のセッション状態に依存させない。** このファイルは CI（GitHub Actions の Windows
    # runner）でも走るため、「いまロックされているか」を assert すると実行環境で結果が変わる。
    # 純関数へ合成バイト列を渡し、解釈だけを固定する。
    #
    # ヘルパは BeforeAll に置く——Describe 直下の関数定義は discovery スコープに属し、
    # It の実行時には見えない（Pester 5 以降のスコープ分離・実測で CommandNotFound）。
    BeforeAll {
        function New-WtsInfoExBuffer {
            param([int]$Level = 1, [int]$SessionId = 7, [int]$SessionState = 0, [int]$SessionFlags = 1, [int]$Size = 40)
            $b = New-Object byte[] $Size
            [BitConverter]::GetBytes($Level).CopyTo($b, 0)
            [BitConverter]::GetBytes($SessionId).CopyTo($b, 8)
            [BitConverter]::GetBytes($SessionState).CopyTo($b, 12)
            [BitConverter]::GetBytes($SessionFlags).CopyTo($b, 16)
            $b
        }
    }

    It 'SessionFlags=0（WTS_SESSIONSTATE_LOCK）をロックと読む' {
        $r = ConvertFrom-SnotraWtsInfoEx -Buffer (New-WtsInfoExBuffer -SessionFlags 0) -ExpectedSessionId 7
        $r.Locked | Should -BeTrue
        $r.SessionId | Should -Be 7
    }

    It 'SessionFlags=1（WTS_SESSIONSTATE_UNLOCK）を非ロックと読む' {
        (ConvertFrom-SnotraWtsInfoEx -Buffer (New-WtsInfoExBuffer -SessionFlags 1) -ExpectedSessionId 7).Locked | Should -BeFalse
    }

    It 'SessionId が呼び出し元と食い違えば「読めた」と言わない（オフセット仮定の検算）' {
        # オフセットが 1 つずれると、無関係な 0 が「ロック中」に化ける。SessionId の一致が
        # その沈黙経路に対する唯一の検知点である
        { ConvertFrom-SnotraWtsInfoEx -Buffer (New-WtsInfoExBuffer -SessionId 3) -ExpectedSessionId 7 } |
            Should -Throw '*SessionId が呼び出し元と一致しません*'
    }

    It 'Level が 1 でなければ SessionFlags の位置を仮定しない' {
        { ConvertFrom-SnotraWtsInfoEx -Buffer (New-WtsInfoExBuffer -Level 2) -ExpectedSessionId 7 } |
            Should -Throw '*Level が 1 ではありません*'
    }

    It 'LOCK/UNLOCK 以外の SessionFlags を非ロックへ倒さない（未知値は判定不能）' {
        { ConvertFrom-SnotraWtsInfoEx -Buffer (New-WtsInfoExBuffer -SessionFlags -1) -ExpectedSessionId 7 } |
            Should -Throw '*ロック状態を判定できません*'
    }

    It 'バッファが短ければ範囲外を読まない' {
        { ConvertFrom-SnotraWtsInfoEx -Buffer (New-Object byte[] 12) -ExpectedSessionId 7 } |
            Should -Throw '*短すぎます*'
    }
}

Describe 'Assert-SnotraSessionUnlocked（#866 倒す向きの非対称）' {
    It 'ロックと判定できたら止める' {
        Mock -ModuleName SnotraSmoke Get-SnotraSessionLockState { [pscustomobject]@{ Locked = $true } }
        { Assert-SnotraSessionUnlocked -Operation '窓のキャプチャ' } | Should -Throw '*画面がロックされている*'
    }

    It '非ロックなら素通りする' {
        Mock -ModuleName SnotraSmoke Get-SnotraSessionLockState { [pscustomobject]@{ Locked = $false } }
        { Assert-SnotraSessionUnlocked } | Should -Not -Throw
    }

    It '判定不能は警告のみで続行する（未知ホスト・CI runner で道具を失わないため）' {
        Mock -ModuleName SnotraSmoke Get-SnotraSessionLockState { throw 'WTSQuerySessionInformationW に失敗しました（Win32 error 87）。' }
        { Assert-SnotraSessionUnlocked -WarningAction SilentlyContinue } | Should -Not -Throw
    }
}

Describe 'Set-SnotraForegroundWindow（#1280 前面化の判定）' {
    # 前面を奪えない状況は runner でしか起きないため、**ここでしか通らない経路**を実ハンドルで踏む。
    # 実機配管側は成功パスしか通らず、この分岐を検査しない。
    It '前面になれない窓は、API の戻り値ではなく実際の前面状態で false になる' {
        $warnings = @()
        Set-SnotraForegroundWindow -Handle ([IntPtr]::new(-1)) -TimeoutMs 200 -PollMs 50 `
            -WarningVariable warnings -WarningAction SilentlyContinue | Should -BeFalse
        ($warnings -join "`n") | Should -BeLike '*前面*'
    }

    It '既に前面である窓は true になる（SetForegroundWindow が FALSE を返しても）' {
        $current = Get-SnotraForegroundWindow
        if ($current -eq [IntPtr]::Zero) {
            Set-ItResult -Skipped -Because '前面窓が無い環境では一致を確かめられない'
        }
        Set-SnotraForegroundWindow -Handle $current -TimeoutMs 1000 -PollMs 50 | Should -BeTrue
    }

    It 'ハンドルが Zero なら「前面窓なし」と一致させずに落とす' {
        { Set-SnotraForegroundWindow -Handle ([IntPtr]::Zero) } | Should -Throw '*Zero*'
    }
}

Describe '実機配管' -Tag Integration -Skip:$sessionLocked {
    It '生成した seed を本体が parse して同じプロファイルへ書き込み、キャプチャ寸法が窓矩形と一致する' {
        $profile = Join-Path $TestDrive 'integration-profile'
        $stderr = Join-Path $TestDrive 'integration.err'
        $created = New-SnotraVerificationProfile -ProfileDir $profile -AdditionalSections @'
[general]
show_on_startup = true
auto_hide_on_focus_lost = false
'@
        $proc = $null
        $capture = $null
        try {
            Resolve-SnotraExistingProcess -Policy Reject
            $proc = Start-SnotraProcess -ConfigDir $created.FullPath `
                -FilePath $env:SNOTRA_PESTER_EXE -StandardErrorPath $stderr
            $hwnd = Wait-SnotraWindow -Title 'Snotra' -Process $proc -TimeoutMs 30000
            Start-Sleep -Milliseconds 500
            $capture = Get-SnotraWindowCapture -Handle $hwnd

            $rectWidth = $capture.Rect.Right - $capture.Rect.Left
            $rectHeight = $capture.Rect.Bottom - $capture.Rect.Top
            $capture.Bitmap.Width | Should -Be $rectWidth
            $capture.Bitmap.Height | Should -Be $rectHeight
            @(Select-String -Path $stderr -SimpleMatch '[config] ').Count | Should -Be 0

            # `[config]` 不在だけでは、実ユーザー側の別の有効 config を読んでも合格する。
            # profile 作成時に古い *.bin は消しているため、ここでの生成は意図した
            # SNOTRA_CONFIG_DIR を本体が実際に使った肯定的証拠になる。
            $indexPath = Join-Path $created.FullPath 'index.bin'
            $indexDeadline = [DateTime]::UtcNow.AddSeconds(10)
            while (-not (Test-Path -LiteralPath $indexPath) -and [DateTime]::UtcNow -lt $indexDeadline) {
                if ($proc.HasExited) { break }
                Start-Sleep -Milliseconds 100
            }
            Test-Path -LiteralPath $indexPath | Should -BeTrue
        } finally {
            if ($null -ne $capture) { $capture.Bitmap.Dispose() }
            if ($null -ne $proc -and -not $proc.HasExited) {
                Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
            }
        }
    }

    It 'フォルダ復帰後の次打鍵を復元クエリの末尾へ追加する' {
        $profile = Join-Path $TestDrive 'caret-profile'
        $stderr = Join-Path $TestDrive 'caret.err'
        $scanRoot = Join-Path $TestDrive 'caret-fixture'
        $folder = Join-Path $scanRoot 'alpha'
        New-Item -ItemType Directory -Force -Path $folder | Out-Null
        Set-Content -LiteralPath (Join-Path $folder 'aa-child.txt') -Value 'fixture'
        $scanRootToml = (Resolve-Path -LiteralPath $scanRoot).Path.Replace('\', '/')
        # この検査はキャレット位置だけを見るので、アイコンは判定に一切入らない。
        # runner ではシェルのアイコン問い合わせが恒久的に失敗し続け（#872）、その再要求が
        # 打鍵から egui_input:changed までの観測時間を押し広げる。要求そのものを外す。
        $created = New-SnotraVerificationProfile -ProfileDir $profile -ShowIcons $false `
            -AdditionalSections @'
[general]
show_on_startup = true
auto_hide_on_focus_lost = false
'@ `
            -PathEntries @"
[[paths.scan]]
path = "$scanRootToml"
extensions = [".snotra-no-match"]
include_folders = true
"@
        $proc = $null
        $pressKey = {
            param([byte]$VirtualKey)
            Send-SnotraKey -VirtualKey $VirtualKey
            Start-Sleep -Milliseconds 50
            Send-SnotraKey -VirtualKey $VirtualKey -Up
            Start-Sleep -Milliseconds 50
        }
        $waitForInputChange = {
            param(
                [string]$Scope,
                [int]$AfterChars,
                [bool]$AppendedAtEnd,
                [Nullable[int]]$BeforeChars = $null,
                [int]$TimeoutMs = 5000
            )
            $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
            while ([DateTime]::UtcNow -lt $deadline) {
                $matched = @(Read-SnotraTraceEvents -Path $stderr | Where-Object {
                    $_.event -eq 'egui_input:changed' -and
                    $_.data.scope -eq $Scope -and
                    $_.data.after_chars -eq $AfterChars -and
                    $_.data.appended_at_end -eq $AppendedAtEnd -and
                    ($null -eq $BeforeChars -or $_.data.before_chars -eq $BeforeChars)
                } | Select-Object -Last 1)
                if ($matched.Count -gt 0) { return $matched[0] }
                if ($null -ne $proc -and $proc.HasExited) { break }
                Start-Sleep -Milliseconds 100
            }
            return $null
        }

        try {
            Resolve-SnotraExistingProcess -Policy Reject
            $proc = Start-SnotraProcess -ConfigDir $created.FullPath -Trace `
                -FilePath $env:SNOTRA_PESTER_EXE -StandardErrorPath $stderr
            $hwnd = Wait-SnotraWindow -Title 'Snotra' -Process $proc -TimeoutMs 30000

            $indexPath = Join-Path $created.FullPath 'index.bin'
            $indexDeadline = [DateTime]::UtcNow.AddSeconds(10)
            while (-not (Test-Path -LiteralPath $indexPath) -and [DateTime]::UtcNow -lt $indexDeadline) {
                if ($proc.HasExited) { break }
                Start-Sleep -Milliseconds 100
            }
            Test-Path -LiteralPath $indexPath | Should -BeTrue
            @(Select-String -Path $stderr -SimpleMatch '[config] ').Count | Should -Be 0

            Set-SnotraForegroundWindow -Handle $hwnd | Should -BeTrue
            Start-Sleep -Milliseconds 300
            foreach ($vk in [byte[]](0x41, 0x4C, 0x50, 0x48, 0x41)) { & $pressKey $vk }
            # runner が遅いと複数文字が同一フレームへまとまるため、準備入力は最終状態だけを見る。
            & $waitForInputChange -Scope 'search' -AfterChars 5 -AppendedAtEnd $true |
                Should -Not -BeNullOrEmpty

            & $pressKey 0x27 # Right: 唯一の検索結果である alpha フォルダへ入る。
            Start-Sleep -Milliseconds 300
            & $pressKey 0x41
            & $pressKey 0x41
            & $waitForInputChange -Scope 'folder' -AfterChars 2 -AppendedAtEnd $true |
                Should -Not -BeNullOrEmpty

            & $pressKey 0x1B # Escape: alpha を復元し、次の z が末尾へ入るべき経路。
            & $pressKey 0x5A
            & $waitForInputChange -Scope 'search' -BeforeChars 5 -AfterChars 6 `
                -AppendedAtEnd $true | Should -Not -BeNullOrEmpty
        } catch {
            Write-Host '--- caret integration stderr trace ---'
            if (Test-Path -LiteralPath $stderr) {
                $stderrLines = @(Get-Content -LiteralPath $stderr)
                if ($stderrLines.Count -eq 0) {
                    Write-Host '(stderr is empty)'
                } else {
                    $stderrLines | ForEach-Object { Write-Host $_ }
                }
            } else {
                Write-Host "(stderr file not found: $stderr)"
            }
            Write-Host '--- end caret integration stderr trace ---'
            throw
        } finally {
            if ($null -ne $proc -and -not $proc.HasExited) {
                Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
            }
        }
    }
}
