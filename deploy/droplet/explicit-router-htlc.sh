#!/bin/bash
set -uo pipefail
FNN=/opt/fnn/fnn-cli
RPC=http://127.0.0.1:18227
NEW=0313dcf9cf18711b1b473a78ea56222dc44dcbfdf559d24dd937a0657d3bcb108f
OLD=034c662ff2cb6c290c50d31df4e8640dba489f73dfdeb43dd1faede96021505381
NEW_OUT=0xd10e7f653cf16e00897a05a73c5abde989f7c91a18c0c75d2094544c3a9e70ed00000000
OLD_OUT=0x02cdc113af3672ebb74a9081d8aafc6ea9e758e7c52e95fce5ccc11fdc9a7fdf00000000
THIRD=0389647832dd45fbfefa803a23b05fc7d6fb9a72569025329892d71d332a1c3678

echo "=== build_router to NEW with channel ==="
# hops_info JSON — try object form and tuple form
for HOPS in \
  "[{\"pubkey\":\"$NEW\",\"channel_outpoint\":\"$NEW_OUT\"}]" \
  "[{\"pubkey\":\"$NEW\"}]" \
  "[{\"pubkey\":\"$OLD\",\"channel_outpoint\":\"$OLD_OUT\"}]"
do
  echo "hops=$HOPS"
  $FNN -u "$RPC" --raw-data payment build_router --hops-info "$HOPS" --amount 10000000 2>&1 || true
done

echo "=== send_payment_with_router help ==="
$FNN -u "$RPC" payment send_payment_with_router --help 2>&1 | head -n 40

# Build then send to NEW
ROUTER=$($FNN -u "$RPC" --raw-data payment build_router \
  --hops-info "[{\"pubkey\":\"$NEW\"}]" --amount 10000000 2>&1) || true
echo "ROUTER=$ROUTER"

if echo "$ROUTER" | grep -q router_hops; then
  # extract router_hops array
  HOPS_JSON=$(echo "$ROUTER" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(json.dumps(d.get("router_hops",[])))')
  echo "HOPS_JSON=$HOPS_JSON"
  OUT=$($FNN -u "$RPC" --raw-data payment send_payment_with_router \
    --router "$HOPS_JSON" --keysend true --timeout 90 2>&1) || true
  echo "$OUT"
fi

# Multi-hop attempt: NEW then THIRD if possible
echo "=== build_router NEW -> THIRD ==="
$FNN -u "$RPC" --raw-data payment build_router \
  --hops-info "[{\"pubkey\":\"$NEW\"},{\"pubkey\":\"$THIRD\"}]" --amount 10000000 2>&1 || true

echo "=== journal recent pathfind ==="
journalctl -u fspdevs-fnn --since '10 minutes ago' --no-pager | grep -iE 'pathfind|no path|build_route|blacklist|history|RemoveAck|TLC' | tail -n 50

echo DONE
