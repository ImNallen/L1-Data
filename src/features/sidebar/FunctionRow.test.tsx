import { undo } from "@codemirror/commands";
import type { ReactElement } from "react";
import { afterEach, beforeEach, expect, it } from "vitest";
import type { DatabaseRef, FunctionInfo } from "../../ipc/bindings";
import { useWorkspace, type Tab } from "../../state/workspace";
import { connectionConfig } from "../../testing/connections";
import { dropEditor, editorStateOf, insertAtCursor, moveCursor, seedEditor, textOf } from "../editor/editorStates";
import { FunctionRow } from "./ConnectionTree";

const postgres = { connectionId: "pg", database: "shop" };
const sqlServer = { connectionId: "mssql", database: "shop" };
const pgFunction: FunctionInfo = { oid: 12, name: "display name", args: "unparsed display arguments", returns: "int", kind: "Function", insertText: "order_count()" };
const msProcedure: FunctionInfo = { ...pgFunction, kind: "Procedure", insertText: "EXEC [odd]]schema].[proc]]name] NULL, NULL" };

beforeEach(() => {
  useWorkspace.setState({
    configs: {
      pg: connectionConfig("pg", "PostgreSQL"),
      mssql: { ...connectionConfig("mssql", "SQL Server"), databaseKind: "SqlServer" },
    },
  });
});

afterEach(() => {
  for (const id of ["active", "other"]) dropEditor(id);
  useWorkspace.setState(useWorkspace.getInitialState(), true);
});

function routineRow(db: DatabaseRef, fn: FunctionInfo) {
  const props = { path: "connection/shop/schema/#functions", depth: 4, db, fn };
  const row: ReactElement<{ onDoubleClick: () => void }> = FunctionRow(props);
  return row;
}

function target(db: DatabaseRef | null) {
  const tab: Tab = { id: "active", title: "Query", console: { db, mode: "AutoCommit" }, table: null };
  const other: Tab = { ...tab, id: "other", console: { db: db?.connectionId === postgres.connectionId ? sqlServer : postgres, mode: "AutoCommit" } };
  useWorkspace.setState({ tabs: { active: tab, other }, order: ["other", "active"], active: "active" });
}

function prepareEdits(db: DatabaseRef | null) {
  target(db);
  seedEditor("active", "SELECT ; -- retain suffix");
  moveCursor("active", 7);
  insertAtCursor("active", "42");
  moveCursor("active", 0);
  seedEditor("other", "SELECT 99");
}

function expectRefused(click: () => void) {
  const workspace = useWorkspace.getState();
  const before = editorStateOf("active");
  click();
  expect(textOf("active")).toBe("SELECT 42; -- retain suffix");
  expect(textOf("other")).toBe("SELECT 99");
  expect(editorStateOf("active").selection.eq(before.selection)).toBe(true);
  expect(useWorkspace.getState().active).toBe(workspace.active);
  expect(useWorkspace.getState().tabs).toEqual(workspace.tabs);
  expect(useWorkspace.getState().toast?.text).toBeTruthy();
  let state = editorStateOf("active");
  expect(undo({ state, dispatch: (tr) => (state = tr.state) })).toBe(true);
  expect(state.doc.toString()).toBe("SELECT ; -- retain suffix");
}

it.each([
  { db: sqlServer, kind: "Function", insertText: "[odd]]schema].[scalar]]name]()", prefix: "SELECT " },
  { db: sqlServer, kind: "Function", insertText: "[odd]]schema].[table]]name](NULL, NULL)", prefix: "SELECT * FROM " },
  { db: sqlServer, kind: "Procedure", insertText: msProcedure.insertText, prefix: "" },
  { db: postgres, kind: "Function", insertText: "order_count()", prefix: "SELECT " },
  { db: postgres, kind: "Procedure", insertText: "touch_order()", prefix: "SELECT " },
] satisfies { db: DatabaseRef; kind: FunctionInfo["kind"]; insertText: string; prefix: string }[])("inserts the catalog's $kind template verbatim into a same-kind tab at the active cursor", ({ db, kind, insertText, prefix }) => {
  target({ ...db, database: "reporting" });
  seedEditor("active", `${prefix}; -- retain suffix`);
  moveCursor("active", prefix.length);
  seedEditor("other", "SELECT 99");

  routineRow(db, { ...pgFunction, kind, insertText }).props.onDoubleClick();

  expect(textOf("active")).toBe(`${prefix}${insertText}; -- retain suffix`);
  expect(editorStateOf("active").selection.main.head).toBe(prefix.length + insertText.length);
  expect(textOf("other")).toBe("SELECT 99");
  expect(useWorkspace.getState().tabs.active?.console.db).toEqual({ ...db, database: "reporting" });
  expect(useWorkspace.getState().toast).toBeNull();
});

it.each([
  { source: sqlServer, destination: postgres, fn: msProcedure, label: "SQL Server" },
  { source: postgres, destination: sqlServer, fn: pgFunction, label: "PostgreSQL" },
])("refuses a $label routine in a different-kind active tab", ({ source, destination, fn, label }) => {
  prepareEdits(destination);
  expectRefused(() => routineRow(source, fn).props.onDoubleClick());
  expect(useWorkspace.getState().toast?.text).toContain(label);
});

it.each([
  { name: "no chosen target", db: null, active: "active" },
  { name: "an unsaved connection", db: { connectionId: "missing", database: "shop" }, active: "active" },
  { name: "a missing active tab", db: postgres, active: "missing" },
  { name: "no active tab", db: postgres, active: null },
])("refuses insertion with $name", ({ db, active }) => {
  prepareEdits(db);
  useWorkspace.setState({ active });
  expectRefused(() => routineRow(postgres, pgFunction).props.onDoubleClick());
});

it.each([postgres, sqlServer])("refuses a routine whose source connection was removed for $connectionId", (db) => {
  prepareEdits(db);
  expectRefused(() => routineRow({ connectionId: "removed", database: "shop" }, pgFunction).props.onDoubleClick());
});

it("checks the destination when double-clicked after its target changes", () => {
  prepareEdits(sqlServer);
  const row = routineRow(sqlServer, msProcedure);
  useWorkspace.getState().patchConsole("active", { db: postgres });
  expectRefused(row.props.onDoubleClick);
});
