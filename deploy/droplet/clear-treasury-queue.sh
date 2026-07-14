#!/bin/bash
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get install -y -qq sqlite3 >/dev/null
mapfile -t DBS < <(find /root/.fiber-agent /opt/fspdevs -name '*.db' 2>/dev/null || true)
echo "DBS=${#DBS[@]}"
for db in "${DBS[@]:-}"; do
  echo "clearing $db"
  sqlite3 "$db" 'DELETE FROM offline_telemetry_queue;' || true
done
systemctl restart fspdevs-treasury
sleep 20
journalctl -u fspdevs-treasury --since '25 seconds ago' --no-pager
systemctl is-active fspdevs-treasury
