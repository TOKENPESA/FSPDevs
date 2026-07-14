#!/bin/bash
set -euo pipefail
KEY="$(openssl rand -hex 32)"
if grep -q '^FSP_AGENT_SECRET_KEY=' /etc/fspdevs/treasury.env; then
  sed -i "s|^FSP_AGENT_SECRET_KEY=.*|FSP_AGENT_SECRET_KEY=${KEY}|" /etc/fspdevs/treasury.env
else
  echo "FSP_AGENT_SECRET_KEY=${KEY}" >> /etc/fspdevs/treasury.env
fi
sed -i 's/^FIBER_AGENT_ALLOW_DEV_KEYS=.*/FIBER_AGENT_ALLOW_DEV_KEYS=false/' /etc/fspdevs/treasury.env
systemctl enable fspdevs-treasury
systemctl restart fspdevs-treasury
sleep 4
systemctl is-active fspdevs-treasury
systemctl status fspdevs-treasury --no-pager -l | head -n 35
echo ---
curl -fsS -m 5 -o /dev/null -w 'catalog:%{http_code}\n' http://127.0.0.1:19444/api/modules/catalog || true
curl -fsS -m 5 -o /dev/null -w 'nginx:%{http_code}\n' http://127.0.0.1/api/modules/catalog || true
journalctl -u fspdevs-treasury -n 40 --no-pager
