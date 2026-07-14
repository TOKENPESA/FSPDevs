param(
    [Parameter(Mandatory = $true)]
    [string]$DropletIp,
    [Parameter(Mandatory = $true)]
    [ValidateSet("mfa", "treasury")]
    [string]$Role,
    [string]$SshUser = "root",
    [string]$SshKey = "$env:USERPROFILE\.ssh\fsp-droplet-admin",
    [string]$MfaHost = "",
    [string]$MfaAgentWsToken = "",
    [string]$Branch = "main"
)

$ErrorActionPreference = "Stop"

if ($Role -eq "treasury" -and -not $MfaHost) {
    Write-Error "Treasury Hub bootstrap requires -MfaHost (MFA droplet IP or host:port)."
}

Write-Host "=== FSPDevs $Role droplet bootstrap ===" -ForegroundColor Cyan
Write-Host "Target: ${SshUser}@${DropletIp}"
Write-Host ""

$sshArgs = @()
if (Test-Path $SshKey) {
    $sshArgs = @("-i", $SshKey)
}

$bundle = Join-Path $env:TEMP "fspdevs-droplet-$(Get-Date -Format 'yyyyMMddHHmmss')"
New-Item -ItemType Directory -Path $bundle -Force | Out-Null
Copy-Item -Recurse -Force (Join-Path $PSScriptRoot "*") $bundle

$remoteDir = "/tmp/fspdevs-droplet"
& scp @sshArgs -r "$bundle\*" "${SshUser}@${DropletIp}:${remoteDir}/"

$remoteCmd = @"
set -e
export FSPDEVS_ROLE=$Role
export FSPDEVS_BRANCH=$Branch
export MFA_HOST='$MfaHost'
export MFA_AGENT_WS_TOKEN='$MfaAgentWsToken'
bash ${remoteDir}/bootstrap.sh
"@

& ssh @sshArgs "${SshUser}@${DropletIp}" $remoteCmd

Write-Host ""
if ($Role -eq "mfa") {
    Write-Host "Done. MFA console: http://${DropletIp}/mfa-console/" -ForegroundColor Green
} else {
    Write-Host "Done. Treasury Hub: http://${DropletIp}/" -ForegroundColor Green
}
