/**
 * Split capacity ribbon — outbound (mint) left, inbound (slate) right.
 * @param {{
 *   outboundShannons: number,
 *   inboundShannons: number,
 *   maxShannons: number,
 *   label?: string,
 * }} props
 */
export default function ChannelCapacityRibbon({
  outboundShannons,
  inboundShannons,
  maxShannons,
  label = "Channel capacity",
}) {
  const safeMax = Math.max(maxShannons, 1);
  const outboundPct = Math.min(100, (outboundShannons / safeMax) * 100);
  const inboundPct = Math.min(100, (inboundShannons / safeMax) * 100);

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-3">
        <span className="telemetry-label">{label}</span>
        <span className="font-mono text-[10px] text-slate-500">
          MAX <span className="text-slate-300">{maxShannons.toLocaleString()}</span> shannons
        </span>
      </div>
      <div className="relative h-3 overflow-hidden rounded-sm border border-institutional-slate bg-obsidian">
        <div
          className="absolute inset-y-0 left-0 bg-liquidity-mint shadow-glowMint transition-all duration-500"
          style={{ width: `${outboundPct}%` }}
          title={`Outbound ${outboundShannons.toLocaleString()} shannons`}
        />
        <div
          className="absolute inset-y-0 right-0 bg-institutional-slate transition-all duration-500"
          style={{ width: `${inboundPct}%` }}
          title={`Inbound ${inboundShannons.toLocaleString()} shannons`}
        />
        <div className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-slate-600/80" />
      </div>
      <div className="flex justify-between font-mono text-[10px]">
        <span className="text-liquidity-mint">
          OUT ↑ {outboundShannons.toLocaleString()}
        </span>
        <span className="text-slate-400">
          IN ↓ {inboundShannons.toLocaleString()}
        </span>
      </div>
    </div>
  );
}
