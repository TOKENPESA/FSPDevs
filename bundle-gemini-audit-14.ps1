# Generate FSPDevs-gemini-audit-14.zip — Gemini milestone v10.0 (App Store + Super Console).

$ErrorActionPreference = "Stop"
$repo = $PSScriptRoot
$outDir = Join-Path $repo "FSPDevs-gemini-audit-14"
$zipPath = Join-Path $repo "FSPDevs-gemini-audit-14.zip"

$sourceMap = [ordered]@{
    "GEMINI_MILESTONE_STATUS.txt"          = "GEMINI_MILESTONE_STATUS.txt"
    "01-mfa-mfa_storage.rs"                = "master-fiber-agent\src\mfa_storage.rs"
    "02-mfa-storage_error.rs"              = "master-fiber-agent\src\storage_error.rs"
    "03-mfa-policies-registry.rs"        = "master-fiber-agent\src\policies\registry.rs"
    "04-mfa-plugin_routes.rs"              = "master-fiber-agent\src\api\plugin_routes.rs"
    "05-mfa-auth.rs"                       = "master-fiber-agent\src\auth.rs"
    "06-mfa-config.rs"                     = "master-fiber-agent\src\config.rs"
    "07-mfa-health.rs"                     = "master-fiber-agent\src\handlers\health.rs"
    "08-mfa-lib.rs"                        = "master-fiber-agent\src\lib.rs"
    "09-fa-modules-registry.rs"            = "fiber-agent\src\modules\registry.rs"
    "10-dashboard-module-api.js"           = "dashboard\dashboard-module-api.js"
    "11-dashboard-module-ui.js"            = "dashboard\dashboard-module-ui.js"
    "12-dashboard-config.js"               = "dashboard\config.js"
    "13-dashboard-regulatory-core.js"      = "dashboard\regulatory-core.js"
    "14-mfa-module-store-api.js"           = "mfa-console\js\mfa-module-store-api.js"
    "15-mfa-app-store-index.js"            = "mfa-console\js\modules\app-store\index.js"
    "16-mfa-app-store-panel.js"            = "mfa-console\js\modules\app-store\app-store-panel.js"
    "17-fa-module-store-api.js"            = "fiber-agent\src-tauri\frontend\js\fa-module-store-api.js"
    "18-fa-app-store-panel.js"             = "fiber-agent\src-tauri\frontend\js\modules\app-store\app-store-panel.js"
    "19-integrity.mjs"                     = "scripts\integrity.mjs"
    "20-sync-sidecar-ui.mjs"               = "scripts\sync-sidecar-ui.mjs"
    "21-start-live-mfa.ps1"                = "fnn-testnet\start-live-mfa.ps1"
    "22-fsp-App.jsx"                       = "fsp-console\src\App.jsx"
    "23-fsp-SuperAppConsole.jsx"           = "fsp-console\src\components\superapp\SuperAppConsole.jsx"
    "24-fsp-mfa-api.js"                    = "fsp-console\src\api\mfa.js"
    "25-fsp-ModuleRegistry.jsx"            = "fsp-console\src\components\registry\ModuleRegistry.jsx"
    "26-fsp-StandaloneMeshController.jsx"  = "fsp-console\src\components\superapp\StandaloneMeshController.jsx"
}

Write-Host "=== FSPDevs Gemini Milestone v10.0 (audit-14) ===" -ForegroundColor Cyan

if (-not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

$readmePath = Join-Path $outDir "AUDIT_SNAPSHOT_README.txt"
if (-not (Test-Path $readmePath)) {
    throw "Missing AUDIT_SNAPSHOT_README.txt in $outDir"
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
Write-Host "  1. Paste GEMINI_MILESTONE_STATUS.txt as prompt"
Write-Host "  2. Attach FSPDevs-gemini-audit-14.zip"
Write-Host ""
Write-Host "Optional background: FSPDevs-gemini-audit-13.zip (phases 0-3 RWA)"
