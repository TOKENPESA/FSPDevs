# FSPDevs — two-droplet deploy (no simulated fleet)

Topology:

```
[ Laptop / desktop ]  fiber-agent sidecar (retail edge)
         │  WebSocket / HTTP
         ▼
[ MFA droplet ]       master_fiber_agent + mfa-console  (port 80)
         ▲
         │  MFA_HOST + WS token
[ Treasury Hub droplet ]  fiber-agent-daemon (corporate hub FA)
```

**Not** installed on either droplet: the 8-agent / 1024-agent simulated fleet. Edge agents run on remote laptops and desktops.

## Roles

| Droplet | Compose file | Process |
|---------|--------------|---------|
| MFA supervisor | `docker-compose.mfa.yml` | `master_fiber_agent` + nginx console |
| Treasury Hub | `docker-compose.treasury.yml` | `fiber-agent-daemon` (AGENT_ID=1 by default) |

## Requirements

- Two Ubuntu 22.04+ / Debian 12+ droplets (2 GB RAM is enough per role without fleets)
- Ports **22**, **80** (443 later for TLS)
- SSH access; for Treasury Hub you need the MFA IP + matching `MFA_AGENT_WS_TOKEN`

## Bootstrap MFA droplet

On the MFA droplet:

```bash
export FSPDEVS_ROLE=mfa
curl -fsSL https://raw.githubusercontent.com/TOKENPESA/FSPDevs/main/deploy/droplet/bootstrap.sh | sudo bash
```

Or from Windows (after SSH key works):

```powershell
.\deploy\droplet\remote-bootstrap.ps1 -DropletIp 167.99.150.153 -Role mfa
```

Save the printed `MFA_API_TOKEN` and `MFA_AGENT_WS_TOKEN`.

## Bootstrap Treasury Hub droplet

On the Treasury Hub droplet (replace IPs/tokens):

```bash
export FSPDEVS_ROLE=treasury
export MFA_HOST=167.99.150.153          # MFA droplet public IP (nginx :80)
export MFA_AGENT_WS_TOKEN='same-as-mfa-droplet'
sudo bash /opt/fspdevs/deploy/droplet/bootstrap.sh
```

From Windows:

```powershell
.\deploy\droplet\remote-bootstrap.ps1 `
  -DropletIp YOUR_TREASURY_IP `
  -Role treasury `
  -MfaHost 167.99.150.153 `
  -MfaAgentWsToken 'paste-from-mfa-.env'
```

After both are up, set `HUB_RPC_URL` on the MFA `.env` to the Treasury Hub FNN endpoint when live FNN is installed (until then mock clearing is fine).

## Laptop / desktop sidecars

On each edge machine (Windows example):

```powershell
$env:AGENT_ID = "44"
$env:MFA_HOST = "167.99.150.153"              # MFA droplet
$env:MFA_AGENT_WS_TOKEN = "paste-from-mfa"
# Optional local FNN:
# $env:FNN_RPC_URL = "http://127.0.0.1:8227"
cd fiber-agent
cargo run --release --bin fiber-agent-daemon
```

## Security notes

- No simulated fleet containers on droplets
- FA control WS uses HMAC headers (`X-MFA-Agent-Auth` / `X-Agent-ID` / `X-MFA-Timestamp`) — never `?token=`
- FA module API requires `Authorization: Bearer $FIBER_AGENT_API_TOKEN`
- Fiber FNN RPC (when biscuit auth is enabled on the node) requires `FNN_BISCUIT_TOKEN` on both MFA and FA — clients send `Authorization: Bearer $FNN_BISCUIT_TOKEN` on every JSON-RPC call
- Prefer binding FA API to `127.0.0.1`; public traffic through nginx
- Add TLS before public pilot:
  `sudo bash deploy/droplet/setup-tls.sh mfa.example.com ops@example.com`
- Keep `FIBER_AGENT_ALLOW_DEV_KEYS=false` on Treasury Hub
