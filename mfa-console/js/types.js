/**
 * Shared JSDoc structural types for the MFA console UI host.
 */

/**
 * @typedef {Object} MfaPanel
 * @property {string} id
 * @property {string} title
 * @property {string} [navLabel]
 * @property {string} [navDescription]
 * @property {string} [navIcon]
 * @property {string} [badge]
 * @property {() => string} render
 * @property {(mountEl: HTMLElement, ctx: MfaUiHostContext) => void | Promise<void>} mount
 * @property {(ctx: MfaUiHostContext) => string} [renderAside]
 */

/**
 * @typedef {Object} MfaModule
 * @property {string} id
 * @property {string} label
 * @property {string} [navLabel]
 * @property {string} [navIcon]
 * @property {string} [hint]
 * @property {MfaPanel[]} panels
 * @property {boolean} [topLevel]
 * @property {(ctx: MfaUiHostContext) => void | Promise<void>} [initialize]
 */

/**
 * @typedef {Object} PanelRoute
 * @property {string} id
 * @property {'panel' | 'dashboard'} type
 * @property {string} label
 * @property {string} icon
 * @property {string} [hint]
 * @property {MfaPanel} [panel]
 * @property {MfaModule} [module]
 */

/**
 * @typedef {Object} ModuleNavGroup
 * @property {string} id
 * @property {string} label
 * @property {string} icon
 * @property {string} [hint]
 * @property {PanelRoute[]} children
 */

/**
 * @typedef {Object} MfaUiHostContext
 * @property {HTMLElement} root
 * @property {(() => void) | undefined} [connectMonitor]
 */

/**
 * @typedef {import('../../dashboard/types.js').MonitorEnvelope} MonitorEnvelope
 */

/**
 * @typedef {Object} MfaRuntimeDetail
 * @property {string} [service]
 * @property {string} [hubRpcUrl]
 * @property {number | null} [hubFunding]
 * @property {number} [simulationEdgeNodes]
 * @property {number} [connectedAgents]
 * @property {number[]} [connectedAgentIds]
 * @property {boolean} [monitorConnected]
 * @property {number} [monitorLiveNodes]
 * @property {number} [offlineNodes]
 * @property {number} [healCount]
 * @property {number} [liquidityInjections]
 * @property {boolean} [clearingRegionalReady]
 * @property {boolean} [clearingMockActive]
 * @property {string} [clearingCorporateVault]
 * @property {string} [clearingEnterprisePath]
 * @property {string} [clearingTopologyJournal]
 * @property {string[]} [assetCorridors]
 * @property {string[]} [runningPlugins]
 * @property {number} [complianceTicketTtl]
 * @property {number | null} [collectedAtUnix]
 * @property {string} [clearingHint]
 * @property {string} [error]
 */

export {};
