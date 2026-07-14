#!/bin/bash
set -uo pipefail
FNN=/opt/fnn/fnn-cli
RPC=http://127.0.0.1:18227
NEW=0313dcf9cf18711b1b473a78ea56222dc44dcbfdf559d24dd937a0657d3bcb108f
OLD=034c662ff2cb6c290c50d31df4e8640dba489f73dfdeb43dd1faede96021505381
THIRD=0389647832dd45fbfefa803a23b05fc7d6fb9a72569025329892d71d332a1c3678
AMOUNT=10000000
MAX_FEE=2000000

echo "=== channels / peers ==="
$FNN -u "$RPC" channel list_channels
$FNN -u "$RPC" info node_info | head -n 30

poll_hash() {
  local PH="$1"
  local LABEL="$2"
  [[ -z "$PH" ]] && return 1
  for i in $(seq 1 20); do
    S=$($FNN -u "$RPC" --raw-data payment get_payment --payment-hash "$PH" 2>/dev/null || echo '{}')
    ST=$(echo "$S" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("status",""))' 2>/dev/null || true)
    ERR=$(echo "$S" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("failed_error") or "")' 2>/dev/null || true)
    echo "$LABEL poll $i status=$ST err=$ERR"
    if [[ "$ST" == "Success" || "$ST" == "Failed" ]]; then
      echo "$S"
      return 0
    fi
    sleep 3
  done
}

try_trampoline() {
  local DEST="$1"
  local HOP="$2"
  local LABEL="$3"
  local HOPS
  HOPS=$(printf '["%s"]' "$HOP")
  echo "=== $LABEL ==="
  echo "target=$DEST trampoline=$HOPS amount=$AMOUNT max_fee=$MAX_FEE"
  OUT=$($FNN -u "$RPC" --raw-data payment send_payment \
    --target-pubkey "$DEST" \
    --amount "$AMOUNT" \
    --keysend true \
    --trampoline-hops "$HOPS" \
    --final-tlc-expiry-delta 14400000 \
    --max-fee-amount "$MAX_FEE" \
    --timeout 120 2>&1) || true
  echo "$OUT"
  PH=$(echo "$OUT" | python3 -c 'import sys,json,re
raw=sys.stdin.read(); i=raw.rfind("{")
print(json.loads(raw[i:]).get("payment_hash","") if i>=0 else "")' 2>/dev/null || true)
  echo "PAYMENT_HASH=$PH"
  poll_hash "$PH" "$LABEL"
}

echo "=== dry_run pathfind THIRD ==="
$FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$THIRD" --amount "$AMOUNT" --keysend true \
  --max-fee-amount "$MAX_FEE" --dry-run true 2>&1 || true

echo "=== dry_run trampoline NEW->THIRD ==="
HOPS=$(printf '["%s"]' "$NEW")
$FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$THIRD" --amount "$AMOUNT" --keysend true \
  --trampoline-hops "$HOPS" --final-tlc-expiry-delta 14400000 \
  --max-fee-amount "$MAX_FEE" --dry-run true 2>&1 || true

echo "=== dry_run trampoline OLD->THIRD ==="
HOPS=$(printf '["%s"]' "$OLD")
$FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$THIRD" --amount "$AMOUNT" --keysend true \
  --trampoline-hops "$HOPS" --final-tlc-expiry-delta 14400000 \
  --max-fee-amount "$MAX_FEE" --dry-run true 2>&1 || true

echo "=== dry_run trampoline NEW->OLD ==="
$FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$OLD" --amount "$AMOUNT" --keysend true \
  --trampoline-hops "$(printf '["%s"]' "$NEW")" --final-tlc-expiry-delta 14400000 \
  --max-fee-amount "$MAX_FEE" --dry-run true 2>&1 || true

echo "=== dry_run trampoline OLD->NEW ==="
$FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$NEW" --amount "$AMOUNT" --keysend true \
  --trampoline-hops "$(printf '["%s"]' "$OLD")" --final-tlc-expiry-delta 14400000 \
  --max-fee-amount "$MAX_FEE" --dry-run true 2>&1 || true

# Live: try whichever dry_runs look promising; always attempt NEW->THIRD and OLD->THIRD
try_trampoline "$THIRD" "$NEW" "LIVE NEW->THIRD"
try_trampoline "$THIRD" "$OLD" "LIVE OLD->THIRD"
try_trampoline "$OLD" "$NEW" "LIVE NEW->OLD"
try_trampoline "$NEW" "$OLD" "LIVE OLD->NEW"

echo "=== recent payments ==="
$FNN -u "$RPC" payment list_payments --limit 8
echo "=== channels final ==="
$FNN -u "$RPC" channel list_channels
echo DONE
