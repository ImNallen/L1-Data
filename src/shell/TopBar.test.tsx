import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, expect, it } from "vitest";
import type { ConsoleState, TransactionLoss } from "../ipc/bindings";
import { useLive } from "../state/live";
import { useWorkspace, type Tab } from "../state/workspace";
import { TopBar } from "./TopBar";

const workspace = useWorkspace.getInitialState();
const live = useLive.getInitialState();
const saved = { tabs: workspace.tabs, active: workspace.active, consoles: live.consoles };

afterEach(() => {
  Object.assign(workspace, { tabs: saved.tabs, active: saved.active });
  live.consoles = saved.consoles;
});

function lostSession(transaction: TransactionLoss): string {
  const tab: Tab = { id: "t1", title: "Query", console: { db: { connectionId: "c1", database: "shop" }, mode: "AutoCommit" }, table: null };
  const state: ConsoleState = {
    tabId: tab.id,
    rev: 1,
    config: { connectionId: "c1", database: "shop", schema: null, mode: "AutoCommit" },
    session: { kind: "Lost", reason: "cancelled", transaction },
    txn: "Idle",
    txnDepth: null,
    run: { kind: "Idle" },
  };
  Object.assign(workspace, { tabs: { [tab.id]: tab }, active: tab.id });
  live.consoles = { [tab.id]: state };
  return renderToStaticMarkup(<TopBar hosted={null} />);
}

it("shows a session reset without promising rollback when the transaction outcome is unknown", () => {
  const html = lostSession("Unknown");
  expect(html).toContain("session reset");
  expect(html).toContain("Earlier commits remain committed; the last transaction outcome may be unknown.");
  expect(html).not.toContain("rolled back");
});

it("retains the confirmed PostgreSQL rollback message", () => {
  const html = lostSession("RolledBack");
  expect(html).toContain("rolled back");
  expect(html).toContain("Transaction rolled back: connection lost");
  expect(html).not.toContain("session reset");
});

it("does not claim a lost transaction when an idle PostgreSQL session closes", () => {
  const html = lostSession("None");
  expect(html).not.toContain("rolled back");
  expect(html).not.toContain("session reset");
});
