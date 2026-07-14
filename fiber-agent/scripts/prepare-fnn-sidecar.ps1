# Copy fnn-testnet/fnn.exe into src-tauri/binaries/fnn-<target-triple>.exe for Tauri externalBin.
param(
    [string]$SourceFnn = ""
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$repo = Split-Path -Parent $root
$binDir = Join-Path $root "src-tauri\binaries"
New-Item -ItemType Directory -Path $binDir -Force | Out-Null

if (-not $SourceFnn) {
    $SourceFnn = Join-Path $repo "fnn-testnet\fnn.exe"
}
if (-not (Test-Path $SourceFnn)) {
    Write-Error "fnn.exe not found at $SourceFnn. Install/extract the Fiber FNN release into fnn-testnet/ first."
}

$hostTriple = (rustc -Vv | Select-String '^host:').ToString().Split(':')[1].Trim()
if (-not $hostTriple) {
    Write-Error "Could not determine rustc host triple."
}

$dest = Join-Path $binDir "fnn-$hostTriple.exe"
Copy-Item $SourceFnn $dest -Force
Write-Host "Prepared Tauri externalBin sidecar:"
Write-Host "  $dest"
Write-Host "  size=$((Get-Item $dest).Length) bytes"
Write-Host ""
Write-Host "Configured in tauri.conf.json as: binaries/fnn"
Write-Host "Build NSIS installer with: npm run tauri:build"
