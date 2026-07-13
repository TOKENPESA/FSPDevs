# Generate FSPDevs-gemini-audit-11.zip — Gemini progress audit (FSP v7.0 · July 2026).

$ErrorActionPreference = "Stop"
$repo = $PSScriptRoot
$outDir = Join-Path $repo "FSPDevs-gemini-audit-11"
$zipPath = Join-Path $repo "FSPDevs-gemini-audit-11.zip"

$sourceMap = [ordered]@{
    "01-Cargo.toml"                 = "Cargo.toml"
    "02-mesh-network.rs"            = "mesh-core\src\network.rs"
    "03-sidecar-module_host.rs"     = "fiber-agent\src\module_host.rs"
    "04-sidecar-daemon.rs"          = "fiber-agent\src\daemon.rs"
    "05-sidecar-tauri-commands.rs"  = "fiber-agent\src-tauri\src\commands.rs"
    "06-mfa-auth.rs"                = "master-fiber-agent\src\auth.rs"
    "07-dashboard-monitor.js"       = "dashboard\events\monitor.js"
    "08-sidecar-oob-fallback.js"    = "fiber-agent\src-tauri\frontend\js\oob-fallback.js"
    "09-integrity.mjs"              = "scripts\integrity.mjs"
    "10-dashboard-money.js"         = "dashboard\money.js"
}

Write-Host "=== FSPDevs Gemini Progress Audit v7.0 (11 files) ===" -ForegroundColor Cyan

if (-not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

$readmePath = Join-Path $outDir "AUDIT_SNAPSHOT_README.txt"
if (-not (Test-Path $readmePath)) {
    throw "Missing AUDIT_SNAPSHOT_README.txt in $outDir - create it before bundling."
}

Get-ChildItem $outDir -File | Where-Object { $_.Name -ne "AUDIT_SNAPSHOT_README.txt" } | Remove-Item -Force

foreach ($destName in $sourceMap.Keys) {
    $rel = $sourceMap[$destName]
    $src = Join-Path $repo $rel
    if (-not (Test-Path $src)) {
        throw "Missing source file: $rel"
    }
    Copy-Item $src (Join-Path $outDir $destName) -Force
    Write-Host "  + $destName from $rel"
}

$fileCount = (Get-ChildItem $outDir -File).Count
Write-Host ""
Write-Host "Staged $fileCount files in $outDir" -ForegroundColor DarkGray

if (Test-Path $zipPath) {
    Remove-Item $zipPath -Force
}
Compress-Archive -Path (Join-Path $outDir "*") -DestinationPath $zipPath -CompressionLevel Optimal

$zipSizeKb = [math]::Round((Get-Item $zipPath).Length / 1KB, 1)
Write-Host ""
Write-Host "Created: $zipPath ($zipSizeKb KB)" -ForegroundColor Green
Write-Host ""
Write-Host "Upload to Gemini:" -ForegroundColor Yellow
Write-Host "  1. Paste FSPDevs-gemini-audit-11/AUDIT_SNAPSHOT_README.txt as prompt"
Write-Host "  2. Attach FSPDevs-gemini-audit-11.zip"
