import ChannelCapacityRibbon from "../ui/ChannelCapacityRibbon.jsx";
import HoldToExecute from "../ui/HoldToExecute.jsx";

/** @type {{ nodeId: string, asset: string, outbound: number, inbound: number, max: number, status: string }[]} */
const PEERS = [
  { nodeId: "FA-12", asset: "CKB", outbound: 42_000_000, inbound: 18_000_000, max: 60_000_000, status: "ACTIVE" },
  { nodeId: "FA-19", asset: "xUDT", outbound: 1_200_000, inbound: 890_000, max: 2_500_000, status: "ACTIVE" },
  { nodeId: "FA-33", asset: "RGB++", outbound: 512, inbound: 256, max: 1024, status: "LOW" },
  { nodeId: "FA-44", asset: "CKB", outbound: 8_000_000, inbound: 22_000_000, max: 30_000_000, status: "REBALANCE" },
];

/**
 * @param {{ nodeId: string, asset: string, outbound: number, inbound: number, max: number, status: string }} peer
 */
function PeerRow({ peer }) {
  const statusColor =
    peer.status === "ACTIVE"
      ? "text-liquidity-mint"
      : peer.status === "LOW"
        ? "text-warning-amber"
        : "text-sovereign-cyan";

  return (
    <tr className="border-b border-institutional-slate/80 hover:bg-institutional-slate/20">
      <td className="px-3 py-3 font-mono text-sm tabular-nums text-sovereign-cyan">{peer.nodeId}</td>
      <td className="px-3 py-3 font-mono text-xs text-slate-300">{peer.asset}</td>
      <td className="min-w-[200px] px-3 py-3">
        <ChannelCapacityRibbon
          outboundShannons={peer.outbound}
          inboundShannons={peer.inbound}
          maxShannons={peer.max}
          label=""
        />
      </td>
      <td className={`px-3 py-3 font-mono text-[10px] font-semibold ${statusColor}`}>
        [ {peer.status} ]
      </td>
      <td className="px-3 py-3">
        <div className="flex flex-wrap gap-2">
          <HoldToExecute onExecute={() => {}} className="!min-w-0 !px-2 !py-2 !text-[10px]">
            Close channel
          </HoldToExecute>
          <button
            type="button"
            className="border border-institutional-slate px-2 py-2 font-sans text-[10px] font-medium uppercase text-slate-400 hover:border-sovereign-cyan hover:text-sovereign-cyan"
          >
            Rebalance
          </button>
        </div>
      </td>
    </tr>
  );
}

export default function DirectPeerChannelTable() {
  return (
    <section className="panel-border overflow-hidden">
      <div className="border-b border-institutional-slate px-4 py-3">
        <h3 className="font-sans text-sm font-semibold text-slate-100">Direct peer channel mapper</h3>
        <p className="font-mono text-[10px] text-slate-500">
          L2 adjacency only · no MFA routing overlay
        </p>
      </div>
      <div className="overflow-x-auto">
        <table className="w-full min-w-[720px] text-left">
          <thead>
            <tr className="border-b border-institutional-slate bg-institutional-slate/30">
              {["Node ID", "Asset", "Capacity", "State", "Actions"].map((h) => (
                <th key={h} className="telemetry-label px-3 py-2 text-left">
                  {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {PEERS.map((peer) => (
              <PeerRow key={peer.nodeId} peer={peer} />
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}
