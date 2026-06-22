# Start all 1024 FA sidecars + embedded FNN in one process (recommended for full mesh).
param(
    [int]$From = 1,
    [int]$To = 0,
    [switch]$LiveFnn
)

$ErrorActionPreference = "Stop"
$repo = Split-Path $PSScriptRoot -Parent
$sidecar = Join-Path $repo "fiber-agent"
$pubkeys = Join-Path $PSScriptRoot "mesh-pubkeys.json"

function Invoke-Cargo {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments)
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    & cargo @Arguments
    $code = $LASTEXITCODE
    $ErrorActionPreference = $prev
    if ($code -ne 0) { exit $code }
}

if (-not (Test-Path $pubkeys)) {
    Copy-Item (Join-Path $PSScriptRoot "mesh-pubkeys.live.example.json") $pubkeys -ErrorAction SilentlyContinue
}

if ($To -le 0) {
    $To = if ($env:MESH_SIMULATION_EDGE_NODES) { [int]$env:MESH_SIMULATION_EDGE_NODES } else { 1024 }
}
if ($To -lt 1 -or $To -gt 1024) {
    Write-Error "To must be 1..1024 (got $To). Set in dashboard or -To N"
}

function Wait-MfaReady {
    param([int]$MaxAttempts = 30)
    for ($i = 1; $i -le $MaxAttempts; $i++) {
        try {
            $r = Invoke-WebRequest -Uri "http://127.0.0.1:1025/" -TimeoutSec 3 -UseBasicParsing
            if ($r.StatusCode -eq 200) {
                Write-Host "MFA is ready on :1025" -ForegroundColor Green
                return
            }
        } catch {
            Write-Host "Waiting for MFA on :1025 ($i/$MaxAttempts)..." -ForegroundColor Yellow
            Start-Sleep -Seconds 2
        }
    }
    Write-Error @"
MFA is not running on http://127.0.0.1:1025

Start it in another terminal FIRST:
  cd fnn-testnet
  .\start-live-mfa.ps1

Then open the dashboard (npm run serve:dashboard) and click Connect.
"@
}

Write-Host "=== TPXDevs 1024-FA Mesh Fleet ===" -ForegroundColor Cyan
Write-Host "Checking MFA is up before launching $To sidecars..."
Wait-MfaReady
Write-Host ""

$env:MESH_FLEET_FROM = "$From"
$env:MESH_FLEET_TO = "$To"
$env:MESH_SIMULATION_EDGE_NODES = "$To"
$env:MESH_FLEET_QUIET = "true"
$env:MESH_FLEET_STAGGER_MS = "15"
$env:MESH_FLEET_HEARTBEAT_MS = "30000"
$env:MESH_PUBKEY_REGISTRY_PATH = $pubkeys
$env:MFA_AGENT_WS_TOKEN = if ($env:MFA_AGENT_WS_TOKEN) { $env:MFA_AGENT_WS_TOKEN } else { "tpxdevs-local-ws" }
$env:MFA_HOST = if ($env:MFA_HOST) { $env:MFA_HOST } else { "127.0.0.1:1025" }

if ($LiveFnn) {
    Write-Host "Live FNN mode: spawning fnn.exe for FA-$From..=$To first..."
    & (Join-Path $PSScriptRoot "spawn-live-fnn-nodes.ps1") -From $From -To $To
    $env:MESH_FLEET_LIVE_FNN = "true"
    $env:MESH_FNN_AUTO_PORTS = "true"
    Remove-Item Env:FNN_MODE -ErrorAction SilentlyContinue
} else {
    $env:MESH_FLEET_LIVE_FNN = "false"
    $env:FNN_MODE = "simulate"
}

Set-Location $sidecar
Write-Host "Building mesh-fleet-daemon..."
Invoke-Cargo build --bin mesh-fleet-daemon --release

Write-Host "Launching FA-$From..=$To ..."
Invoke-Cargo run --release --bin mesh-fleet-daemon
