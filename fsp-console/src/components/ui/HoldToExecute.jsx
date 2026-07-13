import { useCallback, useRef, useState } from "react";

const HOLD_MS = 2500;

/**
 * Destructive / irreversible actions — 2.5s hold with Warning Amber border loop.
 * @param {{
 *   children: React.ReactNode,
 *   onExecute: () => void,
 *   holdMs?: number,
 *   className?: string,
 *   disabled?: boolean,
 * }} props
 */
export default function HoldToExecute({
  children,
  onExecute,
  holdMs = HOLD_MS,
  className = "",
  disabled = false,
}) {
  const [progress, setProgress] = useState(0);
  const [holding, setHolding] = useState(false);
  const frameRef = useRef(0);
  const startRef = useRef(0);

  const cancel = useCallback(() => {
    if (frameRef.current) cancelAnimationFrame(frameRef.current);
    frameRef.current = 0;
    startRef.current = 0;
    setHolding(false);
    setProgress(0);
  }, []);

  const tick = useCallback(() => {
    const elapsed = performance.now() - startRef.current;
    const pct = Math.min(100, (elapsed / holdMs) * 100);
    setProgress(pct);
    if (pct >= 100) {
      cancel();
      onExecute();
      return;
    }
    frameRef.current = requestAnimationFrame(tick);
  }, [cancel, holdMs, onExecute]);

  const start = useCallback(() => {
    if (disabled) return;
    setHolding(true);
    startRef.current = performance.now();
    frameRef.current = requestAnimationFrame(tick);
  }, [disabled, tick]);

  const perimeter = 2 * (120 + 36);

  return (
    <button
      type="button"
      disabled={disabled}
      onPointerDown={start}
      onPointerUp={cancel}
      onPointerLeave={cancel}
      onPointerCancel={cancel}
      className={`relative min-h-[44px] min-w-[120px] border border-warning-amber/40 bg-obsidian px-4 py-3 font-sans text-xs font-semibold uppercase tracking-wider text-warning-amber transition-colors hover:border-warning-amber disabled:cursor-not-allowed disabled:opacity-40 ${className}`}
    >
      <svg
        className="pointer-events-none absolute inset-0 h-full w-full"
        aria-hidden="true"
      >
        <rect
          x="1"
          y="1"
          width="calc(100% - 2px)"
          height="calc(100% - 2px)"
          fill="none"
          stroke="rgba(245, 158, 11, 0.25)"
          strokeWidth="2"
          rx="0"
        />
        <rect
          x="1"
          y="1"
          width="calc(100% - 2px)"
          height="calc(100% - 2px)"
          fill="none"
          stroke="#F59E0B"
          strokeWidth="2"
          strokeDasharray={perimeter}
          strokeDashoffset={perimeter - (perimeter * progress) / 100}
          rx="0"
          style={{ transition: holding ? "none" : "stroke-dashoffset 0.15s" }}
        />
      </svg>
      <span className="relative z-10 flex flex-col items-center gap-1">
        {children}
        {holding && (
          <span className="font-mono text-[10px] tabular-nums text-warning-amber/80">
            [ {((holdMs - (progress / 100) * holdMs) / 1000).toFixed(1)}s ]
          </span>
        )}
      </span>
    </button>
  );
}
