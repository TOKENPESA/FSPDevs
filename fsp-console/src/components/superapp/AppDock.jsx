import { useProfile } from "../../context/ProfileContext.jsx";

/** @typedef {{ id: string, index: string, label: string, shortLabel: string }} DockApp */

/** @type {DockApp[]} */
export const DOCK_APPS = [
  { id: "vault", index: "01", label: "Core Vault", shortLabel: "Vault" },
  { id: "jungukuu", index: "02", label: "JunguKuu", shortLabel: "DICOBA" },
  { id: "lume", index: "03", label: "LUME Trade", shortLabel: "LUME" },
  { id: "float-bridge", index: "04", label: "Float Bridge", shortLabel: "Float" },
  { id: "sovereign", index: "05", label: "Sovereign Mesh", shortLabel: "OOB" },
];

/**
 * @param {{
 *   activeId: string,
 *   onSelect: (id: string) => void,
 *   layout?: 'side' | 'bottom',
 * }} props
 */
export default function AppDock({ activeId, onSelect, layout = "side" }) {
  const { isRetail } = useProfile();
  const isBottom = layout === "bottom" || isRetail;

  if (isBottom) {
    return (
      <nav
        className="panel-border fixed inset-x-0 bottom-0 z-50 grid grid-cols-5 border-x-0 border-b-0 bg-obsidian lg:static lg:grid-cols-1 lg:border lg:border-institutional-slate"
        aria-label="Super App dock"
      >
        {DOCK_APPS.map((app) => {
          const active = app.id === activeId;
          return (
            <button
              key={app.id}
              type="button"
              onClick={() => onSelect(app.id)}
              className={`flex min-h-[56px] flex-col items-center justify-center gap-1 border-t-2 px-1 py-2 font-sans text-[10px] font-semibold uppercase tracking-wide transition-colors lg:min-h-[52px] lg:flex-row lg:justify-start lg:gap-3 lg:border-l-2 lg:border-t-0 lg:px-4 lg:text-left ${
                active
                  ? "border-sovereign-cyan bg-sovereign-cyan/10 text-sovereign-cyan"
                  : "border-transparent text-slate-500 hover:bg-institutional-slate/40 hover:text-slate-200"
              }`}
            >
              <span className="font-mono text-[9px] tabular-nums opacity-70">{app.index}</span>
              <span className="truncate">{app.shortLabel}</span>
            </button>
          );
        })}
      </nav>
    );
  }

  return (
    <nav
      className="panel-border flex w-full shrink-0 flex-col border-y-0 border-l-0 lg:w-52"
      aria-label="Super App dock"
    >
      <div className="border-b border-institutional-slate px-4 py-3">
        <p className="telemetry-label">Workspace mux</p>
        <p className="mt-1 font-mono text-xs text-sovereign-cyan">MINI-APP DOCK</p>
      </div>
      {DOCK_APPS.map((app) => {
        const active = app.id === activeId;
        return (
          <button
            key={app.id}
            type="button"
            onClick={() => onSelect(app.id)}
            className={`flex items-center gap-3 border-l-2 px-4 py-3 text-left transition-colors ${
              active
                ? "border-sovereign-cyan bg-sovereign-cyan/10 text-sovereign-cyan"
                : "border-transparent text-slate-400 hover:bg-institutional-slate/30 hover:text-slate-100"
            }`}
          >
            <span className="font-mono text-[10px] tabular-nums text-slate-500">{app.index}</span>
            <span className="font-sans text-sm font-medium">{app.label}</span>
          </button>
        );
      })}
    </nav>
  );
}
