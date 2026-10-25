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
import { dropRowCaches, PAGE_SIZE, rowCacheFor } from "../../state/rowCache";
import { markDeleted, NO_EDITS, setCell, useTableEdits } from "../../state/tableEdits";
import { useWorkspace } from "../../state/workspace";
import { Overlays } from "../../shell/Overlays";
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
const foreignKey = (name: string, columns: string[], refTable: string, refColumns: string[]) => ({
  name,
  columns,
  kind: { kind: "Foreign" as const, refSchema: "public", refTable, refColumns, onDelete: "NoAction" as const, onUpdate: "NoAction" as const },
});
const ordersDetail = (oid: number, extraKeys: RelationDetail["keys"]): RelationDetail => ({
  db: shopDb,
  oid,
  catalogVersion: 1,
  defaults: [],
  keys: [
    { name: "orders_pkey", columns: ["id"], kind: { kind: "Primary" } },
    foreignKey("orders_customer_id_fkey", ["customer_id"], "customers", ["id"]),
    ...extraKeys,
  ],
  indexes: [],
  triggers: [],
});

type ResultSpec = {
  columns?: { name: string; attnum: number | null }[];
  databaseKind?: DatabaseKind;
  allRows?: ResultMeta["allRows"];
  cut?: [number, number][];
  cell?: (row: number, col: number) => string;
  keys?: RelationDetail["keys"];
};

const ORDER_COLUMNS = [
  { name: "id", attnum: 1 },
  { name: "customer_id", attnum: 2 },
];

let detailReads = 0;

function queryResult(rows: (string | null)[][], spec: ResultSpec = {}): ResultMeta {
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
  mockIPC((cmd, args): RowPage | RelationDetail | TableScript | string | null => {
    if (cmd === "result_page") return { start: 0, rows, cut: spec.cut ?? [] };
    if (cmd === "result_cell") {
      const a = args as { row: number; col: number };
      return spec.cell!(a.row, a.col);
    }
    if (cmd === "relation_detail") {
      detailReads++;
      return ordersDetail(orders.oid, spec.keys ?? []);
    }
    if (cmd === "table_script") return { text: "SELECT * FROM public.customers", acceptedTexts: ["SELECT * FROM public.customers"] };
    return null;
  });
  return {
    databaseKind: spec.databaseKind ?? "Postgres",
    ref: { tabId: "q1", runId: 1, index: 0 },
    db: shopDb,
    columns: (spec.columns ?? ORDER_COLUMNS).map(({ name, attnum }) => ({
      name,
      type: { display: name === "status" ? "text" : "bigint", category: name === "status" ? "Text" : "Integer" },
      source: attnum === null ? null : { tableOid: orders.oid, attnum },
    })),
    rowCount: rows.length,
    rowsSeen: rows.length,
    truncated: null,
    elapsedMs: 1,
    allRows: spec.allRows ?? { kind: "Statement" },
  };
}

