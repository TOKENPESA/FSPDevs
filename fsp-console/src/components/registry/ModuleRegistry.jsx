import { useCallback, useEffect, useState } from "react";
import { Cpu, Download, Power, RefreshCw } from "lucide-react";
import ConfigureModal from "./ConfigureModal.jsx";
import SecureButton from "../ui/SecureButton.jsx";
import {
  mergePluginRegistry,
  mfaModuleApi,
} from "../../api/mfa.js";

function HardwareToggle({ checked, disabled, onChange, moduleId }) {
  return (
    <label
      className={`flex cursor-pointer items-center gap-3 ${disabled ? "cursor-not-allowed opacity-50" : ""}`}
      htmlFor={`toggle-${moduleId}`}
    >
      <span className="telemetry-label">Hardware mount</span>
      <button
        id={`toggle-${moduleId}`}
        type="button"
        role="switch"
        aria-checked={checked}
        disabled={disabled}
        onClick={() => !disabled && onChange(!checked)}
        className={`relative h-7 w-14 shrink-0 rounded-sm border transition-colors ${
          checked
            ? "border-liquidity-mint/60 bg-liquidity-mint/20"
            : "border-institutional-slate bg-institutional-slate/50"
        }`}
      >
        <span
          className={`absolute top-0.5 flex h-5 w-5 items-center justify-center rounded-sm transition-all ${
            checked
              ? "left-[calc(100%-1.375rem)] bg-liquidity-mint text-obsidian shadow-glowMint"
              : "left-0.5 bg-slate-600 text-slate-300"
          }`}
        >
          <Power size={12} strokeWidth={2.5} />
        </span>
      </button>
      <span className={`font-mono text-[10px] ${checked ? "text-liquidity-mint" : "text-slate-500"}`}>
        {checked ? "[ MOUNTED ]" : "[ UNMOUNTED ]"}
      </span>
    </label>
  );
}

function ModuleCard({
  module,
  loadState,
  onToggle,
  onInstall,
  onConfigure,
  onUninstall,
}) {
  const kindColor =
    module.kind === "clearing" ? "text-warning-amber" : "text-sovereign-cyan";

  return (
    <article className="panel-border flex flex-col rounded-lg p-4">
      <div className="mb-3 flex items-start justify-between gap-2">
        <div className="flex items-center gap-2">
          <Cpu size={16} className={kindColor} />
          <div>
            <h3 className="font-mono text-sm font-semibold text-slate-100">{module.name}</h3>
            <p className={`font-mono text-[10px] uppercase ${kindColor}`}>{module.id}</p>
          </div>
        </div>
        <span
          className={`rounded border px-2 py-0.5 font-mono text-[10px] ${
            module.mounted
              ? "border-liquidity-mint/40 text-liquidity-mint"
              : module.installed
                ? "border-warning-amber/40 text-warning-amber"
                : "border-slate-600 text-slate-500"
          }`}
        >
          {module.mounted ? "ACTIVE" : module.installed ? "PAUSED" : "AVAILABLE"}
        </span>
      </div>

      <p className="mb-4 flex-1 font-sans text-xs leading-relaxed text-slate-400">
        {module.description}
      </p>

      {loadState && (
        <p className="mb-3 font-mono text-[10px] text-warning-amber">{loadState}</p>
      )}

      {module.installed ? (
        <HardwareToggle
          moduleId={module.id}
          checked={module.mounted}
          disabled={Boolean(loadState)}
          onChange={(next) => onToggle(module.id, next)}
        />
      ) : (
        <p className="font-mono text-[10px] text-slate-500">[ NOT IN REGISTRY ]</p>
      )}

      <div className="mt-4 flex flex-wrap gap-2 border-t border-institutional-slate pt-4">
        {!module.installed ? (
          <button
            type="button"
            onClick={() => onInstall(module)}
            disabled={Boolean(loadState)}
            className="flex flex-1 items-center justify-center gap-2 rounded border border-liquidity-mint/40 bg-liquidity-mint/10 px-3 py-2 font-sans text-xs font-medium text-liquidity-mint hover:bg-liquidity-mint/20 disabled:opacity-50"
          >
            <Download size={14} />
            Install
          </button>
        ) : (
          <>
            <button
              type="button"
              onClick={() => onConfigure(module)}
              className="flex flex-1 items-center justify-center gap-2 rounded border border-institutional-slate px-3 py-2 font-sans text-xs font-medium text-slate-300 hover:border-sovereign-cyan/40 hover:text-sovereign-cyan"
            >
              Configure
            </button>
            <SecureButton onConfirm={() => onUninstall(module.id)} className="flex-1">
              Uninstall
            </SecureButton>
          </>
        )}
      </div>
    </article>
  );
}

