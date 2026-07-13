import { useState } from "react";
import { Activity, Gauge } from "lucide-react";
import { useProfile } from "../../context/ProfileContext.jsx";
import { useMfaRuntime } from "../../hooks/useMfaRuntime.js";
import AppDock from "./AppDock.jsx";
import StandaloneMeshController from "./StandaloneMeshController.jsx";
import VaultMiniApp from "./miniapps/VaultMiniApp.jsx";
import JunguKuuMiniApp from "./miniapps/JunguKuuMiniApp.jsx";
import LumeTradeMiniApp from "./miniapps/LumeTradeMiniApp.jsx";
import FloatBridgeMiniApp from "./miniapps/FloatBridgeMiniApp.jsx";

/** @type {Record<string, React.ComponentType>} */
const MINI_APPS = {
  vault: VaultMiniApp,
  jungukuu: JunguKuuMiniApp,
  lume: LumeTradeMiniApp,
  "float-bridge": FloatBridgeMiniApp,
  sovereign: StandaloneMeshController,
};

function TreasuryTelemetryBar() {
  const { runtime } = useMfaRuntime();
  const iops = runtime.online ? 12480 + runtime.connectedAgents * 120 : 0;
  const iopsPct = Math.min(100, (iops / 16000) * 100);

  return (
    <header className="panel-border flex shrink-0 flex-wrap items-center gap-4 border-x-0 border-t-0 px-4 py-3">
      <div>
        <p className="telemetry-label">Super App · Fiber Sidecar Console</p>
        <p className="font-mono text-xs text-sovereign-cyan">TREASURY_HUB · workspace mux</p>
      </div>
      <div className="hidden h-8 w-px bg-institutional-slate md:block" />
      <div className="flex min-w-[180px] flex-1 items-center gap-2">
        <Gauge size={14} className="text-warning-amber" />
        <div className="min-w-0 flex-1">
          <div className="mb-1 flex justify-between font-mono text-[10px]">
            <span className="text-slate-500">Backpressure IOPS</span>
            <span className="tabular-nums text-slate-300">{iops.toLocaleString()}</span>
          </div>
          <div className="h-1 bg-institutional-slate">
            <div
              className="h-full bg-sovereign-cyan transition-all"
              style={{ width: `${iopsPct}%` }}
            />
          </div>
        </div>
      </div>
      <div className="flex items-center gap-2">
        <Activity
          size={14}
          className={runtime.online ? "text-liquidity-mint" : "text-red-400"}
        />
        <span className="font-mono text-[10px] tabular-nums text-slate-400">
          MFA {runtime.online ? "LINKED" : "SEVERED"} · {runtime.connectedAgents} FA
        </span>
      </div>
    </header>
  );
}

function RetailHeader() {
  return (
    <header className="panel-border border-x-0 border-t-0 px-4 py-4">
      <p className="font-mono text-lg font-bold text-sovereign-cyan">FSP RETAIL</p>
      <p className="font-sans text-sm text-slate-400">Tap a module below · sovereign checkout ready</p>
    </header>
  );
}

export default function SuperAppConsole() {
  const { isRetail, isTreasury, profile } = useProfile();
  const [activeApp, setActiveApp] = useState(isRetail ? "sovereign" : "vault");

  const ActiveView = MINI_APPS[activeApp] ?? VaultMiniApp;

  return (
    <div className="flex h-full min-h-screen flex-col bg-obsidian">
      {isTreasury ? <TreasuryTelemetryBar /> : <RetailHeader />}

      <div
        className={`flex min-h-0 flex-1 flex-col ${
          isRetail ? "pb-[72px] lg:pb-0" : "lg:flex-row"
        }`}
      >
        {!isRetail && (
          <div className="hidden lg:block">
            <AppDock activeId={activeApp} onSelect={setActiveApp} layout="side" />
          </div>
        )}

        <main
          className={`min-h-0 flex-1 overflow-auto p-3 md:p-5 ${
            isTreasury ? "lg:grid lg:grid-cols-1 xl:gap-0" : ""
          }`}
        >
          <div
            className={
              isTreasury && activeApp !== "sovereign"
                ? "mx-auto max-w-7xl"
                : "mx-auto max-w-7xl w-full"
            }
          >
            <ActiveView />
          </div>
        </main>
      </div>

      <div className={isRetail ? "lg:hidden" : "lg:hidden"}>
        <AppDock activeId={activeApp} onSelect={setActiveApp} layout="bottom" />
      </div>

      {isRetail && (
        <div className="hidden lg:block">
          <AppDock activeId={activeApp} onSelect={setActiveApp} layout="side" />
        </div>
      )}

      <footer className="hidden border-t border-institutional-slate px-4 py-2 font-mono text-[9px] text-slate-600 md:block">
        PROFILE={profile} · viewport adaptive · mini-app sandbox
      </footer>
    </div>
  );
}
