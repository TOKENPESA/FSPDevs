#!/bin/bash
set -euo pipefail
TOKEN_FILE=/tmp/mfa_api_token.txt
API_TOKEN="$(cat "$TOKEN_FILE")"
if grep -q '^MFA_API_TOKEN=' /etc/fspdevs/treasury.env; then
  sed -i "s|^MFA_API_TOKEN=.*|MFA_API_TOKEN=${API_TOKEN}|" /etc/fspdevs/treasury.env
else
  echo "MFA_API_TOKEN=${API_TOKEN}" >> /etc/fspdevs/treasury.env
fi
systemctl restart fspdevs-treasury
sleep 5
systemctl is-active fspdevs-treasury
journalctl -u fspdevs-treasury -n 20 --no-pager
