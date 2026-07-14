#!/bin/bash
set -uo pipefail
FNN=/opt/fnn/fnn-cli
RPC=http://127.0.0.1:18227
NEW=0313dcf9cf18711b1b473a78ea56222dc44dcbfdf559d24dd937a0657d3bcb108f
OLD=034c662ff2cb6c290c50d31df4e8640dba489f73dfdeb43dd1faede96021505381
THIRD=0389647832dd45fbfefa803a23b05fc7d6fb9a72569025329892d71d332a1c3678

send_via_router() {
  local TARGET="$1"
  local LABEL="$2"
  echo "=== build+send $LABEL -> $TARGET ==="
  ROUTER=$($FNN -u "$RPC" --raw-data payment build_router \
    --hops-info "[{\"pubkey\":\"$TARGET\"}]" --amount 10000000)
  echo "$ROUTER"
  HOPS_JSON=$(echo "$ROUTER" | python3 -c 'import sys,json; print(json.dumps(json.load(sys.stdin)["router_hops"]))')
  OUT=$($FNN -u "$RPC" --raw-data payment send_payment_with_router \
    --router "$HOPS_JSON" --keysend true 2>&1) || true
  echo "$OUT"
  PH=$(echo "$OUT" | python3 -c 'import sys,json
raw=sys.stdin.read(); i=raw.rfind("{")
print(json.loads(raw[i:]).get("payment_hash","") if i>=0 else "")' 2>/dev/null || true)
  echo "PH=$PH"
  for i in $(seq 1 20); do
    [[ -z "$PH" ]] && break
    S=$($FNN -u "$RPC" --raw-data payment get_payment --payment-hash "$PH" 2>/dev/null || echo '{}')
    ST=$(echo "$S" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("status",""))' 2>/dev/null || true)
    echo "poll $i status=$ST"
    echo "$S" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d.get("failed_error"))' 2>/dev/null || true
    [[ "$ST" == "Success" || "$ST" == "Failed" ]] && break
    sleep 3
  done
}

send_via_router "$NEW" "NEW"
send_via_router "$OLD" "OLD"

echo "=== try 2-hop build NEW then OLD ==="
$FNN -u "$RPC" --raw-data payment build_router \
  --hops-info "[{\"pubkey\":\"$NEW\"},{\"pubkey\":\"$OLD\"}]" --amount 10000000 2>&1 || true

echo "=== try 2-hop build OLD then NEW ==="
$FNN -u "$RPC" --raw-data payment build_router \
  --hops-info "[{\"pubkey\":\"$OLD\"},{\"pubkey\":\"$NEW\"}]" --amount 10000000 2>&1 || true

echo "=== try 2-hop NEW then THIRD ==="
$FNN -u "$RPC" --raw-data payment build_router \
  --hops-info "[{\"pubkey\":\"$NEW\"},{\"pubkey\":\"$THIRD\"}]" --amount 10000000 2>&1 || true

# If any 2-hop builds, send it
for HOPS in \
  "[{\"pubkey\":\"$NEW\"},{\"pubkey\":\"$OLD\"}]" \
  "[{\"pubkey\":\"$OLD\"},{\"pubkey\":\"$NEW\"}]"
do
  ROUTER=$($FNN -u "$RPC" --raw-data payment build_router --hops-info "$HOPS" --amount 10000000 2>/dev/null) || continue
  if echo "$ROUTER" | grep -q router_hops; then
    echo "MULTI_ROUTER=$ROUTER"
    HOPS_JSON=$(echo "$ROUTER" | python3 -c 'import sys,json; print(json.dumps(json.load(sys.stdin)["router_hops"]))')
    OUT=$($FNN -u "$RPC" --raw-data payment send_payment_with_router --router "$HOPS_JSON" --keysend true 2>&1) || true
    echo "$OUT"
    PH=$(echo "$OUT" | python3 -c 'import sys,json
raw=sys.stdin.read(); i=raw.rfind("{")
print(json.loads(raw[i:]).get("payment_hash","") if i>=0 else "")' 2>/dev/null || true)
    for i in $(seq 1 20); do
      [[ -z "$PH" ]] && break
      S=$($FNN -u "$RPC" --raw-data payment get_payment --payment-hash "$PH")
      ST=$(echo "$S" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("status",""))')
      echo "multi_poll $i $ST err=$(echo "$S" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("failed_error"))')"
      [[ "$ST" == "Success" || "$ST" == "Failed" ]] && break
      sleep 3
    done
    break
  fi
done

$FNN -u "$RPC" channel list_channels
echo DONE
