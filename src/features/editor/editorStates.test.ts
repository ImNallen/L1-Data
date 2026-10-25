// @vitest-environment jsdom
import { syntaxTree } from "@codemirror/language";
import { CompletionContext, type CompletionResult } from "@codemirror/autocomplete";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import type { Catalog, CompletionContext as EngineContext, DatabaseKind, Expect, KeyInfo, ScopeRelation } from "../../ipc/bindings";
import { buildCatalogView } from "../../state/catalog";
import { useRelationDetail } from "../../state/relationDetail";
import { shopCatalog } from "../../testing/shopCatalog";
import {
  configureSql,
  docOf,
  dropEditor,
  editorStateOf,
  flashRanges,
  insertAtCursor,
  isDirty,
  markSynced,
  markSyncedText,
  moveCursor,
  onBuffersChanged,
  replaceDoc,
  seedEditor,
} from "./editorStates";

afterEach(() => {
  dropEditor("t1");
  clearMocks();
  useRelationDetail.setState({ entries: {} });
});

type Reply = Extract<EngineContext, { kind: "Complete" }>;
const table = (reference: string, name: string, schema: string | null = null, columnAliases: string[] = []): ScopeRelation => ({
  reference,
  source: { kind: "Table", schema, name },
  columnAliases,
});
const reply = (expect: Expect, relations: ScopeRelation[] = [], more: Partial<Reply> = {}): Reply => ({
  kind: "Complete",
  from: 0,
  expect,
  qualifier: null,
  relations,
  ctes: [],
  keywords: [],
  functions: [],
  ...more,
});

function foreignKey(columns: string[], refTable: string, refColumns: string[]): KeyInfo {
  return { name: `fk_${refTable}`, columns, kind: { kind: "Foreign", refSchema: "public", refTable, refColumns, onDelete: "NoAction", onUpdate: "NoAction" } };
}

const ORDER_KEYS = [foreignKey(["customer_id"], "customers", ["id"]), foreignKey(["customer_id", "ship_address_id"], "addresses", ["customer_id", "id"])];

type Setup = { keys?: Record<number, KeyInfo[]>; explicit?: boolean; catalog?: Catalog; kind?: DatabaseKind };

async function completeAtEnd(
  text: string,
  engine: Reply | { kind: "None" },
  { keys = {}, explicit = true, catalog = shopCatalog("c1", 1), kind = catalog.databaseKind }: Setup = {},
): Promise<{ labels: string[]; applies: unknown[]; sent: unknown[] }> {
  const sent: unknown[] = [];
  mockIPC((cmd, args) => {
    if (cmd === "completion_context") {
      sent.push(args);
      return engine.kind === "Complete" ? { ...engine, from: engine.from || text.length } : engine;
    }
    if (cmd === "relation_detail") {
      const { db, oid } = args as { db: { connectionId: string; database: string }; oid: number };
      return { db, oid, catalogVersion: 1, defaults: [], keys: keys[oid] ?? [], indexes: [], triggers: [] };
    }
    return null;
  });
  seedEditor("t1", text);
  configureSql("t1", buildCatalogView(catalog), kind);
  const state = editorStateOf("t1");
  const pos = state.doc.length;
  const context = new CompletionContext(state, pos, explicit);
  const labels: string[] = [];
  const applies: unknown[] = [];
  for (const source of state.languageDataAt<(c: CompletionContext) => Promise<CompletionResult | null> | CompletionResult | null>("autocomplete", pos)) {
    const result = await source(context);
    labels.push(...(result?.options.map((o) => o.label) ?? []));
    applies.push(...(result?.options.map((o) => o.apply ?? o.label) ?? []));
  }
  return { labels, applies, sent };
}

const oidOf = (name: string) => shopCatalog("c1", 1).schemas[1]!.relations.find((r) => r.name === name)!.oid;

it("completes the columns of an aliased table in the default schema", async () => {
  const { labels, sent } = await completeAtEnd(
    "SELECT * FROM orders o WHERE o.",
    reply({ kind: "Expression" }, [table("o", "orders")], { qualifier: { kind: "Relation", index: 0 } }),
  );
  expect(labels).toEqual(["id", "customer_id", "status", "total", "created_at"]);
  expect(sent).toEqual([{ text: "SELECT * FROM orders o WHERE o.", cursor: 31, databaseKind: "Postgres" }]);
});

it("completes the columns of an aliased table in another schema", async () => {
  const { labels } = await completeAtEnd(
    "SELECT * FROM lab.type_zoo z WHERE z.",
    reply({ kind: "Expression" }, [table("z", "type_zoo", "lab")], { qualifier: { kind: "Relation", index: 0 } }),
  );
  expect(labels).toEqual(["id", "tstz", "m"]);
});

it("offers only relations and CTEs where a relation stands", async () => {
  const { labels } = await completeAtEnd("WITH recent AS (SELECT 1) SELECT * FROM ", reply({ kind: "Relation" }, [], { ctes: ["recent"], keywords: [] }));
  expect(labels.slice(0, 2)).toEqual(["recent", "addresses"]);
  expect(labels).toContain("lab.type_zoo");
  expect(labels).not.toContain("id");
  expect(labels).not.toContain("SELECT");
});

