# Generate config.yml + dev key for one mesh FA node.
param(
    [Parameter(Mandatory = $true)]
    [int]$AgentId,
    [int]$RpcBase = 18000,
    [int]$P2pBase = 28000
)

$ErrorActionPreference = "Stop"
if ($AgentId -lt 1 -or $AgentId -gt 1024) {
    Write-Error "AgentId must be 1..1024"
}

$root = $PSScriptRoot
$nodeDir = Join-Path $root "nodes" ("fa-{0:D4}" -f $AgentId)
$rpcPort = $RpcBase + $AgentId
$p2pPort = $P2pBase + $AgentId

New-Item -ItemType Directory -Path $nodeDir -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $nodeDir "ckb") -Force | Out-Null

$keyPath = Join-Path $nodeDir "ckb\key"
if (-not (Test-Path $keyPath)) {
    $bytes = New-Object byte[] 32
    $bytes[0] = [byte](($AgentId -shr 8) -band 0xFF)
    $bytes[1] = [byte]($AgentId -band 0xFF)
    $bytes[31] = 0xA5
    $hex = -join ($bytes | ForEach-Object { $_.ToString("x2") })
    Set-Content -Path $keyPath -Value $hex -NoNewline -Encoding ascii
}

$template = Get-Content (Join-Path $root "templates\node-config.yml.template") -Raw
$config = $template -replace '\{\{AGENT_ID\}\}', "$AgentId" `
    -replace '\{\{RPC_PORT\}\}', "$rpcPort" `
    -replace '\{\{P2P_PORT\}\}', "$p2pPort"
Set-Content -Path (Join-Path $nodeDir "config.yml") -Value $config -Encoding utf8

[PSCustomObject]@{
    AgentId = $AgentId
    DataDir = $nodeDir
    RpcUrl  = "http://127.0.0.1:$rpcPort"
    P2pAddr = "/ip4/127.0.0.1/tcp/$p2pPort"
    RpcPort = $rpcPort
    P2pPort = $p2pPort
}
