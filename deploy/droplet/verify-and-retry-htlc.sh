#!/bin/bash
set -uo pipefail
FNN=/opt/fnn/fnn-cli
RPC=http://127.0.0.1:18227
NEWPEER=0313dcf9cf18711b1b473a78ea56222dc44dcbfdf559d24dd937a0657d3bcb108f
EXISTING=034c662ff2cb6c290c50d31df4e8640dba489f73dfdeb43dd1faede96021505381
THIRD=0389647832dd45fbfefa803a23b05fc7d6fb9a72569025329892d71d332a1c3678
AMOUNT=50000000

echo "=== direct keysend on NEW channel (proves liquidity) ==="
LIVE=$($FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$NEWPEER" \
  --amount "$AMOUNT" \
  --keysend true \
  --final-tlc-expiry-delta 14400000 \
  --max-fee-amount 1000000 \
  --timeout 90 2>&1) || true
echo "$LIVE"
PH=$(echo "$LIVE" | python3 -c 'import sys,json
raw=sys.stdin.read(); i=raw.rfind("{")
print(json.loads(raw[i:]).get("payment_hash","") if i>=0 else "")' 2>/dev/null || true)
for i in 1 2 3 4 5 6 7 8; do
  [[ -z "$PH" ]] && break
  S=$($FNN -u "$RPC" --raw-data payment get_payment --payment-hash "$PH")
  echo "direct_poll $i $S"
  ST=$(echo "$S" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("status",""))')
  [[ "$ST" == "Success" || "$ST" == "Failed" ]] && break
  sleep 3
done

echo "=== graph sample (newpeer + existing) ==="
$FNN -u "$RPC" graph graph_nodes --limit 20 2>&1 | head -n 80 || true
$FNN -u "$RPC" --raw-data graph graph_channels --limit 30 2>&1 | python3 -c '
import sys,json
raw=sys.stdin.read()
try:
  d=json.loads(raw)
except Exception as e:
  print(raw[:500]); raise SystemExit
chs=d.get("channels") or d.get("result",{}).get("channels") or []
want=("0313dcf9","034c662f","03896478","0313d0a1")
for c in chs:
  s=json.dumps(c)
  if any(w in s for w in want):
    print(s[:300])
print("total_channels_returned", len(chs))
' 2>/dev/null || true

echo "=== trampoline retry NEWPEER -> THIRD max_fee=2000000 ==="
HOPS=$(printf '["%s"]' "$NEWPEER")
LIVE2=$($FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$THIRD" \
  --amount 10000000 \
  --keysend true \
  --trampoline-hops "$HOPS" \
  --final-tlc-expiry-delta 14400000 \
  --max-fee-amount 2000000 \
  --timeout 120 2>&1) || true
echo "$LIVE2"
PH2=$(echo "$LIVE2" | python3 -c 'import sys,json
raw=sys.stdin.read(); i=raw.rfind("{")
print(json.loads(raw[i:]).get("payment_hash","") if i>=0 else "")' 2>/dev/null || true)
for i in $(seq 1 20); do
  [[ -z "$PH2" ]] && break
  S=$($FNN -u "$RPC" --raw-data payment get_payment --payment-hash "$PH2")
  echo "multi_poll $i $S"
  ST=$(echo "$S" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("status",""))')
  [[ "$ST" == "Success" || "$ST" == "Failed" ]] && break
  sleep 4
done

echo "=== trampoline EXISTING -> THIRD via new topology max_fee=2000000 ==="
HOPS3=$(printf '["%s"]' "$EXISTING")
LIVE3=$($FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$THIRD" \
  --amount 10000000 \
  --keysend true \
  --trampoline-hops "$HOPS3" \
  --final-tlc-expiry-delta 14400000 \
  --max-fee-amount 2000000 \
  --timeout 120 2>&1) || true
echo "$LIVE3"

$FNN -u "$RPC" channel list_channels
echo DONE
