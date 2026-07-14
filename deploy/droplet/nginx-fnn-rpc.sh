#!/bin/bash
set -euo pipefail

# Keep FNN RPC on loopback; expose via nginx only to MFA droplet.
python3 - <<'PY'
from pathlib import Path
p = Path("/opt/fnn/config.yml")
text = p.read_text()
text = text.replace('listening_addr: "0.0.0.0:8227"', 'listening_addr: "127.0.0.1:8227"')
p.write_text(text)
print("rpc bound to 127.0.0.1:8227")
PY

cat > /etc/nginx/sites-available/fnn-rpc-mfa.conf <<'EOF'
# JSON-RPC proxy for MFA enterprise clearinghouse (Hub FNN is loopback-only).
server {
    listen 8227;
    listen [::]:8227;
    server_name _;

    allow 167.99.150.153;
    allow 127.0.0.1;
    deny all;

    location / {
        proxy_pass http://127.0.0.1:18227;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_read_timeout 120s;
        proxy_connect_timeout 10s;
    }
}
EOF

# FNN stays on 127.0.0.1:8227 internally; nginx listens publicly on 8227 and
# forwards to an alternate local port — so remap FNN to 18227 instead.
python3 - <<'PY'
from pathlib import Path
p = Path("/opt/fnn/config.yml")
text = p.read_text()
text = text.replace('listening_addr: "127.0.0.1:8227"', 'listening_addr: "127.0.0.1:18227"')
p.write_text(text)
print("fnn internal rpc -> 127.0.0.1:18227")
PY

# Simpler: nginx proxy TO 8227 while listening on another public port? MFA expects :8227.
# So: FNN 18227, nginx public 8227 -> 18227.

ln -sf /etc/nginx/sites-available/fnn-rpc-mfa.conf /etc/nginx/sites-enabled/fnn-rpc-mfa.conf
nginx -t
systemctl reload nginx

systemctl restart fspdevs-fnn
sleep 5
systemctl is-active fspdevs-fnn
ss -lntp | grep -E '8227|18227' || true

curl -fsS -m 8 http://127.0.0.1:18227/ -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"node_info","params":[]}' \
  | python3 -c 'import sys,json; print("fnn", json.load(sys.stdin)["result"]["node_name"])'

# Local through nginx
curl -fsS -m 8 http://127.0.0.1:8227/ -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"node_info","params":[]}' \
  | python3 -c 'import sys,json; print("nginx", json.load(sys.stdin)["result"]["node_name"])'

# Keep treasury pointing at loopback FNN directly (faster, no nginx hop)
if grep -q '^FNN_RPC_URL=' /etc/fspdevs/treasury.env; then
  sed -i 's|^FNN_RPC_URL=.*|FNN_RPC_URL=http://127.0.0.1:18227|' /etc/fspdevs/treasury.env
else
  echo 'FNN_RPC_URL=http://127.0.0.1:18227' >> /etc/fspdevs/treasury.env
fi
systemctl restart fspdevs-treasury
sleep 2
systemctl is-active fspdevs-treasury
echo NGX_RPC_DONE
