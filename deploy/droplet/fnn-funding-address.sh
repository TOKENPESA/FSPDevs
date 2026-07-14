#!/bin/bash
set -euo pipefail
# Encode FNN default_funding_lock_script as a CKB testnet (ckt) Bech32m full address.
python3 - <<'PY'
import json, urllib.request

# Minimal Bech32m (BIP-350) for CKB full format payload 0x00 | code_hash | hash_type | args
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
    return hrp + "1" + "".join([CHARSET[d] for d in combined])

req = urllib.request.Request(
    "http://127.0.0.1:18227/",
    data=b'{"jsonrpc":"2.0","id":1,"method":"node_info","params":[]}',
    headers={"Content-Type": "application/json"},
)
info = json.load(urllib.request.urlopen(req, timeout=10))["result"]
lock = info["default_funding_lock_script"]
code_hash = bytes.fromhex(lock["code_hash"][2:])
# hash_type "type" == 0x01
hash_type = {"data": 0, "type": 1, "data1": 2, "data2": 4}[lock["hash_type"]]
args = bytes.fromhex(lock["args"][2:])
payload = bytes([0x00]) + code_hash + bytes([hash_type]) + args
addr = bech32m_encode("ckt", convertbits(payload, 8, 5))
print("funding_address", addr)
print("fiber_pubkey", info["pubkey"])
print("p2p", info["addresses"][0] if info.get("addresses") else "")
print("peers", info["peers_count"])
print("channels", info["channel_count"])
PY
