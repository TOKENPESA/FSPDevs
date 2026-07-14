#!/bin/bash
set -uo pipefail
FNN=/opt/fnn/fnn-cli
RPC=http://127.0.0.1:18227
PEER=034c662ff2cb6c290c50d31df4e8640dba489f73dfdeb43dd1faede96021505381
DEST=0313dcf9cf18711b1b473a78ea56222dc44dcbfdf559d24dd937a0657d3bcb108f
HOPS=$(printf '["%s"]' "$PEER")
echo "hops=$HOPS"
OUT=$($FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$DEST" \
  --amount 50000000 \
  --keysend true \
  --trampoline-hops "$HOPS" \
  --final-tlc-expiry-delta 14400000 \
  --max-fee-amount 1005000 \
  --timeout 90 2>&1) || true
echo "$OUT"
HASH=$(echo "$OUT" | python3 -c 'import sys,json; 
raw=sys.stdin.read()
# find last JSON object
start=raw.rfind("{")
print(json.loads(raw[start:]).get("payment_hash","" ) if start>=0 else "")' 2>/dev/null || true)
echo "HASH=$HASH"
sleep 12
if [[ -n "$HASH" ]]; then
  $FNN -u "$RPC" --raw-data payment get_payment --payment-hash "$HASH"
fi
$FNN -u "$RPC" channel list_channels
