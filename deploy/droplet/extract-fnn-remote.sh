#!/bin/bash
# One-shot extract using already-downloaded portable archive on Hub.
set -euo pipefail
FNN_DIR=/opt/fnn
ARCHIVE=/tmp/fnn_v0.8.0-x86_64-linux-portable.tar.gz

mkdir -p "$FNN_DIR" "$FNN_DIR/ckb" /var/log/fnn /etc/fspdevs
tar -xzf "$ARCHIVE" -C "$FNN_DIR"
chmod +x "$FNN_DIR"/fnn "$FNN_DIR"/fnn-cli "$FNN_DIR"/fnn-migrate
ls -la "$FNN_DIR"/fnn*
"$FNN_DIR/fnn" --version || true

if [[ ! -f "$FNN_DIR/ckb/key" ]]; then
  openssl rand -hex 32 > "$FNN_DIR/ckb/key"
  chmod 600 "$FNN_DIR/ckb/key"
fi

if [[ ! -f /etc/fspdevs/fnn.env ]]; then
  PW="$(openssl rand -hex 16)"
  cat > /etc/fspdevs/fnn.env <<EOF
FIBER_SECRET_KEY_PASSWORD=${PW}
RUST_LOG=info
EOF
  chmod 600 /etc/fspdevs/fnn.env
fi

echo "extract-fnn-remote: OK"
