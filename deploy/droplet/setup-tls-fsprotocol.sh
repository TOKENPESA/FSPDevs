#!/bin/bash
# Issue Let's Encrypt cert for fsprotocol.com MFA hostnames.
set -euo pipefail

EMAIL="${1:-admin@fsprotocol.com}"
DOMAINS=(fsprotocol.com www.fsprotocol.com mfa.fsprotocol.com)
PRIMARY=fsprotocol.com
NGINX_SITE=/etc/nginx/sites-available/fspdevs-mfa
NGINX_ENABLED=/etc/nginx/sites-enabled/fspdevs-mfa

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq certbot python3-certbot-nginx nginx

mkdir -p /var/www/html /etc/nginx/sites-available /etc/nginx/sites-enabled

# HTTP-only bootstrap covering all hostnames (ACME + proxy).
cat > "$NGINX_SITE" <<'EOF'
map $http_upgrade $connection_upgrade {
    default upgrade;
    ''      close;
}

server {
    listen 80 default_server;
    listen [::]:80 default_server;
    server_name fsprotocol.com www.fsprotocol.com mfa.fsprotocol.com;

    client_max_body_size 64k;

    location ^~ /.well-known/acme-challenge/ {
        root /var/www/html;
        default_type "text/plain";
    }

    location /mfa-console/ {
        alias /opt/fspdevs/mfa-console/;
        try_files $uri $uri/ /mfa-console/index.html;
    }

    location / {
        proxy_pass http://127.0.0.1:1025;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection $connection_upgrade;
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

CERTBOT_ARGS=()
for d in "${DOMAINS[@]}"; do
  CERTBOT_ARGS+=(-d "$d")
done

set +e
certbot --nginx \
  "${CERTBOT_ARGS[@]}" \
  --non-interactive \
  --agree-tos \
  -m "$EMAIL" \
  --redirect \
  --staple-ocsp \
  --keep-until-expiring
RC=$?
set -e

if [[ $RC -ne 0 ]]; then
  echo "Full SAN cert failed (RC=$RC); falling back to apex + www only…"
  certbot --nginx \
    -d fsprotocol.com \
    -d www.fsprotocol.com \
    --non-interactive \
    --agree-tos \
    -m "$EMAIL" \
    --redirect \
    --staple-ocsp \
    --keep-until-expiring
fi

# Harden TLS if needed
python3 - <<'PY'
from pathlib import Path
site = Path("/etc/nginx/sites-available/fspdevs-mfa")
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
if "ssl_certificate" in text and "ssl_protocols TLSv1.2 TLSv1.3" not in text:
    idx = text.find("ssl_certificate_key")
    if idx != -1:
        end = text.find(";", idx)
        if end != -1:
            site.write_text(text[: end + 1] + harden + text[end + 1 :])
            print("TLS harden applied")
else:
    print("TLS harden already present or pending cert")
PY

nginx -t
systemctl reload nginx

# Point MFA public origin / WS allowlist at HTTPS domains
ENV=/etc/fspdevs/mfa.env
if [[ -f "$ENV" ]]; then
  touch "$ENV"
  chmod 600 "$ENV"
  for key in PUBLIC_ORIGIN MFA_WS_ALLOWED_ORIGINS; do
    sed -i "/^${key}=/d" "$ENV"
  done
  cat >> "$ENV" <<EOF
PUBLIC_ORIGIN=https://mfa.fsprotocol.com
MFA_WS_ALLOWED_ORIGINS=https://mfa.fsprotocol.com,https://fsprotocol.com,https://www.fsprotocol.com,http://127.0.0.1:8088
EOF
  systemctl restart fspdevs-mfa || true
  sleep 3
  systemctl is-active fspdevs-mfa || true
fi

echo
echo "TLS done. Probe:"
curl -fsS -m 15 "https://${PRIMARY}/" | head -c 120 || true
echo
curl -k -fsS -m 10 -o /dev/null -w "mfa.https=%{http_code}\n" "https://mfa.fsprotocol.com/" || true
echo FSPROTOCOL_TLS_DONE
