import { undo } from "@codemirror/commands";
import { Children, isValidElement, type ComponentProps, type ReactElement, type ReactNode } from "react";
import { afterEach, beforeEach, expect, it } from "vitest";
import type { DatabaseRef, FunctionInfo, SchemaInfo } from "../../ipc/bindings";
import { useWorkspace, type Tab } from "../../state/workspace";
import { connectionConfig } from "../../testing/connections";
import { dropEditor, editorStateOf, insertAtCursor, moveCursor, seedEditor, textOf } from "../editor/editorStates";
import { FilteredSchemaNode, FunctionRow, SchemaNode } from "./ConnectionTree";

const postgres = { connectionId: "pg", database: "shop" };
const sqlServer = { connectionId: "mssql", database: "shop" };
const pgFunction: FunctionInfo = { oid: 12, name: "display name", args: "unparsed display arguments", returns: "int", kind: "Function", insertText: "order_count()" };
const msProcedure: FunctionInfo = { ...pgFunction, kind: "Procedure", insertText: "EXEC [odd]]schema].[proc]]name] NULL, NULL" };
const crossKindCases = [
  { source: sqlServer, destination: postgres, fn: msProcedure, label: "SQL Server" },
  { source: postgres, destination: sqlServer, fn: pgFunction, label: "PostgreSQL" },
];
const configs = {
  pg: connectionConfig("pg", "PostgreSQL"),
  mssql: { ...connectionConfig("mssql", "SQL Server"), databaseKind: "SqlServer" },
} satisfies ReturnType<typeof useWorkspace.getState>["configs"];

beforeEach(() => {
  useWorkspace.setState({ configs });
});

afterEach(() => {
  for (const id of ["active", "other"]) dropEditor(id);
  useWorkspace.setState(useWorkspace.getInitialState(), true);
});

function routineRow(db: DatabaseRef, fn: FunctionInfo) {
  const row: ReactElement<{ onDoubleClick: () => void }> = FunctionRow({ path: "connection/shop/schema/#functions", depth: 4, db, fn });
  return row;
}

function routineRows(node: ReactNode): ReactElement<ComponentProps<typeof FunctionRow>>[] {
  return Children.toArray(node).flatMap((child) => {
    if (isValidElement<ComponentProps<typeof FunctionRow>>(child) && child.type === FunctionRow) return [child];
    return isValidElement<{ children?: ReactNode }>(child) ? routineRows(child.props.children) : [];
  });
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
  { source: postgres, config: configs.pg, fn: pgFunction },
  { source: sqlServer, config: configs.mssql, fn: msProcedure },
])("inserts a routine from $source.connectionId into another saved connection of the same kind", ({ source, config, fn }) => {
  const destination = { connectionId: "another", database: "reporting" };
  useWorkspace.getState().upsertConfig({ ...config, id: destination.connectionId });
  prepareEdits(destination);

  routineRow(source, fn).props.onDoubleClick();

  expect(textOf("active")).toBe(`${fn.insertText}SELECT 42; -- retain suffix`);
  expect(editorStateOf("active").selection.main.head).toBe(fn.insertText.length);
  expect(useWorkspace.getState().tabs.active?.console.db).toEqual(destination);
  expect(useWorkspace.getState().toast).toBeNull();
});

it.each(crossKindCases)("refuses a $label routine in a different-kind active tab", ({ source, destination, fn, label }) => {
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

it.each(crossKindCases.flatMap((test) => [{ ...test, tree: "normal" }, { ...test, tree: "filtered" }]))(
  "carries the $label source through the $tree tree and inserts only after the target kind matches",
  ({ source, destination, fn, tree }) => {
    prepareEdits(destination);
    const expanded = { has: () => true, toggle: () => {} };
    const schema: SchemaInfo = { name: "schema", system: false, tableCount: 0, relations: [], functions: [fn] };
    const node = tree === "normal"
      ? SchemaNode({ path: "unrelated/display/path", depth: 2, db: source, schema, version: 1, expanded })
      : FilteredSchemaNode({ path: "unrelated/display/path", db: source, match: { name: schema.name, relations: [], functions: [{ fn, ranges: [] }] }, expanded });
    const rows = routineRows(node);
    expect(rows).toHaveLength(1);
    for (const row of rows) {
      expect(row.props.db).toEqual(source);
      const rendered: ReactElement<{ onDoubleClick: () => void }> = FunctionRow(row.props);
      expectRefused(rendered.props.onDoubleClick);
      useWorkspace.getState().patchConsole("active", { db: { ...source, database: "reporting" } });
      rendered.props.onDoubleClick();
      expect(textOf("active")).toBe(`${fn.insertText}SELECT 42; -- retain suffix`);
      expect(editorStateOf("active").selection.main.head).toBe(fn.insertText.length);
      expect(textOf("other")).toBe("SELECT 99");
      expect(useWorkspace.getState().tabs.active?.console.db).toEqual({ ...source, database: "reporting" });
    }
  },
);
