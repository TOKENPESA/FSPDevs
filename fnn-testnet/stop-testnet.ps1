# Stop FNN testnet node if running
$procs = Get-Process -Name "fnn" -ErrorAction SilentlyContinue
if (-not $procs) {
    Write-Host "No fnn process is running."
    exit 0
}

foreach ($p in $procs) {
    Write-Host "Stopping fnn (PID $($p.Id))..."
    Stop-Process -Id $p.Id -Force
}

Start-Sleep -Seconds 2
Write-Host "FNN stopped."
