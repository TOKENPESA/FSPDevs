# FSP Command Console

Industrial high-fidelity React dashboard for Fiber Sidecar Protocol (FSP) and Master Fiber Agent (MFA).

## Stack

- React 18 + Vite
- Tailwind CSS (FSP design tokens)
- Lucide React icons
- HTML5 Canvas topology placeholder

## Design tokens

| Token | Hex | Usage |
|-------|-----|-------|
| Obsidian Base | `#0B0F17` | Background / canvas |
| Institutional Slate | `#1E293B` | Borders / containers |
| Liquidity Mint | `#10B981` | Success / liquidity / active |
| Warning Amber | `#F59E0B` | Warnings / pending |
| Sovereign Cyan | `#06B6D4` | Graph / tunnels / MFA |

Typography: `font-mono` (JetBrains Mono) for all numeric/crypto values; `font-sans` (Inter) for labels.

## MFA integration

The console talks to the **live MFA supervisor** on `127.0.0.1:1025`:

| View | MFA data |
|------|----------|
| **MFA** (default) | Health, connected FAs, running plugins, hub RPC |
| **Registry** | Live hot-swap API (`/api/modules/*`) — install, toggle, uninstall |
| **Matrix / Treasury** | Demo telemetry (topology canvas + L2 mock reserves) |

**Dev:** Vite proxies `/mfa-api` → `:1025` (no CORS issues).  
**Prod build:** Direct calls to `:1025` (CORS allows `:5173`).

Start MFA first:

```powershell
.\fnn-testnet\start-live-mfa.ps1
```

Legacy vanilla console: http://127.0.0.1:8088/mfa-console/

## Components

- `AppLayout` — responsive shell (sidebar + header / mobile bottom rail)
- `GlobalMonitor` — split topology canvas + terminal clearing log
- `TreasuryVault` — multi-asset L2 reserves + `ChannelCapacityRibbon`
- `ModuleRegistry` — hot-swap grid with hardware toggles + `SecureButton` uninstall
- `SecureButton` — 2-second hold-to-confirm for destructive actions
