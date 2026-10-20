import { CompletionContext, type CompletionResult } from "@codemirror/autocomplete";
import { afterEach, expect, it } from "vitest";
import { buildCatalogView } from "../../state/catalog";
import { shopCatalog } from "../../testing/shopCatalog";
import { configureSql, dropEditor, editorStateOf, seedEditor } from "./editorStates";

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
