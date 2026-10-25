import type { Completion } from "@codemirror/autocomplete";
import { create } from "zustand";
import { commands, type Catalog, type ColumnInfo, type ColumnSource, type DatabaseKind, type DatabaseRef, type RelKind, type Relation } from "../ipc/bindings";
import { dbKey, type DbKey } from "./dbKey";

export type CatalogView = {
  catalog: Catalog;
  qualifiedTables: Completion[];
  functions: Completion[];
  relationsById: Map<number, { schema: string; rel: Relation }>;
};

export function isViewKind(kind: RelKind): boolean {
  return kind === "View" || kind === "MaterializedView";
}

export function quoteName(kind: DatabaseKind, name: string): string {
  if (kind === "SqlServer") return /^[A-Za-z_][\w@$#]*$/.test(name) ? name : `[${name.replaceAll("]", "]]")}]`;
  return /^[a-z_][a-z0-9_$]*$/.test(name) ? name : `"${name.replaceAll('"', '""')}"`;
}

export function buildCatalogView(catalog: Catalog): CatalogView {
  const qualifiedTables: Completion[] = [];
  const functions: Completion[] = [];
  const relationsById = new Map<number, { schema: string; rel: Relation }>();

  for (const schema of catalog.schemas) {
    for (const rel of schema.relations) {
      relationsById.set(rel.oid, { schema: schema.name, rel });
      const view = isViewKind(rel.kind);
      qualifiedTables.push({
        label: `${schema.name}.${rel.name}`,
        apply: rel.qualifiedName,
        type: view ? "type" : "class",
        detail: view ? "view" : "table",
      });
    }
    const bare = schema.name === catalog.defaultSchema;
    for (const fn of schema.functions) {
      if (fn.kind === "Procedure" || fn.returns === "trigger") continue;
      const qualified = bare || catalog.databaseKind === "SqlServer" ? fn.insertText : `${quoteName(catalog.databaseKind, schema.name)}.${fn.insertText}`;
      functions.push({ label: bare ? fn.name : `${schema.name}.${fn.name}`, detail: `(${fn.args})`, type: "function", apply: qualified });
    }
  }

  return { catalog, qualifiedTables, functions, relationsById };
}

export function sourceColumn(view: CatalogView | undefined, source: ColumnSource | null): ColumnInfo | undefined {
  if (!view || !source) return undefined;
  return view.relationsById.get(source.tableOid)?.rel.columns.find((c) => c.attnum === source.attnum);
}

export function isPrimaryKey(view: CatalogView | undefined, source: ColumnSource | null): boolean {
  return sourceColumn(view, source)?.pk ?? false;
}

type CatalogStore = {
  views: Record<DbKey, CatalogView>;
  /** Fetches `catalog_get(ref)` when the held version is older. One fetch per key at a time. */
  ensure(ref: DatabaseRef, version: number): Promise<void>;
  /** Drops every view not in `keep`: a disconnect, a hidden database, or a dropped one. */
  prune(keep: ReadonlySet<DbKey>): void;
};

const inflight = new Map<DbKey, number>();

export const useCatalog = create<CatalogStore>()((set, get) => ({
  views: {},
  async ensure(ref, version) {
    const key = dbKey(ref);
    const loaded = get().views[key]?.catalog.version ?? -1;
    if (loaded >= version || (inflight.get(key) ?? -1) >= version) return;
    inflight.set(key, version);
    try {
      const res = await commands.catalogGet(ref);
      // A prune while the fetch was out means the catalog is no longer wanted.
      if (res.status === "error" || inflight.get(key) !== version) return;
      const current = get().views[key]?.catalog.version ?? -1;
      if (res.data.version > current) set((s) => ({ views: { ...s.views, [key]: buildCatalogView(res.data) } }));
    } finally {
      if (inflight.get(key) === version) inflight.delete(key);
    }
  },
  prune(keep) {
    for (const key of inflight.keys()) if (!keep.has(key)) inflight.delete(key);
    set((s) => {
      const kept = Object.entries(s.views).filter(([key]) => keep.has(key as DbKey));
      return kept.length === Object.keys(s.views).length ? {} : { views: Object.fromEntries(kept) };
    });
  },
}));

export function viewFor(views: Record<DbKey, CatalogView>, ref: DatabaseRef | null | undefined): CatalogView | undefined {
  return ref ? views[dbKey(ref)] : undefined;
}

export function findRelation(view: CatalogView | undefined, schema: string, name: string): Relation | undefined {
  return view?.catalog.schemas.find((s) => s.name === schema)?.relations.find((r) => r.name === name);
}
