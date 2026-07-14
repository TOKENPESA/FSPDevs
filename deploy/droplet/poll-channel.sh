#!/bin/bash
set -euo pipefail
FNN_RPC=http://127.0.0.1:18227
for i in 1 2 3 4 5 6; do
  echo "=== poll $i ==="
  /opt/fnn/fnn-cli -u "$FNN_RPC" channel list_channels 2>/dev/null | sed -n '1,25p'
  state="$(/opt/fnn/fnn-cli -u "$FNN_RPC" --raw-data channel list_channels 2>/dev/null | python3 -c 'import sys,json; c=json.load(sys.stdin).get("channels") or []; print(c[0]["state"]["state_name"] if c else "none")')"
  echo "state=$state"
  if [[ "$state" == "ChannelReady" ]]; then
    echo CHANNEL_READY
    break
  fi
  sleep 20
done
/opt/fnn/fnn-cli -u "$FNN_RPC" --raw-data channel list_channels
echo POLL_DONE
