#!/bin/bash
set -euo pipefail
systemctl is-active fspdevs-treasury
journalctl -u fspdevs-treasury --since '30 seconds ago' --no-pager | tail -n 15 || true
curl -fsS -m 8 https://mfa.fsprotocol.com/ | python3 -c 'import sys,json; d=json.load(sys.stdin); print("connected_agents", d.get("connected_agents"), d.get("connected_agent_ids"))'
echo VERIFY_OK
