#!/bin/bash
set -euo pipefail

echo "=== Hub FNN ==="
systemctl is-active fspdevs-fnn
systemctl is-active fspdevs-treasury
grep -E '^(FNN_|AGENT_|MFA_)' /etc/fspdevs/treasury.env | sed 's/=.*/=***/' || true
grep FNN_RPC_URL /etc/fspdevs/treasury.env || true
grep FNN_MODE /etc/fspdevs/treasury.env || echo 'FNN_MODE unset (good)'

echo "=== node_info ==="
curl -fsS -m 10 http://127.0.0.1:8227/ -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"node_info","params":[]}' | head -c 500
echo

echo "=== funding lock args (from node_info) ==="
curl -fsS -m 10 http://127.0.0.1:8227/ -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"node_info","params":[]}' \
  | python3 -c 'import sys,json; r=json.load(sys.stdin)["result"]; print("pubkey", r["pubkey"]); print("lock_args", r["default_funding_lock_script"]["args"]); print("peers", r["peers_count"]); print("channels", r["channel_count"])'

# Try list_channels / ckb address via fnn-cli if available
if [[ -x /opt/fnn/fnn-cli ]]; then
  echo "=== fnn-cli help (address) ==="
  /opt/fnn/fnn-cli --help 2>&1 | head -n 40 || true
fi

ss -lntp | grep -E '8227|8228' || true
echo VERIFY_HUB_DONE
