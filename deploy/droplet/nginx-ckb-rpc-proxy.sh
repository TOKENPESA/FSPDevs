#!/bin/bash
# Expose Treasury :8114 / :8116 as HTTPS reverse proxies to testnet.ckbapp.dev
# (Hub FNN already uses that RPC; no local full node required).
set -euo pipefail

CONF_SRC="${1:-/opt/fspdevs/deploy/droplet/nginx/ckb-rpc-proxy.conf}"
if [[ ! -f "$CONF_SRC" ]]; then
  CONF_SRC="/tmp/ckb-rpc-proxy.conf"
fi
install -m 0644 "$CONF_SRC" /etc/nginx/sites-available/ckb-rpc-proxy.conf
ln -sfn /etc/nginx/sites-available/ckb-rpc-proxy.conf /etc/nginx/sites-enabled/ckb-rpc-proxy.conf

nginx -t
systemctl reload nginx

ufw allow 8114/tcp comment 'CKB JSON-RPC proxy' || true
ufw allow 8116/tcp comment 'CKB indexer RPC proxy' || true

ss -lntp | grep -E '8114|8116' || true
curl -fsS -m 20 http://127.0.0.1:8114/ -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"get_tip_block_number","params":[]}'
echo
