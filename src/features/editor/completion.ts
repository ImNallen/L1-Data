import type { Completion, CompletionContext as EditorContext, CompletionResult, CompletionSource } from "@codemirror/autocomplete";
import type { EditorView } from "@codemirror/view";
import { commands, type CompletionContext, type DatabaseKind, type Relation, type ScopeRelation } from "../../ipc/bindings";
import { isViewKind, quoteName, type CatalogView } from "../../state/catalog";
import { detailKey, useRelationDetail } from "../../state/relationDetail";
import { shortType } from "../results/format";

type Complete = Extract<CompletionContext, { kind: "Complete" }>;
type Resolved = { schema: string; rel: Relation };
type Column = { name: string; detail: string };

const BOOST = { join: 99, column: 4, reference: 3, relation: 2, function: 1, keyword: -1 };

const JOIN_DETAIL_WAIT_MS = 250;

function insertCallWithCursorInside(text: string) {
  return (view: EditorView, _c: Completion, from: number, to: number) => {
    const open = text.indexOf("(");
    const anchor = open >= 0 && text.endsWith(")") ? from + open + 1 : from + text.length;
    view.dispatch({ changes: { from, to, insert: text }, selection: { anchor }, userEvent: "input.complete" });
  };
}

function schemaNamed(view: CatalogView, name: string) {
  const schemas = view.catalog.schemas;
  return schemas.find((s) => s.name === name) ?? schemas.find((s) => s.name.toLowerCase() === name.toLowerCase());
}

function resolveTable(view: CatalogView | undefined, schema: string | null, name: string): Resolved | undefined {
  if (!view) return undefined;
  const target = schema ?? view.catalog.defaultSchema;
  const s = target === null ? undefined : schemaNamed(view, target);
  const rel = s && (s.relations.find((r) => r.name === name) ?? s.relations.find((r) => r.name.toLowerCase() === name.toLowerCase()));
  return s && rel ? { schema: s.name, rel } : undefined;
}

function columnsOf(view: CatalogView | undefined, rel: ScopeRelation): Column[] {
  const from = rel.reference ?? "";
  if (rel.source.kind === "Table") {
    const found = resolveTable(view, rel.source.schema, rel.source.name);
    return found?.rel.columns.map((c) => ({ name: c.name, detail: `${shortType(c.typeDisplay)} · ${from}` })) ?? [];
  }
  const named = rel.source.columns.map((name) => ({ name, detail: from }));
  const starred = rel.source.star.flatMap(
    (t) => resolveTable(view, t.schema, t.name)?.rel.columns.map((c) => ({ name: c.name, detail: `${shortType(c.typeDisplay)} · ${from}` })) ?? [],
  );
  return [...named, ...starred];
}

function columnOptions(view: CatalogView | undefined, rel: ScopeRelation, kind: DatabaseKind): Completion[] {
  return columnsOf(view, rel).map((c) => ({ label: c.name, apply: quoteName(kind, c.name), type: "property", detail: c.detail, boost: BOOST.column }));
}

function relationOption(rel: Relation, kind: DatabaseKind, label = rel.name, apply = quoteName(kind, rel.name)): Completion {
  const view = isViewKind(rel.kind);
  return { label, apply, type: view ? "type" : "class", detail: view ? "view" : "table", boost: BOOST.relation };
}

function keywordOptions(reply: Complete): Completion[] {
  return reply.keywords.map((k) => ({ label: k, type: "keyword", boost: BOOST.keyword }));
}

/** Keys load per relation in scope, so a catalog refresh never reads every key in the database. */
async function joinConditions(view: CatalogView, reply: Complete, target: number, kind: DatabaseKind): Promise<Completion[]> {
  const resolve = (r: ScopeRelation | undefined) =>
    r?.source.kind === "Table" && r.reference ? resolveTable(view, r.source.schema, r.source.name) : undefined;
  const joined = reply.relations[target];
  const t = resolve(joined);
  const ref = joined?.reference;
  if (!t || !ref) return [];
  const others = reply.relations.flatMap((r, i) => {
    const found = i === target ? undefined : resolve(r);
    return found && r.reference ? [{ ref: r.reference, ...found }] : [];
  });
  if (!others.length) return [];
  const { db, version } = view.catalog;
  const details = useRelationDetail.getState();
  const loaded = Promise.all([t, ...others].map((r) => details.ensure(db, version, r.rel.oid)));
  await Promise.race([loaded, new Promise((resolve) => setTimeout(resolve, JOIN_DETAIL_WAIT_MS))]);
  const keysOf = (oid: number) => useRelationDetail.getState().entries[detailKey(db, oid)]?.detail?.keys ?? [];
  const col = (ref: string, name: string) => `${ref}.${quoteName(kind, name)}`;
  const conditions: string[] = [];
  for (const o of others) {
    for (const key of keysOf(t.rel.oid)) {
      if (key.kind.kind === "Foreign" && key.kind.refSchema === o.schema && key.kind.refTable === o.rel.name) {
        const refs = key.kind.refColumns;
        conditions.push(key.columns.map((c, i) => `${col(ref, c)} = ${col(o.ref, refs[i]!)}`).join(" AND "));
      }
    }
    for (const key of keysOf(o.rel.oid)) {
      if (key.kind.kind === "Foreign" && key.kind.refSchema === t.schema && key.kind.refTable === t.rel.name) {
        const refs = key.kind.refColumns;
        conditions.push(key.columns.map((c, i) => `${col(ref, refs[i]!)} = ${col(o.ref, c)}`).join(" AND "));
      }
    }
  }
  return [...new Set(conditions)].map((label) => ({ label, type: "text", detail: "foreign key", boost: BOOST.join }));
}

