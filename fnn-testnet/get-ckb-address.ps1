# Prints CKB testnet address for this FNN node's default funding lock script.
param(
    [string]$RpcUrl = "http://127.0.0.1:8227"
)

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$cli = Join-Path $root "fnn-cli.exe"
if (-not (Test-Path $cli)) {
    Write-Error "fnn-cli.exe not found in $root"
}

$env:FIBER_SECRET_KEY_PASSWORD = if ($env:FIBER_SECRET_KEY_PASSWORD) { $env:FIBER_SECRET_KEY_PASSWORD } else { "tpxdevs-local" }

$json = & $cli -u $RpcUrl info node_info -o json | ConvertFrom-Json
$script = $json.default_funding_lock_script
$argsHex = $script.args -replace '^0x', ''
$codeHash = $script.code_hash -replace '^0x', ''
$hashType = if ($script.hash_type -eq 'type') { 1 } else { 0 }

$py = @"
import sys
CHARSET = 'qpzry9x8gf2tvdw0s3jn54khce6mua7l'
def polymod(values):
    GEN = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3]
    chk = 1
    for v in values:
        b = chk >> 25
        chk = ((chk & 0x1ffffff) << 5) ^ v
        for i in range(5):
            if (b >> i) & 1: chk ^= GEN[i]
    return chk
def hrp_expand(hrp):
    return [ord(x) >> 5 for x in hrp] + [0] + [ord(x) & 31 for x in hrp]
def create_checksum(hrp, data, const):
    values = hrp_expand(hrp) + data
    mod = polymod(values + [0,0,0,0,0,0]) ^ const
    return [(mod >> 5 * (5 - i)) & 31 for i in range(6)]
def convertbits(data, frombits, tobits, pad=True):
    acc = 0; bits = 0; ret = []
    maxv = (1 << tobits) - 1
    max_acc = (1 << (frombits + tobits - 1)) - 1
    for value in data:
        acc = ((acc << frombits) | value) & max_acc
        bits += frombits
        while bits >= tobits:
            bits -= tobits
            ret.append((acc >> bits) & maxv)
    if pad and bits:
        ret.append((acc << (tobits - bits)) & maxv)
    return ret
def encode(hrp, raw, variant='bech32m'):
    const = 0x2bc830a3 if variant == 'bech32m' else 1
    data = convertbits(raw, 8, 5)
    combined = data + create_checksum(hrp, data, const)
    return hrp + '1' + ''.join(CHARSET[d] for d in combined)
args = bytes.fromhex('$argsHex')
code_hash = bytes.fromhex('$codeHash')
payload = bytes([0x00]) + code_hash + bytes([$hashType]) + args
print(encode('ckt', payload, 'bech32m'))
"@

$addr = python -c $py
Write-Host "CKB testnet address (default funding lock):"
Write-Host $addr
Write-Host ""
Write-Host "Fund at: https://faucet.nervos.org/"
Write-Host "Lock args: $($script.args)"
