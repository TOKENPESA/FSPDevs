#!/bin/bash
# Apply Phase A hardening on Treasury Hub after sources are synced to /opt/fspdevs.
set -euo pipefail
source /root/.cargo/env 2>/dev/null || true
cd /opt/fspdevs

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
echo "Building fiber-agent-daemon (Phase A)…"
cargo build --release -p fiber_agent_sidecar --bin fiber-agent-daemon

BIN_SRC="target/release/fiber-agent-daemon"
BIN_DST="/opt/fspdevs/target/release/fiber-agent-daemon"
if [[ "$(readlink -f "$BIN_SRC")" != "$(readlink -f "$BIN_DST")" ]]; then
  install -m 0755 "$BIN_SRC" "$BIN_DST"
else
  chmod 0755 "$BIN_DST"
  echo "Binary already at $BIN_DST"
fi

mkdir -p /etc/fspdevs
touch /etc/fspdevs/treasury.env
chmod 600 /etc/fspdevs/treasury.env

if ! grep -q '^FIBER_AGENT_API_TOKEN=' /etc/fspdevs/treasury.env; then
  TOKEN="$(openssl rand -hex 24)"
  echo "FIBER_AGENT_API_TOKEN=${TOKEN}" >> /etc/fspdevs/treasury.env
  echo "Generated FIBER_AGENT_API_TOKEN"
else
  echo "FIBER_AGENT_API_TOKEN already set"
fi

if grep -q '^FIBER_AGENT_BIND_ADDR=' /etc/fspdevs/treasury.env; then
  sed -i 's|^FIBER_AGENT_BIND_ADDR=.*|FIBER_AGENT_BIND_ADDR=127.0.0.1:19444|' /etc/fspdevs/treasury.env
else
  echo 'FIBER_AGENT_BIND_ADDR=127.0.0.1:19444' >> /etc/fspdevs/treasury.env
fi

systemctl daemon-reload
systemctl restart fspdevs-treasury
sleep 6
systemctl is-active fspdevs-treasury
journalctl -u fspdevs-treasury -n 40 --no-pager
echo PHASE_A_HUB_DONE