function qualifiedOptions(view: CatalogView | undefined, reply: Complete, kind: DatabaseKind): Completion[] {
  const q = reply.qualifier;
  if (!q) return [];
  if (q.kind === "Relation") {
    const rel = reply.relations[q.index];
    return rel ? columnOptions(view, rel, kind) : [];
  }
  if (!view) return [];
  const [first, second] = q.parts;
  if (q.parts.length === 2 && first !== undefined && second !== undefined) {
    const found = resolveTable(view, first, second);
    return found ? found.rel.columns.map((c) => ({ label: c.name, apply: quoteName(kind, c.name), type: "property", detail: shortType(c.typeDisplay) })) : [];
  }
  const schema = q.parts.length === 1 && first !== undefined ? schemaNamed(view, first) : undefined;
  if (!schema) return [];
  const relations = schema.relations.map((r) => relationOption(r, kind));
  if (reply.expect.kind === "Relation") return relations;
  const functions = schema.functions
    .filter((f) => f.kind !== "Procedure")
    .map((f): Completion => ({
      label: f.name,
      detail: `(${f.args})`,
      type: "function",
      apply: insertCallWithCursorInside(kind === "SqlServer" ? `${quoteName(kind, f.name)}()` : f.insertText),
      boost: BOOST.function,
    }));
  return [...relations, ...functions];
}

export async function completionOptions(view: CatalogView | undefined, reply: Complete, kind: DatabaseKind): Promise<Completion[]> {
  if (reply.qualifier) return qualifiedOptions(view, reply, kind);
  const expect = reply.expect;
  if (expect.kind === "Keyword" || expect.kind === "Alias") return keywordOptions(reply);
  if (expect.kind === "Relation") {
    const schema = view?.catalog.defaultSchema;
    const local = view?.catalog.schemas.find((s) => s.name === schema)?.relations.map((r) => relationOption(r, kind)) ?? [];
    const ctes = reply.ctes.map((name): Completion => ({ label: name, type: "type", detail: "CTE", boost: BOOST.relation }));
    return [...ctes, ...local, ...(view?.qualifiedTables ?? []), ...keywordOptions(reply)];
  }
  const joins = expect.kind === "JoinCondition" && view ? await joinConditions(view, reply, expect.target, kind) : [];
  const columns = reply.relations.flatMap((r) => columnOptions(view, r, kind));
  const references = reply.relations.flatMap((r): Completion[] => (r.reference ? [{ label: r.reference, type: "variable", boost: BOOST.reference }] : []));
  const catalogFunctions = (view?.functions ?? []).map((f) => ({
    ...f,
    apply: typeof f.apply === "string" ? insertCallWithCursorInside(f.apply) : f.apply,
    boost: BOOST.function,
  }));
  const builtins = reply.functions.map((name): Completion => ({ label: name, type: "function", apply: insertCallWithCursorInside(`${name}()`) }));
  return [...joins, ...columns, ...references, ...catalogFunctions, ...builtins, ...keywordOptions(reply)];
}

function opensUnasked(ctx: EditorContext, expect: Complete["expect"]): boolean {
  if (ctx.explicit) return true;
  if (expect.kind === "Alias") return false;
  return ctx.matchBefore(/[\w$#]+/) !== null || ctx.state.sliceDoc(ctx.pos - 1, ctx.pos) === "." || expect.kind === "JoinCondition";
}

export function engineCompletion(view: CatalogView | undefined, kind: DatabaseKind): CompletionSource {
  return async (ctx: EditorContext): Promise<CompletionResult | null> => {
    const before = ctx.state.sliceDoc(ctx.pos - 1, ctx.pos);
    if (!ctx.explicit && !/[\w$#.\s]/.test(before)) return null;
    const reply = await commands.completionContext(ctx.state.doc.toString(), ctx.pos, kind);
    if (ctx.aborted || reply.kind === "None" || !opensUnasked(ctx, reply.expect)) return null;
    const options = await completionOptions(view, reply, kind);
    if (ctx.aborted || !options.length) return null;
    return { from: reply.from, options, validFor: /^[\w$#]*$/ };
  };
}
