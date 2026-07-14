#!/bin/bash
set -uo pipefail
FNN=/opt/fnn/fnn-cli
RPC=http://127.0.0.1:18227
NEWPEER=0313dcf9cf18711b1b473a78ea56222dc44dcbfdf559d24dd937a0657d3bcb108f
EXISTING=034c662ff2cb6c290c50d31df4e8640dba489f73dfdeb43dd1faede96021505381
THIRD=0389647832dd45fbfefa803a23b05fc7d6fb9a72569025329892d71d332a1c3678
AMOUNT=50000000
# Fiber recommended maximal_fee=502500 for prior attempt
MAX_FEE=502500
HOPS=$(printf '["%s"]' "$NEWPEER")

echo "=== channels ==="
$FNN -u "$RPC" channel list_channels

echo "=== LIVE trampoline NEWPEER -> THIRD fee=$MAX_FEE ==="
LIVE=$($FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$THIRD" \
  --amount "$AMOUNT" \
  --keysend true \
  --trampoline-hops "$HOPS" \
  --final-tlc-expiry-delta 14400000 \
  --max-fee-amount "$MAX_FEE" \
  --timeout 120 2>&1) || true
echo "$LIVE"
PH=$(echo "$LIVE" | python3 -c 'import sys,json
raw=sys.stdin.read(); i=raw.rfind("{")
print(json.loads(raw[i:]).get("payment_hash","") if i>=0 else "")' 2>/dev/null || true)

if [[ -z "$PH" ]]; then
  echo "=== alt: trampoline NEWPEER -> EXISTING ==="
  LIVE=$($FNN -u "$RPC" --raw-data payment send_payment \
    --target-pubkey "$EXISTING" \
    --amount "$AMOUNT" \
    --keysend true \
    --trampoline-hops "$HOPS" \
    --final-tlc-expiry-delta 14400000 \
    --max-fee-amount "$MAX_FEE" \
    --timeout 120 2>&1) || true
  echo "$LIVE"
  PH=$(echo "$LIVE" | python3 -c 'import sys,json
raw=sys.stdin.read(); i=raw.rfind("{")
print(json.loads(raw[i:]).get("payment_hash","") if i>=0 else "")' 2>/dev/null || true)
fi

if [[ -z "$PH" ]]; then
  echo "=== alt2: pathfind Hub->EXISTING via NEWPEER trampoline with higher fee ==="
  MAX_FEE=2000000
  LIVE=$($FNN -u "$RPC" --raw-data payment send_payment \
    --target-pubkey "$THIRD" \
    --amount "$AMOUNT" \
    --keysend true \
    --trampoline-hops "$HOPS" \
    --final-tlc-expiry-delta 14400000 \
    --max-fee-amount "$MAX_FEE" \
    --timeout 120 2>&1) || true
  echo "$LIVE"
  PH=$(echo "$LIVE" | python3 -c 'import sys,json
raw=sys.stdin.read(); i=raw.rfind("{")
print(json.loads(raw[i:]).get("payment_hash","") if i>=0 else "")' 2>/dev/null || true)
fi

echo "PAYMENT_HASH=$PH"
for i in $(seq 1 30); do
  [[ -z "$PH" ]] && break
  STAT=$($FNN -u "$RPC" --raw-data payment get_payment --payment-hash "$PH" 2>/dev/null || echo '{}')
  echo "poll $i $STAT"
  S=$(echo "$STAT" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("status",""))' 2>/dev/null || true)
  [[ "$S" == "Success" || "$S" == "Failed" ]] && break
  sleep 4
done

$FNN -u "$RPC" channel list_channels
$FNN -u "$RPC" payment list_payments --limit 5
echo DONE
