import {
  Activity,
  Boxes,
  Gauge,
  LayoutDashboard,
  Network,
  Server,
  Wallet,
} from "lucide-react";

const NAV = [
  { id: "supervisor", label: "MFA", icon: Server },
  { id: "monitor", label: "Matrix", icon: Network },
  { id: "treasury", label: "Treasury", icon: Wallet },
  { id: "registry", label: "Registry", icon: Boxes },
];

/**
 * @param {{ activeView: string, onNavigate: (id: string) => void }} props
 */
function SidebarNav({ activeView, onNavigate }) {
  return (
    <nav className="flex flex-col gap-1 p-3" aria-label="Primary navigation">
      <div className="mb-4 px-2">
        <div className="flex items-center gap-2">
          <div className="flex h-9 w-9 items-center justify-center rounded border border-sovereign-cyan/40 bg-sovereign-cyan/10 font-mono text-xs font-bold text-sovereign-cyan">
            MFA
          </div>
          <div>
            <p className="font-sans text-sm font-semibold text-slate-100">Master Fiber Agent</p>
            <p className="font-mono text-[10px] text-slate-500">FSP · L2 SUPERVISOR</p>
          </div>
        </div>
      </div>
      {NAV.map(({ id, label, icon: Icon }) => {
        const active = activeView === id;
        return (
          <button
            key={id}
            type="button"
            onClick={() => onNavigate(id)}
            className={`flex items-center gap-3 rounded px-3 py-2.5 text-left font-sans text-sm transition-colors ${
              active
                ? "border border-sovereign-cyan/30 bg-sovereign-cyan/10 text-sovereign-cyan"
                : "border border-transparent text-slate-400 hover:bg-institutional-slate/40 hover:text-slate-200"
            }`}
          >
            <Icon size={18} strokeWidth={1.75} />
            {label}
          </button>
        );
      })}
      <div className="mt-auto border-t border-institutional-slate pt-4">
        <div className="panel-border rounded p-3">
          <p className="telemetry-label">Supervisor</p>
          <p className="mt-1 font-mono text-xs text-sovereign-cyan">127.0.0.1:1025</p>
          <p className="mt-1 font-mono text-[10px] text-slate-500">mesh-fleet-daemon</p>
        </div>
      </div>
    </nav>
  );
}

/**
 * @param {{ iops: number, iopsMax: number, supervisorOnline: boolean, meshStatus: 'online' | 'idle' | 'offline', peerCount: number, mfaOnline?: boolean, runningPlugins?: number, supervisorError?: string }} props
 */
