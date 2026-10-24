import { syntaxTree } from "@codemirror/language";
import { CompletionContext, type CompletionResult } from "@codemirror/autocomplete";
import { afterEach, expect, it } from "vitest";
import { buildCatalogView } from "../../state/catalog";
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

afterEach(() => dropEditor("t1"));

async function completeAtEnd(text: string): Promise<string[]> {
  seedEditor("t1", text);
  configureSql("t1", buildCatalogView(shopCatalog("c1", 1)));
  const state = editorStateOf("t1");
  const pos = state.doc.length;
  const context = new CompletionContext(state, pos, true);
  const labels: string[] = [];
  for (const source of state.languageDataAt<(c: CompletionContext) => Promise<CompletionResult | null> | CompletionResult | null>("autocomplete", pos)) {
    const result = await source(context);
    if (result) labels.push(...result.options.map((o) => o.label));
  }
  return labels;
}

it("completes the columns of an aliased table in the default schema", async () => {
  expect(await completeAtEnd("SELECT * FROM orders o WHERE o.")).toEqual(["id", "customer_id", "status", "total", "created_at"]);
});

it("completes the columns of an aliased table in another schema", async () => {
  expect(await completeAtEnd("SELECT * FROM lab.type_zoo z WHERE z.")).toEqual(["id", "tstz", "m"]);
});

it("changes to SQL Server syntax before a catalog arrives while retaining the document", () => {
  seedEditor("t1", "SELECT @name FROM [line items]");
  configureSql("t1", undefined, "SqlServer");
  const state = editorStateOf("t1");
  const names: string[] = [];
  syntaxTree(state).iterate({ enter: (node) => { if (node.from === 18) names.push(node.name); } });
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
