# One-shot checklist for live testnet dashboard payments (prints steps; does not start long-running processes).
param(
    [int]$SourceAgent = 44,
    [int]$DestAgent = 45,
    [long]$AmountShannons = 100000000
)

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$repo = Split-Path $root -Parent

Write-Host @"

================================================================================
  LIVE TESTNET DASHBOARD — setup checklist
================================================================================

  Dashboard:  http://127.0.0.1:8088  (npm run serve:dashboard in $repo)
  Route:      FA-$SourceAgent -> FA-$DestAgent, amount $AmountShannons shannons

  OPEN THESE TERMINALS (in order):

  1) FNN + public channel
     cd $root
     .\setup-public-channel.ps1
     (Fund testnet wallet at faucet if prompted, then re-run until ChannelReady)

  2) MFA with mesh pubkeys
     cd $root
     .\start-live-mfa.ps1

  3) Live sidecar (sender) — NO FNN_MODE=simulate
     cd $root
     .\start-live-sidecar.ps1 -AgentId $SourceAgent

  4) Dashboard
     cd $repo
     npm run serve:dashboard

  5) Dashboard: Connect monitor WS, set Source=$SourceAgent Dest=$DestAgent, click Route & Pay

  DESTINATION FA-${DestAgent}:
    - Second live sidecar on its own FNN, OR
    - Entry in mesh-pubkeys.json (see mesh-pubkeys.live.example.json - FA-45 = public testnet node1)

  Env (already set by scripts above):
    MESH_PUBKEY_REGISTRY_PATH = $root\mesh-pubkeys.json
    MFA_AGENT_WS_TOKEN        = tpxdevs-local-ws
    FNN_RPC_URL               = http://127.0.0.1:8227

================================================================================

"@ -ForegroundColor Cyan

if (-not (Test-Path (Join-Path $root "mesh-pubkeys.json"))) {
    $ex = Join-Path $root "mesh-pubkeys.live.example.json"
    if (Test-Path $ex) {
        Copy-Item $ex (Join-Path $root "mesh-pubkeys.json")
        Write-Host "Created mesh-pubkeys.json from example." -ForegroundColor Yellow
    }
}

$fnn = Get-Process -Name "fnn" -ErrorAction SilentlyContinue
if ($fnn) {
    Write-Host "FNN: running (PID $($fnn.Id))" -ForegroundColor Green
} else {
    Write-Host "FNN: not running - run .\start-testnet.ps1" -ForegroundColor Yellow
}
