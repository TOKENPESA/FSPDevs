import { useEffect, useRef, useState } from "react";
import { Radio, Terminal } from "lucide-react";

const SEED_LOG = [
  {
    ts: "18:14:02.441",
    hop: "FA-12→FA-44→MFA",
    x402: "x402-v2.1",
    ms: 38,
    shannons: "2,400,000",
    status: "SETTLED",
  },
  {
    ts: "18:14:03.102",
    hop: "FA-07→FA-19→FA-33",
    x402: "x402-v2.1",
    ms: 52,
    shannons: "890,000",
    status: "CLEARING",
  },
  {
    ts: "18:14:03.887",
    hop: "MFA→CKB-L2",
    x402: "x402-v2.0",
    ms: 124,
    shannons: "15,000,000",
    status: "HTLC_LOCK",
  },
];

function TopologyCanvas() {
  const canvasRef = useRef(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let frame = 0;
    let raf = 0;

    const draw = () => {
      const { width, height } = canvas;
      ctx.fillStyle = "#0B0F17";
      ctx.fillRect(0, 0, width, height);

      const cx = width / 2;
      const cy = height / 2;
      const t = frame * 0.012;

      // Grid
      ctx.strokeStyle = "rgba(30, 41, 59, 0.6)";
      ctx.lineWidth = 1;
      for (let x = 0; x < width; x += 32) {
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, height);
        ctx.stroke();
      }
      for (let y = 0; y < height; y += 32) {
        ctx.beginPath();
        ctx.moveTo(0, y);
        ctx.lineTo(width, y);
        ctx.stroke();
      }

      // Hub node (MFA)
      ctx.beginPath();
      ctx.arc(cx, cy, 14, 0, Math.PI * 2);
      ctx.fillStyle = "#06B6D4";
      ctx.shadowColor = "#06B6D4";
      ctx.shadowBlur = 16;
      ctx.fill();
      ctx.shadowBlur = 0;

      // Orbiting FA nodes
      const nodes = 8;
      for (let i = 0; i < nodes; i += 1) {
        const angle = (i / nodes) * Math.PI * 2 + t;
        const r = Math.min(width, height) * 0.32;
        const nx = cx + Math.cos(angle) * r;
        const ny = cy + Math.sin(angle) * r;

        ctx.strokeStyle = "rgba(6, 182, 212, 0.35)";
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(cx, cy);
        ctx.lineTo(nx, ny);
        ctx.stroke();

        ctx.beginPath();
        ctx.arc(nx, ny, 6, 0, Math.PI * 2);
        ctx.fillStyle = i % 3 === 0 ? "#10B981" : "#1E293B";
        ctx.strokeStyle = "#1E293B";
        ctx.lineWidth = 1;
        ctx.fill();
        ctx.stroke();
      }

      ctx.font = "11px JetBrains Mono, monospace";
      ctx.fillStyle = "#64748b";
      ctx.fillText("[ TOPOLOGY PLACEHOLDER · WebGL/r3f ]", 12, height - 12);

      frame += 1;
      raf = requestAnimationFrame(draw);
    };

    const resize = () => {
      const rect = canvas.parentElement?.getBoundingClientRect();
      if (!rect) return;
      canvas.width = rect.width * devicePixelRatio;
      canvas.height = rect.height * devicePixelRatio;
      canvas.style.width = `${rect.width}px`;
      canvas.style.height = `${rect.height}px`;
      ctx.setTransform(devicePixelRatio, 0, 0, devicePixelRatio, 0, 0);
    };

    resize();
    draw();
    window.addEventListener("resize", resize);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener("resize", resize);
    };
  }, []);

  return (
    <canvas
      ref={canvasRef}
      className="h-full w-full"
      aria-label="Live network topology graph placeholder"
    />
  );
}

