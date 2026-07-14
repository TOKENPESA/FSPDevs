#!/bin/bash
set -euo pipefail
ENV=/etc/fspdevs/treasury.env
touch "$ENV"
chmod 600 "$ENV"
sed -i '/^MFA_HOST=/d' "$ENV"
sed -i '/^MFA_WS_SECURE=/d' "$ENV"
echo 'MFA_HOST=mfa.fsprotocol.com' >> "$ENV"
echo 'MFA_WS_SECURE=true' >> "$ENV"
systemctl stop fspdevs-treasury
sqlite3 /root/.fiber-agent/fa-1.db 'DELETE FROM offline_telemetry_queue;' 2>/dev/null || true
grep -E '^MFA_HOST=|^MFA_WS_SECURE=' "$ENV"
systemctl start fspdevs-treasury
sleep 8
systemctl is-active fspdevs-treasury
journalctl -u fspdevs-treasury --since '20 seconds ago' --no-pager | tail -n 25
echo HUB_HTTPS_MFA_DONE
