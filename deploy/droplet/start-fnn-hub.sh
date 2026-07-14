#!/bin/bash
set -euo pipefail

cp /tmp/fnn-testnet-hub.config.yml /opt/fnn/config.yml
sed -i 's/\r$//' /opt/fnn/config.yml
cp /tmp/fspdevs-fnn.service /etc/systemd/system/fspdevs-fnn.service
sed -i 's/\r$//' /etc/systemd/system/fspdevs-fnn.service

# P2P for Fiber testnet peers
ufw allow 8228/tcp || true

systemctl daemon-reload
systemctl enable fspdevs-fnn
systemctl restart fspdevs-fnn
sleep 5
systemctl is-active fspdevs-fnn
journalctl -u fspdevs-fnn -n 40 --no-pager

# Point Treasury FA-1 at live FNN (not simulate)
# FNN RPC is loopback :18227; nginx :8227 is MFA-only proxy (see nginx-fnn-rpc.sh).
if grep -q '^FNN_RPC_URL=' /etc/fspdevs/treasury.env; then
  sed -i 's|^FNN_RPC_URL=.*|FNN_RPC_URL=http://127.0.0.1:18227|' /etc/fspdevs/treasury.env
else
  echo 'FNN_RPC_URL=http://127.0.0.1:18227' >> /etc/fspdevs/treasury.env
fi
sed -i '/^FNN_MODE=/d' /etc/fspdevs/treasury.env
systemctl restart fspdevs-treasury
sleep 3
systemctl is-active fspdevs-treasury

# Probe FNN RPC (internal loopback port)
sleep 3
curl -fsS -m 10 http://127.0.0.1:18227/ -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"node_info","params":[]}' || \
curl -fsS -m 10 http://127.0.0.1:18227/ -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"node_info","params":{}}' || true

echo FNN_SETUP_DONE