export default function ModuleRegistry() {
  const [modules, setModules] = useState([]);
  const [syncState, setSyncState] = useState("[ SYNCING REGISTRY ]");
  const [loadError, setLoadError] = useState("");
  const [pending, setPending] = useState(/** @type {Record<string, string>} */ ({}));
  const [configureTarget, setConfigureTarget] = useState(null);
  const [configError, setConfigError] = useState("");

  const withPending = useCallback((id, message) => {
    setPending((prev) => ({ ...prev, [id]: message }));
  }, []);

  const clearPending = useCallback((id) => {
    setPending((prev) => {
      const next = { ...prev };
      delete next[id];
      return next;
    });
  }, []);

  const refresh = useCallback(async () => {
    setSyncState("[ SYNCING REGISTRY ]");
    setLoadError("");
    try {
      const [catalog, installed] = await Promise.all([
        mfaModuleApi.fetchCatalog(),
        mfaModuleApi.fetchInstalled(),
      ]);
      setModules(mergePluginRegistry(catalog, installed));
      setSyncState(
        `Synced · ${installed.length} installed · ${catalog.length} in catalog`,
      );
    } catch (err) {
      const message = err instanceof Error ? err.message : "MFA registry unreachable";
      setLoadError(message);
      setSyncState("[ MFA OFFLINE ]");
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const handleToggle = useCallback(
    async (id, mount) => {
      withPending(id, mount ? "[ HTLC LOCK PENDING ]" : "[ DRAINING MOUNT ]");
      try {
        await mfaModuleApi.toggleModule(id, mount);
        await refresh();
      } catch (err) {
        setLoadError(err instanceof Error ? err.message : "Toggle failed");
      } finally {
        clearPending(id);
      }
    },
    [clearPending, refresh, withPending],
  );

  const handleInstall = useCallback(
    (module) => {
      setConfigureTarget(module);
      setConfigError("");
    },
    [],
  );

  const handleUninstall = useCallback(
    async (id) => {
      withPending(id, "[ AWAITING ZK CLEARANCE ]");
      try {
        await mfaModuleApi.uninstallModule(id);
        await refresh();
      } catch (err) {
        setLoadError(err instanceof Error ? err.message : "Uninstall failed");
      } finally {
        clearPending(id);
      }
    },
    [clearPending, refresh, withPending],
  );

  const handleSaveConfig = useCallback(
    async (json) => {
      if (!configureTarget) return;
      if (json === "__INVALID__") {
        setConfigError("Invalid JSON — rejected by schema validator");
        return;
      }
      let parsed = {};
      try {
        parsed = JSON.parse(json || "{}");
      } catch {
        setConfigError("Invalid JSON — rejected by schema validator");
        return;
      }
      withPending(configureTarget.id, "[ MOUNTING PLUGIN ]");
      try {
        await mfaModuleApi.installModule(configureTarget.id, parsed);
        setConfigureTarget(null);
        setConfigError("");
        await refresh();
      } catch (err) {
        setConfigError(err instanceof Error ? err.message : "Install failed");
      } finally {
        clearPending(configureTarget.id);
      }
    },
    [clearPending, configureTarget, refresh, withPending],
  );

  return (
    <section className="space-y-4">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="font-sans text-lg font-semibold text-slate-100">
            MFA Policy Registry & Hot-Swap
          </h2>
          <p className="font-mono text-xs text-slate-500">
            Live supervisor plugins · 127.0.0.1:1025/api/modules/*
          </p>
        </div>
        <div className="flex items-center gap-3">
          <span className="font-mono text-[10px] text-sovereign-cyan">{syncState}</span>
          <button
            type="button"
            onClick={() => void refresh()}
            className="flex items-center gap-2 rounded border border-institutional-slate px-3 py-2 font-sans text-xs font-medium text-slate-300 hover:border-sovereign-cyan/40 hover:text-sovereign-cyan"
          >
            <RefreshCw size={14} />
            Refresh
          </button>
        </div>
      </header>

      {loadError && (
        <div className="panel-border rounded-lg border-red-900/40 p-4">
          <p className="font-mono text-sm text-red-400">[ REGISTRY FETCH FAILED ]</p>
          <p className="mt-1 font-sans text-xs text-slate-400">{loadError}</p>
        </div>
      )}

      {modules.length === 0 && !loadError ? (
        <div className="panel-border rounded-lg p-8 text-center">
          <p className="font-mono text-sm text-warning-amber">[ AWAITING ZK CLEARANCE ]</p>
          <p className="mt-2 font-sans text-xs text-slate-500">Loading MFA plugin catalog…</p>
        </div>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-2 2xl:grid-cols-4">
          {modules.map((module) => (
            <ModuleCard
              key={module.id}
              module={module}
              loadState={pending[module.id]}
              onToggle={handleToggle}
              onInstall={handleInstall}
              onConfigure={setConfigureTarget}
              onUninstall={handleUninstall}
            />
          ))}
        </div>
      )}

      <ConfigureModal
        open={Boolean(configureTarget)}
        moduleName={configureTarget?.name ?? ""}
        configJson={configureTarget?.config ?? "{}"}
        error={configError}
        onClose={() => {
          setConfigureTarget(null);
          setConfigError("");
        }}
        onSave={(json) => void handleSaveConfig(json)}
      />
    </section>
  );
}
