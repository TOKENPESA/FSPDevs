# Start MFA with mesh pubkey registry for live testnet dashboard payments.
param(
    [string]$MeshPubkeysPath = ""
)

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$repo = Split-Path $root -Parent
$mfa = Join-Path $repo "master-fiber-agent"

function Invoke-Cargo {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments)
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    & cargo @Arguments
    $code = $LASTEXITCODE
    $ErrorActionPreference = $prev
    if ($code -ne 0) { exit $code }
}

if (-not $MeshPubkeysPath) {
    $MeshPubkeysPath = Join-Path $root "mesh-pubkeys.json"
    $example = Join-Path $root "mesh-pubkeys.live.example.json"
    if (-not (Test-Path $MeshPubkeysPath) -and (Test-Path $example)) {
        Copy-Item $example $MeshPubkeysPath
        Write-Host "Created $MeshPubkeysPath from live example."
    }
}

$env:MESH_PUBKEY_REGISTRY_PATH = $MeshPubkeysPath
$env:MFA_AGENT_WS_TOKEN = if ($env:MFA_AGENT_WS_TOKEN) { $env:MFA_AGENT_WS_TOKEN } else { "tpxdevs-local-ws" }
$env:HUB_RPC_URL = if ($env:HUB_RPC_URL) { $env:HUB_RPC_URL } else { "http://127.0.0.1:8227" }

Write-Host "=== Master Fiber Agent (live testnet) ===" -ForegroundColor Cyan
Write-Host "Mesh pubkeys: $MeshPubkeysPath"
Write-Host "Hub RPC:      $($env:HUB_RPC_URL)"
Write-Host ""

Set-Location $mfa
Invoke-Cargo run
