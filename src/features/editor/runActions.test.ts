import { undo } from "@codemirror/commands";
import { mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { ConsoleState, Settings, TableQuery, TableScript, TabSnapshot } from "../../ipc/bindings";
import { connectionConfig } from "../../testing/connections";
import { useLive } from "../../state/live";
import { useTableEdits } from "../../state/tableEdits";
import { useWorkspace, workspaceSnapshot } from "../../state/workspace";
import { dropEditor, editorStateOf, insertAtCursor, moveCursor, seedEditor, textOf } from "./editorStates";
import { browseTemplate, formatActive, loadTableOnce, openTableTab, saveSettings, setTabMode, setTabTarget, runOrChoose, openSqlTab, isBrowsing } from "./runActions";

afterEach(() => {
  vi.unstubAllGlobals();
  for (const id of useWorkspace.getState().order) dropEditor(id);
  useWorkspace.setState(useWorkspace.getInitialState(), true);
  useLive.setState(useLive.getInitialState(), true);
  useTableEdits.setState(useTableEdits.getInitialState(), true);
});

const orders = { db: { connectionId: "c1", database: "shop" }, schema: "public", relation: "orders" };
const ordersScript = {
  text: "SELECT * FROM public.orders",
  acceptedTexts: ["SELECT * FROM public.orders", "SELECT * FROM public.orders;"],
} satisfies TableScript;

beforeEach(() => {
  vi.stubGlobal("window", { crypto });
  mockIPC((cmd) => cmd === "table_script" ? ordersScript : null);
});

it("uses the engine's canonical template for bracketed table names", async () => {
  const calls: unknown[] = [];
  mockIPC((cmd, args) => {
    if (cmd === "table_script") { calls.push(args); return { text: "SELECT * FROM [Sales].[line items]", acceptedTexts: ["SELECT * FROM [Sales].[line items]"] } satisfies TableScript; }
  });
  const db = { connectionId: "mssql", database: "shop" };
  await openTableTab({ db, schema: "Sales", relation: "line items" });
  const tab = useWorkspace.getState().tabs[useWorkspace.getState().active!];
  expect(calls).toEqual([{ db, schema: "Sales", table: "line items" }]);
  expect(tab && textOf(tab.id)).toBe("SELECT * FROM [Sales].[line items]");
  expect(tab && isBrowsing(tab)).toBe(true);
});

it("opens the results when a table opens, new or already in a tab", async () => {
  const ref = { db: { connectionId: "c1", database: "shop" }, schema: "public", relation: "orders" };
  useWorkspace.setState({ layout: { ...useWorkspace.getState().layout, results: false } });
  await openTableTab(ref);
  const tab = useWorkspace.getState().tabs[useWorkspace.getState().active!];
  expect([tab, useWorkspace.getState().layout.results]).toEqual([
    {
      id: tab?.id,
      title: "orders",
      console: { db: { connectionId: "c1", database: "shop" }, mode: "AutoCommit" },
      table: { schema: "public", table: "orders", page: 0, query: { orderBy: "", filters: [], where: "" } },
    },
    true,
  ]);

  useWorkspace.setState({ layout: { ...useWorkspace.getState().layout, results: false } });
  await openTableTab(ref);
  expect(Object.keys(useWorkspace.getState().tabs)).toHaveLength(1);
  expect(useWorkspace.getState().layout.results).toBe(true);
});

it("writes settings one at a time, so the write that lands last carries every change", async () => {
  vi.stubGlobal("window", { crypto });
  const pending: { settings: Settings; finish: () => void }[] = [];
  let inFlight = 0;
  let overlapped = false;
  let persisted: Settings | null = null;
  mockIPC((cmd, args) => {
    if (cmd !== "settings_set") return;
    const { settings } = args as { settings: Settings };
    overlapped ||= ++inFlight > 1;
    return new Promise<void>((resolve) =>
      pending.push({
        settings,
        finish: () => {
          inFlight--;
          persisted = settings;
          resolve();
        },
      }),
    );
  });
  const flush = () => new Promise((r) => setTimeout(r, 0));

  const saves = Promise.all([saveSettings({ maxRows: 5 }), saveSettings({ colorScheme: "Dark" })]);
  for (await flush(); pending.length > 0; await flush()) pending.pop()!.finish();
  await saves;

  expect({ overlapped, persisted }).toMatchObject({ overlapped: false, persisted: { maxRows: 5, colorScheme: "Dark" } });
});

async function openWithConsole(mode: "AutoCommit" | "Manual" = "AutoCommit") {
  await openTableTab(orders);
  const id = useWorkspace.getState().active!;
  const console: ConsoleState = {
    tabId: id,
    rev: 1,
    config: { connectionId: "c1", database: "shop", schema: null, mode },
    session: { kind: "None" },
    txn: "Idle",
    txnDepth: null,
    run: { kind: "Idle" },
  };
  useLive.setState({ consoles: { [id]: console } });
  return id;
}

it("switches a table tab to manual transactions through console_configure", async () => {
  vi.stubGlobal("window", { crypto });
  const calls: { cmd: string; args: unknown }[] = [];
  mockIPC((cmd, args) => {
    if (cmd === "table_script") return ordersScript;
    calls.push({ cmd, args });
    return null;
  });
  const id = await openWithConsole();
  calls.length = 0;
  await setTabMode(id, "Manual");
  expect(calls).toEqual([
    { cmd: "console_configure", args: { tabId: id, config: { connectionId: "c1", database: "shop", schema: null, mode: "Manual" } } },
  ]);
  expect(useWorkspace.getState().tabs[id]?.console.mode).toBe("Manual");
});

it("keeps a tab on its database while it has staged edits", async () => {
  vi.stubGlobal("window", { crypto });
  const calls: string[] = [];
  mockIPC((cmd) => {
    if (cmd === "table_script") return ordersScript;
    calls.push(cmd);
    return null;
  });
  const id = await openWithConsole();
  calls.length = 0;
  useTableEdits.getState().addRow(id);
  await setTabTarget(id, { connectionId: "c1", database: "reporting" });
  expect([calls, useWorkspace.getState().tabs[id]?.console.db, useWorkspace.getState().toast?.text]).toEqual([
    [],
    { connectionId: "c1", database: "shop" },
    "Submit or revert the pending changes first.",
  ]);

  useTableEdits.getState().clear(id);
  await setTabTarget(id, { connectionId: "c1", database: "reporting" });
  expect([calls, useWorkspace.getState().tabs[id]?.console.db]).toEqual([["console_configure"], { connectionId: "c1", database: "reporting" }]);
});

function openScript(text: string, edit: () => unknown) {
  const sent: unknown[] = [];
  mockIPC((cmd, args) => {
    if (cmd !== "editor_format") return null;
    sent.push(args);
    return edit();
  });
  seedEditor("script", text);
  useWorkspace.getState().addTab({ id: "script", title: "Untitled", console: { db: null, mode: "AutoCommit" }, table: null });
  return sent;
}

it("formats as its own undo step, apart from the typing before it, and leaves the cursor after the edit", async () => {
  vi.stubGlobal("window", { crypto });
  const sent = openScript("select 1;\nselect a,b from", () => ({ from: 10, to: 27, insert: "SELECT a,\n  b\nFROM t" }));
  moveCursor("script", 25);
  insertAtCursor("script", " t");
  const typed = textOf("script");
  await formatActive();
  let state = editorStateOf("script");
  expect([sent, state.doc.toString(), state.selection.main.head]).toEqual([
    [{ text: typed, databaseKind: "Postgres", target: { kind: "cursor", offset: 27 } }],
    "select 1;\nSELECT a,\n  b\nFROM t",
    30,
  ]);
  undo({ state, dispatch: (tr) => (state = tr.state) });
  expect(state.doc.toString()).toBe(typed);
});

it("toasts a refused format, stays quiet with nothing to format, and drops an edit for text that changed", async () => {
  vi.stubGlobal("window", { crypto });
  let reply: () => unknown = () => Promise.reject({ kind: "Unsafe" });
  openScript("select 1", () => reply());
  await formatActive();
  expect([useWorkspace.getState().toast?.text, textOf("script")]).toEqual(["Couldn't format this statement safely.", "select 1"]);

  useWorkspace.setState({ toast: null });
  reply = () => Promise.reject({ kind: "NothingToFormat" });
  await formatActive();
  expect([useWorkspace.getState().toast, textOf("script")]).toEqual([null, "select 1"]);

  reply = () => {
    insertAtCursor("script", "-- typed\n");
    return { from: 0, to: 8, insert: "SELECT 1" };
  };
  await formatActive();
  expect(textOf("script")).toBe("-- typed\nselect 1");
});

it("loads a table tab's page on first show, and never runs edited text unasked", async () => {
  vi.stubGlobal("window", { crypto });
  const runs: unknown[] = [];
  mockIPC((cmd, args) => {
    if (cmd === "table_script") return ordersScript;
    if (cmd === "console_run") runs.push((args as { request: unknown }).request);
    if (cmd === "console_run") return { runId: 1, statements: [] };
    return null;
  });
  const query = { orderBy: "total DESC", filters: [{ column: "status", op: "Eq", value: "paid" }], where: "total > 10" } satisfies TableQuery;
  const table = { schema: "public", table: "orders", page: 2, query };
  for (const [id, text] of [["browsing", "SELECT * FROM public.orders"], ["edited", "SELECT * FROM public.orders WHERE status = 'pending'"]] as const) {
    seedEditor(id, text);
    useWorkspace.getState().addTab({ id, title: "orders", console: { db: { connectionId: "c1", database: "shop" }, mode: "AutoCommit" }, table });
  }
  await loadTableOnce("edited");
  await loadTableOnce("browsing");
  await vi.waitFor(() => expect(runs).toHaveLength(1));
  await new Promise((r) => setTimeout(r, 0));
  expect(runs).toEqual([{ kind: "tablePage", schema: "public", table: "orders", page: 2, query }]);
});

it.each([
  "SELECT * FROM public.orders",
  "SELECT * FROM public.orders;",
  "\n SELECT * FROM public.orders\n ",
  "\t SELECT * FROM public.orders;\n",
])("restores a saved browse document %j with its page, filters and order", async (text) => {
  const runs: unknown[] = [];
  mockIPC((cmd, args) => {
    if (cmd === "table_script") return ordersScript;
    if (cmd === "console_run") {
      runs.push(args);
      return { runId: 1, statements: [] };
    }
    return null;
  });
  const id = crypto.randomUUID();
  const query = { orderBy: "total DESC", filters: [{ column: "status", op: "Eq", value: "paid" }], where: "total > 10" } satisfies TableQuery;
  const saved: TabSnapshot = {
    id, title: "orders", text,
    console: { connectionId: "c1", database: "shop", schema: null, mode: "AutoCommit" },
    table: { schema: "public", table: "orders", page: 2, query },
  };
  const ws = useWorkspace.getState();
  seedEditor(id, text);
  ws.restore({ tabs: [saved], active: id, layout: ws.layout }, [connectionConfig("c1", "PostgreSQL")], ws.settings);
  await loadTableOnce(id);
  await vi.waitFor(() => expect(runs).toHaveLength(1));
  expect(runs).toEqual([{ tabId: id, request: { kind: "tablePage", schema: "public", table: "orders", page: 2, query } }]);
  const tab = useWorkspace.getState().tabs[id];
  expect(tab && isBrowsing(tab)).toBe(true);
  expect(workspaceSnapshot(useWorkspace.getState(), textOf).tabs).toEqual([saved]);
});

it.each([
  "SELECT * FROM public.orders WHERE status = 'pending'",
  "SELECT * FROM public.orders;;",
  "SELECT * FROM public.orders; -- changed",
])("keeps a changed saved document %j out of table browsing", async (text) => {
  const runs: unknown[] = [];
  mockIPC((cmd, args) => {
    if (cmd === "table_script") return ordersScript;
    if (cmd === "console_run") runs.push(args);
    return null;
  });
  const id = crypto.randomUUID();
  seedEditor(id, text);
  useWorkspace.getState().addTab({ id, title: "orders", console: { db: orders.db, mode: "AutoCommit" }, table: { schema: "public", table: "orders", page: 2, query: { orderBy: "id DESC", filters: [], where: "" } } });
  await loadTableOnce(id);
  const tab = useWorkspace.getState().tabs[id];
  expect(tab && isBrowsing(tab)).toBe(false);
  expect(runs).toEqual([]);
  expect(textOf(id)).toBe(text);
});

it("compares engine-supplied strings without interpreting their punctuation", async () => {
  const db = { connectionId: crypto.randomUUID(), database: "shop" };
  mockIPC((cmd) => cmd === "table_script" ? { text: "engine template", acceptedTexts: ["engine template", "another engine form"] } satisfies TableScript : null);
  const id = crypto.randomUUID();
  seedEditor(id, "\n another engine form\t");
  useWorkspace.getState().addTab({ id, title: "orders", console: { db, mode: "AutoCommit" }, table: { schema: "public", table: "orders", page: 0, query: { orderBy: "", filters: [], where: "" } } });
  await browseTemplate(db, "public", "orders");
  const tab = useWorkspace.getState().tabs[id];
  expect(tab && isBrowsing(tab)).toBe(true);
  seedEditor(id, "engine template;");
  expect(tab && isBrowsing(tab)).toBe(false);
});

it("drops a table script reply after its dialect changes and loads the current dialect", async () => {
  const connectionId = crypto.randomUUID();
  const config = connectionConfig(connectionId, "dev");
  useWorkspace.getState().upsertConfig(config);
  const db = { connectionId, database: "shop" };
  let finish: (script: TableScript) => void = () => { throw new Error("table script was not requested"); };
  mockIPC((cmd) => cmd === "table_script" ? new Promise<TableScript>((resolve) => { finish = resolve; }) : null);
  const pending = browseTemplate(db, "public", "orders");
  useWorkspace.getState().upsertConfig({ ...config, databaseKind: "SqlServer" });
  finish(ordersScript);
  expect(await pending).toBeNull();
  mockIPC((cmd) => cmd === "table_script" ? { text: "SELECT * FROM [public].[orders]", acceptedTexts: ["SELECT * FROM [public].[orders]"] } satisfies TableScript : null);
  await openTableTab({ db, schema: "public", relation: "orders" });
  const tab = useWorkspace.getState().tabs[useWorkspace.getState().active!];
  expect(tab && textOf(tab.id)).toBe("SELECT * FROM [public].[orders]");
  expect(tab && isBrowsing(tab)).toBe(true);
});

it("drops a cached page load after its dialect changes while awaiting the template", async () => {
  const connectionId = crypto.randomUUID();
  const config = connectionConfig(connectionId, "dev");
  useWorkspace.getState().upsertConfig(config);
  const db = { connectionId, database: "shop" };
  await openTableTab({ db, schema: "public", relation: "orders" });
  const id = useWorkspace.getState().active!;
  const calls: string[] = [];
  mockIPC((cmd) => { calls.push(cmd); return null; });
  const pending = loadTableOnce(id);
  useWorkspace.getState().upsertConfig({ ...config, databaseKind: "SqlServer" });
  await pending;
  expect(calls).toEqual([]);
});

it("never runs edited browse text as a script after awaiting console setup", async () => {
  await openTableTab(orders);
  const id = useWorkspace.getState().active!;
  const calls: string[] = [];
  mockIPC((cmd) => {
    calls.push(cmd);
    if (cmd === "console_configure") insertAtCursor(id, "SELECT 42;\n");
    if (cmd === "console_run") return { runId: 1, statements: [] };
    return null;
  });
  await loadTableOnce(id);
  await vi.waitFor(() => expect(calls).toContain("console_configure"));
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(calls).toEqual(["console_configure"]);
  expect(textOf(id)).toBe("SELECT 42;\nSELECT * FROM public.orders");
});

it("drops formatting when the connection changes while the document stays the same", async () => {
  openScript("select 1", () => {
    useWorkspace.getState().patchConsole("script", { db: { connectionId: "sqlserver", database: "master" } });
    return { from: 0, to: 8, insert: "SELECT 1" };
  });
  await formatActive();
  expect(textOf("script")).toBe("select 1");
});

it("passes SQL Server kind to text services before a catalog loads and reports unsupported formatting", async () => {
  useWorkspace.setState({ configs: { ms: { ...connectionConfig("ms", "SQL Server"), databaseKind: "SqlServer" } } });
  const sent = openScript("declare @x int = 1", () => Promise.reject({ kind: "Unsupported" }));
  useWorkspace.getState().patchConsole("script", { db: { connectionId: "ms", database: "master" } });
  await formatActive();
  expect(sent).toEqual([{ text: "declare @x int = 1", target: { kind: "cursor", offset: 0 }, databaseKind: "SqlServer" }]);
  expect(useWorkspace.getState().toast?.text).toBe("Formatting is not supported for this SQL construct.");
});

it("does not run an outline response after switching the target with unchanged text", async () => {
  useWorkspace.setState({ configs: { ms: { ...connectionConfig("ms", "SQL Server"), databaseKind: "SqlServer" } } });
  const requests: unknown[] = [];
  const runs: unknown[] = [];
  const id = openSqlTab({ db: { connectionId: "ms", database: "master" }, text: "select 1" });
  mockIPC((cmd, args) => {
    if (cmd === "script_outline") {
      requests.push(args);
      useWorkspace.getState().patchConsole(id, { db: { connectionId: "ms", database: "other" } });
      return { statements: [{ from: 0, to: 8 }], atCursor: 0, unitKind: "Batch" };
    }
    if (cmd === "console_run") runs.push(args);
    return null;
  });
  await runOrChoose();
  expect(requests).toEqual([{ text: "select 1", cursor: 0, databaseKind: "SqlServer" }]);
  expect(runs).toEqual([]);
});

it.each(["database", "mode"])("drops a pending run when its %s changes during console setup", async (change) => {
  const calls: string[] = [];
  let finish: () => void = () => { throw new Error("console setup was not requested"); };
  const id = openSqlTab({ db: { connectionId: "c1", database: "shop" }, text: "SELECT 1" });
  mockIPC((cmd) => {
    calls.push(cmd);
    if (cmd === "console_configure") return new Promise<null>((resolve) => { finish = () => resolve(null); });
    if (cmd === "script_outline") return { statements: [{ from: 0, to: 8 }], atCursor: 0, unitKind: "Statement" };
    return null;
  });
  const pending = runOrChoose();
  await new Promise((resolve) => setTimeout(resolve, 0));
  if (change === "database") await setTabTarget(id, { connectionId: "ms", database: "master" });
  else await setTabMode(id, "Manual");
  finish();
  await pending;
  expect(calls).toEqual(["console_configure"]);
});
