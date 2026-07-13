/**
 * Hot-swap module / plugin store UI — catalog panel + installed registry.
 */

import { createLogger } from "./logger.js";

/** @typedef {import("./dashboard-module-api.js").ModuleApiClient} ModuleApiClient */
/** @typedef {import("./dashboard-module-api.js").ModuleCatalogEntry} ModuleCatalogEntry */
/** @typedef {import("./dashboard-module-api.js").InstalledModuleRecord} InstalledModuleRecord */

/**
 * @typedef {Object} ModuleStoreShellOptions
 * @property {string} [catalogTitle]
 * @property {string} [catalogHint]
 * @property {string} [installedTitle]
 * @property {string} [installedHint]
 * @property {string} [entityLabel]
 */

/**
 * @typedef {ModuleStoreShellOptions & {
 *   api: ModuleApiClient,
 *   escapeHtml: (value: unknown) => string,
 *   safeUserMessage: (error: unknown, fallback?: string) => string,
 *   scope?: string,
 * }} ModuleStoreUiOptions
 */

const DEFAULT_SHELL = {
  catalogTitle: "Available catalog",
  catalogHint: "Native packages ready for hot-mount without restarting the host.",
  installedTitle: "Installed registry",
  installedHint: "Live mounts synchronized with the backend Arc<RwLock> registry.",
  entityLabel: "module",
};

/**
 * @param {ModuleStoreShellOptions} [options]
 */
export function renderModuleStoreShell(options = {}) {
  const labels = { ...DEFAULT_SHELL, ...options };
  const entity = labels.entityLabel;
  return `
    <div class="module-store" data-module-store>
      <div class="module-store-toolbar">
        <button type="button" class="panel-btn" data-action="refresh-store">Refresh</button>
        <p class="module-store-status" data-store-status role="status" aria-live="polite"></p>
      </div>
      <div class="module-store-grid">
        <section class="workspace-card module-store-panel" aria-labelledby="module-store-catalog-head">
          <div class="workspace-card-head" id="module-store-catalog-head">
            <h2>${labels.catalogTitle}</h2>
            <p class="panel-hint">${labels.catalogHint}</p>
          </div>
          <div class="module-store-list" data-catalog-list>
            <p class="module-store-empty">Loading catalog…</p>
          </div>
        </section>
        <section class="workspace-card module-store-panel" aria-labelledby="module-store-installed-head">
          <div class="workspace-card-head" id="module-store-installed-head">
            <h2>${labels.installedTitle}</h2>
            <p class="panel-hint">${labels.installedHint}</p>
          </div>
          <div class="module-store-list" data-installed-list>
            <p class="module-store-empty">Loading registry…</p>
          </div>
        </section>
      </div>
      <div class="module-store-modal" data-install-modal hidden>
        <div class="module-store-modal-backdrop" data-action="close-install-modal"></div>
        <div class="module-store-modal-card" role="dialog" aria-modal="true" aria-labelledby="module-store-modal-title">
          <div class="module-store-modal-head">
            <h3 id="module-store-modal-title">Install ${entity}</h3>
            <button type="button" class="panel-btn" data-action="close-install-modal" aria-label="Close">Close</button>
          </div>
          <p class="panel-hint" data-install-target></p>
          <label class="workspace-field" for="module-store-config-json">
            <span>Configuration JSON</span>
            <textarea
              id="module-store-config-json"
              class="module-store-config-input"
              rows="10"
              spellcheck="false"
              placeholder='{ "key": "value" }'
            ></textarea>
          </label>
          <p class="module-store-modal-error" data-install-error hidden></p>
          <div class="module-store-modal-actions">
            <button type="button" class="panel-btn" data-action="close-install-modal">Cancel</button>
            <button type="button" class="panel-btn panel-btn-primary" data-action="confirm-install">Install</button>
          </div>
        </div>
      </div>
    </div>`;
}

/**
 * @param {HTMLElement} root
 * @param {ModuleStoreUiOptions} options
 */
