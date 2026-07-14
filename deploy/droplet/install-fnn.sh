#!/bin/bash
# Install Nervos Fiber Network Node (FNN) on Treasury Hub for CKB testnet.
set -euo pipefail

FNN_VERSION="${FNN_VERSION:-v0.8.0}"
FNN_DIR="${FNN_DIR:-/opt/fnn}"
PUBLIC_IP="${PUBLIC_IP:-134.122.120.65}"
ARCHIVE="fnn_${FNN_VERSION}-x86_64-linux-portable.tar.gz"
URL="https://github.com/nervosnetwork/fiber/releases/download/${FNN_VERSION}/${ARCHIVE}"

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq curl ca-certificates tar openssl

mkdir -p "$FNN_DIR" "$FNN_DIR/ckb" /var/log/fnn
cd /tmp
curl -fsSL -o "$ARCHIVE" "$URL"
# Portable archive has binaries at root (fnn, fnn-cli) + config/ — do not strip.
tar -xzf "$ARCHIVE" -C "$FNN_DIR"
chmod +x "$FNN_DIR"/fnn "$FNN_DIR"/fnn-cli "$FNN_DIR"/fnn-migrate 2>/dev/null || true
ls -la "$FNN_DIR"/fnn*

# Dev CKB wallet key (fund from testnet faucet for live channels)
if [[ ! -f "$FNN_DIR/ckb/key" ]]; then
  openssl rand -hex 32 > "$FNN_DIR/ckb/key"
  chmod 600 "$FNN_DIR/ckb/key"
fi

# Password for encrypted key store on first start
if [[ ! -f /etc/fspdevs/fnn.env ]]; then
  mkdir -p /etc/fspdevs
  PW="$(openssl rand -hex 16)"
  cat > /etc/fspdevs/fnn.env <<EOF
FIBER_SECRET_KEY_PASSWORD=${PW}
RUST_LOG=info
EOF
  chmod 600 /etc/fspdevs/fnn.env
fi

echo "FNN binaries installed under $FNN_DIR"
"$FNN_DIR/fnn" --version || true
