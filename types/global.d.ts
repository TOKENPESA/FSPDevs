/** Ambient browser extensions for FSP UI shells. */
interface TauriInvokeCore {
  invoke(command: string, args?: Record<string, unknown>): Promise<unknown>;
}

interface TauriEventApi {
  listen(
    event: string,
    handler: (event: { payload: unknown }) => void,
  ): Promise<() => void>;
}

interface TauriGlobal {
  core?: TauriInvokeCore;
  event?: TauriEventApi;
}

declare global {
  interface Window {
    __TAURI__?: TauriGlobal;
    updateInvoicePreview?: () => void | Promise<void>;
    updatePowerProfile?: (targetProfile: string) => Promise<void>;
    recalculateFees?: () => Promise<void>;
    syncFloatReserves?: () => void;
    syncDicobaContribution?: () => void;
    submitChamaContribution?: () => Promise<void>;
    triggerManualRebalanceTest?: () => Promise<void>;
    __MFA_MESH_BOOTED__?: boolean;
  }
}

export {};
