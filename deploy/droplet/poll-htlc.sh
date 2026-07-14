#!/bin/bash
set -uo pipefail
FNN=/opt/fnn/fnn-cli
RPC=http://127.0.0.1:18227
DIRECT=0xc005beab1b14a44c2b628f388ea25be82c4482b98552def4dcc618bf0e0def19
MULTI=0x45fe009ec29baa9ba64d37e7a30811024693b6f9d4d82fcda8f77b933329e6c2

for i in 1 2 3 4 5 6 7 8 9 10 11 12; do
  echo "=== poll $i ==="
  echo -n "DIRECT: "
  $FNN -u "$RPC" --raw-data payment get_payment --payment-hash "$DIRECT" 2>/dev/null || echo fail
  echo -n "MULTI: "
  $FNN -u "$RPC" --raw-data payment get_payment --payment-hash "$MULTI" 2>/dev/null || echo fail
  DSTAT=$($FNN -u "$RPC" --raw-data payment get_payment --payment-hash "$DIRECT" 2>/dev/null | python3 -c 'import sys,json; print(json.load(sys.stdin).get("status",""))' 2>/dev/null || true)
  MSTAT=$($FNN -u "$RPC" --raw-data payment get_payment --payment-hash "$MULTI" 2>/dev/null | python3 -c 'import sys,json; print(json.load(sys.stdin).get("status",""))' 2>/dev/null || true)
  echo "parsed direct=$DSTAT multi=$MSTAT"
  if [[ "$DSTAT" =~ ^(Success|Failed|Succeeded)$ ]] && [[ "$MSTAT" =~ ^(Success|Failed|Succeeded|Created)$ ]]; then
    # multi may stay Created if never launched properly; break when direct terminal and multi not Inflight
    if [[ "$MSTAT" != "Inflight" && "$MSTAT" != "Created" ]] || [[ "$i" -ge 4 && "$DSTAT" =~ ^(Success|Failed) && "$MSTAT" != "Inflight" ]]; then
      :
    fi
  fi
  if [[ "$DSTAT" =~ ^(Success|Failed)$ ]] && [[ "$MSTAT" =~ ^(Success|Failed)$ ]]; then
    break
  fi
  if [[ "$DSTAT" =~ ^(Success|Failed)$ ]] && [[ "$MSTAT" == "Created" ]] && [[ $i -ge 6 ]]; then
    break
  fi
  sleep 5
done

echo "=== channels ==="
$FNN -u "$RPC" channel list_channels
echo "=== journal tlcs ==="
journalctl -u fspdevs-fnn --since '5 minutes ago' --no-pager | grep -iE 'payment|tlc|keysend|RemoveTlc|AddTlc|Failed|Success|route' | tail -n 40
echo POLL_DONE
