#!/bin/bash
set -euo pipefail
curl -fsS -m 10 http://134.122.120.65:8227/ -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"node_info","params":[]}' \
  | python3 -c 'import sys,json; r=json.load(sys.stdin)["result"]; print("mfa_hub_rpc_ok", r["node_name"], r["pubkey"][:20], "peers", r["peers_count"])'
echo MFA_HUB_RPC_OK
