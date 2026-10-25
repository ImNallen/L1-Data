import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

declare const process: { env: Record<string, string | undefined> };

function port(name: string, fallback: number): number {
  const value = process.env[name];
  if (value === undefined) return fallback;
  const parsed = Number(value);
  if (!/^\d+$/.test(value) || parsed < 1 || parsed > 65535) throw new Error(`${name} must be a port from 1 to 65535, got ${JSON.stringify(value)}`);
  return parsed;
}

export default defineConfig({
  plugins: [react(), tailwindcss()],
  define: { __L1_DATA_BRIDGE_URL__: JSON.stringify(`http://127.0.0.1:${port("L1_DATA_BRIDGE_PORT", 7077)}`) },
  server: { port: port("L1_DATA_VITE_PORT", 5173), strictPort: true, watch: { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"] } },
  clearScreen: false,
  envPrefix: ["VITE_", "TAURI_ENV_"],
  test: { include: ["src/**/*.test.ts", "src/**/*.test.tsx"] },
});
