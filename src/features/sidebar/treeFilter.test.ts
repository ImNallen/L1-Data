import { expect, it } from "vitest";
import type { ConnectionState } from "../../ipc/bindings";
import { buildCatalogView } from "../../state/catalog";
import { dbKey } from "../../state/dbKey";
import { connectedState, connectionConfig, databaseState } from "../../testing/connections";
import { shopCatalog } from "../../testing/shopCatalog";
import { parseQuery } from "../../ui/textMatch";
import { filterTree, firstRelation, type TreeFilter } from "./treeFilter";

const configs = { p: connectionConfig("p", "prod"), d: connectionConfig("d", "dev"), off: connectionConfig("off", "archive") };
const views = Object.fromEntries(["p", "d", "off"].map((id) => [dbKey({ connectionId: id, database: "shop" }), buildCatalogView(shopCatalog(id, 1))]));

function search(query: string, connections: Record<string, ConnectionState>) {
  return filterTree(parseQuery(query), configs, connections, views);
}

/** Each row the filtered tree draws, by its tree path; a relation shown only for its columns is marked. */
function rows(filter: TreeFilter): string[] {
  return filter.connections.flatMap(({ config, databases }) =>
    databases.flatMap(({ db, schemas, extensions }) => [
      ...schemas.flatMap(({ name, relations, functions, sequences, types, synonyms }) => {
        const at = `${config.name}/${db.database}/${name}`;
        return [
          ...relations.flatMap(({ rel, ranges, columns }) => [
            `${at}/${rel.name}${ranges ? "" : " (parent)"}`,
            ...columns.map((c) => `${at}/${rel.name}/#columns/${c.column.name}`),
          ]),
          ...functions.map((f) => `${at}/#functions/${f.fn.name}`),
          ...sequences.map((s) => `${at}/#sequences/${s.seq.name}`),
          ...types.map((t) => `${at}/#types/${t.type.name}`),
          ...synonyms.map((s) => `${at}/#synonyms/${s.syn.name}`),
        ];
      }),
      ...extensions.map((e) => `${config.name}/${db.database}/#extensions/${e.ext.name}`),
    ]),
  );
}

const prodOnly = { p: connectedState("p", [databaseState("postgres", false, ["public"]), databaseState("shop", true, ["public"])]) };

it("lists matching tables, then views, then functions, with matching columns under their table", () => {
  const filter = search("order", prodOnly);
  expect(rows(filter)).toEqual([
    "prod/shop/public/order_items",
    "prod/shop/public/order_items/#columns/order_id",
    "prod/shop/public/orders",
    "prod/shop/public/payments (parent)",
    "prod/shop/public/payments/#columns/order_id",
    "prod/shop/public/shipments (parent)",
    "prod/shop/public/shipments/#columns/order_id",
    "prod/shop/public/paid_orders",
    "prod/shop/public/#functions/order_count",
    "prod/shop/public/#functions/touch_order",
  ]);
  expect([filter.count, filter.searched]).toEqual([8, 1]);
});

it("carries the ranges to highlight for each matched name", () => {
  const relations = search("order id", prodOnly).connections[0]?.databases[0]?.schemas[0]?.relations;
  expect(relations?.map((r) => [r.rel.name, r.ranges, r.columns.map((c) => [c.column.name, c.ranges])])).toEqual([
    [
      "order_items",
      null,
      [
        [
          "order_id",
          [
            { start: 0, end: 5 },
            { start: 6, end: 8 },
          ],
        ],
      ],
    ],
    [
      "payments",
      null,
      [
        [
          "order_id",
          [
            { start: 0, end: 5 },
            { start: 6, end: 8 },
          ],
        ],
      ],
    ],
    [
      "shipments",
      null,
      [
        [
          "order_id",
          [
            { start: 0, end: 5 },
            { start: 6, end: 8 },
          ],
        ],
      ],
    ],
    [
      "paid_orders",
      [
        { start: 2, end: 4 },
        { start: 5, end: 10 },
      ],
      [],
    ],
  ]);
});

it("searches only connected instances, shown databases, and shown schemas, ordering connections by name", () => {
  const connections = {
    ...prodOnly,
    d: connectedState("d", [databaseState("shop", true, ["lab", "public"])]),
    off: { id: "off", rev: 1, status: { kind: "Disconnected" } } satisfies ConnectionState,
  };
  expect(rows(search("zoo", connections))).toEqual(["dev/shop/lab/type_zoo"]);
  expect(rows(search("orders", connections))).toEqual([
    "dev/shop/public/orders",
    "dev/shop/public/paid_orders",
    "prod/shop/public/orders",
    "prod/shop/public/paid_orders",
  ]);
  expect(search("orders", connections).searched).toBe(2);
});

it("drops every container without a match and searches nothing before a catalog loads", () => {
  expect(search("zzqx", prodOnly)).toEqual({ connections: [], count: 0, searched: 1 });
  expect(search("shop public", prodOnly).connections).toEqual([]);
  const unloaded = { p: connectedState("p", [databaseState("reporting", true, ["public"])]) };
  expect(search("order", unloaded)).toEqual({ connections: [], count: 0, searched: 0 });
});

it("matches type, synonym, and extension names, and ignores labels, fields, and targets", () => {
  const catalog = shopCatalog("p", 1);
  const lab = catalog.schemas.find((s) => s.name === "lab");
  if (!lab) throw new Error("lab missing");
  lab.types = [
    { oid: 1, name: "mood", quoted: "mood", kind: { kind: "Enum", labels: ["sad", "ok", "happy"] } },
    {
      oid: 2,
      name: "pet",
      quoted: "pet",
      kind: {
        kind: "Composite",
        fields: [
          { name: "id", typeName: "integer" },
          { name: "name", typeName: "text" },
        ],
      },
    },
  ];
  lab.synonyms = [{ oid: 3, name: "nick", quoted: "nick", target: "secret_target" }];
  catalog.extensions = [{ oid: 4, name: "plpgsql", quoted: "plpgsql", version: "1.0" }];
  const key = dbKey({ connectionId: "p", database: "shop" });
  const localViews = { ...views, [key]: buildCatalogView(catalog) };
  const connections = { p: connectedState("p", [databaseState("shop", true, ["lab", "public"])]) };
  const found = (query: string) => filterTree(parseQuery(query), configs, connections, localViews);
  expect(rows(found("mood"))).toEqual(["prod/shop/lab/#types/mood"]);
  expect(rows(found("plpgsql"))).toEqual(["prod/shop/#extensions/plpgsql"]);
  expect(rows(found("nick"))).toEqual(["prod/shop/lab/#synonyms/nick"]);
  expect(found("sad").count).toBe(0);
  expect(found("secret_target").count).toBe(0);
  expect(found("integer").count).toBe(0);
});

it("names the first table or view whose name starts words of the query, else the first the filter lists", () => {
  const first = (query: string) => firstRelation(search(query, prodOnly), parseQuery(query));
  expect(first("order")).toEqual({ db: { connectionId: "p", database: "shop" }, schema: "public", relation: "order_items" });
  expect(first("customer_id")).toEqual({ db: { connectionId: "p", database: "shop" }, schema: "public", relation: "addresses" });
  expect(first("pa")).toEqual({ db: { connectionId: "p", database: "shop" }, schema: "public", relation: "payments" });
  expect(first("rder")).toEqual({ db: { connectionId: "p", database: "shop" }, schema: "public", relation: "order_items" });
  expect(first("touch")).toBeNull();
});
