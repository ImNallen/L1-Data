import type { ConfigEnv, UserConfig } from "vite";
import { afterEach, describe, expect, it, vi } from "vitest";
import config from "../../vite.config.ts";

function resolve(env: Partial<ConfigEnv>): UserConfig {
  if (typeof config !== "function") throw new Error("vite.config.ts must export a config function");
  return config({ command: "serve", mode: "development", ...env }) as UserConfig;
}

describe("vite config ports", () => {
  afterEach(() => vi.unstubAllEnvs());

  it("serves on the ports the environment names", () => {
    vi.stubEnv("L1_DATA_VITE_PORT", "05273");
    vi.stubEnv("L1_DATA_BRIDGE_PORT", "7177");
    const served = resolve({});
    expect(served.server?.port).toBe(5273);
    expect(served.define?.__L1_DATA_BRIDGE_URL__).toBe(JSON.stringify("http://127.0.0.1:7177"));
  });

  it("refuses to serve on a value that is not ASCII digits from 1 to 65535", () => {
    for (const bad of ["", "+5173", " 5173", "5173 ", "5_173", "０", "0", "65536"]) {
      vi.stubEnv("L1_DATA_VITE_PORT", bad);
      expect(() => resolve({}), JSON.stringify(bad)).toThrow("L1_DATA_VITE_PORT must be a port from 1 to 65535");
    }
  });

  it("ignores the port variables when building or testing", () => {
    vi.stubEnv("L1_DATA_VITE_PORT", "");
    vi.stubEnv("L1_DATA_BRIDGE_PORT", "abc");
    expect(() => resolve({ command: "build", mode: "production" })).not.toThrow();
    expect(() => resolve({ mode: "test" })).not.toThrow();
  });
});