function LogLine({ entry }) {
  const statusColor =
    entry.status === "SETTLED"
      ? "text-liquidity-mint"
      : entry.status === "HTLC_LOCK"
        ? "text-warning-amber"
        : "text-sovereign-cyan";

  return (
    <div className="terminal-line border-b border-institutional-slate/50 px-3 py-2 hover:bg-institutional-slate/20">
      <span className="text-slate-600">{entry.ts}</span>{" "}
      <span className="text-sovereign-cyan">{entry.hop}</span>{" "}
      <span className="text-slate-500">|</span>{" "}
      <span className="text-slate-400">{entry.x402}</span>{" "}
      <span className="text-slate-500">|</span>{" "}
      <span className="text-slate-300">{entry.ms}ms</span>{" "}
      <span className="text-slate-500">|</span>{" "}
      <span className="text-slate-200">{entry.shannons} sh</span>{" "}
      <span className={`ml-2 ${statusColor}`}>[ {entry.status} ]</span>
    </div>
  );
}

export default function GlobalMonitor() {
  const [logs, setLogs] = useState(SEED_LOG);
  const [streamState, setStreamState] = useState("LIVE");

  useEffect(() => {
    const id = setInterval(() => {
      const now = new Date();
      const ts = now.toTimeString().slice(0, 12);
      setLogs((prev) => {
        const next = [
          {
            ts,
            hop: `FA-${Math.floor(Math.random() * 64) + 1}→MFA`,
            x402: "x402-v2.1",
            ms: Math.floor(Math.random() * 80) + 20,
            shannons: (Math.floor(Math.random() * 5_000_000) + 100_000).toLocaleString(),
            status: Math.random() > 0.7 ? "HTLC_LOCK" : "SETTLED",
          },
          ...prev,
        ];
        return next.slice(0, 48);
      });
    }, 3200);
    return () => clearInterval(id);
  }, []);

  return (
    <section className="flex h-full min-h-[520px] flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h2 className="font-sans text-lg font-semibold text-slate-100">Global Matrix Monitor</h2>
          <p className="font-mono text-xs text-slate-500">
            ring-1024 · sovereign-cyan tunnels · X402 clearing trace
          </p>
        </div>
        <div className="flex items-center gap-2 rounded border border-institutional-slate px-3 py-1.5">
          <Radio size={14} className="animate-pulse-slow text-liquidity-mint" />
          <span className="font-mono text-[10px] text-liquidity-mint">[ {streamState} ]</span>
        </div>
      </div>

      <div className="panel-border flex min-h-0 flex-1 flex-col overflow-hidden lg:flex-row">
        {/* Left — topology canvas */}
        <div className="relative min-h-[240px] flex-1 border-b border-institutional-slate lg:min-h-0 lg:border-b-0 lg:border-r">
          <div className="absolute left-3 top-3 z-10 flex items-center gap-2 rounded border border-institutional-slate bg-obsidian/90 px-2 py-1">
            <NetworkIcon />
            <span className="telemetry-label">Topology</span>
          </div>
          <TopologyCanvas />
        </div>

        {/* Right — terminal log */}
        <div className="flex w-full flex-col lg:w-[min(440px,42%)]">
          <div className="flex items-center gap-2 border-b border-institutional-slate px-3 py-2">
            <Terminal size={14} className="text-sovereign-cyan" />
            <span className="telemetry-label">Multi-hop clearing log</span>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto bg-obsidian/80">
            {logs.length === 0 ? (
              <p className="p-4 font-mono text-xs text-warning-amber">
                [ AWAITING ZK CLEARANCE ]
              </p>
            ) : (
              logs.map((entry, i) => <LogLine key={`${entry.ts}-${i}`} entry={entry} />)
            )}
          </div>
        </div>
      </div>
    </section>
  );
}

function NetworkIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#06B6D4" strokeWidth="1.75">
      <circle cx="5" cy="5" r="2" />
      <circle cx="19" cy="5" r="2" />
      <circle cx="12" cy="12" r="2.5" />
      <circle cx="5" cy="19" r="2" />
      <circle cx="19" cy="19" r="2" />
      <path d="M7 5h10M5 7v10M19 7v10M7 19h10" />
    </svg>
  );
}
