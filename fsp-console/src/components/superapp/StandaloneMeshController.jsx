import SovereignConnectivityBlock from "./SovereignConnectivityBlock.jsx";
import DirectPeerChannelTable from "./DirectPeerChannelTable.jsx";
import OobFallbackPanel from "./OobFallbackPanel.jsx";

export default function StandaloneMeshController() {
  return (
    <div className="space-y-4">
      <header>
        <h2 className="font-sans text-lg font-semibold text-slate-100">
          Works without MFA
        </h2>
        <p className="font-mono text-xs text-slate-500">
          Sovereign sidecar execution · localized P2P · OOB clearing fallback
        </p>
      </header>

      <SovereignConnectivityBlock />
      <DirectPeerChannelTable />
      <OobFallbackPanel />
    </div>
  );
}
