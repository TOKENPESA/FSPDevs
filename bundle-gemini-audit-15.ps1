# Generate FSPDevs-gemini-audit-15.zip — Sidecar device-ship audit (Win/Android + FNN + MFA).

$ErrorActionPreference = "Stop"
$repo = $PSScriptRoot
$outDir = Join-Path $repo "FSPDevs-gemini-audit-15"
$zipPath = Join-Path $repo "FSPDevs-gemini-audit-15.zip"

$sourceMap = [ordered]@{
    "GEMINI_SIDECAR_DEVICE_SHIP_PROMPT.txt" = "GEMINI_SIDECAR_DEVICE_SHIP_PROMPT.txt"
    "GEMINI_SIDECAR_DEVICE_SHIP_AUDIT.txt"  = "GEMINI_SIDECAR_DEVICE_SHIP_AUDIT.txt"
    "00-INVENTORY.env"                      = "deploy\droplet\INVENTORY.env"
    "01-tauri.conf.json"                    = "fiber-agent\src-tauri\tauri.conf.json"
    "02-tauri-lib.rs"                       = "fiber-agent\src-tauri\src\lib.rs"
    "03-tauri-commands.rs"                  = "fiber-agent\src-tauri\src\commands.rs"
    "04-tauri-fnn_address.rs"               = "fiber-agent\src-tauri\src\fnn_address.rs"
    "05-fa-package.json"                    = "fiber-agent\package.json"
    "06-fa-env.example"                     = "fiber-agent\.env.example"
    "07-fa-daemon.rs"                       = "fiber-agent\src\daemon.rs"
    "08-fa-mfa_ws_auth.rs"                  = "fiber-agent\src\mfa_ws_auth.rs"
    "09-fa-lib.rs"                          = "fiber-agent\src\lib.rs"
    "10-fa-fnn_client.rs"                   = "fiber-agent\src\fnn_client.rs"
    "11-fa-dicoba_bridge.rs"                = "fiber-agent\src\dicoba_bridge.rs"
    "12-fa-mesh_ports.rs"                   = "fiber-agent\src\mesh_ports.rs"
    "13-fa-hot_swap.rs"                     = "fiber-agent\src\hot_swap.rs"
    "14-fa-storage.rs"                      = "fiber-agent\src\storage.rs"
    "15-fa-identity.rs"                     = "fiber-agent\src\identity.rs"
    "16-mfa-auth.rs"                        = "master-fiber-agent\src\auth.rs"
    "17-mfa-handlers-auth.rs"               = "master-fiber-agent\src\handlers\auth.rs"
    "18-setup-android-env.ps1"              = "fiber-agent\scripts\setup-android-env.ps1"
    "19-setup-android-env.sh"               = "fiber-agent\scripts\setup-android-env.sh"
    "20-start-testnet.ps1"                  = "fnn-testnet\start-testnet.ps1"
    "21-setup-testnet-key.ps1"              = "fnn-testnet\setup-testnet-key.ps1"
    "22-get-ckb-address.ps1"                = "fnn-testnet\get-ckb-address.ps1"
    "23-droplet-README.md"                  = "deploy\droplet\README.md"
    "24-install-fnn.sh"                     = "deploy\droplet\install-fnn.sh"
    "25-nginx-fnn-rpc.sh"                   = "deploy\droplet\nginx-fnn-rpc.sh"
    "26-setup-tls-fsprotocol.sh"            = "deploy\droplet\setup-tls-fsprotocol.sh"
    "27-verify-phase-b-live.sh"             = "deploy\droplet\verify-phase-b-live.sh"
    "28-funding.js"                         = "fiber-agent\src-tauri\frontend\js\funding.js"
    "29-oob-fallback.js"                    = "fiber-agent\src-tauri\frontend\js\oob-fallback.js"
    "30-sidecar-runtime.js"                 = "fiber-agent\src-tauri\frontend\js\sidecar-runtime.js"
    "31-dashboard-stats.js"                 = "fiber-agent\src-tauri\frontend\js\dashboard-stats.js"
    "32-capabilities-default.json"          = "fiber-agent\src-tauri\capabilities\default.json"
}

Write-Host "=== FSPDevs Gemini audit-15 (Sidecar device ship) ===" -ForegroundColor Cyan

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
Write-Host "  1. Paste GEMINI_SIDECAR_DEVICE_SHIP_PROMPT.txt as the prompt"
Write-Host "  2. Attach GEMINI_SIDECAR_DEVICE_SHIP_AUDIT.txt"
Write-Host "  3. Attach FSPDevs-gemini-audit-15.zip"
