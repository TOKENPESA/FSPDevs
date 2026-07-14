#!/bin/bash
# Build/restart MFA with Phase B (nonce persistence + rate limits).
set -euo pipefail
source /root/.cargo/env 2>/dev/null || true
cd /opt/fspdevs

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
echo "Building master_fiber_agent (Phase B)…"
cargo build --release -p master_fiber_agent

BIN_SRC="target/release/master_fiber_agent"
BIN_DST="/opt/fspdevs/target/release/master_fiber_agent"
if [[ "$(readlink -f "$BIN_SRC" 2>/dev/null || realpath "$BIN_SRC")" != "$(readlink -f "$BIN_DST" 2>/dev/null || realpath "$BIN_DST")" ]]; then
  install -m 0755 "$BIN_SRC" "$BIN_DST"
else
  chmod 0755 "$BIN_DST"
  echo "Binary already at $BIN_DST"
fi

# Ensure SmartIp rate limit (nginx) is the default; unset peer-ip if present.
ENV=/etc/fspdevs/mfa.env
mkdir -p /etc/fspdevs
touch "$ENV"
chmod 600 "$ENV"
sed -i '/^MFA_RATE_LIMIT_PEER_IP=/d' "$ENV"

systemctl daemon-reload
systemctl restart fspdevs-mfa
sleep 5
systemctl is-active fspdevs-mfa
journalctl -u fspdevs-mfa -n 50 --no-pager | tail -n 40
echo PHASE_B_MFA_DONE
