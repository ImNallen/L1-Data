// @vitest-environment jsdom
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, expect, it, vi } from "vitest";
import type { ColumnMeta, DatabaseKind, RelationDetail, ResultMeta, RowPage, TableScript, TypeCategory } from "../../ipc/bindings";
import { buildCatalogView, useCatalog } from "../../state/catalog";
import { dbKey } from "../../state/dbKey";
import { useRelationDetail } from "../../state/relationDetail";
import { dropRowCaches } from "../../state/rowCache";
import { markDeleted, NO_EDITS, setCell } from "../../state/tableEdits";
import { useWorkspace } from "../../state/workspace";
import { shopCatalog } from "../../testing/shopCatalog";
import { dropEditor } from "../editor/editorStates";
import { gridEditing, gridKeyColumns, HeaderIcon, nextFocus, primaryKeyColumns, ResultGrid, selectRows } from "./ResultGrid";

const iconOf = (category: TypeCategory, primaryKey = false) => {
  const html = renderToStaticMarkup(<HeaderIcon column={{ name: "c", type: { display: "t", category }, source: null }} primaryKey={primaryKey} />);
  return html.match(/lucide-([a-z-]+)/)?.[1];
};

it("marks each column with an icon for its type", () => {
  const categories: TypeCategory[] = [
    "Integer",
    "Numeric",
    "Text",
    "Bool",
    "Temporal",
    "Json",
    "Uuid",
    "Binary",
    "Network",
    "Array",
    "Enum",
    "Composite",
    "Range",
    "Other",
  ];
  expect(categories.map((c) => iconOf(c))).toEqual([
    "hash",
    "hash",
    "case-sensitive",
    "toggle-left",
    "calendar",
    "braces",
    "fingerprint-pattern",
    "binary",
    "network",
    "brackets",
    "list",
    "box",
    "move-horizontal",
    "circle-dot",
  ]);
});

it("shows the key instead of the type on a primary key column", () => {
  expect(iconOf("Integer", true)).toBe("key-round");
});

it("looks primary keys up in the catalog of the database the rows came from", () => {
  const shop = buildCatalogView(shopCatalog("conn", 1));
  const orders = shop.catalog.schemas[1]!.relations.find((r) => r.name === "orders")!;
  // In `reporting`, the OID `orders` has in `shop` belongs to a table whose key is its second column.
  const reporting = buildCatalogView({
    databaseKind: "Postgres",
    db: { connectionId: "conn", database: "reporting" },
    version: 2,
    defaultSchema: "public",
    schemas: [
      {
        name: "public",
        system: false,
        tableCount: 1,
        relations: [
          {
            ...orders,
            name: "daily",
            columns: [
              { ...orders.columns[0]!, name: "orders", pk: false },
              { ...orders.columns[1]!, name: "day", pk: true },
            ],
          },
        ],
        functions: [],
      },
    ],
  });
  const views = { [dbKey(shop.catalog.db)]: shop, [dbKey(reporting.catalog.db)]: reporting };
  const column = (name: string, attnum: number): ColumnMeta => ({
    name,
    type: { display: "bigint", category: "Numeric" },
    source: { tableOid: orders.oid, attnum },
  });
  const columns = [column("a", orders.columns[0]!.attnum), column("b", orders.columns[1]!.attnum)];
  expect(primaryKeyColumns(views, { db: { connectionId: "conn", database: "shop" }, columns })).toEqual([true, false]);
  expect(primaryKeyColumns(views, { db: { connectionId: "conn", database: "reporting" }, columns })).toEqual([false, true]);
  expect(primaryKeyColumns(views, { db: { connectionId: "conn", database: "postgres" }, columns })).toEqual([false, false]);
});

it("moves the active cell by key, clamped to the result", () => {
  const from = { row: 5, col: 2 };
  const at = (key: string, mod = false) => nextFocus(key, mod, from, 100, 8);
  expect(at("ArrowDown")).toEqual({ row: 6, col: 2 });
  expect(at("ArrowLeft")).toEqual({ row: 5, col: 1 });
  expect(at("PageUp")).toEqual({ row: 0, col: 2 });
  expect(at("PageDown")).toEqual({ row: 25, col: 2 });
  expect(at("Tab")).toBeNull();
});

