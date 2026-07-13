import { useEffect, useState } from "react";
import QrPayloadDisplay from "./QrPayloadDisplay.jsx";
import HoldToExecute from "../ui/HoldToExecute.jsx";

const SAMPLE_PAYLOAD =
  "fsp:oob:v1:fa44:guarantor_sig:ckb:2400000:exp:1752345600:hash:9f3ac812";

export default function OobFallbackPanel() {
  const [status, setStatus] = useState("[ AWAITING PEER OUT-OF-BAND PROTOCOL SCAN ]");
  const [htlcSeconds, setHtlcSeconds] = useState(847);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    const id = setInterval(() => {
      setHtlcSeconds((s) => (s > 0 ? s - 1 : 3600));
    }, 1000);
    return () => clearInterval(id);
  }, []);

  const htlcPct = ((3600 - htlcSeconds) / 3600) * 100;
  const mins = Math.floor(htlcSeconds / 60);
  const secs = htlcSeconds % 60;

  const generatePayload = () => {
    setBusy(true);
    setStatus("[ BROADCASTING LOCAL ISOMORPHIC CELL TO PEER ]");
    setTimeout(() => {
      setBusy(false);
      setStatus("[ AWAITING PEER OUT-OF-BAND PROTOCOL SCAN ]");
    }, 1800);
  };

  const sweepLiquidity = () => {
    setStatus("[ RECORDING SECURE LOCAL SQLITE ENTRY ]");
    setTimeout(() => setStatus("[ AWAITING PEER OUT-OF-BAND PROTOCOL SCAN ]"), 1200);
  };

  return (
    <section className="panel-border border-warning-amber/30 p-4">
      <header className="mb-4 border-b border-institutional-slate pb-3">
        <h3 className="font-sans text-sm font-semibold text-slate-100">
          Out-of-band fallback panel
        </h3>
        <p className="mt-1 font-mono text-xs text-warning-amber">{status}</p>
      </header>

      <div className="grid gap-6 lg:grid-cols-[auto_1fr]">
        <div className="flex flex-col items-center gap-4">
          <QrPayloadDisplay payload={SAMPLE_PAYLOAD} size={168} />
          <button
            type="button"
            onClick={generatePayload}
            disabled={busy}
            className="w-full border border-sovereign-cyan/50 bg-sovereign-cyan/10 px-4 py-3 font-sans text-xs font-semibold uppercase tracking-wider text-sovereign-cyan hover:bg-sovereign-cyan/20 disabled:opacity-50"
          >
            Regenerate signed payload
          </button>
        </div>

        <div className="space-y-4">
          <div className="border border-institutional-slate p-4">
            <p className="telemetry-label">HTLC lock expiration · ExpiringLockManager</p>
            <div className="mt-3 flex items-center gap-4">
              <div
                className="relative flex h-20 w-20 shrink-0 items-center justify-center rounded-full border-2 border-institutional-slate"
                style={{
                  background: `conic-gradient(#F59E0B ${htlcPct}%, #1E293B ${htlcPct}%)`,
                }}
              >
                <div className="flex h-14 w-14 flex-col items-center justify-center bg-obsidian">
                  <span className="font-mono text-sm font-bold tabular-nums text-warning-amber">
                    {mins}:{String(secs).padStart(2, "0")}
                  </span>
                </div>
              </div>
              <div>
                <p className="font-mono text-xs text-slate-300">
                  Local lock <span className="text-warning-amber">#{String(htlcSeconds).padStart(4, "0")}</span>
                </p>
                <p className="mt-1 font-sans text-xs text-slate-500">
                  Managed natively by sidecar — no MFA ticket required
                </p>
              </div>
            </div>
          </div>

          <div className="border border-institutional-slate p-4 font-mono text-xs text-slate-400">
            <p className="text-slate-500">Payload envelope</p>
            <pre className="mt-2 overflow-x-auto whitespace-pre-wrap break-all text-sovereign-cyan">
              {SAMPLE_PAYLOAD}
            </pre>
          </div>

          <HoldToExecute onExecute={sweepLiquidity} className="w-full">
            Manual liquidity sweep
          </HoldToExecute>
        </div>
      </div>
    </section>
  );
}
