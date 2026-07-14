#!/bin/bash
set -uo pipefail
FNN=/opt/fnn/fnn-cli
RPC=http://127.0.0.1:18227
CHANNEL_PEER=034c662ff2cb6c290c50d31df4e8640dba489f73dfdeb43dd1faede96021505381
# Connected peer with no direct Hub channel — forces pathfinding / trampoline
DEST=0389647832dd45fbfefa803a23b05fc7d6fb9a72569025329892d71d332a1c3678
# 1 CKB
AMOUNT=100000000
MAX_FEE=5000000000

echo "=== balance before ==="
$FNN -u "$RPC" channel list_channels

echo "=== dry_run: pathfind to non-adjacent DEST ==="
set +e
$FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$DEST" \
  --amount "$AMOUNT" \
  --keysend true \
  --max-fee-amount "$MAX_FEE" \
  --dry-run true
DRY1=$?
echo "dry_run_exit=$DRY1"

echo "=== dry_run: trampoline via channel peer -> DEST ==="
$FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$DEST" \
  --amount "$AMOUNT" \
  --keysend true \
  --trampoline-hops "[\"$CHANNEL_PEER\"]" \
  --max-fee-amount "$MAX_FEE" \
  --dry-run true
DRY2=$?
echo "trampoline_dry_run_exit=$DRY2"

echo "=== LIVE direct keysend to channel peer (HTLC baseline) ==="
$FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$CHANNEL_PEER" \
  --amount "$AMOUNT" \
  --keysend true \
  --final-tlc-expiry-delta 14400000 \
  --max-fee-amount "$MAX_FEE" \
  --timeout 60
DIRECT=$?
echo "direct_keysend_exit=$DIRECT"

PAYMENT_HASH=$( $FNN -u "$RPC" --raw-data payment list_payments 2>/dev/null | python3 -c '
import sys,json
try:
  d=json.load(sys.stdin)
except Exception:
  print("")
  raise SystemExit
pays=d.get("payments") or d.get("result",{}).get("payments") or []
if isinstance(d, dict) and "payment_hash" in d:
  print(d.get("payment_hash",""))
elif pays:
  print(pays[0].get("payment_hash",""))
' )

# Prefer get_payment poll on newest status from last send output stored? re-list
echo "=== poll recent payments ==="
$FNN -u "$RPC" payment list_payments --limit 3

echo "=== LIVE multi-hop attempt (trampoline peer -> DEST) ==="
$FNN -u "$RPC" --raw-data payment send_payment \
  --target-pubkey "$DEST" \
  --amount "$AMOUNT" \
  --keysend true \
  --trampoline-hops "[\"$CHANNEL_PEER\"]" \
  --final-tlc-expiry-delta 14400000 \
  --max-fee-amount "$MAX_FEE" \
  --timeout 90
MULTI=$?
echo "multi_hop_exit=$MULTI"

echo "=== payments after ==="
$FNN -u "$RPC" payment list_payments --limit 5

echo "=== channel balances after ==="
$FNN -u "$RPC" channel list_channels

echo HTCL_SCRIPT_DONE
