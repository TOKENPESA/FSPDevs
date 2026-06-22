# Start real fnn.exe processes for a range of FA nodes (use with MESH_FNN_AUTO_PORTS sidecars or live fleet).
param(
    [int]$From = 1,
    [int]$To = 16,
    [int]$BatchSize = 8
)

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$fnn = Join-Path $root "fnn.exe"
if (-not (Test-Path $fnn)) {
    Write-Error "fnn.exe not found in $root"
}
if ($From -lt 1 -or $To -gt 1024 -or $From -gt $To) {
    Write-Error "From/To must be 1..1024 with From <= To"
}

$env:FIBER_SECRET_KEY_PASSWORD = if ($env:FIBER_SECRET_KEY_PASSWORD) { $env:FIBER_SECRET_KEY_PASSWORD } else { "tpxdevs-local" }
$pidFile = Join-Path $root "nodes\mesh-fnn.pids"
New-Item -ItemType Directory -Path (Join-Path $root "nodes") -Force | Out-Null
@() | Set-Content $pidFile

Write-Host "=== Spawning live FNN nodes FA-$From..=$To ===" -ForegroundColor Cyan

$started = 0
for ($id = $From; $id -le $To; $id++) {
    $meta = & (Join-Path $root "New-FnnNodeConfig.ps1") -AgentId $id
    $data = $meta.DataDir
    $config = Join-Path $data "config.yml"

    $proc = Start-Process -FilePath $fnn -ArgumentList @("-c", $config, "-d", ".") `
        -WorkingDirectory $data -WindowStyle Hidden -PassThru
    Add-Content $pidFile $proc.Id
    $started++
    Write-Host "  FA-$id  RPC $($meta.RpcUrl)  PID $($proc.Id)"

    if ($started % $BatchSize -eq 0) {
        Start-Sleep -Seconds 2
    }
}

Write-Host ""
Write-Host "Started $started FNN process(es). PIDs: $pidFile"
Write-Host "Sidecars: set MESH_FNN_AUTO_PORTS=true and AGENT_ID per process, or use mesh-fleet-daemon with MESH_FLEET_LIVE_FNN=true"
