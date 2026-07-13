# Generate FSPDevs-gemini-audit-10.zip — max 10 files for Gemini deep audit (v6.0).

$ErrorActionPreference = "Stop"
$repo = $PSScriptRoot
$outDir = Join-Path $repo "FSPDevs-gemini-audit-10"
$zipPath = Join-Path $repo "FSPDevs-gemini-audit-10.zip"

$sourceMap = [ordered]@{
    "01-Cargo.toml"                 = "Cargo.toml"
    "02-mfa-lib.rs"                 = "master-fiber-agent\src\lib.rs"
    "03-mfa-health.rs"              = "master-fiber-agent\src\handlers\health.rs"
    "04-mfa-ws_agent.rs"            = "master-fiber-agent\src\handlers\ws_agent.rs"
    "05-mfa-auth.rs"                = "master-fiber-agent\src\auth.rs"
    "06-sidecar-module_registry.rs" = "fiber-agent\src\module_registry.rs"
    "07-sidecar-tauri-commands.rs"  = "fiber-agent\src-tauri\src\commands.rs"
    "08-sidecar-daemon.rs"          = "fiber-agent\src\daemon.rs"
    "09-dashboard-money.js"         = "dashboard\money.js"
}

Write-Host "=== FSPDevs Gemini Audit Snapshot v6.0 (10 files) ===" -ForegroundColor Cyan

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
Write-Host "  1. Paste FSPDevs-gemini-audit-10/AUDIT_SNAPSHOT_README.txt as prompt"
Write-Host "  2. Attach FSPDevs-gemini-audit-10.zip"
