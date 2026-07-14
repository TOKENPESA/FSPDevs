#!/bin/bash
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive

# Swap for release builds
if ! swapon --show | grep -q /swapfile; then
  fallocate -l 2G /swapfile || dd if=/dev/zero of=/swapfile bs=1M count=2048
  chmod 600 /swapfile
  mkswap /swapfile
  swapon /swapfile
  grep -q '/swapfile' /etc/fstab || echo '/swapfile none swap sw 0 0' >> /etc/fstab
fi

apt-get update -qq
apt-get install -y -qq ca-certificates curl git ufw nginx openssl pkg-config libssl-dev build-essential clang make

if [[ ! -f /root/.cargo/env ]]; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
# shellcheck disable=SC1091
source /root/.cargo/env
rustup default stable
rustc --version

mkdir -p /opt/fspdevs /opt/fspdevs/data /etc/fspdevs /root/.fiber-agent
tar -xzf /tmp/fspdevs-treasury-src.tgz -C /opt/fspdevs
cp /opt/fspdevs/deploy/droplet/Cargo.workspace.treasury.toml /opt/fspdevs/Cargo.toml
sed -i 's/\r$//' /opt/fspdevs/Cargo.toml

# mesh pubkeys placeholder for testnet
mkdir -p /opt/fspdevs/fnn-testnet
echo '{}' > /opt/fspdevs/fnn-testnet/mesh-pubkeys.json

# Treasury env → MFA for testnet
cat > /etc/fspdevs/treasury.env <<'EOF'
AGENT_ID=1
FIBER_AGENT_BIND_ADDR=0.0.0.0:19444
MFA_HOST=167.99.150.153
MFA_AGENT_WS_TOKEN=0d189246b71905072034893ce65ec20a92d3d3b751dde677
MESH_PUBKEY_REGISTRY_PATH=/opt/fspdevs/fnn-testnet/mesh-pubkeys.json
FIBER_AGENT_ALLOW_DEV_KEYS=false
FNN_MODE=simulate
FIBER_AGENT_STATE_DIR=/root/.fiber-agent
EOF
chmod 600 /etc/fspdevs/treasury.env

# nginx + systemd + ufw
cp /opt/fspdevs/deploy/droplet/nginx/treasury-host.conf /etc/nginx/sites-available/fspdevs-treasury
sed -i 's/\r$//' /etc/nginx/sites-available/fspdevs-treasury
ln -sfn /etc/nginx/sites-available/fspdevs-treasury /etc/nginx/sites-enabled/fspdevs-treasury
rm -f /etc/nginx/sites-enabled/default
nginx -t
systemctl enable nginx
systemctl restart nginx

cp /opt/fspdevs/deploy/droplet/systemd/fspdevs-treasury.service /etc/systemd/system/fspdevs-treasury.service
sed -i 's/\r$//' /etc/systemd/system/fspdevs-treasury.service
systemctl daemon-reload

ufw --force reset
ufw default deny incoming
ufw default allow outgoing
ufw allow OpenSSH
ufw allow 80/tcp
ufw allow 443/tcp
ufw --force enable

echo PROVISION_OK
free -h
