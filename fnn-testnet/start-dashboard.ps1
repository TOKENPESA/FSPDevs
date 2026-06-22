# Serve dashboard on :8088 (required for MFA CORS / monitor WebSocket).
$ErrorActionPreference = "Stop"
$repo = Split-Path $PSScriptRoot -Parent

$on8088 = Get-NetTCPConnection -LocalPort 8088 -State Listen -ErrorAction SilentlyContinue
if ($on8088) {
    $pid = ($on8088 | Select-Object -First 1).OwningProcess
    $proc = Get-Process -Id $pid -ErrorAction SilentlyContinue
    Write-Host "Dashboard already on http://localhost:8088 (PID $pid, $($proc.ProcessName))" -ForegroundColor Green
    Write-Host "Open that URL — do not use a second serve on another port."
    exit 0
}

Set-Location $repo
Write-Host "Starting dashboard at http://localhost:8088 ..." -ForegroundColor Cyan
npm run serve:dashboard