it("jumps to the edges with Home, End, and the modifier", () => {
  const from = { row: 5, col: 2 };
  const at = (key: string, mod = false) => nextFocus(key, mod, from, 100, 8);
  expect(at("Home")).toEqual({ row: 5, col: 0 });
  expect(at("End")).toEqual({ row: 5, col: 7 });
  expect(at("Home", true)).toEqual({ row: 0, col: 0 });
  expect(at("End", true)).toEqual({ row: 99, col: 7 });
  expect(at("ArrowDown", true)).toEqual({ row: 99, col: 2 });
  expect(at("ArrowUp", true)).toEqual({ row: 0, col: 2 });
  expect(at("ArrowRight", true)).toEqual({ row: 5, col: 7 });
  expect(at("ArrowLeft", true)).toEqual({ row: 5, col: 0 });
});

it("selects whole rows from the first to the last column, focused on the first", () => {
  expect(selectRows(4, 4, 6)).toEqual({ anchor: { row: 4, col: 5 }, focus: { row: 4, col: 0 } });
  expect(selectRows(2, 7, 6)).toEqual({ anchor: { row: 2, col: 5 }, focus: { row: 7, col: 0 } });
});

it("keeps generated columns read-only and requires database-scoped edit provenance", () => {
  const catalog = shopCatalog("ms", 1);
  const relation = catalog.schemas[1]!.relations.find((r) => r.name === "orders")!;
  relation.columns[0]!.writable = false;
  const meta = {
    databaseKind: catalog.databaseKind,
    db: catalog.db,
    columns: relation.columns.map((column) => ({
      name: column.name,
      type: { display: column.typeDisplay, category: column.category },
      source: { tableOid: relation.oid, attnum: column.attnum },
    })),
  };
  const view = buildCatalogView(catalog);
  const editing = gridEditing(view, "public", "orders", meta);
  expect(editing?.pkColumns).toEqual(["id"]);
  expect(editing?.writableColumns.has("id")).toBe(false);
  expect(editing?.writableColumns.has("status")).toBe(true);
  expect(gridEditing(view, "public", "orders", { ...meta, db: { ...meta.db, database: "reporting" } })).toBeNull();
  expect(gridEditing(view, "public", "orders", { ...meta, columns: meta.columns.map((c) => ({ ...c, source: null })) })).toBeNull();
});

it.each<{ catalogKind: DatabaseKind; resultKind: DatabaseKind }>([
  { catalogKind: "SqlServer", resultKind: "SqlServer" },
  { catalogKind: "SqlServer", resultKind: "Postgres" },
  { catalogKind: "Postgres", resultKind: "SqlServer" },
])("keeps a $resultKind result read-only with a $catalogKind catalog despite valid key provenance", ({ catalogKind, resultKind }) => {
  const catalog = shopCatalog("ms", 1);
  const relation = catalog.schemas[1]!.relations.find((r) => r.name === "orders")!;
  const meta = {
    databaseKind: catalog.databaseKind,
    db: catalog.db,
    columns: relation.columns.map((column) => ({
      name: column.name,
      type: { display: column.typeDisplay, category: column.category },
      source: { tableOid: relation.oid, attnum: column.attnum },
    })),
  };
  expect(gridEditing(buildCatalogView(catalog), "public", "orders", meta)?.pkColumns).toEqual(["id"]);
  expect(gridEditing(buildCatalogView({ ...catalog, databaseKind: catalogKind }), "public", "orders", { ...meta, databaseKind: resultKind })).toBeNull();
});

it.each(["update", "delete"])("keeps original key columns for a pending %s when the key changes or editing is refused", (change) => {
  const selected = [{ column: "id", value: "1" }];
  const edits = change === "update" ? setCell(NO_EDITS, selected, "note", "edited", "original") : markDeleted(NO_EDITS, [selected], true);
  expect(gridKeyColumns({ pkColumns: ["alternate"], writableColumns: new Set(["note"]) }, edits)).toEqual(["id"]);
  expect(gridKeyColumns(null, edits)).toEqual(["id"]);
});