it("offers columns, catalog functions with their arguments, built-ins, and the keywords that can follow in an expression", async () => {
  const { labels } = await completeAtEnd(
    "SELECT * FROM orders WHERE ",
    reply({ kind: "Expression" }, [table("orders", "orders")], { functions: ["date_trunc"], keywords: ["AND", "ORDER BY"] }),
  );
  expect(labels).toEqual([
    "id",
    "customer_id",
    "status",
    "total",
    "created_at",
    "orders",
    "apply_coupon",
    "customer_ltv",
    "order_count",
    "slow_echo",
    "touch_order",
    "date_trunc",
    "AND",
    "ORDER BY",
  ]);
});

it("offers the foreign key join condition first, with the joined relation's columns on the left", async () => {
  const text = "select * from orders o join customers c on ";
  const { labels } = await completeAtEnd(
    text,
    reply({ kind: "JoinCondition", target: 1 }, [table("o", "orders"), table("c", "customers")]),
    { keys: { [oidOf("orders")]: ORDER_KEYS }, explicit: false },
  );
  expect(labels[0]).toBe("c.id = o.customer_id");
  expect(labels).toContain("email");
});

it("joins every column of a composite foreign key with AND", async () => {
  const { labels } = await completeAtEnd(
    "select * from orders o join addresses a on ",
    reply({ kind: "JoinCondition", target: 1 }, [table("o", "orders"), table("a", "addresses")]),
    { keys: { [oidOf("orders")]: ORDER_KEYS } },
  );
  expect(labels[0]).toBe("a.customer_id = o.customer_id AND a.id = o.ship_address_id");
});

it("reads a foreign key from the joined relation to one already in scope", async () => {
  const { labels } = await completeAtEnd(
    "select * from customers c join orders o on ",
    reply({ kind: "JoinCondition", target: 1 }, [table("c", "customers"), table("o", "orders")]),
    { keys: { [oidOf("orders")]: ORDER_KEYS } },
  );
  expect(labels[0]).toBe("o.customer_id = c.id");
});

it("leaves an alias being typed alone, and lists the keywords only on request", async () => {
  const alias = reply({ kind: "Alias" }, [table("orders", "orders")], { keywords: ["ORDER BY", "WHERE"] });
  expect((await completeAtEnd("select * from orders o", alias, { explicit: false })).labels).toEqual([]);
  expect((await completeAtEnd("select * from orders o", alias)).labels).toEqual(["ORDER BY", "WHERE"]);
});

it("opens the keywords unasked when the word after a relation starts a reserved keyword", async () => {
  const text = "select * from orders wh";
  const keyword = reply({ kind: "Keyword" }, [table("orders", "orders")], { keywords: ["WHERE", "ORDER BY"], from: text.length - 2 });
  expect((await completeAtEnd(text, keyword, { explicit: false })).labels).toEqual(["WHERE", "ORDER BY"]);
});

it("stays closed after a space unless the engine says a join condition starts there", async () => {
  expect((await completeAtEnd("select * from orders o ", reply({ kind: "Keyword" }, [], { keywords: ["WHERE"] }), { explicit: false })).labels).toEqual([]);
  expect((await completeAtEnd("select 'abc", { kind: "None" })).labels).toEqual([]);
});

it("sends the SQL Server kind and offers the expansion of a derived relation", async () => {
  const derived: ScopeRelation = { reference: "r", source: { kind: "Derived", columns: ["n"], star: [{ schema: null, name: "coupons" }] }, columnAliases: [] };
  seedEditor("t1", "");
  const sent: unknown[] = [];
  mockIPC((_cmd, args) => {
    sent.push(args);
    return reply({ kind: "Expression" }, [derived], { qualifier: { kind: "Relation", index: 0 }, from: 2 });
  });
  configureSql("t1", buildCatalogView(shopCatalog("c1", 1)), "SqlServer");
  insertAtCursor("t1", "r.");
  const state = editorStateOf("t1");
  const [source] = state.languageDataAt<(c: CompletionContext) => Promise<CompletionResult | null>>("autocomplete", 2);
  const result = await source!(new CompletionContext(state, 2, false));
  expect(result?.options.map((o) => o.label)).toEqual(["n", "code", "percent_off", "expires_at"]);
  expect(sent).toEqual([{ text: "r.", cursor: 2, databaseKind: "SqlServer" }]);
});

