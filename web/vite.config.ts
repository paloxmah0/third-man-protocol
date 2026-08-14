import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// One origin, one app. The frontend calls relative paths (/auth, /agreements, ...)
// and Vite proxies them to the Rust gateway on 8080 in dev. No CORS, no split base URLs.
const backend = process.env.BACKEND_URL || "http://127.0.0.1:8080";

const backendPaths = [
  "/auth", "/kyc", "/profile", "/agreements", "/otp", "/attachments", "/proofs",
  "/milestones", "/collateral", "/escrow", "/disputes", "/arbiters", "/points",
  "/receipts", "/ledger", "/health",
];

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: backendPaths.reduce((acc, p) => {
      acc[p] = { target: backend, changeOrigin: true };
      return acc;
    }, {}),
  },
});
