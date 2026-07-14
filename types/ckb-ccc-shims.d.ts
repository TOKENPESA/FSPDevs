/** Ambient shims for CCC packages loaded via esm.sh import maps in Tauri. */
declare module "@ckb-ccc/core" {
  export const ccc: any;
  export class ClientPublicTestnet {
    constructor(...args: any[]);
  }
  export class Address {
    static fromString(...args: any[]): Promise<any>;
  }
  export class Transaction {
    static from(...args: any[]): any;
  }
  export function fixedPointFrom(...args: any[]): any;
  export function isWebview(ua: string): boolean;
}

declare module "@ckb-ccc/joy-id" {
  export namespace JoyId {
    export class CkbSigner {
      constructor(client: any, name: string, icon: string, ...rest: any[]);
      connect(): Promise<void>;
      isConnected(): Promise<boolean>;
      getInternalAddress(): Promise<string>;
      prepareTransaction(tx: any): Promise<any>;
      sendTransaction(tx: any): Promise<string>;
      client: any;
    }
    export function getJoyIdSigners(
      client: any,
      name: string,
      icon: string,
      preferredNetworks?: any[],
    ): Array<{ name: string; signer: any }>;
  }
  export class CkbSigner {
    constructor(client: any, name: string, icon: string, ...rest: any[]);
    connect(): Promise<void>;
    isConnected(): Promise<boolean>;
    getInternalAddress(): Promise<string>;
    prepareTransaction(tx: any): Promise<any>;
    sendTransaction(tx: any): Promise<string>;
    client: any;
  }
}

declare module "@ckb-ccc/connector" {
  const connector: unknown;
  export default connector;
}
