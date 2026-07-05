<#
.SYNOPSIS
  リリースタグから抽出したバージョンを Cargo.toml / package.json / src-tauri/tauri.conf.json
  の 3 ファイルへ注入する。

.DESCRIPTION
  .github/workflows/release.yml の "Set version from tag" ステップに埋め込まれていた
  インライン PowerShell を切り出したスクリプト。3 ファイルへの書き換え方式は元のインライン
  処理と同一（Cargo.toml は正規表現置換、package.json / tauri.conf.json は
  ConvertFrom-Json → プロパティ代入 → ConvertTo-Json）。

  Cargo.lock は対象外（元のインライン処理と同じ。workspace.package.version を
  参照する Cargo.lock 内エントリの追従は cargo 側の責務とし、このスクリプトは
  バージョン文字列が定義された 3 ファイルのみを扱う）。

.PARAMETER Version
  設定する semver 文字列（例: "0.12.0", "0.12.0-alpha7"）。先頭の "v" は含めない。

.PARAMETER RepoRoot
  リポジトリルートのパス。省略時はカレントディレクトリ。

.PARAMETER WhatIf
  実際には書き換えず、対象ファイルと設定予定のバージョンを表示して終了する。

.EXAMPLE
  pwsh -File scripts/inject-version.ps1 -Version 0.12.0-alpha7

.EXAMPLE
  pwsh -File scripts/inject-version.ps1 -Version 0.12.0 -WhatIf
#>
param(
  [Parameter(Mandatory = $true)]
  [string]$Version,

  [string]$RepoRoot = (Get-Location).Path,

  [switch]$WhatIf
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$') {
  throw "Invalid version format: $Version"
}

$cargoTomlPath   = Join-Path $RepoRoot "Cargo.toml"
$packageJsonPath = Join-Path $RepoRoot "package.json"
$tauriConfPath   = Join-Path $RepoRoot "src-tauri/tauri.conf.json"

foreach ($p in @($cargoTomlPath, $packageJsonPath, $tauriConfPath)) {
  if (-not (Test-Path $p)) {
    throw "File not found: $p"
  }
}

if ($WhatIf) {
  Write-Host "[WhatIf] Would set version to '$Version' in:"
  Write-Host "  - $cargoTomlPath"
  Write-Host "  - $packageJsonPath"
  Write-Host "  - $tauriConfPath"
  Write-Host "[WhatIf] Cargo.lock is intentionally left untouched (matches current release.yml behavior)."
  exit 0
}

# Cargo.toml (workspace) — [workspace.package] version 行を正規表現置換
$cargoContent = Get-Content $cargoTomlPath -Raw -Encoding utf8
$newCargoContent = $cargoContent -replace '(?m)^version = "[^"]+"', "version = `"$Version`""
Set-Content $cargoTomlPath -Value $newCargoContent -Encoding utf8 -NoNewline

# package.json
$pkg = Get-Content $packageJsonPath -Raw -Encoding utf8 | ConvertFrom-Json
$pkg.version = $Version
$pkg | ConvertTo-Json -Depth 10 | Set-Content $packageJsonPath -Encoding utf8

# tauri.conf.json
$conf = Get-Content $tauriConfPath -Raw -Encoding utf8 | ConvertFrom-Json
$conf.version = $Version
$conf | ConvertTo-Json -Depth 10 | Set-Content $tauriConfPath -Encoding utf8

# 書き換え後の検証: 3ファイルから読み戻して Version と一致するか確認する
$verifyErrors = @()

$cargoCheck = Get-Content $cargoTomlPath -Raw -Encoding utf8
$escapedVersion = [regex]::Escape($Version)
if ($cargoCheck -notmatch "(?m)^version = `"$escapedVersion`"") {
  $verifyErrors += "Cargo.toml: version '$Version' not found after write"
}

$pkgCheck = Get-Content $packageJsonPath -Raw -Encoding utf8 | ConvertFrom-Json
if ($pkgCheck.version -ne $Version) {
  $verifyErrors += "package.json: expected version '$Version', got '$($pkgCheck.version)'"
}

$confCheck = Get-Content $tauriConfPath -Raw -Encoding utf8 | ConvertFrom-Json
if ($confCheck.version -ne $Version) {
  $verifyErrors += "src-tauri/tauri.conf.json: expected version '$Version', got '$($confCheck.version)'"
}

if ($verifyErrors.Count -gt 0) {
  Write-Host "Version injection verification failed:" -ForegroundColor Red
  foreach ($e in $verifyErrors) {
    Write-Host " - $e" -ForegroundColor Red
  }
  exit 1
}

Write-Host "Version set to '$Version' in Cargo.toml, package.json, src-tauri/tauri.conf.json (verified)." -ForegroundColor Green
