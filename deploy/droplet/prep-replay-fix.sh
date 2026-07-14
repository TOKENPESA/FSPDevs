#!/bin/bash
set -euo pipefail
# Clear FA-1 telemetry replay watermark + offline queue.
systemctl stop fspdevs-treasury
# Find and wipe offline telemetry queue if present
find /opt/fspdevs /var/lib/fspdevs /root -name '*offline_telemetry*' 2>/dev/null | head
# Agent data dir often under working directory
for d in /opt/fspdevs /opt/fspdevs/data /var/lib/fiber-agent /root/.fiber-agent; do
  if [[ -d "$d" ]]; then
    find "$d" -iname '*telemetry*' 2>/dev/null | head -n 20
  fi
done
echo FIX_PREP_DONE
