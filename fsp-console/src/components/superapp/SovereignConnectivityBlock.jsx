import { useEffect, useState } from "react";
import { Database, Radio, Wifi } from "lucide-react";

/**
 * @param {{ label: string, value: string, unit?: string, accent?: string }} props
 */
function TelemetryCell({ label, value, unit, accent = "text-slate-100" }) {
  return (
    <div className="border border-institutional-slate bg-obsidian p-3">
      <p className="telemetry-label">{label}</p>
      <p className={`mt-1 font-mono text-lg font-semibold tabular-nums ${accent}`}>{value}</p>
      {unit && <p className="font-mono text-[10px] text-slate-500">{unit}</p>}
    </div>
  );
}

export default function SovereignConnectivityBlock() {
  const [gossip, setGossip] = useState(1240);
  const [syncState, setSyncState] = useState("SYNCED");

  useEffect(() => {
    const id = setInterval(() => {
      setGossip((g) => Math.max(800, g + Math.floor(Math.random() * 80) - 30));
      setSyncState(Math.random() > 0.92 ? "REPLAY" : "SYNCED");
    }, 2000);
    return () => clearInterval(id);
  }, []);

  return (
    <section className="panel-border border-sovereign-cyan/30 p-4">
      <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <div>
          <p className="font-mono text-sm font-bold tracking-wide text-sovereign-cyan">
            [ OPERATION MODE: SOVEREIGN MESH ]
          </p>
          <p className="mt-1 font-sans text-xs text-slate-400">
            Central clearinghouse bypassed · localized SQLite ledger authoritative
          </p>
        </div>
        <div className="flex items-center gap-2 border border-sovereign-cyan/40 px-3 py-1.5">
          <Radio size={14} className="animate-pulse-slow text-sovereign-cyan" />
          <span className="font-mono text-[10px] text-sovereign-cyan">P2P ACTIVE</span>
        </div>
      </div>

      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <TelemetryCell
          label="Direct connected peers"
          value="7"
          unit="FA nodes · ring adjacency"
          accent="text-sovereign-cyan"
        />
        <TelemetryCell
          label="Gossip packet velocity"
          value={gossip.toLocaleString()}
          unit="packets / min · local mesh"
          accent="text-liquidity-mint"
        />
        <TelemetryCell
          label="SQLite ledger sync"
          value={syncState}
          unit="agent_state.db · WAL mode"
          accent={syncState === "SYNCED" ? "text-liquidity-mint" : "text-warning-amber"}
        />
        <TelemetryCell
          label="MFA uplink"
          value="SEVERED"
          unit="out-of-band fallback armed"
          accent="text-warning-amber"
        />
      </div>

      <div className="mt-3 flex flex-wrap gap-4 border-t border-institutional-slate pt-3 font-mono text-[10px] text-slate-500">
        <span className="flex items-center gap-1">
          <Wifi size={12} className="text-sovereign-cyan" /> mesh-pubkeys.json · local
        </span>
        <span className="flex items-center gap-1">
          <Database size={12} className="text-liquidity-mint" /> ExpiringLockManager · native
        </span>
      </div>
    </section>
  );
}