function TelemetryHeader({ iops, iopsMax, supervisorOnline, meshStatus, peerCount, mfaOnline = true, runningPlugins = 0, supervisorError = "" }) {
  const iopsPct = Math.min(100, (iops / Math.max(iopsMax, 1)) * 100);
  const statusColor =
    meshStatus === "online"
      ? "text-liquidity-mint"
      : meshStatus === "idle"
        ? "text-warning-amber"
        : "text-red-400";
  const statusLabel =
    meshStatus === "online"
      ? "MESH ONLINE"
      : meshStatus === "idle"
        ? "MESH IDLE"
        : "SUPERVISOR OFFLINE";
  const statusHint =
    meshStatus === "offline"
      ? supervisorError || "Start MFA: fnn-testnet/start-live-mfa.ps1"
      : meshStatus === "idle"
        ? "MFA up · no sidecars connected"
        : `${peerCount} FA(s) on mesh`;

  return (
    <header className="panel-border flex shrink-0 flex-wrap items-center gap-4 border-x-0 border-t-0 px-4 py-3 lg:px-6">
      <div className="flex items-center gap-2">
        <LayoutDashboard size={18} className="text-sovereign-cyan" />
        <h1 className="font-sans text-sm font-semibold tracking-wide text-slate-100">
          MFA Command Console
        </h1>
      </div>

      <div className="hidden h-5 w-px bg-institutional-slate sm:block" />

      <div className="flex min-w-[200px] flex-1 items-center gap-3">
        <Gauge size={16} className="shrink-0 text-warning-amber" />
        <div className="min-w-0 flex-1">
          <div className="mb-1 flex items-center justify-between">
            <span className="telemetry-label">System backpressure (IOPS)</span>
            <span className="font-mono text-[10px] text-slate-400">
              <span className={iopsPct > 85 ? "text-warning-amber" : "text-slate-300"}>
                {iops.toLocaleString()}
              </span>
              /{iopsMax.toLocaleString()}
            </span>
          </div>
          <div className="h-1.5 overflow-hidden rounded-full bg-institutional-slate">
            <div
              className={`h-full transition-all duration-300 ${
                iopsPct > 85 ? "bg-warning-amber" : "bg-sovereign-cyan"
              }`}
              style={{ width: `${iopsPct}%` }}
            />
          </div>
        </div>
      </div>

      <div className="flex items-center gap-4">
        <div className="flex items-center gap-2">
          <Activity size={16} className={statusColor} />
          <div>
            <p className="telemetry-label">Network</p>
            <p className={`font-mono text-xs font-semibold ${statusColor}`}>{statusLabel}</p>
            <p className="hidden font-mono text-[9px] text-slate-500 lg:block">{statusHint}</p>
          </div>
        </div>
        <div className="hidden text-right sm:block">
          <p className="telemetry-label">Connected FAs</p>
          <p className="font-mono text-sm text-liquidity-mint">{peerCount}</p>
        </div>
        <div className="hidden text-right md:block">
          <p className="telemetry-label">Plugins</p>
          <p className={`font-mono text-sm ${runningPlugins > 0 ? "text-liquidity-mint" : "text-warning-amber"}`}>
            {mfaOnline ? runningPlugins : "—"}
          </p>
        </div>
      </div>
    </header>
  );
}

/**
 * @param {{
 *   children: React.ReactNode,
 *   activeView: string,
 *   onNavigate: (id: string) => void,
 *   telemetry?: { iops: number, iopsMax: number, supervisorOnline: boolean, meshStatus: 'online' | 'idle' | 'offline', peerCount: number, mfaOnline?: boolean, runningPlugins?: number, supervisorError?: string },
 * }} props
 */
export default function AppLayout({
  children,
  activeView,
  onNavigate,
  telemetry = { iops: 0, iopsMax: 16000, supervisorOnline: false, meshStatus: "offline", peerCount: 0, mfaOnline: false, runningPlugins: 0, supervisorError: "" },
}) {
  return (
    <div className="flex h-full min-h-screen flex-col bg-obsidian lg:flex-row">
      {/* Desktop sidebar */}
      <aside className="panel-border hidden w-60 shrink-0 flex-col border-y-0 border-l-0 lg:flex">
        <SidebarNav activeView={activeView} onNavigate={onNavigate} />
      </aside>

      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <TelemetryHeader {...telemetry} />
        <main className="min-h-0 flex-1 overflow-auto p-3 pb-20 lg:p-5 lg:pb-5">
          {children}
        </main>
      </div>

      {/* Mobile bottom rail */}
      <nav
        className="panel-border fixed inset-x-0 bottom-0 z-50 flex border-x-0 border-b-0 lg:hidden"
        aria-label="Mobile navigation"
      >
        {NAV.map(({ id, label, icon: Icon }) => {
          const active = activeView === id;
          return (
            <button
              key={id}
              type="button"
              onClick={() => onNavigate(id)}
              className={`flex flex-1 flex-col items-center gap-1 py-3 font-sans text-[10px] font-medium uppercase tracking-wider ${
                active ? "text-sovereign-cyan" : "text-slate-500"
              }`}
            >
              <Icon size={20} strokeWidth={1.75} />
              {label}
            </button>
          );
        })}
      </nav>
    </div>
  );
}
