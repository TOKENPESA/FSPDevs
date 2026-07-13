# Generate FSPDevs-gemini-audit-13.zip — Gemini implementation status deep-learn (FSP v9.0 · July 2026).

$ErrorActionPreference = "Stop"
$repo = $PSScriptRoot
$outDir = Join-Path $repo "FSPDevs-gemini-audit-13"
$zipPath = Join-Path $repo "FSPDevs-gemini-audit-13.zip"

$sourceMap = [ordered]@{
    "01-Cargo.toml"                        = "Cargo.toml"
    "02-mesh-types.rs"                     = "mesh-core\src\types.rs"
    "03-mesh-network.rs"                   = "mesh-core\src\network.rs"
    "04-mesh-lib.rs"                       = "mesh-core\src\lib.rs"
    "05-sidecar-fnn_client.rs"             = "fiber-agent\src\fnn_client.rs"
    "06-sidecar-module_system.rs"          = "fiber-agent\src\module_system.rs"
    "07-sidecar-module_host.rs"            = "fiber-agent\src\module_host.rs"
    "08-sidecar-module_registry.rs"        = "fiber-agent\src\module_registry.rs"
    "09-sidecar-module_catalog.rs"         = "fiber-agent\src\module_catalog.rs"
    "10-sidecar-module_profile.rs"         = "fiber-agent\src\module_profile.rs"
    "11-sidecar-modules-mod.rs"            = "fiber-agent\src\modules\mod.rs"
    "12-sidecar-lume_yielding.rs"          = "fiber-agent\src\modules\lume_yielding.rs"
    "13-sidecar-securities_compliance.rs"  = "fiber-agent\src\modules\securities_compliance.rs"
    "14-sidecar-fiber_agent_swarm.rs"      = "fiber-agent\src\modules\fiber_agent_swarm.rs"
    "15-sidecar-dicoba_module.rs"          = "fiber-agent\src\modules\dicoba_module.rs"
    "16-sidecar-fiat_bridge_module.rs"     = "fiber-agent\src\modules\fiat_bridge_module.rs"
    "17-sidecar-storage.rs"                = "fiber-agent\src\storage.rs"
    "18-sidecar-lib.rs"                    = "fiber-agent\src\lib.rs"
    "19-mfa-graph.rs"                      = "master-fiber-agent\src\graph.rs"
    "20-mfa-routing.rs"                    = "master-fiber-agent\src\routing.rs"
    "21-mfa-clearing.rs"                   = "master-fiber-agent\src\clearing.rs"
    "22-mfa-handlers-route.rs"             = "master-fiber-agent\src\handlers\route.rs"
    "23-mfa-handlers-clearing.rs"          = "master-fiber-agent\src\handlers\clearing.rs"
    "24-mfa-workers-background.rs"         = "master-fiber-agent\src\workers\background.rs"
    "25-mfa-lib.rs"                        = "master-fiber-agent\src\lib.rs"
    "26-integrity.mjs"                     = "scripts\integrity.mjs"
    "27-package.json"                      = "package.json"
    "28-jsconfig.json"                     = "jsconfig.json"
    "29-dashboard-money.js"                = "dashboard\money.js"
    "30-dashboard-fetch-timeout.js"        = "dashboard\fetch-timeout.js"
    "31-dashboard-logger.js"               = "dashboard\logger.js"
    "32-fsp-fixed-math-index.js"           = "packages\fsp-fixed-math\index.js"
    "33-profiles-full.profile.toml"        = "fiber-agent\profiles\full.profile.toml"
}

Write-Host "=== FSPDevs Gemini Implementation Status v9.0 (35 files) ===" -ForegroundColor Cyan

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
    Write-Host "  + $destName"
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
Write-Host "  1. Paste FSPDevs-gemini-audit-13/AUDIT_SNAPSHOT_README.txt as prompt"
Write-Host "  2. Attach FSPDevs-gemini-audit-13.zip"
Write-Host ""
Write-Host "Optional full repo: bundle-gemini-audit.ps1 -> FSPDevs-code-audit-v6.zip"
