import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, expect, it, vi } from "vitest";
import type { LimitsOutcome, LimitsReport } from "../../ipc/bindings";
import { UsageLimits, UsageRefresh } from "./UsageDialog";
import { useLimits } from "./useLimits";

const initialLimits = { ...useLimits.getInitialState() };
afterEach(() => {
  Object.assign(useLimits.getInitialState(), initialLimits);
  vi.useRealTimers();
});
const text = (html: string | undefined) =>
  html
    ?.replace(/<[^>]+>/g, " ")
    .replace(/\s+/g, " ")
    .trim();
const byTestId = (html: string, id: string) => text(new RegExp(`data-testid="${id}"[^>]*>(.*?)</(?:span|p|button)>`).exec(html)?.[1]);

const NOW = Date.UTC(2026, 9, 17, 12, 0, 0);
const MIN = 60_000;

const LIMITS: LimitsReport = {
  checkedAtMs: NOW - 2 * MIN,
  sources: [
    {
      source: "Claude",
      name: "Claude",
      outcome: {
        kind: "Ready",
        plan: "Max",
        windows: [
          { id: "session", label: "Session", leftPercent: 10, resetsAtMs: NOW + 86 * MIN, level: "Low" },
          { id: "weekly_all", label: "Weekly", leftPercent: 40, resetsAtMs: NOW + (3 * 24 + 4) * 60 * MIN, level: "Normal" },
          { id: "weekly_scoped:Opus", label: "Weekly · Opus", leftPercent: 0, resetsAtMs: null, level: "Exhausted" },
        ],
      },
    },
  ],
};

function renderUsage(limits: { report: LimitsReport | null; loading?: boolean; error?: string | null }) {
  vi.useFakeTimers({ now: NOW });
  Object.assign(useLimits.getInitialState(), { loading: false, error: null, ...limits });
  return renderToStaticMarkup(
    <>
      <UsageRefresh />
      <UsageLimits />
    </>,
  );
}

const has = (html: string, testId: string) => html.includes(`data-testid="${testId}"`);
const testIds = (html: string, prefix: string) => [...html.matchAll(new RegExp(`data-testid="(${prefix}[^"]*)"`, "g"))].map(([, id]) => id);
const between = (html: string, from: string, to: string) => {
  const start = html.indexOf(`data-testid="${from}"`);
  const end = html.lastIndexOf("<", html.indexOf(`data-testid="${to}"`));
  return text(html.slice(start, end).replace(/^[^>]*>/, ""));
};

it("draws a card per window with what is left, the warning, and the countdown", () => {
  const html = renderUsage({ report: LIMITS });
  expect(byTestId(html, "usage-checked")).toBe("Checked 2m ago");
  expect(html).toMatch(/data-testid="usage-refresh"(?![^>]*disabled)/);
  expect(testIds(html, "limit-")).toEqual(["limit-Claude-session", "limit-Claude-weekly_all", "limit-Claude-weekly_scoped:Opus"]);
  expect(between(html, "limit-Claude-session", "limit-Claude-weekly_all")).toBe("Session 10% left Running low 10% 1h 26m");
  expect(between(html, "limit-Claude-weekly_all", "limit-Claude-weekly_scoped:Opus")).toBe("Weekly 40% left 40% 3d 4h");
  expect(text(html)).toContain("Weekly · Opus 0% left Limit reached 0%");
  expect(html).toMatch(/role="meter" aria-label="Session left" aria-valuenow="10" aria-valuemin="0" aria-valuemax="100"/);
  expect(text(html)).toContain("Claude · Max");
  expect(text(html)).toContain("Limits come from each provider&#x27;s own account and reset on its schedule.");
});

it("says how to sign in, that a sign-in has no limits, a failed read with a retry, and a missing CLI as one quiet header", () => {
  const renderOutcome = (outcome: LimitsOutcome, name = "Claude") =>
    renderUsage({ report: { checkedAtMs: NOW, sources: [{ source: "Claude", name, outcome }] } });
  const signedOut = renderOutcome({ kind: "SignedOut", signInCommand: "claude auth login" }, "Work Claude");
  expect(text(signedOut)).toContain("Work Claude");
  expect(byTestId(signedOut, "usage-note-Claude")).toBe("Signed out. Run claude auth login in a terminal.");
  expect(byTestId(renderOutcome({ kind: "NoLimits", plan: null }), "usage-note-Claude")).toBe("No subscription limits on this sign-in.");
  const failed = renderOutcome({ kind: "Failed", message: "exited with code 1: boom" });
  expect(text(/data-testid="usage-failed-Claude"[^>]*>(.*?)<\/div>/.exec(failed)?.[1])).toBe("exited with code 1: boom Try again");
  const missing = renderOutcome({ kind: "NotFound", binary: "claude on PATH (/usr/bin)" });
  expect(byTestId(missing, "usage-missing-Claude")).toBe("· not installed");
  expect([testIds(missing, "limit-"), testIds(missing, "usage-note-")]).toEqual([[], []]);
});

it("shows a skeleton and Checking before the first report, and keeps the last report on screen during a refresh", () => {
  const first = renderUsage({ report: null, loading: true });
  expect([has(first, "usage-loading"), byTestId(first, "usage-checked")]).toEqual([true, "Checking…"]);
  expect(first).toMatch(/data-testid="usage-refresh"[^>]*disabled=""/);
  const again = renderUsage({ report: LIMITS, loading: true });
  expect([has(again, "usage-loading"), has(again, "limit-Claude-session"), byTestId(again, "usage-checked")]).toEqual([false, true, "Checking…"]);
  const failed = renderUsage({ report: null, error: "IPC down" });
  expect([has(failed, "usage-loading"), byTestId(failed, "usage-error")]).toEqual([false, "IPC down"]);
});
