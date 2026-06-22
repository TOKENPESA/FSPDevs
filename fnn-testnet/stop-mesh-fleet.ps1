# Stop live FNN nodes spawned by spawn-live-fnn-nodes.ps1
$ErrorActionPreference = "SilentlyContinue"
$root = $PSScriptRoot
$pidFile = Join-Path $root "nodes\mesh-fnn.pids"

if (Test-Path $pidFile) {
    Get-Content $pidFile | ForEach-Object {
        $pid = [int]$_
        if (Get-Process -Id $pid -ErrorAction SilentlyContinue) {
            Stop-Process -Id $pid -Force
            Write-Host "Stopped FNN PID $pid"
        }
    }
    Remove-Item $pidFile -Force
}

Get-Process -Name "mesh_fleet_daemon", "mesh-fleet-daemon", "fiber_agent_daemon", "fiber-agent-daemon" -ErrorAction SilentlyContinue | ForEach-Object {
    Stop-Process -Id $_.Id -Force
    Write-Host "Stopped $($_.ProcessName) PID $($_.Id)"
}

Write-Host "Mesh fleet stop complete."
