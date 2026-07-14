#!/bin/bash
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get install -y -qq sqlite3 >/dev/null
DB=/opt/fspdevs/data/mfa-supervisor.db
echo "DB=$DB"
sqlite3 "$DB" "SELECT agent_id, high_water_mark FROM agent_telemetry_nonces;"
sqlite3 "$DB" "DELETE FROM agent_telemetry_nonces WHERE agent_id = 1;"
echo "cleared agent 1"
sqlite3 "$DB" "SELECT agent_id, high_water_mark FROM agent_telemetry_nonces;"
systemctl restart fspdevs-mfa
sleep 5
journalctl -u fspdevs-mfa --since '10 seconds ago' --no-pager | grep -E 'nonce|rate limit|operational' || true
echo MFA_NONCE_CLEARED
