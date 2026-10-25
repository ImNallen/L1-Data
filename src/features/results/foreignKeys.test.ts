import { expect, it } from "vitest";
import type { ColumnMeta, KeyInfo, RelationDetail } from "../../ipc/bindings";
import { buildCatalogView } from "../../state/catalog";
import { shopCatalog } from "../../testing/shopCatalog";
import { keyedRelations, keyLinks, tableColumns } from "./foreignKeys";

const catalog = shopCatalog("c1", 1);
const orders = catalog.schemas[1]!.relations.find((r) => r.name === "orders")!;
orders.columns[1]!.fk = true;
const view = buildCatalogView(catalog);

const foreign = (name: string, columns: string[], refTable: string, refColumns: string[]): KeyInfo => ({
  name,
  columns,
  kind: { kind: "Foreign", refSchema: "public", refTable, refColumns, onDelete: "NoAction", onUpdate: "NoAction" },
});
const detail: RelationDetail = {
  db: catalog.db,
  oid: orders.oid,
  catalogVersion: 1,
  defaults: [],
  keys: [
    { name: "orders_pkey", columns: ["id"], kind: { kind: "Primary" } },
    foreign("orders_customer_id_fkey", ["customer_id"], "customers", ["id"]),
    foreign("orders_customer_id_status_fkey", ["customer_id", "status"], "addresses", ["customer_id", "id"]),
  ],
  indexes: [],
  triggers: [],
};

const column = (name: string, attnum?: number): ColumnMeta => ({
  name,
  type: { display: "bigint", category: "Integer" },
  source: attnum === undefined ? null : { tableOid: orders.oid, attnum },
});

it("links each foreign key whose columns are all in the result, by result column index in key order", () => {
  const someColumns = tableColumns(view, { db: catalog.db, columns: [column("customer_id", 2), column("n")] }, null);
  expect(keyLinks(someColumns, () => detail)).toEqual([
    { name: "orders_customer_id_fkey", schema: "public", table: "customers", resultColumns: [0], refColumns: ["id"] },
  ]);

  const allColumns = tableColumns(view, { db: catalog.db, columns: [column("status", 3), column("id", 1), column("customer_id", 2)] }, null);
  expect(keyLinks(allColumns, () => detail).map((l) => [l.table, l.resultColumns])).toEqual([
    ["customers", [2]],
    ["addresses", [2, 0]],
  ]);
});

it("matches a column without a source by name only on a browsing table tab, and only in the catalog's database", () => {
  const result = { db: catalog.db, columns: [column("id"), column("customer_id")] };
  expect(tableColumns(view, result, null)).toEqual([null, null]);
  expect(tableColumns(view, result, { schema: "public", table: "orders" })).toEqual([
    { oid: orders.oid, name: "id", fk: false },
    { oid: orders.oid, name: "customer_id", fk: true },
  ]);
  expect(tableColumns(view, { ...result, db: { connectionId: "c1", database: "reporting" } }, { schema: "public", table: "orders" })).toEqual([null, null]);
});

it("reads keys only for relations that have a foreign key column in the result", () => {
  expect(keyedRelations(tableColumns(view, { db: catalog.db, columns: [column("id", 1), column("status", 3)] }, null))).toEqual([]);
  expect(keyedRelations(tableColumns(view, { db: catalog.db, columns: [column("id", 1), column("customer_id", 2)] }, null))).toEqual([orders.oid]);
});
