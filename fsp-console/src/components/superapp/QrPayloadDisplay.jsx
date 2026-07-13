/**
 * Minimal QR placeholder — encodes payload hash visually (production: wire to Tauri QR SVG).
 * @param {{ payload: string, size?: number }} props
 */
export default function QrPayloadDisplay({ payload, size = 160 }) {
  const cells = 21;
  const hash = payload.split("").reduce((a, c) => a + c.charCodeAt(0), 0);
  const cellSize = size / cells;

  return (
    <div className="inline-block border border-institutional-slate bg-white p-2">
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} role="img" aria-label="Signed OOB payload QR">
        <rect width={size} height={size} fill="#ffffff" />
        {Array.from({ length: cells * cells }, (_, i) => {
          const row = Math.floor(i / cells);
          const col = i % cells;
          const on = ((hash + row * 17 + col * 31) % 5) > 1;
          if (!on) return null;
          return (
            <rect
              key={i}
              x={col * cellSize}
              y={row * cellSize}
              width={cellSize - 0.5}
              height={cellSize - 0.5}
              fill="#0B0F17"
            />
          );
        })}
        <rect x={0} y={0} width={cellSize * 7} height={cellSize * 7} fill="none" stroke="#0B0F17" strokeWidth={cellSize} />
        <rect x={(cells - 7) * cellSize} y={0} width={cellSize * 7} height={cellSize * 7} fill="none" stroke="#0B0F17" strokeWidth={cellSize} />
        <rect x={0} y={(cells - 7) * cellSize} width={cellSize * 7} height={cellSize * 7} fill="none" stroke="#0B0F17" strokeWidth={cellSize} />
      </svg>
      <p className="mt-2 max-w-[200px] truncate font-mono text-[9px] tabular-nums text-slate-600">
        {payload.slice(0, 42)}…
      </p>
    </div>
  );
}
