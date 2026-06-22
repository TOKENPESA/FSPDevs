# Start a live (non-simulated) Fiber Agent sidecar for dashboard payments.
param(
    [Parameter(Mandatory = $true)]
    [int]$AgentId,
    [string]$MfaHost = "127.0.0.1:1025",
    [string]$FnnRpcUrl = "http://127.0.0.1:8227",
    [string]$WsToken = "tpxdevs-local-ws"
)

$ErrorActionPreference = "Stop"
$repo = Split-Path $PSScriptRoot -Parent
$sidecar = Join-Path $repo "fiber-agent"

if (-not (Test-Path $sidecar)) {
    Write-Error "fiber-agent not found at $sidecar"
}

$env:AGENT_ID = "$AgentId"
$env:MFA_HOST = $MfaHost
$env:MFA_AGENT_WS_TOKEN = $WsToken
$env:FNN_RPC_URL = $FnnRpcUrl
Remove-Item Env:FNN_MODE -ErrorAction SilentlyContinue

Write-Host "=== Live sidecar FA-$AgentId ===" -ForegroundColor Cyan
Write-Host "FNN RPC:  $FnnRpcUrl"
Write-Host "MFA:      $MfaHost"
Write-Host "Simulate: OFF (live FNN)"
Write-Host ""

Set-Location $sidecar
cargo run --bin fiber-agent-daemon