export function mountModuleStoreUi(root, options) {
  const labels = { ...DEFAULT_SHELL, ...options };
  const log = createLogger(options.scope ?? "module-store");
  const { api, escapeHtml, safeUserMessage } = options;

  const storeRoot = root.querySelector("[data-module-store]");
  if (!(storeRoot instanceof HTMLElement)) {
    log.error("module store root missing");
    return;
  }

  const catalogList = storeRoot.querySelector("[data-catalog-list]");
  const installedList = storeRoot.querySelector("[data-installed-list]");
  const statusEl = storeRoot.querySelector("[data-store-status]");
  const modal = storeRoot.querySelector("[data-install-modal]");
  const installTarget = storeRoot.querySelector("[data-install-target]");
  const configInput = storeRoot.querySelector("#module-store-config-json");
  const installError = storeRoot.querySelector("[data-install-error]");

  /** @type {ModuleCatalogEntry[]} */
  let catalog = [];
  /** @type {InstalledModuleRecord[]} */
  let installed = [];
  /** @type {ModuleCatalogEntry | null} */
  let pendingInstall = null;
  let busy = false;

  /**
   * @param {string} message
   * @param {"info" | "error"} [kind]
   */
  function setStatus(message, kind = "info") {
    if (!(statusEl instanceof HTMLElement)) return;
    statusEl.textContent = message;
    statusEl.dataset.kind = kind;
  }

  /**
   * @param {string} moduleId
   */
  function isInstalled(moduleId) {
    const key = moduleId.toLowerCase();
    return installed.some(
      (row) =>
        row.module_name.toLowerCase() === key ||
        row.id.toLowerCase() === key,
    );
  }

  /**
   * @param {ModuleCatalogEntry} entry
   */
  function catalogKey(entry) {
    return entry.module_id || entry.module_name;
  }

  function renderCatalog() {
    if (!(catalogList instanceof HTMLElement)) return;
    if (!catalog.length) {
      catalogList.innerHTML = `<p class="module-store-empty">No catalog entries returned.</p>`;
      return;
    }
    catalogList.innerHTML = catalog
      .map((entry) => {
        const key = catalogKey(entry);
        const name = entry.module_name || key;
        const desc = entry.description || "No description provided.";
        const kind = entry.kind ? `<span class="module-store-kind">${escapeHtml(entry.kind)}</span>` : "";
        const mounted = isInstalled(key);
        return `
          <article class="module-store-card" data-catalog-id="${escapeHtml(key)}">
            <div class="module-store-card-head">
              <h4>${escapeHtml(name)}</h4>
              ${kind}
            </div>
            <p class="module-store-desc">${escapeHtml(desc)}</p>
            ${
              entry.rpc_methods?.length
                ? `<p class="module-store-meta">RPC: ${escapeHtml(entry.rpc_methods.join(", "))}</p>`
                : ""
            }
            <button
              type="button"
              class="panel-btn panel-btn-primary"
              data-action="open-install"
              data-module-id="${escapeHtml(key)}"
              data-module-label="${escapeHtml(name)}"
              ${mounted ? "disabled" : ""}
            >
              ${mounted ? "Installed" : "Install"}
            </button>
          </article>`;
      })
      .join("");
  }

  function renderInstalled() {
    if (!(installedList instanceof HTMLElement)) return;
    if (!installed.length) {
      installedList.innerHTML = `<p class="module-store-empty">No ${escapeHtml(labels.entityLabel)}s mounted.</p>`;
      return;
    }
    installedList.innerHTML = installed
      .map((row) => {
        const name = row.module_name || row.id;
        const active = row.is_active !== false;
        return `
          <article class="module-store-card module-store-card--installed" data-installed-id="${escapeHtml(name)}">
            <div class="module-store-card-head">
              <h4>${escapeHtml(name)}</h4>
              <span class="module-store-badge ${active ? "is-active" : "is-paused"}">
                ${active ? "Active" : "Paused"}
              </span>
            </div>
            <label class="module-store-toggle" title="Toggle active state">
              <input
                type="checkbox"
                data-action="toggle-module"
                data-module-name="${escapeHtml(name)}"
                ${active ? "checked" : ""}
                ${busy ? "disabled" : ""}
              />
              <span class="module-store-toggle-track" aria-hidden="true"></span>
              <span class="module-store-toggle-label">${active ? "Running" : "Paused"}</span>
            </label>
            <button
              type="button"
              class="panel-btn module-store-uninstall"
              data-action="uninstall-module"
              data-module-name="${escapeHtml(name)}"
              ${busy ? "disabled" : ""}
            >
              Uninstall
            </button>
          </article>`;
      })
      .join("");
  }

  function paint() {
    renderCatalog();
    renderInstalled();
  }

  async function refreshInstalled() {
    installed = await api.fetchInstalled();
    paint();
  }

  /**
   * @param {string} message
   */
  function renderLoadError(message) {
    const safe = escapeHtml(message);
    const html = `<p class="module-store-empty module-store-error">${safe}</p>`;
    if (catalogList instanceof HTMLElement) catalogList.innerHTML = html;
    if (installedList instanceof HTMLElement) installedList.innerHTML = html;
  }

  async function refreshAll() {
    setStatus("Syncing registry…");
    const [catalogRows, installedRows] = await Promise.all([
      api.fetchCatalog(),
      api.fetchInstalled(),
    ]);
    catalog = catalogRows;
    installed = installedRows;
    paint();
    setStatus(`Synced ${installed.length} mounted · ${catalog.length} in catalog`);
  }

  /**
   * @param {unknown} error
   * @param {string} action
   */
  function handleError(error, action) {
    log.error(`${action} failed`, error);
    const message = safeUserMessage(error, `${action} failed`);
    setStatus(message, "error");
    renderLoadError(message);
  }

  /**
   * @param {() => Promise<unknown>} work
   * @param {string} action
   */
  async function withMutation(work, action) {
    if (busy) return;
    busy = true;
    paint();
    try {
      await work();
      await refreshInstalled();
      setStatus(`${action} succeeded`);
    } catch (error) {
      handleError(error, action);
      throw error;
    } finally {
      busy = false;
      paint();
    }
  }

  function closeInstallModal() {
    pendingInstall = null;
    if (modal instanceof HTMLElement) modal.hidden = true;
    if (installError instanceof HTMLElement) {
      installError.hidden = true;
      installError.textContent = "";
    }
    if (configInput instanceof HTMLTextAreaElement) {
      configInput.value = "{}";
    }
  }

  /**
   * @param {ModuleCatalogEntry} entry
   */
  function openInstallModal(entry) {
    pendingInstall = entry;
    if (modal instanceof HTMLElement) modal.hidden = false;
    if (installTarget instanceof HTMLElement) {
      installTarget.textContent = `Configure ${entry.module_name || entry.module_id} before hot-mount.`;
    }
    if (configInput instanceof HTMLTextAreaElement) {
      configInput.value = "{\n  \n}";
      configInput.focus();
    }
    if (installError instanceof HTMLElement) {
      installError.hidden = true;
      installError.textContent = "";
    }
  }

  storeRoot.addEventListener("click", (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;

    const actionEl = target.closest("[data-action]");
    if (!(actionEl instanceof HTMLElement)) return;
    const action = actionEl.dataset.action;

    if (action === "refresh-store") {
      void refreshAll().catch((error) => handleError(error, "Refresh"));
      return;
    }

    if (action === "close-install-modal") {
      closeInstallModal();
      return;
    }

    if (action === "open-install") {
      const moduleId = actionEl.dataset.moduleId ?? "";
      const entry =
        catalog.find((row) => catalogKey(row) === moduleId) ??
        ({
          module_id: moduleId,
          module_name: actionEl.dataset.moduleLabel ?? moduleId,
        });
      openInstallModal(entry);
      return;
    }

    if (action === "confirm-install") {
      void (async () => {
        if (!pendingInstall) return;
        if (!(configInput instanceof HTMLTextAreaElement)) return;
        if (installError instanceof HTMLElement) {
          installError.hidden = true;
          installError.textContent = "";
        }
        /** @type {Record<string, unknown>} */
        let configJson = {};
        try {
          const raw = configInput.value.trim() || "{}";
          configJson = /** @type {Record<string, unknown>} */ (JSON.parse(raw));
        } catch (error) {
          const msg = safeUserMessage(error, "Configuration must be valid JSON");
          if (installError instanceof HTMLElement) {
            installError.hidden = false;
            installError.textContent = msg;
          }
          log.error("invalid install JSON", error);
          return;
        }
        const moduleName = catalogKey(pendingInstall);
        try {
          await withMutation(
            () => api.installModule(moduleName, configJson),
            `Install ${moduleName}`,
          );
          closeInstallModal();
        } catch (error) {
          if (installError instanceof HTMLElement) {
            installError.hidden = false;
            installError.textContent = safeUserMessage(error, "Install failed");
          }
        }
      })();
      return;
    }

    if (action === "uninstall-module") {
      const moduleName = actionEl.dataset.moduleName;
      if (!moduleName) return;
      void withMutation(
        () => api.uninstallModule(moduleName),
        `Uninstall ${moduleName}`,
      ).catch(() => {});
    }
  });

  storeRoot.addEventListener("change", (event) => {
    const target = event.target;
    if (!(target instanceof HTMLInputElement)) return;
    if (target.dataset.action !== "toggle-module") return;
    const moduleName = target.dataset.moduleName;
    if (!moduleName) return;
    const isActive = target.checked;
    void withMutation(
      () => api.toggleModule(moduleName, isActive),
      `Toggle ${moduleName}`,
    ).catch(() => {
      target.checked = !isActive;
    });
  });

  /**
   * @param {number} [attempt]
   */
  async function initialLoad(attempt = 1) {
    try {
      await refreshAll();
    } catch (error) {
      if (attempt < 5) {
        setStatus(`Registry not ready — retrying (${attempt}/5)…`);
        await new Promise((resolve) => setTimeout(resolve, 1000));
        return initialLoad(attempt + 1);
      }
      handleError(error, "Initial load");
    }
  }

  void initialLoad();
}
