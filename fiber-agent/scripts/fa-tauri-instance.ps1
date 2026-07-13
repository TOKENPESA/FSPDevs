# Shared helpers for isolated per-agent Fiber Sidecar (Tauri) instances on Windows.
param()

function Set-FaTauriInstanceEnv {
    param(
        [Parameter(Mandatory = $true)]
        [int]$AgentId,
        [Parameter(Mandatory = $true)]
        [string]$FiberAgentRoot
    )

    if ($AgentId -lt 1 -or $AgentId -gt 1024) {
        throw "AgentId must be 1..1024 (got $AgentId)"
    }

    $stateRoot = Join-Path $env:LOCALAPPDATA "FSPDevs\FiberAgent"
    $env:AGENT_ID = "$AgentId"
    $env:CARGO_TARGET_DIR = Join-Path $FiberAgentRoot "target-fa-$AgentId"
    $env:FIBER_AGENT_STATE_DIR = Join-Path $stateRoot "state-fa-$AgentId"
    $env:WEBVIEW2_USER_DATA_FOLDER = Join-Path $stateRoot "webview-fa-$AgentId"
    $env:TAURI_CLI_PORT = "$([int](1400 + $AgentId))"

    New-Item -ItemType Directory -Force -Path $env:FIBER_AGENT_STATE_DIR, $env:WEBVIEW2_USER_DATA_FOLDER | Out-Null
}

function Write-FaTauriOverlay {
    param(
        [Parameter(Mandatory = $true)]
        [int]$AgentId,
        [Parameter(Mandatory = $true)]
        [string]$TauriDir
    )

    $overlayPath = Join-Path $TauriDir ".tauri-fa-$AgentId.json"
    $windowLabel = "main-fa-$AgentId"
    $identifier = "com.fspdevs.fiber-agent.fa-$AgentId"

    $overlay = @{
        identifier  = $identifier
        productName = "Fiber Agent FA-$AgentId"
        app         = @{
            withGlobalTauri = $true
            security        = @{
                csp = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src 'self' https://fonts.gstatic.com data:; connect-src 'self' ipc: http://localhost:* http://127.0.0.1:* ws://localhost:* ws://127.0.0.1:*; img-src 'self' data:; object-src 'none'; base-uri 'self'; frame-ancestors 'none'; form-action 'self'"
            }
            windows         = @(
                @{
                    label  = $windowLabel
                    title  = "Fiber Sidecar - FA-$AgentId"
                    width  = 1280
                    height = 720
                }
            )
        }
    }

    $json = $overlay | ConvertTo-Json -Depth 8 -Compress
    $utf8NoBom = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText($overlayPath, $json, $utf8NoBom)
    return $overlayPath
}

function Write-FaTauriInstanceBanner {
    param([int]$AgentId)

    Write-Host "=== Fiber Sidecar FA-$AgentId (isolated instance) ===" -ForegroundColor Cyan
    Write-Host "App id:    com.fspdevs.fiber-agent.fa-$AgentId"
    Write-Host "Window:    main-fa-$AgentId"
    Write-Host "Target:    $($env:CARGO_TARGET_DIR)"
    Write-Host "State:     $($env:FIBER_AGENT_STATE_DIR)"
    Write-Host "WebView2:  $($env:WEBVIEW2_USER_DATA_FOLDER)"
    Write-Host "Dev port:  $($env:TAURI_CLI_PORT) (unused with --no-dev-server)"
    Write-Host ""
}
