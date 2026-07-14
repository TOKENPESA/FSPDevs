#!/bin/bash
# Open a public Fiber testnet channel from Treasury Hub FNN to a connected peer.
set -euo pipefail
FNN_RPC="${FNN_RPC_URL:-http://127.0.0.1:18227}"
# 1000 CKB in shannons (1 CKB = 1e8 shannons)
FUNDING_AMOUNT_SHANNONS="${FUNDING_AMOUNT_SHANNONS:-100000000000}"
PEER_PUBKEY="${PEER_PUBKEY:-}"

if [[ -z "$PEER_PUBKEY" ]]; then
  PEER_PUBKEY="$(python3 - <<PY
import json, urllib.request
req = urllib.request.Request(
    "$FNN_RPC/",
    data=b'{"jsonrpc":"2.0","id":1,"method":"list_peers","params":[{}]}',
    headers={"Content-Type": "application/json"},
)
peers = json.load(urllib.request.urlopen(req, timeout=15))["result"]
items = peers.get("peers", peers if isinstance(peers, list) else [])
if not items:
    raise SystemExit("no connected peers — wait for P2P then retry")
# Prefer first peer that is not ourselves if identifiable
self_pk = None
try:
    info_req = urllib.request.Request(
        "$FNN_RPC/",
        data=b'{"jsonrpc":"2.0","id":1,"method":"node_info","params":[]}',
        headers={"Content-Type": "application/json"},
    )
    self_pk = json.load(urllib.request.urlopen(info_req, timeout=15))["result"]["pubkey"]
except Exception:
    pass
for p in items:
    pk = p.get("pubkey") if isinstance(p, dict) else None
    if pk and pk != self_pk:
        print(pk)
        break
else:
    pk = items[0].get("pubkey") if isinstance(items[0], dict) else None
    if not pk:
        raise SystemExit(f"cannot parse peer pubkey from {items[0]!r}")
    print(pk)
PY
)"
fi

echo "Opening channel → peer $PEER_PUBKEY"
echo "Funding amount (shannons): $FUNDING_AMOUNT_SHANNONS"

/opt/fnn/fnn-cli -u "$FNN_RPC" --raw-data channel open_channel \
  --pubkey "$PEER_PUBKEY" \
  --funding-amount "$FUNDING_AMOUNT_SHANNONS" \
  --public true

echo
sleep 5
echo "=== channels after open attempt ==="
/opt/fnn/fnn-cli -u "$FNN_RPC" channel list_channels --only-pending true || true
/opt/fnn/fnn-cli -u "$FNN_RPC" channel list_channels || true
curl -fsS -m 10 "$FNN_RPC/" -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"node_info","params":[]}' \
  | python3 -c 'import sys,json; r=json.load(sys.stdin)["result"]; print("channels", r["channel_count"], "pending", r["pending_channel_count"], "peers", r["peers_count"])'

journalctl -u fspdevs-fnn --since '2 minutes ago' --no-pager | tail -n 40
echo OPEN_CHANNEL_DONE
