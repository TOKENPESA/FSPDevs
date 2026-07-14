#!/bin/bash
set -euo pipefail
systemctl stop fspdevs-treasury
sleep 1
sqlite3 /root/.fiber-agent/fa-1.db 'DELETE FROM offline_telemetry_queue;'
echo "queue cleared"
