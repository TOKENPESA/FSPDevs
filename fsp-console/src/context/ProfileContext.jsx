import { createContext, useContext, useMemo } from "react";
import { APP_PROFILE, isRetailSidecar, isTreasuryHub } from "../config/appProfile.js";

/** @typedef {import('../config/appProfile.js').AppProfile} AppProfile */

/** @type {React.Context<{ profile: AppProfile, isRetail: boolean, isTreasury: boolean } | null>} */
const ProfileContext = createContext(null);

/** @param {{ children: React.ReactNode }} props */
export function ProfileProvider({ children }) {
  const value = useMemo(
    () => ({
      profile: APP_PROFILE,
      isRetail: isRetailSidecar,
      isTreasury: isTreasuryHub,
    }),
    [],
  );
  return <ProfileContext.Provider value={value}>{children}</ProfileContext.Provider>;
}

export function useProfile() {
  const ctx = useContext(ProfileContext);
  if (!ctx) throw new Error("useProfile requires ProfileProvider");
  return ctx;
}
