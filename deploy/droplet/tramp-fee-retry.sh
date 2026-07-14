#!/bin/bash
set -uo pipefail
FNN=/opt/fnn/fnn-cli
RPC=http://127.0.0.1:18227
OLD=034c662ff2cb6c290c50d31df4e8640dba489f73dfdeb43dd1faede96021505381
THIRD=0389647832dd45fbfefa803a23b05fc7d6fb9a72569025329892d71d332a1c3678
HOPS=$(printf '["%s"]' "$OLD")

for FEE in 100500 10050 200000 500000; do
  echo "=== LIVE OLD->THIRD max_fee=$FEE ==="
  OUT=$($FNN -u "$RPC" --raw-data payment send_payment \
    --target-pubkey "$THIRD" \
    --amount 10000000 \
    --keysend true \
    --trampoline-hops "$HOPS" \
    --final-tlc-expiry-delta 14400000 \
    --max-fee-amount "$FEE" \
    --timeout 120 2>&1) || true
  echo "$OUT"
  PH=$(echo "$OUT" | python3 -c 'import sys,json
raw=sys.stdin.read(); i=raw.rfind("{")
print(json.loads(raw[i:]).get("payment_hash","") if i>=0 else "")' 2>/dev/null || true)
  echo "PH=$PH"
  [[ -z "$PH" ]] && continue
  ST=""
  for i in $(seq 1 12); do
    S=$($FNN -u "$RPC" --raw-data payment get_payment --payment-hash "$PH")
    ST=$(echo "$S" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("status",""))')
    ERR=$(echo "$S" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("failed_error") or "")')
    echo "poll $i status=$ST err=$ERR"
    [[ "$ST" == "Success" || "$ST" == "Failed" ]] && break
    sleep 3
  done
  if [[ "$ST" == "Success" ]]; then
    echo TRAMPOLINE_SUCCESS
    break
  fi
done

$FNN -u "$RPC" payment list_payments --limit 4
$FNN -u "$RPC" channel list_channels
echo DONE
