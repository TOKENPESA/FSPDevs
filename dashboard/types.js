/**
 * Shared structural types for the mesh dashboard canvas and MFA monitor UI.
 */

/**
 * @typedef {'ring' | 'skip' | 'chord' | 'mfa' | 'mesh' | 'heal'} PathKind
 */

/**
 * @typedef {Object} PathStyle
 * @property {string} color
 * @property {number} width
 * @property {number[]} dash
 * @property {number} speed
 * @property {string} [label]
 * @property {number} [alpha]
 */

/**
 * @typedef {Object} MeshEdges
 * @property {Array<[number, number]>} ring
 * @property {Array<[number, number]>} skip
 * @property {Array<[number, number]>} chord
 */

/**
 * @typedef {Object} HealLink
 * @property {number} from
 * @property {number} to
 */

/**
 * @typedef {Object} CommNodeMeta
 * @property {number} at
 * @property {number[]} neighbors
 * @property {number} channels
 * @property {number | null} [outboundShannons]
 * @property {number | null} [inboundShannons]
 */

/**
 * @typedef {Object} CommEdgeMeta
 * @property {number} at
 * @property {string} kind
 * @property {number} a
 * @property {number} b
 */

/**
 * @typedef {Object} NodeLedger
 * @property {number} outbound
 * @property {number} inbound
 * @property {number} at
 */

/**
 * @typedef {Object} PaymentReceipt
 * @property {number} amount
 * @property {number} at
 * @property {number} from
 */

/**
 * @typedef {Object} PaymentSent
 * @property {number} amount
 * @property {number} fee
 * @property {number} [totalDebit]
 * @property {number} at
 * @property {number} to
 */

/**
 * @typedef {Object} NodeVisualMeta
 * @property {string} status
 * @property {number} at
 */

/**
 * @typedef {Object} PaymentTransfer
 * @property {number[]} path
 * @property {number} source
 * @property {number} destination
 * @property {number} amount
 * @property {number} progress
 * @property {'traveling' | 'settled' | 'failed'} phase
 * @property {number} startedAt
 * @property {number | null} settledAt
 * @property {ReturnType<typeof setTimeout> | null} clearTimer
 */

/**
 * @typedef {Object} PathPoint
 * @property {number} x
 * @property {number} y
 * @property {number} from
 * @property {number} to
 * @property {boolean} atDestination
 */

/**
 * @typedef {Object} HubState
 * @property {string} rpcUrl
 * @property {string} fundingShannons
 * @property {string} sidecarAlerts
 */

/**
 * @typedef {Object} LiquidityNodeMeta
 * @property {string} status
 * @property {number} at
 */

/**
 * @typedef {Object} LiquidityState
 * @property {number} injections
 * @property {number} faucetHints
 * @property {number} inFlight
 * @property {number} failed
 * @property {string} lastEvent
 * @property {Map<number, LiquidityNodeMeta>} byNode
 */

/**
 * @typedef {Object} CommState
 * @property {Map<number, CommNodeMeta>} nodes
 * @property {Map<string, CommEdgeMeta>} edges
 * @property {Map<number, { at: number }>} mfaLinks
 * @property {Map<number, NodeLedger>} balances
 * @property {Map<number, PaymentReceipt>} received
 * @property {Map<number, PaymentSent>} sent
 */

/**
 * @typedef {Object} DashboardState
 * @property {number} networkSize
 * @property {Set<number>} dead
 * @property {Set<number>} healed
 * @property {HealLink[]} healLinks
 * @property {number[]} activeRoute
 * @property {PaymentTransfer | null} paymentTransfer
 * @property {number} tick
 * @property {number} healCount
 * @property {boolean} playing
 * @property {number} speed
 * @property {WebSocket | null} ws
 * @property {number | null} hoveredNode
 * @property {{ clientX: number, clientY: number } | null} lastPointer
 * @property {boolean} dirty
 * @property {number} lastFrame
 * @property {number} animTime
 * @property {HubState} hub
 * @property {LiquidityState} liquidity
 * @property {CommState} comm
 * @property {Map<number, NodeVisualMeta>} nodeVisual
 */

