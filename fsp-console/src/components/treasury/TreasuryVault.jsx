import { Hexagon, Layers, Shield } from "lucide-react";
import ChannelCapacityRibbon from "../ui/ChannelCapacityRibbon.jsx";

const ASSETS = [
  {
    id: "ckb-core",
    label: "CKB Core",
    icon: Hexagon,
    balance: "128,400,000,000",
    unit: "shannons",
    hash: "0x9f3a…c812",
    outbound: 42_000_000_000,
    inbound: 86_400_000_000,
    max: 128_400_000_000,
    accent: "text-liquidity-mint",
    border: "border-liquidity-mint/30",
  },
  {
    id: "xudt-stable",
    label: "xUDT Stablecoins",
    icon: Layers,
    balance: "4,820,500.00",
    unit: "minor units",
    hash: "0x44b1…09de",
    outbound: 1_200_000,
    inbound: 3_620_500,
    max: 5_000_000,
    accent: "text-sovereign-cyan",
    border: "border-sovereign-cyan/30",
  },
  {
    id: "rgb-iso",
    label: "RGB++ Isomorphic Cells",
    icon: Shield,
    balance: "2,048",
    unit: "cells",
    hash: "rgb++:iso:7f2c",
    outbound: 512,
    inbound: 1_536,
    max: 2_048,
    accent: "text-warning-amber",
    border: "border-warning-amber/30",
  },
];

function AssetCard({ asset }) {
  const Icon = asset.icon;
  return (
    <article className={`panel-border rounded-lg p-4 ${asset.border}`}>
      <div className="mb-4 flex items-start justify-between gap-3">
        <div className="flex items-center gap-2">
          <div className="flex h-9 w-9 items-center justify-center rounded border border-institutional-slate bg-institutional-slate/30">
            <Icon size={18} className={asset.accent} />
          </div>
          <div>
            <h3 className="font-sans text-sm font-semibold text-slate-100">{asset.label}</h3>
            <p className="font-mono text-[10px] text-slate-500">{asset.hash}</p>
          </div>
        </div>
        <span className="rounded border border-institutional-slate px-2 py-0.5 font-mono text-[10px] text-slate-400">
          L2 RESERVE
        </span>
      </div>

      <div className="mb-4">
        <p className="telemetry-label">Aggregate balance</p>
        <p className={`mt-1 font-mono text-2xl font-semibold tabular-nums ${asset.accent}`}>
          {asset.balance}
        </p>
        <p className="font-mono text-[10px] text-slate-500">{asset.unit}</p>
      </div>

      <ChannelCapacityRibbon
        label="Channel capacity ribbon"
        outboundShannons={asset.outbound}
        inboundShannons={asset.inbound}
        maxShannons={asset.max}
      />
    </article>
  );
}

export default function TreasuryVault() {
  const totalShannons = "133,220,500,000";

  return (
    <section className="space-y-4">
      <header className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h2 className="font-sans text-lg font-semibold text-slate-100">Treasury Hub Vault</h2>
          <p className="font-mono text-xs text-slate-500">
            Layer-2 multi-asset reserves · cryptographic sub-ledger partitions
          </p>
        </div>
        <div className="panel-border rounded px-4 py-2 text-right">
          <p className="telemetry-label">Total L2 exposure</p>
          <p className="font-mono text-xl font-semibold text-liquidity-mint">{totalShannons}</p>
          <p className="font-mono text-[10px] text-slate-500">shannons equivalent</p>
        </div>
      </header>

      <div className="panel-border rounded-lg p-4">
        <ChannelCapacityRibbon
          label="Aggregate hub channel (MFA ↔ Enterprise Clearinghouse)"
          outboundShannons={45_600_000_000}
          inboundShannons={87_620_500_000}
          maxShannons={133_220_500_000}
        />
      </div>

      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
        {ASSETS.map((asset) => (
          <AssetCard key={asset.id} asset={asset} />
        ))}
      </div>

      <div className="panel-border rounded border-dashed p-4">
        <p className="font-mono text-xs text-slate-500">
          [ HTLC LOCK PENDING ] · Papss corridor sync · last vault attestation{" "}
          <span className="text-sovereign-cyan">18:14:09 UTC</span>
        </p>
      </div>
    </section>
  );
}
