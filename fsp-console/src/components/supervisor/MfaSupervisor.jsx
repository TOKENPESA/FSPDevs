import { ExternalLink, Radio, Server, Shield } from "lucide-react";
import { useMfaRuntime } from "../../hooks/useMfaRuntime.js";

function Metric({ label, value, mono = true, accent = "text-slate-100" }) {
  return (
    <div className="panel-border rounded-lg p-4">
      <p className="telemetry-label">{label}</p>
      <p className={`mt-2 text-lg font-semibold ${mono ? "font-mono tabular-nums" : "font-sans"} ${accent}`}>
        {value}
      </p>
    </div>
  );
}

export default function MfaSupervisor() {
  const { runtime, refresh } = useMfaRuntime();

  const networkStatus = runtime.online
    ? runtime.connectedAgents > 0
      ? "online"
      : "degraded"
    : "offline";

  const statusLabel =
    networkStatus === "online"
      ? "[ MFA ONLINE ]"
      : networkStatus === "degraded"
        ? "[ MFA IDLE · NO FAs ]"
        : "[ MFA OFFLINE ]";

  const statusColor =
    networkStatus === "online"
      ? "text-liquidity-mint"
      : networkStatus === "degraded"
        ? "text-warning-amber"
        : "text-red-400";

  return (
    <section className="space-y-4">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="font-sans text-lg font-semibold text-slate-100">
            Master Fiber Agent Supervisor
          </h2>
          <p className="font-mono text-xs text-slate-500">
            127.0.0.1:1025 · mesh control plane · policy plugin host
          </p>
        </div>
        <div className="flex items-center gap-3">
          <span className={`font-mono text-xs font-semibold ${statusColor}`}>{statusLabel}</span>
          <button
            type="button"
            onClick={() => void refresh()}
            className="rounded border border-institutional-slate px-3 py-2 font-sans text-xs text-slate-300 hover:border-sovereign-cyan/40 hover:text-sovereign-cyan"
          >
            Refresh
          </button>
          <a
            href="http://127.0.0.1:8088/mfa-console/"
            target="_blank"
            rel="noreferrer"
            className="flex items-center gap-2 rounded border border-sovereign-cyan/40 bg-sovereign-cyan/10 px-3 py-2 font-sans text-xs text-sovereign-cyan hover:bg-sovereign-cyan/20"
          >
            <ExternalLink size={14} />
            Legacy console
          </a>
        </div>
      </header>

      {runtime.error && !runtime.online && (
        <div className="panel-border rounded-lg border-warning-amber/40 p-4">
          <p className="font-mono text-sm text-warning-amber">[ AWAITING SUPERVISOR ]</p>
          <p className="mt-2 font-sans text-xs text-slate-400">{runtime.error}</p>
          <p className="mt-2 font-mono text-[10px] text-slate-500">
            Start MFA: fnn-testnet/start-live-mfa.ps1
          </p>
        </div>
      )}

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <Metric
          label="Connected sidecars"
          value={runtime.connectedAgents}
          accent={runtime.connectedAgents > 0 ? "text-liquidity-mint" : "text-warning-amber"}
        />
        <Metric
          label="Simulation cap"
          value={runtime.simulationEdgeNodes}
        />
        <Metric
          label="Running plugins"
          value={runtime.runningPlugins.length}
          accent={runtime.runningPlugins.length > 0 ? "text-liquidity-mint" : "text-warning-amber"}
        />
        <Metric
          label="Regional clearing"
          value={runtime.clearingRegionalReady ? "READY" : "BLOCKED"}
          accent={runtime.clearingRegionalReady ? "text-liquidity-mint" : "text-warning-amber"}
        />
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        <article className="panel-border rounded-lg p-4">
          <div className="mb-3 flex items-center gap-2">
            <Server size={16} className="text-sovereign-cyan" />
            <h3 className="font-sans text-sm font-semibold text-slate-100">Hub channel</h3>
          </div>
          <p className="telemetry-label">FNN RPC endpoint</p>
          <p className="mt-1 font-mono text-sm text-sovereign-cyan">{runtime.hubRpcUrl}</p>
          <p className="telemetry-label mt-4">Funding allocation</p>
          <p className="mt-1 font-mono text-sm text-slate-200">
            {runtime.hubFunding != null
              ? `${runtime.hubFunding.toLocaleString()} shannons`
              : "—"}
          </p>
        </article>

        <article className="panel-border rounded-lg p-4">
          <div className="mb-3 flex items-center gap-2">
            <Shield size={16} className="text-liquidity-mint" />
            <h3 className="font-sans text-sm font-semibold text-slate-100">Mounted plugins</h3>
          </div>
          {runtime.runningPlugins.length === 0 ? (
            <p className="font-mono text-xs text-warning-amber">[ NO PLUGINS MOUNTED ]</p>
          ) : (
            <ul className="space-y-2">
              {runtime.runningPlugins.map((name) => (
                <li
                  key={name}
                  className="flex items-center gap-2 rounded border border-institutional-slate px-3 py-2 font-mono text-xs text-liquidity-mint"
                >
                  <Radio size={12} />
                  {name}
                </li>
              ))}
            </ul>
          )}
        </article>
      </div>

      {runtime.connectedAgentIds.length > 0 && (
        <article className="panel-border rounded-lg p-4">
          <p className="telemetry-label">Connected FA IDs</p>
          <p className="mt-2 font-mono text-sm text-slate-300">
            FA-{runtime.connectedAgentIds.join(", FA-")}
          </p>
        </article>
      )}

      {runtime.assetCorridors.length > 0 && (
        <article className="panel-border rounded-lg p-4">
          <p className="telemetry-label">Asset registry corridors</p>
          <p className="mt-2 font-mono text-sm text-slate-300">
            {runtime.assetCorridors.join(" · ")}
          </p>
        </article>
      )}
    </section>
  );
}
