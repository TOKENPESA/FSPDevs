/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,jsx}"],
  theme: {
    extend: {
      colors: {
        obsidian: "#0B0F17",
        "institutional-slate": "#1E293B",
        "liquidity-mint": "#10B981",
        "warning-amber": "#F59E0B",
        "sovereign-cyan": "#06B6D4",
      },
      fontFamily: {
        sans: ["Inter", "system-ui", "sans-serif"],
        mono: ["JetBrains Mono", "ui-monospace", "monospace"],
      },
      boxShadow: {
        panel: "0 0 0 1px rgba(30, 41, 59, 0.9), 0 8px 32px rgba(0, 0, 0, 0.45)",
        glowMint: "0 0 12px rgba(16, 185, 129, 0.35)",
        glowCyan: "0 0 12px rgba(6, 182, 212, 0.35)",
      },
      animation: {
        "pulse-slow": "pulse 3s cubic-bezier(0.4, 0, 0.6, 1) infinite",
      },
    },
  },
  plugins: [],
};
