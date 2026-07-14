#!/bin/bash
set -euo pipefail
LOCK_ARGS=0xd86d9e57fec8ae319d3c7fde9f956878ed9d78e8
CKB_RPC=$(grep -E '^\s*rpc_url:' /opt/fnn/config.yml | head -n1 | awk '{print $2}' | tr -d '"')
CKB_RPC="${CKB_RPC:-https://testnet.ckbapp.dev/}"
echo "CKB_RPC=$CKB_RPC"

python3 - <<PY
import json, urllib.request
lock_args = "$LOCK_ARGS"
script = {
  "code_hash": "0x9bd7e06f3ecf4be0f2fcd2188b23f1b9fcc88e5d4b65a8637b17723bbda3cce8",
  "hash_type": "type",
  "args": lock_args,
}

def call(url, method, params):
    req = urllib.request.Request(
        url,
        data=json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode(),
        headers={"Content-Type": "application/json", "User-Agent": "fspdevs-fund-check/1.0"},
    )
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.load(resp)

urls = ["$CKB_RPC", "https://testnet.ckb.dev/", "https://testnet.ckbapp.dev/"]
ok = False
for url in urls:
    try:
        tip = call(url, "get_tip_block_number", [])
        print("tip_ok", url, tip.get("result"))
        ok = True
        # Indexer-compatible get_cells (Nervos CKB Indexer RPC)
        try:
            cells = call(url, "get_cells", [
                {
                    "script": script,
                    "script_search_mode": "exact",
                    "script_type": "lock",
                    "with_data": False,
                },
                "asc",
                "0x64",
            ])
            objs = cells.get("result", {}).get("objects", [])
            total = 0
            for obj in objs:
                cap = int(obj.get("output", {}).get("capacity", "0x0"), 16)
                total += cap
            print("live_cells", len(objs))
            print("total_capacity_shannon", total)
            print("total_capacity_ckb", total / 100_000_000)
            if objs:
                print("sample_out_point", json.dumps(objs[0].get("out_point", {})))
        except Exception as e:
            print("get_cells_error", e)
        break
    except Exception as e:
        print("rpc_fail", url, e)

if not ok:
    print("NO_CKB_RPC_AVAILABLE")

# FNN peers / channels
fnn = "http://127.0.0.1:18227/"
def fnn_call(method, params):
    req = urllib.request.Request(
        fnn,
        data=json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode(),
        headers={"Content-Type": "application/json"},
    )
    return json.load(urllib.request.urlopen(req, timeout=15))

info = fnn_call("node_info", [])["result"]
print("fnn_channels", info.get("channel_count"), "peers", info.get("peers_count"))
chs = fnn_call("list_channels", [{}])["result"]
print("channels", chs)
peers = fnn_call("list_peers", [{}])["result"]
peer_list = peers.get("peers", peers if isinstance(peers, list) else [])
print("peer_count_detail", len(peer_list) if isinstance(peer_list, list) else peers)
if isinstance(peer_list, list):
    for p in peer_list[:5]:
        print("peer", p.get("pubkey") or p.get("peer_id") or p)
PY

echo BALANCE_CHECK_DONE