it("names the first columns as a column alias list renames them, also in a join condition", async () => {
  const renamed = table("o", "orders", null, ["oid", "cid"]);
  const qualified = reply({ kind: "Expression" }, [renamed], { qualifier: { kind: "Relation", index: 0 } });
  expect((await completeAtEnd("select * from orders o(oid, cid) where o.", qualified)).labels).toEqual(["oid", "cid", "status", "total", "created_at"]);
  const unnest: ScopeRelation = { reference: "u", source: { kind: "Derived", columns: [], star: [] }, columnAliases: ["x"] };
  const fromUnnest = reply({ kind: "Expression" }, [unnest], { qualifier: { kind: "Relation", index: 0 } });
  expect((await completeAtEnd("select * from unnest(array[1]) u(x) where u.", fromUnnest)).labels).toEqual(["x"]);
  const join = reply({ kind: "JoinCondition", target: 1 }, [renamed, table("c", "customers")]);
  const { labels } = await completeAtEnd("select * from orders o(oid, cid) join customers c on ", join, { keys: { [oidOf("orders")]: ORDER_KEYS } });
  expect(labels[0]).toBe("c.id = o.cid");
});

it("inserts each catalog name as the engine quotes it", async () => {
  const catalog = shopCatalog("c1", 1);
  const orders = catalog.schemas[1]!.relations.find((r) => r.name === "orders")!;
  Object.assign(orders, { name: "Orders", quoted: '"Orders"' });
  Object.assign(orders.columns[2]!, { name: "order", quoted: '"order"' });
  const qualified = reply({ kind: "Expression" }, [table("o", "Orders")], { qualifier: { kind: "Relation", index: 0 } });
  const { labels, applies } = await completeAtEnd('select * from "Orders" o where o.', qualified, { catalog });
  expect(applies[labels.indexOf("order")]).toBe('"order"');
  const relations = await completeAtEnd("select * from ", reply({ kind: "Relation" }), { catalog });
  expect(relations.applies[relations.labels.indexOf("Orders")]).toBe('"Orders"');
});

it("matches a name in another case only where the server would", async () => {
  const catalog = shopCatalog("c1", 1);
  catalog.schemas[1]!.relations.find((r) => r.name === "orders")!.name = "Orders";
  const qualified = reply({ kind: "Expression" }, [table("o", "orders")], { qualifier: { kind: "Relation", index: 0 } });
  expect((await completeAtEnd("select * from orders o where o.", qualified, { catalog })).labels).toEqual([]);
  const msCatalog = { ...catalog, databaseKind: "SqlServer" as const };
  expect((await completeAtEnd("SELECT * FROM orders o WHERE o.", qualified, { catalog: msCatalog })).labels).toContain("customer_id");
});

it("changes to SQL Server syntax before a catalog arrives while retaining the document", () => {
  seedEditor("t1", "SELECT @name FROM [line items]");
  configureSql("t1", undefined, "SqlServer");
  const state = editorStateOf("t1");
  const names: string[] = [];
  syntaxTree(state).iterate({
    enter: (node) => {
      if (node.from === 18) names.push(node.name);
    },
  });
  expect(names).toContain("QuotedIdentifier");
  expect(state.doc.toString()).toBe("SELECT @name FROM [line items]");
  configureSql("t1", undefined, "Postgres");
  expect(syntaxTree(editorStateOf("t1")).toString()).not.toContain("QuotedIdentifier");
});

it("never calls a scratch tab dirty", () => {
  seedEditor("t1", "select 1;");
  insertAtCursor("t1", "-- more");
  expect(isDirty("t1")).toBe(false);
});

it("calls a file tab dirty while its text differs from the file, and clean again once the text matches", () => {
  seedEditor("t1", "select 1;", "seeded");
  expect(isDirty("t1")).toBe(false);
  insertAtCursor("t1", "-- more");
  expect(isDirty("t1")).toBe(true);
  replaceDoc("t1", "select 1;");
  expect(isDirty("t1")).toBe(false);
});

it("calls a tab whose file contents are unknown dirty until the file is read", () => {
  seedEditor("t1", "select 1;", "unknown");
  expect(isDirty("t1")).toBe(true);
  markSyncedText("t1", "select 2;");
  expect(isDirty("t1")).toBe(true);
  markSyncedText("t1", "select 1;");
  expect(isDirty("t1")).toBe(false);
  markSynced("t1", null);
  expect(isDirty("t1")).toBe(true);
});

it("tells listeners when a background tab's text or baseline changes, and not for a cursor move or a flash", () => {
  seedEditor("t1", "select 1;", "seeded");
  let calls = 0;
  const stop = onBuffersChanged(() => calls++);
  moveCursor("t1", 3);
  flashRanges("t1", [{ from: 0, to: 6 }]);
  expect(calls).toBe(0);
  replaceDoc("t1", "select 2;");
  expect(calls).toBe(1);
  markSynced("t1", docOf("t1") ?? null);
  expect([calls, isDirty("t1")]).toEqual([2, false]);
  stop();
});

it("forgets a dropped tab's file, so the same id seeded again is a scratch tab", () => {
  seedEditor("t1", "select 1;", "unknown");
  dropEditor("t1");
  expect(docOf("t1")).toBeUndefined();
  markSynced("t1", null);
  seedEditor("t1", "select 1;");
  expect(isDirty("t1")).toBe(false);
});
