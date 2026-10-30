import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Settings } from "../ipc/bindings";
import { useWorkspace } from "../state/workspace";
import { fakeRoot, type FakeRoot } from "../testing/fakeRoot";
import { appearanceVars, gridRowHeight, motionReduced, startAppearance } from "./appearance";

let root: FakeRoot;
let system: { matches: boolean; listeners: (() => void)[] };

beforeEach(() => {
  root = fakeRoot();
  system = { matches: false, listeners: [] };
  vi.stubGlobal("document", { documentElement: root });
  vi.stubGlobal("window", {
    matchMedia: (query: string) => ({
      get matches() {
        return query === "(prefers-reduced-motion: reduce)" && system.matches;
      },
      addEventListener: (_: string, fn: () => void) => system.listeners.push(fn),
      removeEventListener: (_: string, fn: () => void) => system.listeners.splice(system.listeners.indexOf(fn), 1),
    }),
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
  useWorkspace.setState(useWorkspace.getInitialState(), true);
});

const setSettings = (patch: Partial<Settings>) => useWorkspace.setState({ settings: { ...useWorkspace.getState().settings, ...patch } });

it("maps the default appearance settings to the values the stylesheet was drawn with", () => {
  expect(appearanceVars(useWorkspace.getState().settings)).toEqual({
    "--ui-font": '"Inter Variable", ui-sans-serif, system-ui, sans-serif',
    "--ui-scale": "1",
    "--code-font": '"JetBrains Mono Variable", ui-monospace, "SF Mono", Menlo, Consolas, monospace',
    "--code-size": "13px",
    "--code-line-height": "20px",
    "--density": "1",
    "--panel-duration": "150ms",
  });
});

it("scales the interface by size / 13 and sizes code, density, and motion from their settings", () => {
  const vars = appearanceVars({
    interfaceFont: { family: "System", size: 16 },
    codeFont: { family: "Menlo", size: 18 },
    density: "Spacious",
    panelMotionMs: 0,
  });
  expect(vars).toEqual({
    "--ui-font": 'system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
    "--ui-scale": String(16 / 13),
    "--code-font": '"Menlo", ui-monospace, "SF Mono", Menlo, Consolas, monospace',
    "--code-size": "18px",
    "--code-line-height": "28px",
    "--density": "1.15",
    "--panel-duration": "0ms",
  });
  expect(appearanceVars({ ...useWorkspace.getState().settings, density: "Compact" })["--density"]).toBe("0.85");
});

it("paints the vars on the document and repaints when an appearance setting changes", () => {
  const stop = startAppearance();
  expect(root.style.vars.get("--code-size")).toBe("13px");
  setSettings({ codeFont: { family: "JetBrains Mono", size: 15 }, density: "Compact" });
  expect([root.style.vars.get("--code-size"), root.style.vars.get("--density")]).toEqual(["15px", "0.85"]);
  stop();
  setSettings({ panelMotionMs: 300 });
  expect(root.style.vars.get("--panel-duration")).toBe("150ms");
});

it("derives the grid row height from the code size and density, 24 px at the defaults", () => {
  expect([gridRowHeight(13, "Compact"), gridRowHeight(13, "Standard"), gridRowHeight(13, "Spacious")]).toEqual([20, 24, 28]);
  expect([gridRowHeight(10, "Standard"), gridRowHeight(20, "Spacious")]).toEqual([21, 36]);
});

it("reduces motion when the setting is On, or System while the OS asks for it", () => {
  expect([motionReduced("On", false), motionReduced("Off", true), motionReduced("System", true), motionReduced("System", false)]).toEqual([
    true,
    false,
    true,
    false,
  ]);
});

it("marks the root while motion is reduced and follows the OS only while the choice is System", () => {
  const setSystemReduced = (reduced: boolean) => {
    system.matches = reduced;
    for (const fn of system.listeners) fn();
  };
  const stop = startAppearance();
  expect(root.dataset.motion).toBeUndefined();
  setSystemReduced(true);
  expect(root.dataset.motion).toBe("reduced");
  setSettings({ reduceMotion: "Off" });
  expect(root.dataset.motion).toBeUndefined();
  setSystemReduced(false);
  setSettings({ reduceMotion: "On" });
  expect(root.dataset.motion).toBe("reduced");
  setSettings({ reduceMotion: "System" });
  expect(root.dataset.motion).toBeUndefined();
  stop();
  expect(system.listeners).toEqual([]);
  setSystemReduced(true);
  expect(root.dataset.motion).toBeUndefined();
});