afterEach(() => {
  cleanup();
  clearMocks();
  detailReads = 0;
  dropRowCaches("q1");
  for (const id of useWorkspace.getState().order) dropEditor(id);
  useWorkspace.setState(useWorkspace.getInitialState(), true);
  useCatalog.setState(useCatalog.getInitialState(), true);
  useRelationDetail.setState(useRelationDetail.getInitialState(), true);
  useTableEdits.setState(useTableEdits.getInitialState(), true);
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

const EDITING = { pkColumns: ["id"], writableColumns: new Set(["customer_id"]) };
const lastOpened = (table: string) =>
  Object.values(useWorkspace.getState().tabs)
    .filter((t) => t.table?.table === table)
    .at(-1)?.table?.query.filters;
const byId = (value: string) => [{ column: "id", op: "Eq", value }];

async function followFrom(user: ReturnType<typeof userEvent.setup>, cell: HTMLElement, key = "orders_customer_id_fkey") {
  await user.pointer({ keys: "[MouseRight]", target: cell });
  await user.click(await screen.findByTestId(`grid-menu-follow-${key}`));
}

it("follows a key from a SQL Server table page, whose columns carry their table column", async () => {
  const user = userEvent.setup();
  render(<ResultGrid meta={queryResult([["1", "42"]], { databaseKind: "SqlServer", allRows: { kind: "Table" } })} done query={null} editing={null} />);
  await followFrom(user, await screen.findByText("42"));
  await vi.waitFor(() => expect(lastOpened("customers")).toEqual(byId("42")));
});

it("offers no follow and reads no keys for columns without a table column, as a SQL Server statement result has", async () => {
  const user = userEvent.setup();
  const meta = queryResult([["1", "42"]], { databaseKind: "SqlServer", columns: ORDER_COLUMNS.map((c) => ({ ...c, attnum: null })) });
  render(<ResultGrid meta={meta} done query={null} editing={null} />);
  await user.pointer({ keys: "[MouseRight]", target: await screen.findByText("42") });
  await screen.findByTestId("grid-menu-record");
  expect([screen.queryByTestId("grid-menu-follow-orders_customer_id_fkey"), detailReads]).toEqual([null, 0]);
});

it("disables View as record on a staged new row", async () => {
  const user = userEvent.setup();
  const meta = queryResult([["1", "42"]]);
  useTableEdits.getState().addRow("q1");
  render(<ResultGrid meta={meta} done query={null} editing={EDITING} />);
  await screen.findByText("42");
  await user.pointer({ keys: "[MouseRight]", target: screen.getByTestId("grid-cell-1-1") });
  expect((await screen.findByTestId("grid-menu-record")).hasAttribute("data-disabled")).toBe(true);
  await user.keyboard("{Escape}");
  await user.pointer({ keys: "[MouseRight]", target: screen.getByText("42") });
  expect((await screen.findByTestId("grid-menu-record")).hasAttribute("data-disabled")).toBe(false);
});

it("follows a cut key by its full value, and a staged key by the value the grid shows", async () => {
  const user = userEvent.setup();
  const meta = queryResult(
    [
      ["1", "4200"],
      ["2", "4300"],
    ],
    {
      cut: [
        [0, 1],
        [1, 1],
      ],
      cell: (row) => (row === 0 ? "42000000" : "43000000"),
    },
  );
  useTableEdits.getState().edit("q1", (e) => setCell(e, [{ column: "id", value: "2" }], "customer_id", "7", "4300"));
  render(<ResultGrid meta={meta} done query={null} editing={EDITING} />);
  await followFrom(user, await screen.findByTestId("grid-cell-0-1"));
  await vi.waitFor(() => expect(lastOpened("customers")).toEqual(byId("42000000")));
  await followFrom(user, screen.getByText("7"));
  await vi.waitFor(() => expect(lastOpened("customers")).toEqual(byId("7")));
});

it("offers each copy of a single-column key its own follow", async () => {
  const user = userEvent.setup();
  const columns = [...ORDER_COLUMNS, ...ORDER_COLUMNS];
  render(<ResultGrid meta={queryResult([["1", "42", "2", "9"]], { columns })} done query={null} editing={null} />);
  await followFrom(user, await screen.findByText("9"));
  await vi.waitFor(() => expect(lastOpened("customers")).toEqual(byId("9")));
});

it("names the key columns when two keys from one cell reach the same table", async () => {
  const user = userEvent.setup();
  const meta = queryResult([["1", "42", "shipped"]], {
    columns: [...ORDER_COLUMNS, { name: "status", attnum: 3 }],
    keys: [foreignKey("orders_customer_id_status_fkey", ["customer_id", "status"], "customers", ["id", "region"])],
  });
  render(<ResultGrid meta={meta} done query={null} editing={null} />);
  await user.pointer({ keys: "[MouseRight]", target: await screen.findByText("42") });
  await screen.findByTestId("grid-menu-follow-orders_customer_id_status_fkey");
  expect(
    screen
      .getAllByRole("menuitem")
      .map((item) => item.textContent)
      .filter((label) => label?.startsWith("Go to")),
  ).toEqual(["Go to referenced row in customers (customer_id)", "Go to referenced row in customers (customer_id, status)"]);
});

it.each(["{Shift>}{F10}{/Shift}", "{ContextMenu}"])("opens the focused cell's menu from the keyboard with %s", async (keys) => {
  const user = userEvent.setup();
  render(<ResultGrid meta={queryResult([["1", "42"]])} done query={null} editing={null} />);
  await user.click(await screen.findByText("1"));
  await user.keyboard("{ArrowRight}");
  await user.keyboard(keys);
  await user.click(await screen.findByTestId("grid-menu-follow-orders_customer_id_fkey"));
  await vi.waitFor(() => expect(lastOpened("customers")).toEqual(byId("42")));
});

it("returns focus to the grid when a record view opened from the menu closes", async () => {
  const user = userEvent.setup();
  render(
    <>
      <ResultGrid meta={queryResult([["1", "42"]])} done query={null} editing={null} />
      <Overlays />
    </>,
  );
  await user.pointer({ keys: "[MouseRight]", target: await screen.findByText("42") });
  await user.click(await screen.findByTestId("grid-menu-record"));
  await screen.findByTestId("record-dialog");
  await user.keyboard("{Escape}");
  await vi.waitFor(() => expect(document.activeElement).toBe(screen.getByTestId("result-grid")));
});

it("loads its visible rows again after the record view pages them out of the shared cache", async () => {
  const meta = { ...queryResult([]), rowCount: 20_000, rowsSeen: 20_000 };
  mockIPC((cmd, args) => {
    if (cmd !== "result_page") return null;
    const { start, len } = args as { start: number; len: number };
    return { start, rows: Array.from({ length: len }, (_, i) => [String(start + i + 1), "c"]), cut: [] };
  });
  render(<ResultGrid meta={meta} done query={null} editing={null} />);
  await vi.waitFor(() => expect(screen.getByTestId("grid-cell-0-0").textContent).toBe("1"));
  const cache = rowCacheFor(meta.ref);
  for (let page = 10; page < 80; page++) await cache.ensure({ start: page * PAGE_SIZE, end: page * PAGE_SIZE + 1 }, meta.rowCount);
  await vi.waitFor(() => expect(screen.getByTestId("grid-cell-0-0").textContent).toBe("1"));
});
