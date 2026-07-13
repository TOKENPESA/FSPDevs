import { useCallback, useRef, useState } from "react";

/**
 * Destructive / state-altering actions require a 2-second hold.
 * @param {{
 *   children: React.ReactNode,
 *   onConfirm: () => void,
 *   holdMs?: number,
 *   className?: string,
 *   disabled?: boolean,
 * }} props
 */
export default function SecureButton({
  children,
  onConfirm,
  holdMs = 2000,
  className = "",
  disabled = false,
}) {
  const [progress, setProgress] = useState(0);
  const [holding, setHolding] = useState(false);
  const frameRef = useRef(0);
  const startRef = useRef(0);

  const cancelHold = useCallback(() => {
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
      cancelHold();
      onConfirm();
      return;
    }
    frameRef.current = requestAnimationFrame(tick);
  }, [cancelHold, holdMs, onConfirm]);

  const startHold = useCallback(() => {
    if (disabled) return;
    setHolding(true);
    startRef.current = performance.now();
    frameRef.current = requestAnimationFrame(tick);
  }, [disabled, tick]);

  return (
    <button
      type="button"
      disabled={disabled}
      onPointerDown={startHold}
      onPointerUp={cancelHold}
      onPointerLeave={cancelHold}
      onPointerCancel={cancelHold}
      className={`relative overflow-hidden rounded border border-red-900/60 bg-red-950/30 px-4 py-2 font-sans text-xs font-semibold uppercase tracking-wider text-red-300 transition-colors hover:border-red-700 disabled:cursor-not-allowed disabled:opacity-40 ${className}`}
    >
      <span
        className="pointer-events-none absolute inset-y-0 left-0 bg-red-600/25 transition-[width] duration-75"
        style={{ width: `${progress}%` }}
        aria-hidden="true"
      />
      <span className="relative z-10 flex items-center justify-center gap-2">
        {children}
        {holding && (
          <span className="font-mono text-[10px] text-red-200/80">
            [{Math.ceil(((100 - progress) / 100) * (holdMs / 1000))}s]
          </span>
        )}
      </span>
    </button>
  );
}
