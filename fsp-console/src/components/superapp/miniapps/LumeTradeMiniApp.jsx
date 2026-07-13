export default function LumeTradeMiniApp() {
  return (
    <section className="space-y-4">
      <header>
        <h2 className="font-sans text-lg font-semibold text-slate-100">LUME Trade</h2>
        <p className="font-mono text-xs text-slate-500">Yielding order books · RGB++ spread engine</p>
      </header>
      <div className="panel-border p-4">
        <p className="font-mono text-xs text-sovereign-cyan">[ HTLC LOCK PENDING ] · order book sync</p>
        <div className="mt-4 grid gap-2 font-mono text-xs">
          {[
            { side: "BID", price: "1.0024", size: "4,200,000 sh" },
            { side: "ASK", price: "1.0031", size: "2,100,000 sh" },
            { side: "BID", price: "1.0022", size: "890,000 sh" },
          ].map((row, i) => (
            <div
              key={i}
              className="flex justify-between border-b border-institutional-slate/50 py-2 tabular-nums"
            >
              <span className={row.side === "BID" ? "text-liquidity-mint" : "text-warning-amber"}>
                {row.side}
              </span>
              <span className="text-slate-300">{row.price}</span>
              <span className="text-slate-500">{row.size}</span>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
