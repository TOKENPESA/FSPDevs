#!/bin/bash
set -euo pipefail
ss -lntp | grep 8227 || true
systemctl is-active fspdevs-fnn || { journalctl -u fspdevs-fnn -n 30 --no-pager; exit 1; }
curl -fsS -m 8 http://127.0.0.1:8227/ -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"node_info","params":[]}' \
  | python3 -c 'import sys,json; r=json.load(sys.stdin)["result"]; print("local_ok", r["node_name"], "peers", r["peers_count"])'
echo HUB_LOCAL_OK
