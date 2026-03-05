# Build snotra-settings and copy to src-tauri/binaries/ for Tauri bundling.
# Usage: pwsh -NoProfile -File scripts/prepare-sidecar.ps1 [-Release]

param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"

$profile = if ($Release) { "--release" } else { $null }
$targetDir = if ($Release) { "target/release" } else { "target/debug" }

Write-Host "[prepare-sidecar] Building snotra-settings ($($Release ? 'release' : 'debug'))..."
$buildArgs = @("build", "-p", "snotra-settings")
if ($profile) { $buildArgs += $profile }
& cargo @buildArgs
if ($LASTEXITCODE -ne 0) { exit 1 }

# Determine target triple
$triple = (rustc -vV | Select-String "^host:").ToString().Split(": ")[1].Trim()

$src = "$targetDir/snotra-settings.exe"
$destDir = "src-tauri/binaries"
$dest = "$destDir/snotra-settings-${triple}.exe"

if (!(Test-Path $destDir)) { New-Item -ItemType Directory -Path $destDir | Out-Null }
Copy-Item $src $dest -Force
Write-Host "[prepare-sidecar] Copied to $dest"
