import { useEffect, useState } from "react";
import { Settings2, X } from "lucide-react";

/**
 * @param {{
 *   open: boolean,
 *   moduleName: string,
 *   configJson: string,
 *   onClose: () => void,
 *   onSave: (json: string) => void,
 *   error?: string,
 * }} props
 */
export default function ConfigureModal({
  open,
  moduleName,
  configJson,
  onClose,
  onSave,
  error = "",
}) {
  const [draft, setDraft] = useState(configJson);

  useEffect(() => {
    if (open) setDraft(configJson);
  }, [open, configJson]);

  if (!open) return null;

  const handleSave = () => {
    try {
      JSON.parse(draft || "{}");
      onSave(draft.trim() || "{}");
    } catch {
      onSave("__INVALID__");
    }
  };

  return (
    <div
      className="fixed inset-0 z-[100] flex items-end justify-center bg-black/70 p-4 sm:items-center"
      role="dialog"
      aria-modal="true"
      aria-labelledby="configure-modal-title"
    >
      <div className="panel-border w-full max-w-lg rounded-lg">
        <div className="flex items-center justify-between border-b border-institutional-slate px-4 py-3">
          <div className="flex items-center gap-2">
            <Settings2 size={16} className="text-sovereign-cyan" />
            <h3 id="configure-modal-title" className="font-sans text-sm font-semibold text-slate-100">
              Configure · <span className="font-mono text-sovereign-cyan">{moduleName}</span>
            </h3>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded p-1 text-slate-500 hover:bg-institutional-slate hover:text-slate-200"
            aria-label="Close"
          >
            <X size={18} />
          </button>
        </div>

        <div className="space-y-3 p-4">
          <p className="font-sans text-xs text-slate-400">
            JSON configuration payload — hot-mounted without supervisor restart.
          </p>
          <textarea
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            spellCheck={false}
            rows={12}
            className="w-full resize-y rounded border border-institutional-slate bg-obsidian px-3 py-2 font-mono text-xs text-slate-200 outline-none ring-sovereign-cyan/40 focus:ring-1"
            placeholder='{ "critical_capacity_floor": 1000000 }'
          />
          {error && (
            <p className="font-mono text-xs text-red-400">[ CONFIG REJECTED ] {error}</p>
          )}
        </div>

        <div className="flex justify-end gap-2 border-t border-institutional-slate px-4 py-3">
          <button
            type="button"
            onClick={onClose}
            className="rounded border border-institutional-slate px-4 py-2 font-sans text-xs font-medium text-slate-400 hover:text-slate-200"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleSave}
            className="rounded border border-sovereign-cyan/50 bg-sovereign-cyan/10 px-4 py-2 font-sans text-xs font-semibold uppercase tracking-wider text-sovereign-cyan hover:bg-sovereign-cyan/20"
          >
            Apply config
          </button>
        </div>
      </div>
    </div>
  );
}
