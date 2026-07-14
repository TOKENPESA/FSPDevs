#!/bin/bash
set -euo pipefail
source /root/.cargo/env
rustup toolchain install stable
rustup default stable
rustc --version
sed -i 's/\r$//' /opt/fspdevs/Cargo.toml
cd /opt/fspdevs
export CARGO_BUILD_JOBS=1
pkill -f 'cargo build --release -p master_fiber_agent' 2>/dev/null || true
sleep 1
: > /tmp/mfa-build.log
nohup cargo build --release -p master_fiber_agent >/tmp/mfa-build.log 2>&1 </dev/null &
echo $! > /tmp/mfa-build.pid
sleep 15
echo "PID=$(cat /tmp/mfa-build.pid)"
tail -n 50 /tmp/mfa-build.log
free -h
