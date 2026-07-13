/**
 * Runtime profile — Vite uses VITE_APP_PROFILE; spec alias NEXT_PUBLIC_APP_PROFILE supported.
 * @typedef {'RETAIL_SIDECAR' | 'TREASURY_HUB'} AppProfile
 */

/** @returns {AppProfile} */
export function resolveAppProfile() {
  const raw =
    import.meta.env.VITE_APP_PROFILE ??
    import.meta.env.NEXT_PUBLIC_APP_PROFILE ??
    "TREASURY_HUB";
  return raw === "RETAIL_SIDECAR" ? "RETAIL_SIDECAR" : "TREASURY_HUB";
}

export const APP_PROFILE = resolveAppProfile();

export const isRetailSidecar = APP_PROFILE === "RETAIL_SIDECAR";
export const isTreasuryHub = APP_PROFILE === "TREASURY_HUB";
