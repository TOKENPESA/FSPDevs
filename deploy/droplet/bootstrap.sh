#!/usr/bin/env bash
# Bootstrap one DigitalOcean droplet as either MFA or Treasury Hub.
# Usage:
#   sudo FSPDEVS_ROLE=mfa bash bootstrap.sh
#   sudo FSPDEVS_ROLE=treasury MFA_HOST=1.2.3.4 bash bootstrap.sh
set -euo pipefail

REPO_URL="${FSPDEVS_REPO_URL:-https://github.com/TOKENPESA/FSPDevs.git}"
INSTALL_DIR="${FSPDEVS_INSTALL_DIR:-/opt/fspdevs}"
BRANCH="${FSPDEVS_BRANCH:-main}"
ROLE="${FSPDEVS_ROLE:-mfa}"

if [[ "$ROLE" != "mfa" && "$ROLE" != "treasury" ]]; then
  echo "FSPDEVS_ROLE must be 'mfa' or 'treasury' (got: $ROLE)"
  exit 1
fi

if [[ $EUID -ne 0 ]]; then
  echo "Run as root: sudo FSPDEVS_ROLE=$ROLE bash bootstrap.sh"
  exit 1
fi

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq ca-certificates curl git ufw openssl

if ! command -v docker >/dev/null 2>&1; then
  curl -fsSL https://get.docker.com | sh
fi

if ! docker compose version >/dev/null 2>&1; then
  apt-get install -y -qq docker-compose-plugin
fi

ufw --force reset
ufw default deny incoming
ufw default allow outgoing
ufw allow OpenSSH
ufw allow 80/tcp
ufw allow 443/tcp
ufw --force enable

mkdir -p "$INSTALL_DIR"
if [[ ! -d "$INSTALL_DIR/.git" ]]; then
  git clone --branch "$BRANCH" --depth 1 "$REPO_URL" "$INSTALL_DIR"
else
  git -C "$INSTALL_DIR" fetch origin "$BRANCH"
  git -C "$INSTALL_DIR" checkout "$BRANCH"
  git -C "$INSTALL_DIR" pull --ff-only origin "$BRANCH"
fi

cd "$INSTALL_DIR/deploy/droplet"

PUBLIC_IP="$(curl -fsS https://api.ipify.org || hostname -I | awk '{print $1}')"

if [[ "$ROLE" == "mfa" ]]; then
  COMPOSE_FILE="docker-compose.mfa.yml"
  if [[ ! -f .env ]]; then
    API_TOKEN="$(openssl rand -hex 24)"
    WS_TOKEN="$(openssl rand -hex 24)"
    sed "s|YOUR_MFA_DROPLET_IP|${PUBLIC_IP}|g" env.mfa.example > .env
    sed -i "s|REPLACE_MFA_API_TOKEN|${API_TOKEN}|" .env
    sed -i "s|REPLACE_MFA_WS_TOKEN|${WS_TOKEN}|" .env
    echo ""
    echo "Created MFA .env (save tokens securely):"
    grep -E '^(MFA_API_TOKEN|MFA_AGENT_WS_TOKEN|PUBLIC_ORIGIN|HUB_RPC_URL)=' .env
  fi
else
  COMPOSE_FILE="docker-compose.treasury.yml"
  if [[ -z "${MFA_HOST:-}" ]]; then
    echo "Treasury Hub requires MFA_HOST (e.g. MFA_HOST=167.99.150.153)"
    exit 1
  fi
  if [[ ! -f .env ]]; then
    WS_TOKEN="${MFA_AGENT_WS_TOKEN:-REPLACE_MFA_WS_TOKEN}"
    sed "s|YOUR_MFA_DROPLET_IP|${MFA_HOST}|g" env.treasury.example > .env
    # Normalize MFA_HOST line to host or host:port without requiring :1025 when using nginx :80
    sed -i "s|^MFA_HOST=.*|MFA_HOST=${MFA_HOST}|" .env
    if [[ "$WS_TOKEN" != "REPLACE_MFA_WS_TOKEN" ]]; then
      sed -i "s|REPLACE_MFA_WS_TOKEN|${WS_TOKEN}|" .env
    fi
    echo ""
    echo "Created Treasury Hub .env — set MFA_AGENT_WS_TOKEN to match the MFA droplet."
    grep -E '^(AGENT_ID|MFA_HOST|MFA_AGENT_WS_TOKEN)=' .env
  fi
fi

docker compose -f "$COMPOSE_FILE" build
docker compose -f "$COMPOSE_FILE" up -d

echo ""
echo "FSPDevs ${ROLE} droplet is starting on ${PUBLIC_IP}."
if [[ "$ROLE" == "mfa" ]]; then
  echo "  Console: http://${PUBLIC_IP}/mfa-console/"
  echo "  MFA API: http://${PUBLIC_IP}/"
  echo "  Remotes: point laptop sidecars + Treasury Hub to MFA_HOST=${PUBLIC_IP}"
else
  echo "  Hub API: http://${PUBLIC_IP}/"
  echo "  MFA:     ${MFA_HOST}"
fi
echo ""
echo "Logs: cd ${INSTALL_DIR}/deploy/droplet && docker compose -f ${COMPOSE_FILE} logs -f"
