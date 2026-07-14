#!/bin/bash
set -euo pipefail
FNN_RPC="${FNN_RPC_URL:-http://127.0.0.1:18227}"

rpc() {
  local method="$1"
  local params="${2:-[]}"
  curl -fsS -m 20 "$FNN_RPC/" \
    -H 'Content-Type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"${method}\",\"params\":${params}}"
}

echo "=== node_info ==="
rpc node_info '[]' | python3 -c 'import sys,json; r=json.load(sys.stdin)["result"]; print("name", r.get("node_name")); print("peers", r.get("peers_count")); print("channels", r.get("channel_count")); print("pending", r.get("pending_channel_count")); print("lock_args", r["default_funding_lock_script"]["args"])'

echo
echo "=== funding address ==="
FNN_RPC="$FNN_RPC" bash /opt/fspdevs/deploy/droplet/fund-hub.sh 2>/dev/null | sed -n '/FUND THIS/,/=====/p' | head -n 20 || true

echo
echo "=== try list_channels variants ==="
for params in '[]' '{}' '[{}]' '[{"limit":null}]'; do
  echo "-- params=$params"
  out="$(curl -fsS -m 10 "$FNN_RPC/" -H 'Content-Type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"list_channels\",\"params\":$params}" 2>&1 || true)"
  echo "$out" | head -c 500
  echo
done

echo
echo "=== fnn-cli channel / wallet probes ==="
if [[ -x /opt/fnn/fnn-cli ]]; then
  /opt/fnn/fnn-cli -u "$FNN_RPC" info 2>&1 | head -n 40 || true
  /opt/fnn/fnn-cli -u "$FNN_RPC" channel list 2>&1 | head -n 40 || true
  /opt/fnn/fnn-cli -u "$FNN_RPC" help 2>&1 | head -n 50 || true
fi

echo
echo "=== CKB testnet balance via public indexer (lock args) ==="
LOCK_ARGS="$(curl -fsS -m 10 "$FNN_RPC/" -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"node_info","params":[]}' | python3 -c 'import sys,json; print(json.load(sys.stdin)["result"]["default_funding_lock_script"]["args"])')"
python3 - <<PY
import json, urllib.request
lock_args = "$LOCK_ARGS"
script = {
  "code_hash": "0x9bd7e06f3ecf4be0f2fcd2188b23f1b9fcc88e5d4b65a8637b17723bbda3cce8",
  "hash_type": "type",
  "args": lock_args,
}
# Try common testnet CKB RPCs for get_cells / get_tip
endpoints = [
  "https://testnet.ckbapp.dev/",
  "https://testnet.ckb.dev/",
]
for ep in endpoints:
  try:
    tip = json.load(urllib.request.urlopen(urllib.request.Request(
      ep, data=json.dumps({"jsonrpc":"2.0","id":1,"method":"get_tip_block_number","params":[]}).encode(),
      headers={"Content-Type":"application/json"}), timeout=15))
    print("tip", ep, tip.get("result"))
  except Exception as e:
    print("tip_fail", ep, e)
# indexer get_cells
indexer = "https://testnet.ckbapp.dev/"
payload = {
  "id": 1,
  "jsonrpc": "2.0",
  "method": "get_cells",
  "params": [
    {"script": script, "script_search_mode": "exact", "script_type": "lock", "with_data": False},
    "asc",
    "0x64",
  ],
}
try:
  # Some nodes expose indexer on same host under different path; also try mercury style
  req = urllib.request.Request(indexer, data=json.dumps(payload).encode(), headers={"Content-Type":"application/json"})
  resp = json.load(urllib.request.urlopen(req, timeout=20))
  print("get_cells", json.dumps(resp)[:800])
except Exception as e:
  print("get_cells_fail", e)

# Fallback: ckb explorer API style search by address
addr = "ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqwcdk090lkg4cce60rlm60e26rcakwh36q8a679a"
for url in [
  f"https://testnet.explorer.nervos.org/api/v1/addresses/{addr}",
  f"https://pudge-api.explorer.nervos.org/api/v1/addresses/{addr}",
]:
  try:
    req = urllib.request.Request(url, headers={"Accept": "application/vnd.api+json"})
    body = urllib.request.urlopen(req, timeout=20).read().decode()
    print("explorer", url, body[:600])
  except Exception as e:
    print("explorer_fail", url, e)
PY

echo POST_FAUCET_CHECK_DONE
