#!/bin/bash
set -euo pipefail
SITE=/etc/nginx/sites-available/fspdevs-mfa

python3 - <<'PY'
from pathlib import Path
import re
p = Path("/etc/nginx/sites-available/fspdevs-mfa")
text = p.read_text()
# Drop ssl_* keys that duplicate /etc/letsencrypt/options-ssl-nginx.conf
for key in (
    "ssl_protocols",
    "ssl_prefer_server_ciphers",
    "ssl_ciphers",
    "ssl_session_timeout",
    "ssl_session_cache",
    "ssl_session_tickets",
):
    text = re.sub(rf"(?m)^\s*{key}\s+[^;]+;\s*\n", "", text)
if "Strict-Transport-Security" not in text:
    text = text.replace(
        "include /etc/letsencrypt/options-ssl-nginx.conf;",
        "include /etc/letsencrypt/options-ssl-nginx.conf;\n"
        '    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;\n'
        "    add_header X-Content-Type-Options nosniff always;\n"
        "    add_header X-Frame-Options DENY always;",
        1,
    )
p.write_text(text)
print("rewrote", p)
PY

nginx -t
systemctl reload nginx

ENV=/etc/fspdevs/mfa.env
mkdir -p /etc/fspdevs
touch "$ENV"
chmod 600 "$ENV"
sed -i '/^PUBLIC_ORIGIN=/d' "$ENV"
sed -i '/^MFA_WS_ALLOWED_ORIGINS=/d' "$ENV"
cat >> "$ENV" <<'EOF'
PUBLIC_ORIGIN=https://mfa.fsprotocol.com
MFA_WS_ALLOWED_ORIGINS=https://mfa.fsprotocol.com,https://fsprotocol.com,https://www.fsprotocol.com,http://127.0.0.1:8088
EOF

systemctl restart fspdevs-mfa
sleep 3
systemctl is-active nginx
systemctl is-active fspdevs-mfa

curl -fsS -m 15 -o /dev/null -w "fsprotocol=%{http_code}\n" https://fsprotocol.com/
curl -fsS -m 15 -o /dev/null -w "mfa=%{http_code}\n" https://mfa.fsprotocol.com/
curl -fsS -m 15 -o /dev/null -w "www=%{http_code}\n" https://www.fsprotocol.com/
echo TLS_FIX_DONE
