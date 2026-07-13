# Serve MFA Operations Console on :8088 (required for MFA CORS / monitor WebSocket).
$ErrorActionPreference = "Stop"
$repo = Split-Path $PSScriptRoot -Parent

function Stop-DashboardListener {
    $conn = Get-NetTCPConnection -LocalPort 8088 -State Listen -ErrorAction SilentlyContinue
    if (-not $conn) { return }
    $pid = ($conn | Select-Object -First 1).OwningProcess
    $proc = Get-Process -Id $pid -ErrorAction SilentlyContinue
    if ($proc -and $proc.ProcessName -match "node") {
        Write-Host "Stopping stale dashboard server (PID $pid)..." -ForegroundColor Yellow
        Stop-Process -Id $pid -Force -ErrorAction SilentlyContinue
        Start-Sleep -Seconds 1
    }
}

Stop-DashboardListener

Set-Location $repo
Write-Host "Starting MFA Operations Console at http://127.0.0.1:8088/" -ForegroundColor Cyan
Write-Host "Legacy mesh visualizer: http://127.0.0.1:8088/index-legacy.html" -ForegroundColor DarkGray
npm run serve:mfa
