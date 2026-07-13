# Build (if needed) and run a Fiber Sidecar desktop binary — stable multi-instance, no file watcher.
param(
    [Parameter(Mandatory = $true)]
    [int]$AgentId,
    [switch]$Rebuild
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

$exe = Join-Path $env:CARGO_TARGET_DIR "debug\fiber_agent_desktop.exe"
$frontendRoot = Join-Path $tauriDir "frontend"
$frontendStamp = [datetime]::MinValue
if (Test-Path $frontendRoot) {
    $frontendStamp = Get-ChildItem -Path $frontendRoot -Recurse -File -ErrorAction SilentlyContinue |
        Measure-Object -Property LastWriteTime -Maximum |
        Select-Object -ExpandProperty Maximum
}
$exeStamp = if (Test-Path $exe) { (Get-Item $exe).LastWriteTime } else { [datetime]::MinValue }
$frontendStale = $frontendStamp -gt $exeStamp

if ($Rebuild -or -not (Test-Path $exe) -or $frontendStale) {
    if ($frontendStale -and -not $Rebuild) {
        Write-Host "Frontend changed since last build - rebuilding FA-$AgentId..." -ForegroundColor Yellow
    } else {
        Write-Host "Building FA-$AgentId desktop binary..." -ForegroundColor Yellow
    }
    Set-Location $tauriDir
    npx tauri build --debug --no-bundle --config $overlayPath
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

if (-not (Test-Path $exe)) {
    Write-Error "Desktop binary not found at $exe"
}

Write-Host "Starting $exe" -ForegroundColor Green
& $exe
