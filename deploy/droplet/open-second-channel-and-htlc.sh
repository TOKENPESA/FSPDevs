#!/bin/bash
set -uo pipefail
FNN=/opt/fnn/fnn-cli
RPC=http://127.0.0.1:18227
# Prefer first publicly routable alternate; fallback to second candidate
PEER_A=0313dcf9cf18711b1b473a78ea56222dc44dcbfdf559d24dd937a0657d3bcb108f
PEER_B=024714ca19abea4ddc0f3863ffdfb2e2cee76af87c477de4bc67c74a83f8140042
EXISTING=034c662ff2cb6c290c50d31df4e8640dba489f73dfdeb43dd1faede96021505381
FUNDING=100000000000  # 1000 CKB

echo "=== peers / channels before ==="
$FNN -u "$RPC" peer list_peers
$FNN -u "$RPC" channel list_channels

pick_peer() {
  local pk
  for pk in "$PEER_A" "$PEER_B"; do
    if $FNN -u "$RPC" --raw-data peer list_peers 2>/dev/null | grep -q "$pk"; then
      echo "$pk"
      return 0
    fi
  done
  echo "$PEER_A"
}

PEER=$(pick_peer)
echo "Selected second-channel peer: $PEER"

# Ensure connected (already in list usually)
$FNN -u "$RPC" peer connect_peer --pubkey "$PEER" 2>/dev/null || true
sleep 2

echo "=== open_channel 1000 CKB public ==="
OPEN_OUT=$($FNN -u "$RPC" --raw-data channel open_channel \
  --pubkey "$PEER" \
  --funding-amount "$FUNDING" \
  --public true 2>&1) || true
echo "$OPEN_OUT"

echo "=== poll until second ChannelReady (max ~3 min) ==="
READY=0
for i in $(seq 1 36); do
  RAW=$($FNN -u "$RPC" --raw-data channel list_channels 2>/dev/null || echo '{}')
  COUNT=$(echo "$RAW" | python3 -c 'import sys,json
try:
  d=json.load(sys.stdin)
except Exception:
  print(0); raise SystemExit
chs=d.get("channels") or []
ready=sum(1 for c in chs if (c.get("state") or {}).get("state_name")=="ChannelReady")
print(ready)')
  echo "poll $i ready_channels=$COUNT"
  echo "$RAW" | python3 -c 'import sys,json
try:
  d=json.load(sys.stdin)
except Exception:
  raise SystemExit
for c in d.get("channels") or []:
  st=(c.get("state") or {}).get("state_name")
  print(c.get("pubkey","?")[:16], st, "local", c.get("local_balance"))'
  if [[ "$COUNT" -ge 2 ]]; then
    READY=1
    break
  fi
  # also show pending
  $FNN -u "$RPC" channel list_channels --only-pending true 2>/dev/null | head -n 40 || true
  sleep 5
done

echo "READY=$READY"
$FNN -u "$RPC" channel list_channels
$FNN -u "$RPC" info node_info | head -n 25

if [[ "$READY" -ne 1 ]]; then
  echo "SECOND_CHANNEL_NOT_READY — aborting HTLC"
  journalctl -u fspdevs-fnn --since '3 minutes ago' --no-pager | grep -iE 'channel|funding|Collaborat|error|Error' | tail -n 40
  exit 1
fi

# Destination for trampoline: the other peer (not the first hop)
# First hop = EXISTING bootnode; final = newly channeled peer OR vice versa
# Aim: Hub -> EXISTING -> PEER (if EXISTING can reach PEER) OR Hub -> PEER as alternate first hop to SOME other dest
# Best test with 2 channels: pay DEST=PEER with trampoline [EXISTING] if graph allows,
# OR keysend-pathfind to a third peer using either channel.
DEST="$PEER"
# If PEER is the new channel counterpart, pathfind WITHOUT trampoline may use PEER-channel direct;
# for true multi-hop, send to EXISTING via trampoline through NEW peer, or to a third node.
THIRD=0389647832dd45fbfefa803a23b05fc7d6fb9a72569025329892d71d332a1c3678
AMOUNT=50000000  # 0.5 CKB
MAX_FEE=5000000

echo "=== dry_run pathfind to THIRD ==="
$FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$THIRD" \
  --amount "$AMOUNT" \
  --keysend true \
  --max-fee-amount "$MAX_FEE" \
  --dry-run true 2>&1 || true

echo "=== dry_run trampoline EXISTING -> THIRD ==="
HOPS=$(printf '["%s"]' "$EXISTING")
$FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$THIRD" \
  --amount "$AMOUNT" \
  --keysend true \
  --trampoline-hops "$HOPS" \
  --final-tlc-expiry-delta 14400000 \
  --max-fee-amount "$MAX_FEE" \
  --dry-run true 2>&1 || true

echo "=== dry_run trampoline NEWPEER -> THIRD ==="
HOPS2=$(printf '["%s"]' "$PEER")
$FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$THIRD" \
  --amount "$AMOUNT" \
  --keysend true \
  --trampoline-hops "$HOPS2" \
  --final-tlc-expiry-delta 14400000 \
  --max-fee-amount "$MAX_FEE" \
  --dry-run true 2>&1 || true

echo "=== LIVE: pathfind keysend to THIRD (0.5 CKB) ==="
LIVE=$($FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$THIRD" \
  --amount "$AMOUNT" \
  --keysend true \
  --final-tlc-expiry-delta 14400000 \
  --max-fee-amount "$MAX_FEE" \
  --timeout 120 2>&1) || true
echo "$LIVE"
PH=$(echo "$LIVE" | python3 -c 'import sys,json
raw=sys.stdin.read(); i=raw.rfind("{");
import re
print(json.loads(raw[i:]).get("payment_hash","") if i>=0 else "")' 2>/dev/null || true)

if [[ -z "$PH" ]]; then
  echo "=== LIVE fallback: trampoline via EXISTING to THIRD ==="
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
raw=sys.stdin.read(); i=raw.rfind("{");
print(json.loads(raw[i:]).get("payment_hash","") if i>=0 else "")' 2>/dev/null || true)
fi

if [[ -z "$PH" ]]; then
  echo "=== LIVE fallback2: trampoline via NEWPEER to THIRD ==="
  LIVE=$($FNN -u "$RPC" --raw-data payment send_payment \
    --target-pubkey "$THIRD" \
    --amount "$AMOUNT" \
    --keysend true \
    --trampoline-hops "$HOPS2" \
    --final-tlc-expiry-delta 14400000 \
    --max-fee-amount "$MAX_FEE" \
    --timeout 120 2>&1) || true
  echo "$LIVE"
  PH=$(echo "$LIVE" | python3 -c 'import sys,json
raw=sys.stdin.read(); i=raw.rfind("{");
print(json.loads(raw[i:]).get("payment_hash","") if i>=0 else "")' 2>/dev/null || true)
fi

echo "PAYMENT_HASH=$PH"
for i in $(seq 1 24); do
  if [[ -z "$PH" ]]; then break; fi
  STAT=$($FNN -u "$RPC" --raw-data payment get_payment --payment-hash "$PH" 2>/dev/null || echo '{}')
  echo "poll_pay $i $STAT"
  S=$(echo "$STAT" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("status",""))' 2>/dev/null || true)
  if [[ "$S" == "Success" || "$S" == "Failed" ]]; then
    break
  fi
  sleep 5
done

echo "=== channels final ==="
$FNN -u "$RPC" channel list_channels
echo SCRIPT_DONE
