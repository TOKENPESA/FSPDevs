#!/bin/bash
# After intentional FA restart (FA nonce resets to 0), reset MFA HWM for that agent
# so authenticated telemetry can resume. Persistence still protects MFA-only restarts.
set -euo pipefail
AGENT_ID="${1:-1}"
DB="${MFA_SUPERVISOR_DB_PATH:-/opt/fspdevs/data/mfa-supervisor.db}"
# Common local paths
for candidate in "$DB" /opt/fspdevs/.mfa-supervisor/mfa-supervisor.db /root/.mfa-supervisor/mfa-supervisor.db; do
  if [[ -f "$candidate" ]]; then
    DB="$candidate"
    break
  fi
done
echo "Using DB: $DB"
sqlite3 "$DB" "DELETE FROM agent_telemetry_nonces WHERE agent_id = ${AGENT_ID};"
echo "Cleared MFA HWM for FA-${AGENT_ID}"
systemctl restart fspdevs-mfa
sleep 4
systemctl is-active fspdevs-mfa
journalctl -u fspdevs-mfa --since '10 seconds ago' --no-pager | grep -E 'nonce|rate limit|operational' || true
echo NONCE_RESET_DONE