const shopDb = { connectionId: "c1", database: "shop" };
const ordersDetail = (oid: number): RelationDetail => ({
  db: shopDb,
  oid,
  catalogVersion: 1,
  defaults: [],
  keys: [
    { name: "orders_pkey", columns: ["id"], kind: { kind: "Primary" } },
    {
      name: "orders_customer_id_fkey",
      columns: ["customer_id"],
      kind: { kind: "Foreign", refSchema: "public", refTable: "customers", refColumns: ["id"], onDelete: "NoAction", onUpdate: "NoAction" },
    },
  ],
  indexes: [],
  triggers: [],
});

function queryResult(rows: (string | null)[][]): ResultMeta {
  const catalog = shopCatalog("c1", 1);
  const orders = catalog.schemas[1]!.relations.find((r) => r.name === "orders")!;
  orders.columns[1]!.fk = true;
  useCatalog.setState({ views: { [dbKey(shopDb)]: buildCatalogView(catalog) } });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
  mockIPC((cmd): RowPage | RelationDetail | TableScript | null => {
    if (cmd === "result_page") return { start: 0, rows, cut: [] };
    if (cmd === "relation_detail") return ordersDetail(orders.oid);
    if (cmd === "table_script") return { text: "SELECT * FROM public.customers", acceptedTexts: ["SELECT * FROM public.customers"] };
    return null;
  });
  return {
    databaseKind: "Postgres",
    ref: { tabId: "q1", runId: 1, index: 0 },
    db: shopDb,
    columns: [
      { name: "id", type: { display: "bigint", category: "Integer" }, source: { tableOid: orders.oid, attnum: 1 } },
      { name: "customer_id", type: { display: "bigint", category: "Integer" }, source: { tableOid: orders.oid, attnum: 2 } },
    ],
    rowCount: rows.length,
    rowsSeen: rows.length,
    truncated: null,
    elapsedMs: 1,
    allRows: { kind: "Statement" },
  };
}

afterEach(() => {
  cleanup();
  clearMocks();
  dropRowCaches("q1");
  for (const id of useWorkspace.getState().order) dropEditor(id);
  useWorkspace.setState(useWorkspace.getInitialState(), true);
  useCatalog.setState(useCatalog.getInitialState(), true);
  useRelationDetail.setState(useRelationDetail.getInitialState(), true);
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("follows a foreign key from a query result to the referenced row, and not from a NULL", async () => {
  const user = userEvent.setup();
  render(
    <ResultGrid
      meta={queryResult([
        ["1", "42"],
        ["2", null],
      ])}
      done
      query={null}
      editing={null}
    />,
  );
  await user.pointer({ keys: "[MouseRight]", target: await screen.findByText("NULL") });
  expect((await screen.findByTestId("grid-menu-follow-orders_customer_id_fkey")).hasAttribute("data-disabled")).toBe(true);
  await user.keyboard("{Escape}");

  await user.pointer({ keys: "[MouseRight]", target: screen.getByText("42") });
  const follow = await screen.findByTestId("grid-menu-follow-orders_customer_id_fkey");
  expect([follow.textContent, follow.hasAttribute("data-disabled"), screen.queryByTestId("grid-menu-filter-eq")]).toEqual([
    "Go to referenced row in customers",
    false,
    null,
  ]);
  await user.click(follow);
  await vi.waitFor(() =>
    expect(Object.values(useWorkspace.getState().tabs).map((t) => [t.console.db, t.table])).toEqual([
      [shopDb, { schema: "public", table: "customers", page: 0, query: { orderBy: "", where: "", filters: [{ column: "id", op: "Eq", value: "42" }] } }],
    ]),
  );
});

it("opens the focused row as a record with ⌘I or the menu", async () => {
  const user = userEvent.setup();
  const meta = queryResult([
    ["1", "42"],
    ["2", null],
  ]);
  render(<ResultGrid meta={meta} done query={null} editing={null} />);
  await user.click(await screen.findByText("2"));
  await user.keyboard("{Meta>}i{/Meta}");
  expect(useWorkspace.getState().overlay).toEqual({ kind: "record", meta, row: 1 });

  useWorkspace.getState().close();
  await user.pointer({ keys: "[MouseRight]", target: screen.getByText("42") });
  await user.click(await screen.findByTestId("grid-menu-record"));
  expect(useWorkspace.getState().overlay).toEqual({ kind: "record", meta, row: 0 });
});
