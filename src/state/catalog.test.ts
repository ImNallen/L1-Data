import { describe, expect, it } from "vitest";
import { shopCatalog } from "../testing/shopCatalog";
import { buildCatalogView, isPrimaryKey, quoteName, useCatalog } from "./catalog";
import { dbKey } from "./dbKey";

const view = buildCatalogView(shopCatalog("conn", 7));

describe("buildCatalogView", () => {
  it("offers each catalog function with its arguments, bare in the default schema and qualified elsewhere", () => {
    expect(view.functions.map((f) => `${f.label}${f.detail}=${String(f.apply)}`)).toContain("order_count(cust bigint)=order_count()");
    const catalog = shopCatalog("conn", 7);
    catalog.schemas[0]!.functions = [{ oid: 1, name: "mood_of", insertText: "mood_of()", args: "x int", returns: "lab.mood", kind: "Function" }];
    catalog.schemas[1]!.functions.push({ oid: 2, name: "refresh_all", insertText: "CALL refresh_all()", args: "", returns: "", kind: "Procedure" });
    const labels = buildCatalogView(catalog).functions.map((f) => [f.label, f.apply]);
    expect(labels).toContainEqual(["lab.mood_of", "lab.mood_of()"]);
    expect(labels.map(([label]) => label)).not.toContain("refresh_all");
  });

  it("quotes a name for insertion only when it is not a plain identifier", () => {
    expect([quoteName("Postgres", "orders"), quoteName("Postgres", "Line Items"), quoteName("Postgres", 'a"b')]).toEqual(["orders", '"Line Items"', '"a""b"']);
    expect([quoteName("SqlServer", "Orders"), quoteName("SqlServer", "line items"), quoteName("SqlServer", "a]b")]).toEqual([
      "Orders",
      "[line items]",
      "[a]]b]",
    ]);
  });

  it("indexes relations by oid", () => {
    const orders = view.catalog.schemas[1]!.relations.find((r) => r.name === "orders")!;
    expect(view.relationsById.get(orders.oid)).toEqual({ schema: "public", rel: orders });
  });

  it("marks a result column as a primary key only when its source column is one", () => {
    const orders = view.catalog.schemas[1]!.relations.find((r) => r.name === "orders")!;
    const attnum = (name: string) => orders.columns.find((c) => c.name === name)!.attnum;
    expect(isPrimaryKey(view, { tableOid: orders.oid, attnum: attnum("id") })).toBe(true);
    expect(isPrimaryKey(view, { tableOid: orders.oid, attnum: attnum("customer_id") })).toBe(false);
    expect(isPrimaryKey(view, null)).toBe(false);
    expect(isPrimaryKey(undefined, { tableOid: orders.oid, attnum: attnum("id") })).toBe(false);
    expect(isPrimaryKey(view, { tableOid: -1, attnum: 1 })).toBe(false);
  });
});

describe("useCatalog.prune", () => {
  it("keeps only the views in the wanted set", () => {
    const shop = buildCatalogView(shopCatalog("c1", 1));
    const other = buildCatalogView(shopCatalog("c2", 4));
    useCatalog.setState({ views: { [dbKey(shop.catalog.db)]: shop, [dbKey(other.catalog.db)]: other } });
    useCatalog.getState().prune(new Set([dbKey({ connectionId: "c2", database: "shop" })]));
    expect(Object.keys(useCatalog.getState().views)).toEqual(["c2\u0000shop"]);
    expect(useCatalog.getState().views[dbKey({ connectionId: "c2", database: "shop" })]?.catalog.version).toBe(4);
    const unchanged = useCatalog.getState().views;
    useCatalog.getState().prune(new Set([dbKey({ connectionId: "c2", database: "shop" })]));
    expect(useCatalog.getState().views).toBe(unchanged);
  });
});

it("uses the engine's qualified completion text verbatim for bracketed names", () => {
  const catalog = shopCatalog("ms", 1);
  const relation = catalog.schemas[1]!.relations[0]!;
  relation.name = "line items";
  relation.qualifiedName = "[Sales].[line items]";
  catalog.databaseKind = "SqlServer";
  const completion = buildCatalogView(catalog).qualifiedTables.find((c) => c.label === "public.line items");
  expect(completion?.apply).toBe("[Sales].[line items]");
});