/**
 * @typedef {Object} MonitorEnvelope
 * @property {number} schema_version
 * @property {string} event
 * @property {Record<string, unknown>} [payload]
 */

/**
 * @typedef {Object} SidecarPanel
 * @property {string} id
 * @property {string} title
 * @property {string} [navLabel]
 * @property {string} [navDescription]
 * @property {string} [navIcon]
 * @property {string} [badge]
 * @property {string} [navDescription]
 * @property {() => string} render
 * @property {(root: HTMLElement, ctx?: SidecarUiContext) => void | Promise<void> | (() => void) | Promise<() => void> | undefined} mount
 * @property {(root: HTMLElement, ctx?: SidecarUiContext) => void | Promise<void>} [refresh]
 * @property {(snapshot: unknown) => string} [renderAside]
 * @property {(snapshot: unknown) => void} [_repaintStats]
 */

/**
 * @typedef {Object} SidecarModule
 * @property {string} id
 * @property {string} label
 * @property {string} [navLabel]
 * @property {string} [navIcon]
 * @property {string} [hint]
 * @property {string} [navDescription]
 * @property {boolean} [topLevel] - Pin to the sidebar root (sibling of Dashboard).
 * @property {SidecarPanel[]} panels
 * @property {(ctx: SidecarUiContext) => void | Promise<void>} [initialize]
 */

/**
 * @typedef {Object} PanelRoute
 * @property {string} id
 * @property {'panel' | 'dashboard'} type
 * @property {string} label
 * @property {string} icon
 * @property {string} [hint]
 * @property {SidecarPanel} [panel]
 * @property {SidecarModule} [module]
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
 * @typedef {Object} SidecarUiContext
 * @property {HTMLElement} root
 * @property {Record<string, unknown>} [stats]
 * @property {string} [lastSync]
 * @property {() => void | Promise<void>} [refreshLoanPanel]
 */

/**
 * Live sidecar stats snapshot from `get_sidecar_stats` (camelCase JSON).
 * @typedef {Object} SidecarRuntimeStats
 * @property {number} [agentId]
 * @property {string} [fnnMode]
 * @property {string} [hardwareProfile]
 * @property {string} [powerProfile]
 * @property {string} [nodePubkey]
 * @property {string} [fnnBackend]
 * @property {string} [fnnRpcUrl]
 * @property {string} [fnnP2pEndpoint]
 * @property {string} [fnnConnectionStatus]
 * @property {number} [fnnTotalLiquidityShannons]
 * @property {string} [mfaHost]
 * @property {string} [mfaName]
 * @property {string} [mfaWsUrl]
 * @property {string} [mfaConnectionStatus]
 * @property {boolean} [mfaReachable]
 * @property {boolean} [mfaControlConnected]
 * @property {string[]} [mountedModules]
 * @property {string} [sidecarProfile]
 * @property {string} [profileSource]
 * @property {string[]} [configuredModules]
 * @property {number} [meshChannelsTotal]
 * @property {number} [meshChannelsActive]
 * @property {number} [totalLocalBalanceShannons]
 * @property {number} [totalRemoteBalanceShannons]
 * @property {number} [dicobaContributions]
 * @property {number} [dicobaVaultsTotal]
 * @property {number} [edgePending]
 * @property {number} [edgeSettled]
 * @property {number} [edgeFailed]
 * @property {number} [fiatEdgeTransactions]
 * @property {number} [queuedTelemetry]
 * @property {number} [cachedChannels]
 * @property {number} [meshPeerAgentId]
 * @property {string} [meshPeerPubkey]
 * @property {string} [dicobaMemberId]
 * @property {string} [meshPeerDicobaMemberId]
 * @property {number} [fiatConversionRate]
 * @property {number} [criticalFiatFloor]
 * @property {number} [collectedAtUnix]
 */

export {};
