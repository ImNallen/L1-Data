import { mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { ConsoleState, DisconnectError } from "../../ipc/bindings";
import { useLive } from "../../state/live";
import { setCell, useTableEdits } from "../../state/tableEdits";
import { useWorkspace } from "../../state/workspace";
import { closeTab } from "./closeTab";
import { dropEditor } from "./editorStates";
import { openSqlTab } from "./runActions";

const ws = () => useWorkspace.getState();
const flush = () => new Promise((r) => setTimeout(r, 0));
const question = () => {
  const o = ws().overlay;
  return o.kind === "ask" ? o.title : null;
};

let closes: boolean[];
let openTransaction: boolean;

beforeEach(() => {
  vi.stubGlobal("window", { crypto });
  closes = [];
  openTransaction = false;
  mockIPC((cmd, args) => {
    if (cmd !== "console_close") return null;
    const force = (args as { force: boolean }).force;
    closes.push(force);
    if (openTransaction && !force) throw { kind: "OpenTransactions", tabs: [] } satisfies DisconnectError;
    return null;
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
  for (const id of ws().order) dropEditor(id);
  useWorkspace.setState(useWorkspace.getInitialState(), true);
  useLive.setState(useLive.getInitialState(), true);
  useTableEdits.setState(useTableEdits.getInitialState(), true);
});

function tabWithConsole(): string {
  const id = openSqlTab({ db: { connectionId: "c1", database: "shop" } });
  const console: ConsoleState = {
    tabId: id,
    rev: 1,
    config: { connectionId: "c1", database: "shop", schema: null, mode: "Manual" },
    session: { kind: "Open", backendPid: 7 },
    txn: "Open",
    txnDepth: null,
    work: null,
    run: { kind: "Idle" },
  };
  useLive.setState({ consoles: { [id]: console } });
  return id;
}

const stageEdit = (id: string) =>
  useTableEdits.getState().edit(id, (e) => setCell(e, [{ column: "id", value: "1" }], "total", "9", "1"));

it("closes a tab with nothing to lose without asking", async () => {
  const id = openSqlTab();
  await closeTab(id);
  expect([ws().order, question(), closes]).toEqual([[], null, []]);
});

it("asks to discard staged edits, then to roll back, and closes only after both", async () => {
  const id = tabWithConsole();
  openTransaction = true;
  stageEdit(id);
  const closing = closeTab(id);
  expect(question()).toBe("Close tab with unsaved changes?");
  ws().answer("btn");
  await vi.waitFor(() => expect(question()).toBe("Close tab with an open transaction?"));
  expect(ws().order).toEqual([id]);
  ws().answer("btn");
  await closing;
  expect([ws().order, closes]).toEqual([[], [false, true]]);
});

it("keeps the tab and its transaction when the rollback question is cancelled", async () => {
  const id = tabWithConsole();
  openTransaction = true;
  const closing = closeTab(id);
  await vi.waitFor(() => expect(question()).toBe("Close tab with an open transaction?"));
  ws().close();
  await closing;
  await flush();
  expect([ws().order, closes]).toEqual([[id], [false]]);
});

it("keeps the tab and its staged edits when the discard question is cancelled", async () => {
  const id = tabWithConsole();
  stageEdit(id);
  const closing = closeTab(id);
  ws().close();
  await closing;
  expect([ws().order, closes, Object.keys(useTableEdits.getState().byTab)]).toEqual([[id], [], [id]]);
});
