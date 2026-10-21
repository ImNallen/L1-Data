import { mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it, vi } from "vitest";
import { useWorkspace, type TableTab } from "../../state/workspace";
import { cycleTableOrder, filterForValue } from "./tableQuery";

const tab: TableTab = { id: "table", title: "odd table", console: { db: { connectionId: "ms", database: "shop" }, mode: "AutoCommit" }, table: { schema: "dbo", table: "odd table", page: 0, query: { orderBy: "[odd name]", where: "", filters: [] } } };
afterEach(() => { useWorkspace.setState(useWorkspace.getInitialState(), true); vi.unstubAllGlobals(); });

it("filters by a cell's value, using IS NULL for nulls", () => {
  expect(filterForValue("region", "EU-West", false)).toEqual({ column: "region", op: "Eq", value: "EU-West" });
  expect(filterForValue("region", "EU-West", true)).toEqual({ column: "region", op: "Ne", value: "EU-West" });
  expect(filterForValue("region", null, false)).toEqual({ column: "region", op: "IsNull", value: "" });
  expect(filterForValue("region", null, true)).toEqual({ column: "region", op: "NotNull", value: "" });
});

it("cycles header ordering through the engine and stores its SQL unchanged", async () => {
  vi.stubGlobal("window", { crypto });
  useWorkspace.getState().addTab(tab);
  const requests: unknown[] = [];
  mockIPC((cmd, args) => {
    if (cmd === "table_order") { requests.push(args); return { sql: "[odd name] DESC", keys: [{ column: "odd name", desc: true }] }; }
    return null;
  });
  await cycleTableOrder(tab, "odd name", true);
  expect(requests).toEqual([{ db: tab.console.db, orderBy: "[odd name]", edit: { kind: "Cycle", column: "odd name", additive: true } }]);
  expect(useWorkspace.getState().tabs.table?.table?.query.orderBy).toBe("[odd name] DESC");
});

it("drops a sort reply after the user changes an advanced order expression", async () => {
  vi.stubGlobal("window", { crypto });
  useWorkspace.getState().addTab(tab);
  mockIPC((cmd) => {
    if (cmd === "table_order") {
      useWorkspace.getState().setTableQuery(tab.id, { ...tab.table.query, orderBy: "lower([odd name])" });
      return { sql: "[odd name] DESC", keys: [{ column: "odd name", desc: true }] };
    }
    return null;
  });
  await cycleTableOrder(tab, "odd name", false);
  expect(useWorkspace.getState().tabs.table?.table?.query.orderBy).toBe("lower([odd name])");
});


it("keeps rapid additive sorts in order using the previous engine reply", async () => {
  vi.stubGlobal("window", { crypto });
  useWorkspace.getState().addTab(tab);
  const orders: unknown[] = [];
  mockIPC((cmd, args) => {
    if (cmd === "table_order") {
      orders.push(args);
      return { sql: orders.length === 1 ? "[odd name] DESC" : "[odd name] DESC, [other]", keys: null };
    }
    return null;
  });
  await Promise.all([cycleTableOrder(tab, "odd name", false), cycleTableOrder(tab, "other", true)]);
  expect(orders).toEqual([
    { db: tab.console.db, orderBy: "[odd name]", edit: { kind: "Cycle", column: "odd name", additive: false } },
    { db: tab.console.db, orderBy: "[odd name] DESC", edit: { kind: "Cycle", column: "other", additive: true } },
  ]);
  expect(useWorkspace.getState().tabs.table?.table?.query.orderBy).toBe("[odd name] DESC, [other]");
});
