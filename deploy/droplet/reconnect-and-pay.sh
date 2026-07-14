#!/bin/bash
set -uo pipefail
FNN=/opt/fnn/fnn-cli
RPC=http://127.0.0.1:18227
NEW=0313dcf9cf18711b1b473a78ea56222dc44dcbfdf559d24dd937a0657d3bcb108f
OLD=034c662ff2cb6c290c50d31df4e8640dba489f73dfdeb43dd1faede96021505381
ADDR=/ip4/16.163.7.105/tcp/8228/p2p/QmdyQWjPtbK4NWWsvy8s69NGJaQULwgeQDT5ZpNDrTNaeV
THIRD=0389647832dd45fbfefa803a23b05fc7d6fb9a72569025329892d71d332a1c3678

echo "=== peers before ==="
$FNN -u "$RPC" peer list_peers

echo "=== reconnect NEW ==="
$FNN -u "$RPC" peer connect_peer --address "$ADDR" --save true 2>&1 || true
sleep 4
$FNN -u "$RPC" peer list_peers

echo "=== direct OLD 0.1 CKB ==="
OUT1=$($FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$OLD" --amount 10000000 --keysend true --timeout 90 2>&1) || true
echo "$OUT1"

echo "=== direct NEW 0.1 CKB ==="
OUT2=$($FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$NEW" --amount 10000000 --keysend true --timeout 90 2>&1) || true
echo "$OUT2"

echo "=== build_router help ==="
$FNN -u "$RPC" payment build_router --help 2>&1 | head -n 50

echo "=== trampoline NEW->THIRD after reconnect ==="
HOPS=$(printf '["%s"]' "$NEW")
OUT3=$($FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$THIRD" --amount 10000000 --keysend true \
  --trampoline-hops "$HOPS" --final-tlc-expiry-delta 14400000 \
  --max-fee-amount 2000000 --timeout 120 2>&1) || true
echo "$OUT3"

PH=$(echo "$OUT2$OUT3$OUT1" | python3 -c '
import sys,re,json
raw=sys.stdin.read()
hashes=re.findall(r"\"payment_hash\":\s*\"(0x[0-9a-fA-F]+)\"", raw)
print(hashes[-1] if hashes else "")
')
echo "LAST_HASH=$PH"
for i in $(seq 1 15); do
  [[ -z "$PH" ]] && break
  S=$($FNN -u "$RPC" --raw-data payment get_payment --payment-hash "$PH" 2>/dev/null || echo '{}')
  echo "poll $i $S"
  ST=$(echo "$S" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("status",""))' 2>/dev/null || true)
  [[ "$ST" == "Success" || "$ST" == "Failed" ]] && break
  sleep 4
done

$FNN -u "$RPC" channel list_channels
$FNN -u "$RPC" payment list_payments --limit 4
echo DONE
