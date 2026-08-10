BeforeAll {
    Import-Module (Join-Path $PSScriptRoot 'SnotraStartupContract.psm1') -Force

    # 区間の一覧は Rust 側の `Phase` が正本。ここは検査へ渡す表示順であり、
    # `bench-startup.ps1` の `$PhaseKeys` と同じ並びである。
    $script:PhaseKeys = @(
        'pre_main', 'config_load', 'index_load', 'path_merge', 'history_load',
        'engine_build', 'tauri_init', 'windows_create', 'setup_rest', 'hotkey_register'
    )

    # 実機の `startup:ready` ペイロードを模した合成（#1009 の実測値に倣う）。
    # **`ConvertFrom-Json` の産物と同じ形にする**——検査が受け取るのは常にその形である。
    function New-StartupPayload {
        param([hashtable]$Override = @{})

        # **PowerShell の数値リテラルは `_` 区切りを持たない**（実測: `7_800_000` は
        # コマンド名として解決されようとして落ちる）。
        $phases = [ordered]@{
            pre_main = 7800000; config_load = 1500000; index_load = 1600000
            path_merge = 12700000; history_load = 400000; engine_build = 2200000
            tauri_init = 6400000; windows_create = 30000000; setup_rest = 100000
            hotkey_register = 18000000
        }
        $map = [ordered]@{}
        $sum = 0
        foreach ($k in $phases.Keys) {
            $map["${k}_ns"] = $phases[$k]
            $map["${k}_ms"] = [long]([math]::Floor($phases[$k] / 1e6))
            if ($k -ne 'pre_main') { $sum += $phases[$k] }
        }
        # `pre_main` は `post_main` の外側なので総和に入れない（`startup.rs` の `//!`）。
        $map['post_main_ns'] = $sum
        $map['post_main_ms'] = [long]([math]::Floor($sum / 1e6))
        $map['sum_phase_ns'] = $sum
        $map['unmarked_tail_ns'] = 0
        $map['index_load_unattributed_ms'] = 0
        $map['reached_phase'] = 'hotkey_register'
        $map['first_run'] = $false
        $map['cache_hit'] = $true
        $map['include_path_env'] = $true
        $map['ok'] = $true
        $map['reason'] = $null

        foreach ($k in $Override.Keys) { $map[$k] = $Override[$k] }
        return [pscustomobject]$map
    }

    function Test-Payload {
        param($Data, [string]$EventName = 'startup:ready', [double]$ObservedMs = 5000)
        return @(Test-SnotraStartupPayload -Data $Data -PhaseKey $script:PhaseKeys `
                -ObservedWallClockMs $ObservedMs -EventName $EventName)
    }
}

Describe '起動計器の契約検査（#1009）' {
    Context '合格する形' {
        It '整合したペイロードは破れ 0 件' {
            Test-Payload -Data (New-StartupPayload) | Should -HaveCount 0
        }

        It '実際に失敗した起動（ok=false + reason + startup:failed）も破れ 0 件' {
            # **失敗した起動でも契約は守られるべきである。** `startup:failed` の run を検査から
            # 外すと、`startup:ready` を騙る変異へ届かなくなる（#1009 で実測）。
            $data = New-StartupPayload @{
                ok                         = $false
                reason                     = 'hotkey-registration'
                reached_phase              = 'hotkey_register'
            }
            Test-Payload -Data $data -EventName 'startup:failed' | Should -HaveCount 0
        }

        It 'first-run 枝（index_load と unattributed が null）は破れ 0 件' {
            $data = New-StartupPayload @{
                first_run                  = $true
                index_load_ns              = $null
                index_load_ms              = $null
                index_load_unattributed_ms = $null
            }
            Test-Payload -Data $data | Should -HaveCount 0
        }

        It 'include_path_env=false（path_merge が null）は破れ 0 件' {
            $data = New-StartupPayload @{
                include_path_env = $false
                path_merge_ns    = $null
                path_merge_ms    = $null
            }
            Test-Payload -Data $data | Should -HaveCount 0
        }
    }

    Context 'キーの過不足' {
        It 'キーが欠けたら落ちる' {
            $data = New-StartupPayload
            $stripped = [pscustomobject]($data.PSObject.Properties |
                Where-Object { $_.Name -ne 'sum_phase_ns' } |
                ForEach-Object -Begin { $h = [ordered]@{} } -Process { $h[$_.Name] = $_.Value } -End { $h })
            Test-Payload -Data $stripped | Should -Not -HaveCount 0
        }
    }

    Context 'null の規則（双方向）' {
        It '説明されない null は落ちる（順向き）' {
            # `include_path_env=true` なのに `path_merge` が null＝マークの取り落とし。
            $data = New-StartupPayload @{ path_merge_ns = $null; path_merge_ms = $null }
            (Test-Payload -Data $data) -join ' ' | Should -Match '説明されない null: path_merge'
        }

        It 'null であるべき区間に値があったら落ちる（逆向き）' {
            # **スキップした区間に 0 を書く誤りを捕まえる向き。** これが無いと `null` と `0` を
            # 区別する設計の要が守られない。
            $data = New-StartupPayload @{ first_run = $true }
            (Test-Payload -Data $data) -join ' ' | Should -Match 'null であるべき区間に値がある: index_load'
        }

        It 'reached_phase が未知の区間名なら落ちる' {
            $data = New-StartupPayload @{ reached_phase = 'no_such_phase' }
            (Test-Payload -Data $data) -join ' ' | Should -Match 'reached_phase が未知の区間名'
        }
    }

    Context '恒等式' {
        It 'post_main_ns が sum_phase_ns + unmarked_tail_ns と食い違えば落ちる' {
            # 変異 (d)（`sum_phase_ns` を 2 倍）が作る形（#1009 で実機でも赤を実測）。
            $data = New-StartupPayload
            $data.sum_phase_ns = [long]$data.sum_phase_ns * 2
            (Test-Payload -Data $data) -join ' ' | Should -Match '恒等式の破れ'
        }
    }

    Context '外部の壁時計との突き合わせ' {
        It '内側の申告が外から見た経過を超えたら落ちる' {
            Test-Payload -Data (New-StartupPayload) -ObservedMs 10 |
                Should -Not -HaveCount 0
        }

        It '内側 < 外側は正常（下限は意図的に置いていない）' {
            # 終端を手前で打ち切る変異（#1009 の (j)）が素通りするのはこの非対称ゆえである。
            # **その射程はモジュールの doc が正本**——ここは非対称が実在することだけを固定する。
            Test-Payload -Data (New-StartupPayload) -ObservedMs 999999 | Should -HaveCount 0
        }
    }

    Context 'event と ok / reason の整合' {
        It '失敗を startup:ready と騙ったら落ちる' {
            # **変異 (e) そのもの。** #1009 で実機に当てたとき、この検査を足す前は
            # ホットキー登録が実際に失敗しているのにハーネスが全面的に素通りした。
            $data = New-StartupPayload @{ ok = $false; reason = 'hotkey-registration' }
            (Test-Payload -Data $data -EventName 'startup:ready') -join ' ' |
                Should -Match 'event が ok と食い違う'
        }

        It '成功を startup:failed と騙っても落ちる（逆向き）' {
            (Test-Payload -Data (New-StartupPayload) -EventName 'startup:failed') -join ' ' |
                Should -Match 'event が ok と食い違う'
        }

        It 'ok=true なのに reason があれば落ちる' {
            $data = New-StartupPayload @{ reason = 'hotkey-registration' }
            (Test-Payload -Data $data) -join ' ' | Should -Match 'ok=true なのに reason がある'
        }

        It 'ok=false なのに reason が null なら落ちる' {
            $data = New-StartupPayload @{ ok = $false; reason = $null }
            (Test-Payload -Data $data -EventName 'startup:failed') -join ' ' |
                Should -Match 'reason が null'
        }
    }

    Context 'index_load_unattributed_ms の非負性' {
        It '負なら落ちる' {
            # 非負性は「外側が内側を包む」「両者が切り捨て」の 2 前提に乗り、**どちらも機構で
            # 守られていない**（正本は `startup.rs` の `to_json`）。#1023 で前提が実際に動いた。
            $data = New-StartupPayload @{ index_load_unattributed_ms = -1 }
            (Test-Payload -Data $data) -join ' ' | Should -Match 'index_load_unattributed_ms が負'
        }

        It 'null は正常（first-run 枝では LoadOrScanStats 自体が無い）' {
            $data = New-StartupPayload @{
                first_run                  = $true
                index_load_ns              = $null
                index_load_ms              = $null
                index_load_unattributed_ms = $null
            }
            Test-Payload -Data $data | Should -HaveCount 0
        }
    }
}
