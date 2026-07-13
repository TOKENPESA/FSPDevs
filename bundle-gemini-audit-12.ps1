# Generate FSPDevs-gemini-audit-12.zip — Gemini testnet readiness audit (FSP v8.0 · July 2026).

$ErrorActionPreference = "Stop"
$repo = $PSScriptRoot
$outDir = Join-Path $repo "FSPDevs-gemini-audit-12"
$zipPath = Join-Path $repo "FSPDevs-gemini-audit-12.zip"

$sourceMap = [ordered]@{
    "01-Cargo.toml"                 = "Cargo.toml"
    "02-mesh-network.rs"            = "mesh-core\src\network.rs"
    "03-mesh-pubkey.rs"             = "mesh-core\src\pubkey.rs"
    "04-mesh-registry.rs"           = "mesh-core\src\registry.rs"
    "05-sidecar-identity.rs"        = "fiber-agent\src\identity.rs"
    "06-sidecar-peer_packet.rs"     = "fiber-agent\src\peer_packet.rs"
    "07-sidecar-module_host.rs"     = "fiber-agent\src\module_host.rs"
    "08-sidecar-module_system.rs"   = "fiber-agent\src\module_system.rs"
    "09-sidecar-dicoba_module.rs"   = "fiber-agent\src\modules\dicoba_module.rs"
    "10-sidecar-daemon.rs"          = "fiber-agent\src\daemon.rs"
    "11-sidecar-tauri-commands.rs"  = "fiber-agent\src-tauri\src\commands.rs"
    "12-mfa-auth.rs"                = "master-fiber-agent\src\auth.rs"
    "13-sidecar-oob-fallback.js"    = "fiber-agent\src-tauri\frontend\js\oob-fallback.js"
    "14-sidecar-dicoba-member-id.js" = "fiber-agent\src-tauri\frontend\js\dicoba-member-id.js"
    "15-sidecar-fa-instance.ps1"    = "fiber-agent\scripts\fa-tauri-instance.ps1"
    "16-integrity.mjs"              = "scripts\integrity.mjs"
    "17-dashboard-money.js"         = "dashboard\money.js"
    "18-sidecar-mesh.rs"            = "fiber-agent\src\mesh.rs"
}

Write-Host "=== FSPDevs Gemini Testnet Readiness Audit v8.0 (18 files) ===" -ForegroundColor Cyan

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
Write-Host "  1. Paste FSPDevs-gemini-audit-12/AUDIT_SNAPSHOT_README.txt as prompt"
Write-Host "  2. Attach FSPDevs-gemini-audit-12.zip"
