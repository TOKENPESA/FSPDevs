#!/bin/bash
set -euo pipefail
# Run on Treasury Hub; also restart MFA remotely if MFA_HOST given as $1.
systemctl stop fspdevs-treasury
sleep 1
sqlite3 /root/.fiber-agent/fa-1.db 'DELETE FROM offline_telemetry_queue;' 2>/dev/null || true
find /root/.fiber-agent /opt/fspdevs -name '*.db' 2>/dev/null | while read -r db; do
  sqlite3 "$db" 'DELETE FROM offline_telemetry_queue;' 2>/dev/null || true
done
echo "hub queue cleared"
