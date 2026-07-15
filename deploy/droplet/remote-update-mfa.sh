#!/bin/bash
# Sync'd from laptop — rebuild + restart MFA with discoverable_agents.
set -euo pipefail
source /root/.cargo/env 2>/dev/null || true
cd /opt/fspdevs

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
echo "Building master_fiber_agent…"
cargo build --release -p master_fiber_agent

BIN_SRC="target/release/master_fiber_agent"
BIN_DST="/opt/fspdevs/target/release/master_fiber_agent"
if [[ "$(readlink -f "$BIN_SRC" 2>/dev/null || realpath "$BIN_SRC")" != "$(readlink -f "$BIN_DST" 2>/dev/null || realpath "$BIN_DST")" ]]; then
  install -m 0755 "$BIN_SRC" "$BIN_DST"
else
  chmod 0755 "$BIN_DST"
fi

systemctl daemon-reload
systemctl restart fspdevs-mfa
sleep 5
systemctl is-active fspdevs-mfa
curl -fsS -m 8 http://127.0.0.1:1025/ | python3 -c 'import sys,json; d=json.load(sys.stdin); print("discoverable_agents", d.get("discoverable_agents")); print("connected_agent_ids", d.get("connected_agent_ids"))'
echo MFA_UPDATE_DONE
