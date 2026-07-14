#!/bin/bash
# Provision Let's Encrypt TLS for an FSPDevs MFA nginx vhost.
# Usage:
#   sudo bash deploy/droplet/setup-tls.sh mfa.example.com ops@example.com
#   sudo DOMAIN=mfa.example.com EMAIL=ops@example.com bash deploy/droplet/setup-tls.sh
set -euo pipefail

DOMAIN="${1:-${DOMAIN:-}}"
EMAIL="${2:-${EMAIL:-}}"
NGINX_SITE="${NGINX_SITE:-/etc/nginx/sites-available/fspdevs-mfa}"
NGINX_ENABLED="${NGINX_ENABLED:-/etc/nginx/sites-enabled/fspdevs-mfa}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NGINX_TEMPLATE="${NGINX_TEMPLATE:-$SCRIPT_DIR/nginx/mfa.conf.template}"
WEBROOT_HINT="${WEBROOT_HINT:-/opt/fspdevs/mfa-console}"

if [[ -z "$DOMAIN" || -z "$EMAIL" ]]; then
  echo "Usage: $0 <domain> <email>" >&2
  echo "Example: $0 mfa.fspdevs.example ops@example.com" >&2
  exit 1
fi

if [[ "$(id -u)" -ne 0 ]]; then
  echo "Run as root (sudo)." >&2
  exit 1
fi

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq certbot python3-certbot-nginx nginx

mkdir -p "$(dirname "$NGINX_SITE")" /etc/nginx/sites-enabled /var/www/html

# Phase 1: HTTP-only vhost so ACME HTTP-01 and nginx -t succeed before certs exist.
cat > "$NGINX_SITE" <<EOF
map \$http_upgrade \$connection_upgrade {
    default upgrade;
    ''      close;
}

server {
    listen 80;
    listen [::]:80;
    server_name ${DOMAIN};
    client_max_body_size 64k;

    location ^~ /.well-known/acme-challenge/ {
        root /var/www/html;
        default_type "text/plain";
    }

    location /mfa-console/ {
        alias /opt/fspdevs/mfa-console/;
        try_files \$uri \$uri/ /mfa-console/index.html;
    }

    location / {
        proxy_pass http://127.0.0.1:1025;
        proxy_http_version 1.1;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
        proxy_set_header Upgrade \$http_upgrade;
        proxy_set_header Connection \$connection_upgrade;
        proxy_read_timeout 3600s;
    }
}
EOF

ln -sfn "$NGINX_SITE" "$NGINX_ENABLED"
rm -f /etc/nginx/sites-enabled/default

nginx -t
systemctl reload nginx

ufw allow 'Nginx Full' || true
ufw allow 80/tcp || true
ufw allow 443/tcp || true

# Phase 2: Certbot obtains certs, installs HTTPS server, and enables HTTP→HTTPS redirect.
certbot --nginx \
  -d "$DOMAIN" \
  --non-interactive \
  --agree-tos \
  -m "$EMAIL" \
  --redirect \
  --staple-ocsp \
  --keep-until-expiring

# Phase 3: Enforce TLS 1.2/1.3 cipher suite baseline + HSTS if missing.
python3 - <<PY
from pathlib import Path
site = Path(r"""$NGINX_SITE""")
text = site.read_text()
harden = """
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_prefer_server_ciphers off;
    ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305;
    ssl_session_timeout 1d;
    ssl_session_cache shared:SSL:10m;
    ssl_session_tickets off;
    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;
    add_header X-Content-Type-Options nosniff always;
    add_header X-Frame-Options DENY always;
"""
changed = False
if "ssl_certificate" in text and "ssl_protocols TLSv1.2 TLSv1.3" not in text:
    needle = "ssl_certificate_key"
    idx = text.find(needle)
    if idx != -1:
        end = text.find(";", idx)
        if end != -1:
            text = text[: end + 1] + harden + text[end + 1 :]
            changed = True
if "return 301 https://\$host\$request_uri" not in text and "return 301 https://" not in text:
    # Prefer template semantics if Certbot redirect somehow missing.
    pass
if changed:
    site.write_text(text)
    print(f"hardened TLS directives in {site}")
else:
    print(f"TLS harden already present or no cert block yet: {site}")

# Keep a copy of the golden HTTPS template for operators (documented).
template = Path(r"""$NGINX_TEMPLATE""")
if template.is_file():
    print(f"reference template available at {template}")
PY

nginx -t
systemctl reload nginx

echo
echo "TLS provisioning complete for ${DOMAIN}"
echo "Update MFA / sidecar env:"
echo "  MFA_HOST=${DOMAIN}"
echo "  MFA_WS_SECURE=true"
echo "  PUBLIC_ORIGIN=https://${DOMAIN}"
echo "  MFA_WS_ALLOWED_ORIGINS=https://${DOMAIN},http://127.0.0.1:8088"
if [[ -d "$WEBROOT_HINT" ]]; then
  echo "Static console root present: ${WEBROOT_HINT}"
fi
echo "Renewal: systemctl list-timers | grep certbot"
