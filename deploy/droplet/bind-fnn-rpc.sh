#!/bin/bash
set -euo pipefail
# Bind FNN JSON-RPC on all interfaces; firewall restricts to MFA IP.
CFG=/opt/fnn/config.yml
python3 - <<'PY'
from pathlib import Path
p = Path("/opt/fnn/config.yml")
text = p.read_text()
old = 'listening_addr: "127.0.0.1:8227"'
new = 'listening_addr: "0.0.0.0:8227"'
if old not in text:
    raise SystemExit(f"expected {old!r} not found")
p.write_text(text.replace(old, new, 1))
print("patched rpc listening_addr -> 0.0.0.0:8227")
PY
grep -A5 '^rpc:' "$CFG"
ufw allow from 167.99.150.153 to any port 8227 proto tcp || true
systemctl restart fspdevs-fnn
sleep 4
systemctl is-active fspdevs-fnn
ss -lntp | grep 8227
curl -fsS -m 8 http://127.0.0.1:8227/ -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"node_info","params":[]}' | python3 -c 'import sys,json;print("ok",json.load(sys.stdin)["result"]["node_name"])'
echo RPC_BIND_DONE
