# Connect to public Fiber testnet relay and open a funded CKB channel.
param(
    [string]$RpcUrl = "http://127.0.0.1:8227",
    [string]$PeerPubkey = "02b6d4e3ab86a2ca2fad6fae0ecb2e1e559e0b911939872a90abdda6d20302be71",
    [string]$FundingHex = "0xb9e459300",
    [int]$ReadyTimeoutSec = 600
)

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$cli = Join-Path $root "fnn-cli.exe"
if (-not (Test-Path $cli)) {
    Write-Error "fnn-cli.exe not found in $root"
}

$env:FIBER_SECRET_KEY_PASSWORD = if ($env:FIBER_SECRET_KEY_PASSWORD) { $env:FIBER_SECRET_KEY_PASSWORD } else { "tpxdevs-local" }

Write-Host "=== Fiber testnet public channel setup ===" -ForegroundColor Cyan
Write-Host "RPC:    $RpcUrl"
Write-Host "Peer:   $PeerPubkey"
Write-Host "Fund:   $FundingHex (49,900,000,000 shannons)"
Write-Host ""

$proc = Get-Process -Name "fnn" -ErrorAction SilentlyContinue
if (-not $proc) {
    Write-Host "Starting FNN testnet..."
    & (Join-Path $root "start-testnet.ps1")
    Start-Sleep -Seconds 5
}

Write-Host "Local node pubkey:"
& $cli -u $RpcUrl info node_info -o json | ConvertFrom-Json | Select-Object -ExpandProperty pubkey
Write-Host ""

Write-Host "CKB funding address (fund at https://faucet.nervos.org/ if needed):"
& (Join-Path $root "get-ckb-address.ps1") -RpcUrl $RpcUrl
Write-Host ""

Write-Host "Connecting to public peer..."
& $cli -u $RpcUrl peer connect_peer --pubkey $PeerPubkey --save true
Start-Sleep -Seconds 3

$channels = & $cli -u $RpcUrl channel list_channels -o json | ConvertFrom-Json
$existing = $channels.channels | Where-Object { $_.pubkey -eq $PeerPubkey -and $_.state.state_name -eq "ChannelReady" }
if ($existing) {
    Write-Host "Channel already ChannelReady with peer." -ForegroundColor Green
    $existing | Format-List channel_id, local_balance, remote_balance, state
    exit 0
}

Write-Host "Opening channel..."
& $cli -u $RpcUrl channel open_channel --pubkey $PeerPubkey --funding-amount $FundingHex --public true

Write-Host "Waiting for ChannelReady (up to $ReadyTimeoutSec s)..."
$deadline = (Get-Date).AddSeconds($ReadyTimeoutSec)
while ((Get-Date) -lt $deadline) {
    Start-Sleep -Seconds 10
    $channels = & $cli -u $RpcUrl channel list_channels -o json | ConvertFrom-Json
    $ch = $channels.channels | Where-Object { $_.pubkey -eq $PeerPubkey } | Select-Object -First 1
    if ($ch) {
        $stateName = $ch.state.state_name
        Write-Host "  state: $stateName  local: $($ch.local_balance)  remote: $($ch.remote_balance)"
        if ($stateName -eq "ChannelReady") {
            Write-Host ""
            Write-Host "Channel ready — you can Route & Pay from the dashboard." -ForegroundColor Green
            exit 0
        }
    }
}

Write-Error "Timed out waiting for ChannelReady. Check FNN logs in fnn-testnet/data"
