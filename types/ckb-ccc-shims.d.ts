/** Ambient shims for CCC packages loaded via esm.sh import maps in Tauri. */
declare module "@ckb-ccc/core" {
  export const ccc: any;
}

declare module "@ckb-ccc/connector" {
  const connector: unknown;
  export default connector;
}
