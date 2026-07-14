#!/bin/bash
# Treasury Hub — print FNN testnet funding address + channel status.
# Run on the Hub droplet (local FNN RPC on 127.0.0.1:18227).
set -euo pipefail

FNN_RPC="${FNN_RPC_URL:-http://127.0.0.1:18227}"
FAUCET_URL="https://faucet.nervos.org/"

export DEBIAN_FRONTEND=noninteractive
if ! command -v jq >/dev/null 2>&1; then
  apt-get update -qq
  apt-get install -y -qq jq curl ca-certificates
fi

rpc() {
  local method="$1"
  local params="${2:-[]}"
  curl -fsS -m 15 "$FNN_RPC/" \
    -H 'Content-Type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"${method}\",\"params\":${params}}"
}

echo "=== FNN node_info (${FNN_RPC}) ==="
NODE_JSON="$(rpc node_info '[]')"
echo "$NODE_JSON" | jq -C '.result | {version, node_name, pubkey, peers_count, channel_count, pending_channel_count, addresses}'

# Encode default_funding_lock_script → ckt1 Bech32m address (same as fnn-funding-address.sh).
FUNDING_ADDRESS="$(FNN_RPC="$FNN_RPC" python3 - <<'PY'
import json, os, sys, urllib.request

CHARSET = "qpzry9x8gf2tvdw0s3jn54khce6mua7l"
BECH32M_CONST = 0x2BC830A3

def polymod(values):
    GEN = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3]
    chk = 1
    for v in values:
        b = chk >> 25
        chk = ((chk & 0x1ffffff) << 5) ^ v
        for i in range(5):
            chk ^= GEN[i] if ((b >> i) & 1) else 0
    return chk

def hrp_expand(hrp):
    return [ord(x) >> 5 for x in hrp] + [0] + [ord(x) & 31 for x in hrp]

def create_checksum(hrp, data):
    values = hrp_expand(hrp) + data
    polymod_ = polymod(values + [0, 0, 0, 0, 0, 0]) ^ BECH32M_CONST
    return [(polymod_ >> 5 * (5 - i)) & 31 for i in range(6)]

def convertbits(data, frombits, tobits, pad=True):
    acc = 0
    bits = 0
    ret = []
    maxv = (1 << tobits) - 1
    for value in data:
        acc = (acc << frombits) | value
        bits += frombits
        while bits >= tobits:
            bits -= tobits
            ret.append((acc >> bits) & maxv)
    if pad and bits:
        ret.append((acc << (tobits - bits)) & maxv)
    return ret

def bech32m_encode(hrp, data):
    combined = data + create_checksum(hrp, data)
    return hrp + "1" + "".join(CHARSET[d] for d in combined)

rpc = os.environ.get("FNN_RPC", "http://127.0.0.1:18227").rstrip("/")
req = urllib.request.Request(
    rpc + "/",
    data=b'{"jsonrpc":"2.0","id":1,"method":"node_info","params":[]}',
    headers={"Content-Type": "application/json"},
)
info = json.load(urllib.request.urlopen(req, timeout=15))["result"]
lock = info["default_funding_lock_script"]
code_hash = bytes.fromhex(lock["code_hash"][2:])
hash_type = {"data": 0, "type": 1, "data1": 2, "data2": 4}[lock["hash_type"]]
args = bytes.fromhex(lock["args"][2:])
payload = bytes([0x00]) + code_hash + bytes([hash_type]) + args
print(bech32m_encode("ckt", convertbits(payload, 8, 5)))
PY
)"

echo
echo "============================================================"
echo " FUND THIS TESTNET ADDRESS (nervos faucet):"
echo "   ${FUNDING_ADDRESS}"
echo
echo " 1. Open: ${FAUCET_URL}"
echo " 2. Paste the ckt1 address above and claim CKB testnet."
echo " 3. Wait for L1 confirmation, then re-run this script."
echo " 4. Open a Fiber channel (fnn-cli / MFA hub funding path)."
echo "============================================================"
echo

echo "=== list_channels ==="
set +e
CHANNELS_JSON="$(rpc list_channels '[]' 2>/dev/null || rpc list_channels '{}' 2>/dev/null)"
RC=$?
set -e
if [[ $RC -ne 0 || -z "${CHANNELS_JSON}" ]]; then
  echo "(list_channels unavailable or empty — fund address then open a channel)"
else
  echo "$CHANNELS_JSON" | jq -C '.'
  COUNT="$(echo "$CHANNELS_JSON" | jq -r '(.result // .result.channels // []) | length' 2>/dev/null || echo 0)"
  echo
  echo "channels_reported=${COUNT}"
fi

echo
echo "FUND_HUB_DONE"
