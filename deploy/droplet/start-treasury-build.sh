#!/bin/bash
set -euo pipefail
source /root/.cargo/env
cd /opt/fspdevs
export CARGO_BUILD_JOBS=1
: > /tmp/treasury-build.log
nohup cargo build --release -p fiber_agent_sidecar --bin fiber-agent-daemon >/tmp/treasury-build.log 2>&1 </dev/null &
echo $! > /tmp/treasury-build.pid
sleep 12
echo "PID=$(cat /tmp/treasury-build.pid)"
tail -n 30 /tmp/treasury-build.log
free -h
