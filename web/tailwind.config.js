/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        ink: { 900: "#070710", 800: "#0c0c1a", 700: "#131326", 600: "#1c1c38" },
        accent: { DEFAULT: "#7c5cff", glow: "#a78bfa", cyan: "#22d3ee", mint: "#34d399" },
        warn: "#f59e0b",
        bad: "#ef4444",
      },
      fontFamily: {
        sans: ["Inter", "system-ui", "sans-serif"],
        mono: ["'JetBrains Mono'", "ui-monospace", "monospace"],
      },
      backdropBlur: { xs: "2px" },
      animation: {
        "aurora": "aurora 18s ease-in-out infinite",
        "pulse-slow": "pulse 3.5s ease-in-out infinite",
        "fill-ribbon": "fillRibbon 0.8s ease-out forwards",
      },
      keyframes: {
        aurora: {
          "0%,100%": { transform: "translate3d(-5%, -5%, 0) scale(1)", opacity: "0.55" },
          "50%":     { transform: "translate3d(5%, 5%, 0) scale(1.15)", opacity: "0.8" },
        },
        fillRibbon: { from: { strokeDashoffset: "200" }, to: { strokeDashoffset: "0" } },
      },
    },
  },
  plugins: [],
};
