import { useEffect, useMemo } from "react";
import type { RelationDetail, ResultMeta } from "../../ipc/bindings";
import { findRelation, sourceColumn, useCatalog, viewFor, type CatalogView } from "../../state/catalog";
import { sameDb } from "../../state/dbKey";
import { detailKey, useRelationDetail } from "../../state/relationDetail";

export type BrowsedTable = { schema: string; table: string };

type TableColumn = { oid: number; name: string; fk: boolean };

export type KeyLink = { name: string; schema: string; table: string; resultColumns: number[]; refColumns: string[] };

export function tableColumns(view: CatalogView | undefined, meta: Pick<ResultMeta, "db" | "columns">, table: BrowsedTable | null): (TableColumn | null)[] {
  if (!view || !sameDb(view.catalog.db, meta.db)) return meta.columns.map(() => null);
  const browsed = table ? findRelation(view, table.schema, table.table) : undefined;
  return meta.columns.map((c) => {
    const hit = c.source ? sourceColumn(view, c.source) : browsed?.columns.find((column) => column.name === c.name);
    const oid = c.source ? c.source.tableOid : browsed?.oid;
    return hit && oid !== undefined ? { oid, name: hit.name, fk: hit.fk } : null;
  });
}

export function keyedRelations(columns: (TableColumn | null)[]): number[] {
  return [...new Set(columns.flatMap((c) => (c?.fk ? [c.oid] : [])))];
}

export function keyLinks(columns: (TableColumn | null)[], detailOf: (oid: number) => RelationDetail | null | undefined): KeyLink[] {
  return keyedRelations(columns).flatMap((oid) =>
    (detailOf(oid)?.keys ?? []).flatMap((key) => {
      if (key.kind.kind !== "Foreign") return [];
      const at = key.columns.map((name) => columns.findIndex((c) => c?.oid === oid && c.name === name));
      return at.every((i) => i >= 0)
        ? [{ name: key.name, schema: key.kind.refSchema, table: key.kind.refTable, resultColumns: at, refColumns: key.kind.refColumns }]
        : [];
    }),
  );
}

export function useKeyLinks(meta: ResultMeta, table: BrowsedTable | null): KeyLink[] {
  const view = useCatalog((s) => viewFor(s.views, meta.db));
  const schema = table?.schema;
  const name = table?.table;
  const columns = useMemo(
    () => tableColumns(view, meta, schema !== undefined && name !== undefined ? { schema, table: name } : null),
    [view, meta, schema, name],
  );
  const relations = useMemo(() => keyedRelations(columns), [columns]);
  const version = view?.catalog.version;
  useEffect(() => {
    if (version !== undefined) for (const oid of relations) void useRelationDetail.getState().ensure(meta.db, version, oid);
  }, [meta.db, version, relations]);
  const entries = useRelationDetail((s) => s.entries);
  return useMemo(() => keyLinks(columns, (oid) => entries[detailKey(meta.db, oid)]?.detail), [columns, entries, meta.db]);
}
