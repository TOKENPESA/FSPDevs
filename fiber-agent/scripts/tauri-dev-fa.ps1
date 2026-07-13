# Launch an isolated Tauri dev shell for one Fiber Agent.
# Run one terminal per agent, e.g. npm run tauri:dev:44 and npm run tauri:dev:45.
param(
    [Parameter(Mandatory = $true)]
    [int]$AgentId,
    [switch]$SyncUi
)

$ErrorActionPreference = "Stop"
$fiberAgentRoot = Split-Path $PSScriptRoot -Parent
$tauriDir = Join-Path $fiberAgentRoot "src-tauri"

. (Join-Path $PSScriptRoot "fa-tauri-instance.ps1")

Set-FaTauriInstanceEnv -AgentId $AgentId -FiberAgentRoot $fiberAgentRoot
$overlayPath = Write-FaTauriOverlay -AgentId $AgentId -TauriDir $tauriDir
Write-FaTauriInstanceBanner -AgentId $AgentId

Set-Location $fiberAgentRoot
npm run sync:sidecar-ui
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Set-Location $tauriDir
# --no-dev-server: static frontend only (no shared :1430 server).
# --no-watch: avoid cross-instance rebuilds when the other FA starts or syncs UI files.
npx tauri dev --config $overlayPath --no-dev-server --no-watch --port $env:TAURI_CLI_PORT
