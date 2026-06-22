# Creates a dev CKB wallet key for FNN testnet (plain hex; encrypted on first fnn start).
$ErrorActionPreference = "Stop"
$data = Join-Path $PSScriptRoot "data"
$ckbDir = Join-Path $data "ckb"
New-Item -ItemType Directory -Path $ckbDir -Force | Out-Null

$keyPath = Join-Path $ckbDir "key"
if (Test-Path $keyPath) {
    Write-Host "CKB key already exists: $keyPath"
    exit 0
}

# Valid secp256k1 test scalar (dev-only; fund this address on CKB testnet if opening channels)
$devKeyHex = ("2a" * 32)
Set-Content -Path $keyPath -Value $devKeyHex -NoNewline -Encoding ascii
Write-Host "Created dev CKB key at $keyPath"
Write-Host "Set FIBER_SECRET_KEY_PASSWORD before starting fnn (see start-testnet.ps1)."
