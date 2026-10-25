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
  const someColumns = tableColumns(view, { db: catalog.db, columns: [column("customer_id", 2), column("n")] });
  expect(keyLinks(someColumns, () => detail)).toEqual([
    { name: "orders_customer_id_fkey", columns: ["customer_id"], schema: "public", table: "customers", resultColumns: [0], refColumns: ["id"] },
  ]);

  const allColumns = tableColumns(view, { db: catalog.db, columns: [column("status", 3), column("id", 1), column("customer_id", 2)] });
  expect(keyLinks(allColumns, () => detail).map((l) => [l.table, l.resultColumns])).toEqual([
    ["customers", [2]],
    ["addresses", [2, 0]],
  ]);
});

it("gives each copy of a single-column key its own link", () => {
  const twoCopies = tableColumns(view, { db: catalog.db, columns: [column("customer_id", 2), column("id", 1), column("customer_id", 2)] });
  expect(keyLinks(twoCopies, () => detail).map((l) => [l.name, l.resultColumns])).toEqual([
    ["orders_customer_id_fkey", [0]],
    ["orders_customer_id_fkey", [2]],
  ]);
});

it("refuses a multi-column key when the result holds a column of its table twice, as a self-join does", () => {
  const selfJoin = tableColumns(view, { db: catalog.db, columns: [column("id", 1), column("customer_id", 2), column("id", 1), column("status", 3)] });
  expect(keyLinks(selfJoin, () => detail).map((l) => [l.name, l.resultColumns])).toEqual([["orders_customer_id_fkey", [1]]]);
});

it("maps only columns that name a table column, and only in the catalog of the database the rows came from", () => {
  const unsourced = { db: catalog.db, columns: [column("id"), column("customer_id")] };
  expect(tableColumns(view, unsourced)).toEqual([null, null]);
  const sourced = { db: catalog.db, columns: [column("id", 1), column("customer_id", 2)] };
  expect(tableColumns(view, sourced)).toEqual([
    { oid: orders.oid, name: "id", fk: false },
    { oid: orders.oid, name: "customer_id", fk: true },
  ]);
  expect(tableColumns(view, { ...sourced, db: { connectionId: "c1", database: "reporting" } })).toEqual([null, null]);
});

it("reads keys only for relations that have a foreign key column in the result", () => {
  expect(keyedRelations(tableColumns(view, { db: catalog.db, columns: [column("id", 1), column("status", 3)] }))).toEqual([]);
  expect(keyedRelations(tableColumns(view, { db: catalog.db, columns: [column("id", 1), column("customer_id", 2)] }))).toEqual([orders.oid]);
});
