# Start FNN testnet node (v0.8.1 release binary)
param(
    [switch]$Restart
)

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$data = Join-Path $root "data"
$fnn = Join-Path $root "fnn.exe"
$config = Join-Path $data "config.yml"

if (-not (Test-Path $fnn)) {
    Write-Error "fnn.exe not found. Run setup from FSPDevs/fnn-testnet release bundle first."
}

$existing = Get-Process -Name "fnn" -ErrorAction SilentlyContinue
if ($existing) {
    if ($Restart) {
        & (Join-Path $root "stop-testnet.ps1")
    } else {
        Write-Host "FNN is already running (PID $($existing.Id))."
        Write-Host "  RPC:  http://127.0.0.1:8227"
        Write-Host "  Data: $data"
        Write-Host ""
        Write-Host "Use .\stop-testnet.ps1 to stop, or .\start-testnet.ps1 -Restart to restart."
        exit 0
    }
}

New-Item -ItemType Directory -Path $data -Force | Out-Null
if (-not (Test-Path $config)) {
    Copy-Item (Join-Path $root "config\testnet\config.yml") $config -Force
}

& (Join-Path $root "setup-testnet-key.ps1")

$env:FIBER_SECRET_KEY_PASSWORD = if ($env:FIBER_SECRET_KEY_PASSWORD) { $env:FIBER_SECRET_KEY_PASSWORD } else { "fspdevs-local" }
$env:RUST_LOG = if ($env:RUST_LOG) { $env:RUST_LOG } else { "info" }

Write-Host "Starting FNN testnet..."
Write-Host "  RPC:  http://127.0.0.1:8227"
Write-Host "  P2P:  0.0.0.0:8228"
Write-Host "  Data: $data"
Write-Host "  Fund: .\get-ckb-address.ps1 then https://faucet.nervos.org/"
Write-Host ""

Set-Location $data
& $fnn -c $config -d .
