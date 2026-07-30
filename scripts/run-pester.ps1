#Requires -Version 7
[CmdletBinding()]
param(
    [string]$ExePath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$pesterVersion = '6.0.1'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$moduleCache = Join-Path $repoRoot 'target/pester'
$pesterManifest = Join-Path $moduleCache "Pester/$pesterVersion/Pester.psd1"
$testPath = Join-Path $PSScriptRoot 'lib'
$smokeModulePath = Join-Path $testPath 'SnotraSmoke.psm1'

Import-Module $smokeModulePath -Force

if (-not (Test-Path $pesterManifest)) {
    New-Item -ItemType Directory -Force -Path $moduleCache | Out-Null
    Write-Host "Pester $pesterVersion を target/ へ取得します…"
    Save-Module -Name Pester -RequiredVersion $pesterVersion -Path $moduleCache -Repository PSGallery
}

Import-Module $pesterManifest -Force

$resolvedExe = if ($PSBoundParameters.ContainsKey('ExePath')) {
    if ([string]::IsNullOrWhiteSpace($ExePath)) {
        throw '-ExePath を明示する場合は空でないパスを指定してください。'
    }
    if ([IO.Path]::IsPathRooted($ExePath)) {
        $ExePath
    } else {
        Join-Path $repoRoot $ExePath
    }
} else {
    Resolve-SnotraCargoExecutable -RepositoryRoot $repoRoot
}
if (-not (Test-Path $resolvedExe)) {
    throw "Pester の統合テストに使う実行ファイルがありません: $resolvedExe"
}
Write-Host "Pester integration executable: $resolvedExe"

$savedExe = [Environment]::GetEnvironmentVariable('SNOTRA_PESTER_EXE', 'Process')
try {
    $env:SNOTRA_PESTER_EXE = (Resolve-Path $resolvedExe).Path
    $result = Invoke-Pester -Path $testPath -Output Detailed -PassThru
} finally {
    [Environment]::SetEnvironmentVariable('SNOTRA_PESTER_EXE', $savedExe, 'Process')
}

if ($result.FailedCount -gt 0 -or $result.FailedContainersCount -gt 0) {
    exit 1
}
