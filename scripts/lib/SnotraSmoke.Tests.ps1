BeforeAll {
    $modulePath = Join-Path $PSScriptRoot 'SnotraSmoke.psm1'
    Import-Module $modulePath -Force

    # **偽プロセス（#872）。`Stop-SnotraProcessAndWait` を通る経路すべてが使う。**
    # ヘルパは実プロセスを要求せずメンバだけを見る形にしてあるので、これで全経路を測れる
    # （引数へ `[System.Diagnostics.Process]` を付けると `Id` が ReadOnly ゆえ束縛で落ちる・実測）。
    # `Set-StrictMode -Version Latest` の下では、メンバを持たない偽物はメンバアクセスで落ちる。
    # **ファイル先頭に置くのは、`Resolve-SnotraExistingProcess` の Describe からも使うためである。**
    function New-FakeProcess {
        param(
            [bool]$HasExited = $false,
            [bool]$WaitResult = $true,
            [switch]$WaitThrows,
            [int]$Id = 123
        )
        $fake = [pscustomobject]@{ Id = $Id }
        $fake | Add-Member -MemberType NoteProperty -Name HasExited -Value $HasExited
        $waitResult = $WaitResult
        $throws = [bool]$WaitThrows
        $method = {
            param($ms)
            if ($throws) { throw 'Access is denied' }
            return $waitResult
        }.GetNewClosure()
        $fake | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value $method
        return $fake
    }
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
    It '共通セクションと呼び出し側固有の節を持つ seed を作り、古い成果物を除く' {
        $profile = Join-Path $TestDrive 'profile'
        New-Item -ItemType Directory -Force -Path $profile | Out-Null
        Set-Content -Path (Join-Path $profile 'config.toml.bak') -Value 'stale'
        Set-Content -Path (Join-Path $profile 'index.bin') -Value 'stale'

        $created = New-SnotraVerificationProfile -ProfileDir $profile `
            -GeneralSection 'show_on_startup = true' `
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

    It 'auto_update を既定で disabled にする（省略すると実ネットワークの更新チェックが走る・#755/#801 是正）' {
        # GeneralConfig.auto_update の既定は Full（snotra-core/src/config.rs の #[default]）。
        # 検証用プロファイルがこれを踏まないことをここで固定する。
        $created = New-SnotraVerificationProfile -ProfileDir (Join-Path $TestDrive 'auto-update-default')

        (Get-Content -Raw $created.ConfigPath) | Should -Match '(?m)^auto_update = "disabled"\r?$'
    }

    It 'AdditionalSections に [general] を書くと重複テーブルとして throw する' {
        { New-SnotraVerificationProfile -ProfileDir (Join-Path $TestDrive 'general-conflict') `
            -AdditionalSections "[general]`nshow_on_startup = true" } | Should -Throw '*general*'
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

    It 'コンソール窓を見せずに起動する（debug ビルドが前面を奪うのを防ぐ）' {
        # 回帰テスト: #872 現れ方 1（機序は `Start-SnotraProcess` のコメントが正本）。
        # **通った実行では発火しない**ため、この不変条件はここでしか観測されない。
        Mock -ModuleName SnotraSmoke Start-Process { }

        Start-SnotraProcess -ConfigDir 'p' -FilePath 'x.exe' -StandardErrorPath 'e.txt'

        Should -Invoke -ModuleName SnotraSmoke Start-Process -Times 1 -ParameterFilter {
            $WindowStyle -eq 'Hidden'
        }
    }

    It 'NoNewWindow のときは WindowStyle を渡さない（Start-Process の排他な引数）' {
        # 回帰テスト: #872（排他の理由は `Start-SnotraProcess` のコメントが正本）。
        Mock -ModuleName SnotraSmoke Start-Process { }

        Start-SnotraProcess -ConfigDir 'p' -FilePath 'x.exe' -NoNewWindow

        Should -Invoke -ModuleName SnotraSmoke Start-Process -Times 1 -ParameterFilter {
            $NoNewWindow -eq $true -and $null -eq $WindowStyle
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

    It '相対値の CARGO_TARGET_DIR でも RepositoryRoot を起点に解決する（cwd に依存しない・#1179）' {
        # 上の It は**絶対値**を渡すので cwd の影響を受けない。ここが守るのは相対値の枝である
        # ——cargo は相対の CARGO_TARGET_DIR を manifest ではなく**自プロセスの cwd** から解決するため、
        # cwd を固定しないと「worktree を指したつもりでメイン作業コピーの target」を返す（#1179 実測）。
        $repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
        $elsewhere = Join-Path $TestDrive 'elsewhere'
        New-Item -ItemType Directory -Force -Path $elsewhere | Out-Null

        Push-Location -LiteralPath $elsewhere
        try {
            $resolved = Invoke-SnotraEnvironment -Variables @{ CARGO_TARGET_DIR = 'relative-target' } -ScriptBlock {
                Resolve-SnotraCargoExecutable -RepositoryRoot $repositoryRoot
            }
        } finally {
            Pop-Location
        }

        # 期待値は**実装と同じ Join-Path の重ね方**で組む（文字列連結で組むと区切りが静かにずれる）。
        $resolved | Should -Be (Join-Path (Join-Path $repositoryRoot 'relative-target') 'debug/snotra.exe')
    }
}

Describe '起動ハーネスの既定 ExePath はスクリプトの住むコピーから導かれる' {
    # **この性質を守る検査が他に無い**（#1179 で実測）。`$repoRoot` の導出を cwd 起点へ戻すと、
    # Pester も vitest も smoke 自身も**緑のまま** #1179 の欠陥（別の作業コピーを黙って測る）が
    # 復活する。ハーネス自身を起動して確かめるには本体のビルドと実起動が要るので、ここは
    # **ソースの形で縛る**。
    #
    # **守るのは「既定の枝が導出を通ること」であって、`$ExePath` の最終値ではない。** 縛る 3 点は
    # (1) `param()` の既定が空——直書きすると `if (-not $ExePath)` が偽になり**導出が一度も走らない**、
    # (2) `$repoRoot` をスクリプト自身の位置から 1 か所で導くこと、(3) それを導出先へ**実際に渡すこと**。
    # 3 つとも独立に必要である（実測: (2) だけのとき、導出行を無傷のまま使用点の引数を cwd 起点へ
    # 差し替える形が素通りした。(2)(3) だけのとき、既定を直書きへ戻す形が素通りした——**しかもこちらは
    # cwd がスクリプトと同じコピーでも症状が出る**ので D/G より悪い）。
    #
    # **射程の外（受容する残余）**: `$ExePath` が導出の**後で**上書きされる形には届かない。ここは
    # 「どう書かれているか」を見る述語であって、実行時の最終値を追わない。追うには実行時の観測が要り、
    # それには本体のビルドと実起動が要る——このハーネス自身を起動する検査は別の費用の話になる。
    # 同じ理由で、導出式の中身（`Resolve-Path`/`Join-Path` の組み方）とプロファイルの置き場も射程外。
    #
    # **もうひとつの死角**: 対象を数え上げているので 3 本目の起動ハーネスには黙る。
    # 「`Resolve-SnotraCargoExecutable` を呼ぶ `.ps1` すべて」へ広げる手はあるが、
    # `run-pester.ps1` / `visual-input-metrics.ps1` / `visual-check-colors.ps1` は変数名も
    # 導出の形も違うため誤検出になる。**広げずに死角として宣言して止める。**
    #
    # **`-ForEach` で渡す**（素の `foreach` を使わない）。Pester は discovery と run が別相で、
    # 素のループ変数はテスト**名**には展開されるのに `It` の本体では未設定になる——**壊れているのに
    # 正しくパラメータ化されて見える**（実測: `The variable '$harnessName' cannot be retrieved`）。
    It '<_> の既定 ExePath は param から導出へ通り、repoRoot はスクリプト自身の位置から来る（#1179）' -ForEach @(
        'bench-startup.ps1', 'smoke-startup.ps1'
    ) {
        # コメント行は除く——除かないと「関数名に触れたコメントを 1 行足す」だけで赤くなり、
        # しかも文言が事実に反する（実測）。
        $harnessLines = @(Get-Content -LiteralPath (Join-Path $PSScriptRoot "../$_") | Where-Object { $_ -notmatch '^\s*#' })

        # (1) 入口——`param()` の既定が空でなければ、導出は一度も走らない
        $paramDefaults = @($harnessLines | Where-Object { $_ -match '^\s*\[string\]\s*\$ExePath\s*=' })
        $paramDefaults.Count | Should -Be 1 -Because "param() の ExePath 宣言を 1 行で書くこと"
        ($paramDefaults[0] -split '=', 2)[1].Trim().TrimEnd(',').Trim().Trim("'", '"') |
            Should -BeNullOrEmpty -Because "既定を空にして導出へ委ねること（パスを直書きすると導出が走らず #1179 が復活する）"

        # (2) 導出
        $assignments = @($harnessLines | Where-Object { $_ -match '^\s*\$repoRoot\s*=' })
        $assignments.Count | Should -Be 1 -Because "repoRoot の導出が 2 か所に散ると片方だけ退行する"
        $assignments[0] | Should -Match '\$PSScriptRoot' -Because "導出を 1 行で `$PSScriptRoot から書くこと（cwd 起点にすると #1179 が緑のまま復活する）"

        # (3) 使用点——導出しても渡さなければ意味が無い。**件数を縛らず、全件へ課す**
        # （「ちょうど 1 件」は誤検出を生むだけで、2 件になったとき何が壊れるかを言えない）。
        $calls = @($harnessLines | Where-Object { $_ -match 'Resolve-SnotraCargoExecutable' })
        $calls.Count | Should -BeGreaterThan 0 -Because "本体の導出は Resolve-SnotraCargoExecutable へ委ねること"
        foreach ($call in $calls) {
            # `-RepositoryRoot:$repoRoot` のコロン記法も正当なので両方受ける。
            $call | Should -Match '-RepositoryRoot[:\s]\s*\$repoRoot' -Because "導出した repoRoot をそのまま渡すこと（別の値を渡すと #1179 が緑のまま復活する）"
        }
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

Describe 'Wait-SnotraTraceCondition（#872 観測側の 2 つの穴）' {
    BeforeAll {
        function New-SnotraTestTraceFile {
            param([string]$Path, [string[]]$Json)
            $Json | ForEach-Object { "[trace] $_" } | Set-Content -LiteralPath $Path
            $Path
        }
    }

    It '予算が尽きていても条件を必ず 1 度は評価する（停止が期限を跨いでも取りこぼさない）' {
        # 旧実装は `while (now -lt deadline)` ゆえ、期限切れ後は**一度も読まずに** $null を返した。
        # 停止が最後の sleep を跨ぐと、既に成立していた条件を見ないまま諦める（#872 run 1306:
        # 期待した事象は 39.354 に出ていて期限は 42.57 以降だったのに観測されなかった）。
        $path = New-SnotraTestTraceFile -Path (Join-Path $TestDrive 'ready.log') `
            -Json @('{"seq":1,"event":"target","data":{"ok":true}}')

        # **警告を抑える**（#872）。この It は `-TimeoutMs 0` ゆえ必ず「予算を過ぎた評価で
        # 成立しました」を出す。それは `SnotraSmoke.psm1` が**本物の遅着**に出すのと同一文言で、
        # 全実行に 1 件混ざると grep で本物と区別できなくなる（実測: 30 反復すべてに出ていた）。
        # 抑止の形は同じファイルの `:206` / `:224` と揃える。
        $found = Wait-SnotraTraceCondition -Path $path -TimeoutMs 0 -Predicate { $_.event -eq 'target' } `
            -WarningAction SilentlyContinue

        $found | Should -Not -BeNullOrEmpty
        $found.seq | Should -Be 1
    }

    It '読み取りに失敗した周回を「まだ出ていない」と同じ沈黙へ潰さない' {
        # ディレクトリは Test-Path が真で Get-Content が落ちる（実測）。`-ErrorAction SilentlyContinue` は
        # 空を返すので、**読めなかったと未発生が同じ $null に化ける**。
        $dir = Join-Path $TestDrive 'unreadable'
        New-Item -ItemType Directory -Force -Path $dir | Out-Null

        $found = Wait-SnotraTraceCondition -Path $dir -TimeoutMs 0 -Predicate { $true } `
            -WarningVariable warnings -WarningAction SilentlyContinue

        $found | Should -BeNullOrEmpty
        ($warnings -join ' ') | Should -Match '読み取り'
    }

    It '本体が終了していれば予算を待たずに諦める（AbortIfExited）' {
        # **通った実行では発火しないので、ここでしか観測されない。** 諦める条件を scriptblock で
        # 受け取っていた版は、Pester の `It` の中で `.GetNewClosure()` が親スコープの変数を
        # 捕まえられず、**発火しないまま予算いっぱい待つ**退化が黙って入っていた（実測: 予算
        # 4000ms に対し 4031ms）。プロセスを型付きで受け取る形はその間違いを書けなくする。
        $path = New-SnotraTestTraceFile -Path (Join-Path $TestDrive 'never.log') `
            -Json @('{"seq":1,"event":"other","data":{}}')
        $dead = Start-Process -FilePath 'cmd.exe' -ArgumentList '/c exit 0' -PassThru -WindowStyle Hidden
        $dead.WaitForExit()

        $sw = [Diagnostics.Stopwatch]::StartNew()
        $found = Wait-SnotraTraceCondition -Path $path -TimeoutMs 4000 -PollMs 50 `
            -Predicate { $_.event -eq 'target' } -AbortIfExited $dead -WarningAction SilentlyContinue
        $sw.Stop()

        $found | Should -BeNullOrEmpty
        # 諦め損ねていれば予算 4000ms を使い切る。
        $sw.Elapsed.TotalMilliseconds | Should -BeLessThan 2000
    }
}

Describe 'Stop-SnotraProcessAndWait（#872 単一インスタンス衝突）' {
    It '$null は何もせず $true を返す' {
        Stop-SnotraProcessAndWait -Process $null | Should -BeTrue
    }

    It '既に終了しているプロセスは kill しない' {
        Mock -ModuleName SnotraSmoke Stop-Process {}

        Stop-SnotraProcessAndWait -Process (New-FakeProcess -HasExited $true) | Should -BeTrue

        Should -Invoke -ModuleName SnotraSmoke Stop-Process -Times 0
    }

    It '生存しているプロセスを kill して終了を待つ' {
        Mock -ModuleName SnotraSmoke Stop-Process {}

        Stop-SnotraProcessAndWait -Process (New-FakeProcess) | Should -BeTrue

        Should -Invoke -ModuleName SnotraSmoke Stop-Process -Times 1 -ParameterFilter { $Id -eq 123 -and $Force }
    }

    It '期限内に終了しなければ throw せず $false を返す（finally から呼ぶため）' {
        Mock -ModuleName SnotraSmoke Stop-Process {}

        $result = $null
        {
            $result = Stop-SnotraProcessAndWait -Process (New-FakeProcess -WaitResult $false) `
                -TimeoutMs 10 -WarningAction SilentlyContinue
        } | Should -Not -Throw
        $result | Should -BeFalse
    }

    It 'WaitForExit が例外を投げても throw せず $false を返す（他人のプロセスのアクセス拒否）' {
        Mock -ModuleName SnotraSmoke Stop-Process {}

        $result = $null
        {
            $result = Stop-SnotraProcessAndWait -Process (New-FakeProcess -WaitThrows -Id 456) `
                -WarningAction SilentlyContinue
        } | Should -Not -Throw
        $result | Should -BeFalse
    }

    It '-Quiet でも終了待ちの警告は黙らない（黙らせるのは Stop-Process のエラーだけ）' {
        # 実機配管の `AfterAll` と各 It の `finally` は `-Quiet` 付きで呼んでおり、その
        # コメントが「`-Quiet` はこの警告を黙らせない」に荷重をかけている。**`Write-Warning` を
        # `if ($Quiet)` の内側へ動かすリファクタは、この It が無いとテストを 1 本も落とさずに
        # 通り、コメントだけが静かに嘘になる。**
        #
        # **「期限内に終了しなければ throw せず $false を返す」とは意図的に分けてある。**
        # セットアップはほぼ同一（差は `-Quiet` と警告の断言だけ）だが、固定している契約が
        # 違う——あちらは「`finally` から呼べる」、こちらは「`-Quiet` が警告を消さない」。
        # 束ねると、落ちたときにどちらの契約が壊れたのかテスト名が言わなくなる。
        # 検出を担っているのは警告の断言だけである（`$result` は変異の前後とも `$false`・実測）。
        Mock -ModuleName SnotraSmoke Stop-Process {}

        $warnings = @()
        $result = Stop-SnotraProcessAndWait -Process (New-FakeProcess -WaitResult $false) `
            -TimeoutMs 10 -Quiet -WarningVariable warnings -WarningAction SilentlyContinue
        $result | Should -BeFalse
        ($warnings -join "`n") | Should -BeLike '*single-instance*'
    }
}

Describe 'Resolve-SnotraExistingProcess' {
    It 'Reject 方針では既存プロセスを終了せず例外にする' {
        Mock -ModuleName SnotraSmoke Get-Process { @([pscustomobject]@{ Id = 123 }) }
        Mock -ModuleName SnotraSmoke Stop-Process {}

        { Resolve-SnotraExistingProcess -Policy Reject } | Should -Throw '*pid=123*'
        Should -Invoke -ModuleName SnotraSmoke Stop-Process -Times 0
    }

    It 'Stop 方針では列挙した既存プロセスだけを停止し、終了を待つ' {
        # `Stop` 分岐は `Stop-SnotraProcessAndWait` を通るので、偽物にもメンバが要る
        # （生成はファイル先頭の `New-FakeProcess` が正本）。
        $fake = New-FakeProcess
        Mock -ModuleName SnotraSmoke Get-Process { @($fake) }.GetNewClosure()
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
        $created = New-SnotraVerificationProfile -ProfileDir $profile -GeneralSection @'
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
            # **終了を待つ**（#872）。待たずに次の It へ進むと、その It の
            # `Resolve-SnotraExistingProcess -Policy Reject` がこのプロセスを掴んで throw する
            # （実測 3/30）。ここは `finally` なので throw せず、警告と戻り値だけを残す。
            [void](Stop-SnotraProcessAndWait -Process $proc -Quiet)
        }
    }

    It '起動後の最初のフレームで入力欄が打鍵を受け取れる状態になっている' {
        # **この It が守るのは L3（実プロセス層）だけである。** キャレットの断言は
        # `src-tauri/src/egui_shell/view.rs` の kittest が**実コードの並びごと**縛る
        # （#872 の機序再設計・設計書は
        # `docs/superpowers/specs/2026-08-05-caret-test-mechanism-design.md`）。
        #
        # **打鍵を注入しないのは、注入と 3 段の待ちが 7 か月の間欠失敗の構造的前提
        # そのものだったからである**（#872 本文の要素 1 = 前面窓依存・要素 2 = 実時間
        # ポーリング）。OS の打鍵がアプリへ届く配線は `smoke-egui.ps1` が release
        # ビルドで見ている（hotkey VK 列 → `egui_show:done` → 1 文字クエリ →
        # `egui_results:show`）。
        #
        # `egui_input:focus_state` は #938 が**この回帰の検出器として**置いたもので、
        # 偽に戻れば起動直後の打鍵が再び捨てられている（機序の正本は `view.rs` の
        # 当該コメント）。**前面化は残す**——focus 要求は `pre.focused`（窓の OS focus）に
        # 条件づけられており、前面を奪えなければ `has_focus` は真にならない。
        $profile = Join-Path $TestDrive 'caret-profile'
        $stderr = Join-Path $TestDrive 'caret.err'
        # この検査はアイコンを一切見ない。runner ではシェルのアイコン問い合わせが恒久的に
        # 失敗し続け（#872 / #887）、その再要求が起動を押し広げる。要求そのものを外す。
        $created = New-SnotraVerificationProfile -ProfileDir $profile -ShowIcons $false `
            -GeneralSection @'
show_on_startup = true
auto_hide_on_focus_lost = false
'@
        $proc = $null
        try {
            Resolve-SnotraExistingProcess -Policy Reject
            $proc = Start-SnotraProcess -ConfigDir $created.FullPath -Trace `
                -FilePath $env:SNOTRA_PESTER_EXE -StandardErrorPath $stderr
            $hwnd = Wait-SnotraWindow -Title 'Snotra' -Process $proc -TimeoutMs 30000
            Set-SnotraForegroundWindow -Handle $hwnd | Should -BeTrue

            # **予算は 30,000ms** ——待ちは 1 つだけで順序依存が無く、この待ちは
            # 「フレームが 1 度でも回ったか」しか見ないので、遅さそのものは判定に混ざらない。
            # ゆえに広げても隠れる退行が無い（実測でフレーム不回転は 24.3 秒まで観測されている）。
            $seen = Wait-SnotraTraceCondition -Path $stderr -TimeoutMs 30000 -PollMs 100 `
                -AbortIfExited $proc -Description 'egui_input:focus_state（最初のフレーム）' `
                -Predicate { $_.event -eq 'egui_input:focus_state' }
            $seen | Should -Not -BeNullOrEmpty

            # **最初の行に断言する。`Wait-SnotraTraceCondition` の戻り値を使ってはならない。**
            # あれは一致の**最後**を返す（`SnotraSmoke.psm1` の `Select-Object -Last 1`）。
            # #938 の回帰は「frame 1 だけ偽・frame 2 以降は真」という形で現れ（機序の正本は
            # `view.rs` の当該コメント）、`focus_state` は show ごとに 5 行出る。ゆえに最後の行を
            # 見ると**回帰した実装でも真を読んで PASS する**（合成 trace で実測: seq=3 / true）。
            $snapshot = Read-SnotraTraceSnapshot -Path $stderr
            $focusRows = @($snapshot.Events | Where-Object {
                    $_.event -eq 'egui_input:focus_state' -and $_.data.window_focused
                })
            # **部分集合が空でないことを別に断言する。** 省くと主語ゼロで自明に緑になる——
            # `focus_state` は show ごと 5 フレームで尽き、show の外では再武装しない。
            $focusRows.Count | Should -BeGreaterThan 0
            # 窓が focus を持つ最初のフレームで、入力欄も焦点を持っていなければならない。
            $focusRows[0].data.has_focus | Should -BeTrue
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
            [void](Stop-SnotraProcessAndWait -Process $proc -Quiet)
        }
    }

    # **待ちきれなかった生き残りを、ここで赤にする**（#872）。各 It の `finally` は
    # `Write-Warning` しか出せない（`finally` からの throw は元の例外を覆い隠すため）ので、
    # 検出点を置かないと Pester 実行全体から生きた snotra.exe が漏れても誰も見ない
    # ——**`Write-Warning` は `run-pester.ps1` の合否（`FailedCount`）に一切影響しない**。
    # `AfterAll` からの throw は It の例外を覆い隠さないため、ここが正しい層である
    # （`docs/development-principles.md`「構造的設計原則と強制の階梯」の一段上げ）。
    AfterAll {
        # **検査が起動した本体だけに絞る。** `Get-Process -Name 'snotra'` はグローバルで、
        # 開発者が普段使いで起動している実インスタンスも掴む。絞らないと (1) それを予告なく
        # Force kill し、(2)「終了待ちが効いていません」という**誤った診断**で赤くする
        # （その場合の実態は先行インスタンスであり、各 It 冒頭の `Reject` が先に throw する）。
        #
        # **絞る材料が無ければ落とす（fail-closed）。** 空の `$expected` で絞ると一致が常に
        # 空になり、**検出器は主語ゼロで自明に緑を返す**——「効いていない」と「漏れが無い」が
        # 同じ緑に化ける。これは本 PR が `ADR-egui-trace-hatch-empty-only` で塞いだ空文字 env と
        # 同じ形の欠陥であり、検出器の側に開けてはならない。
        $expected = $env:SNOTRA_PESTER_EXE
        if ([string]::IsNullOrWhiteSpace($expected)) {
            throw '実機配管の後始末で SNOTRA_PESTER_EXE が空でした。検査対象を絞れないため、漏れの有無を判定できません。'
        }
        # `.Path` は権限の無いプロセスで例外を投げうるので個別に握りつぶす。
        $leaked = @(Get-Process -Name 'snotra' -ErrorAction SilentlyContinue | Where-Object {
                $path = try { $_.Path } catch { $null }
                $path -and $path -eq $expected
            })
        if ($leaked.Count -gt 0) {
            $ids = $leaked.Id -join ', '
            # **後続の検査を巻き添えにしないよう掃除してから落とす。** ここも待つ——
            # `Stop-Process -Force` は終了を待たない（snotra.exe では実測 3/30 で It の後まで
            # 生き残った・上の実機配管 1 つ目の It の `finally` のコメント）。
            # 待たなければ「掃除できた」と「殺せなかった」が同じ throw に化ける
            # （`Stop-SnotraProcessAndWait` の 5 秒待ちと `Write-Warning` だけが後者を分ける。
            # `-Quiet` はその警告を黙らせない——It「-Quiet でも終了待ちの警告は黙らない」が
            # 固定する）。合否はどちらも赤である。この分岐は必ず throw で終わるので、
            # 緑の run はこの待ちを 1ms も払わない。
            $leaked | ForEach-Object { [void](Stop-SnotraProcessAndWait -Process $_ -Quiet) }
            throw "実機配管の後に検査対象の snotra が残っています（pid=$ids）。終了待ちが効いていません。"
        }
    }
}
