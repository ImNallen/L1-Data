import type { MapForeignKey, MapQuery } from "../../ipc/bindings";
import { Button, cx } from "../../ui/primitives";
import { displayedQueryJoins, occurrenceOid, openDatabaseMap, selectMap, type Exploring } from "./controller";
import { EDGE_LIMIT, NODE_LIMIT } from "./graph";

function ForeignKeyDetail({ edge, map }: { edge: MapForeignKey; map: Exploring }) {
  const name = (oid: number) => {
    const r = map.graph.nodes.get(oid)?.relation;
    return r ? `${r.schema}.${r.name}` : String(oid);
  };
  return (
    <div data-testid="map-fk-detail" className="space-y-1">
      <div className="font-medium text-fg">
        {edge.name} <span className="font-normal text-dim">· Declared foreign key</span>
      </div>
      <div className="break-words font-mono text-ui-11">
        {name(edge.source)} → {name(edge.target)}
      </div>
      <ol className="list-inside list-decimal font-mono text-ui-11">
        {edge.columns.map((p, index) => (
          <li key={index}>
            {p.source} → {p.target}
          </li>
        ))}
      </ol>
      {(!edge.enabled || !edge.validated) && (
        <p className="text-amber">
          {!edge.enabled ? "Disabled. " : ""}
          {!edge.validated ? "Existing rows are not validated. " : ""}This declaration does not prove integrity.
        </p>
      )}
    </div>
  );
}

function QueryDetail({ query, map }: { query: MapQuery; map: Exploring }) {
  const selectedJoin = map.selection.kind === "join" ? query.joins[map.selection.index] : undefined;
  const joins = displayedQueryJoins(map);
  return (
    <div data-testid="map-query-analysis" className="space-y-2">
      <p className="text-dim">
        Current statement · {query.incomplete ? "Partial analysis" : "Supported analysis"} · Session settings may affect table-name matches.
      </p>
      {!query.statement && <p>No statement at the cursor.</p>}
      {!!query.statement && !query.occurrences.length && <p>No supported table sources in this statement.</p>}
      <div className="flex flex-wrap gap-1.5">
        {query.occurrences.map((o) => {
          const oid = occurrenceOid(o);
          const visible = oid !== null && map.graph.nodes.has(oid);
          return (
            <button
              type="button"
              key={o.id}
              data-testid={`map-occurrence-${o.id}`}
              onClick={() => selectMap({ kind: "occurrence", id: o.id })}
              className={cx(
                "rounded border px-2 py-1 text-left text-ui-11 hover:bg-hover",
                o.resolution.kind === "Tentative" ? "border-dashed border-amber text-amber" : "border-control text-muted",
              )}
              title={o.resolution.kind === "Unresolved" ? o.resolution.reason : "Select to highlight this source occurrence"}
            >
              <span className="font-mono">{o.alias ? `${o.name} AS ${o.alias}` : o.name}</span> ·{" "}
              {o.resolution.kind === "Tentative" ? "Name match" : o.resolution.kind === "CatalogMatch" ? "Catalog match" : "Unresolved"}
              {oid !== null && !visible ? " · outside map" : ""}
            </button>
          );
        })}
      </div>
      {joins.length < query.joins.length && (
        <p data-testid="map-query-limit" className="text-amber">
          {query.joins.length - joins.length} SQL equalities are hidden to keep the map within {EDGE_LIMIT} relationships. Simplify the query or reset
          exploration to show more.
        </p>
      )}
      {joins.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {joins.map((join, index) => (
            <button
              key={index}
              type="button"
              data-testid={`map-query-predicate-${index}`}
              className="rounded border border-dashed border-keyword px-2 py-1 font-mono text-ui-11 text-keyword hover:bg-hover"
              onClick={() => selectMap({ kind: "join", index })}
            >
              {query.occurrences.find((o) => o.id === join.source)?.alias ?? `#${join.source}`}.{join.columns.source} ={" "}
              {query.occurrences.find((o) => o.id === join.target)?.alias ?? `#${join.target}`}.{join.columns.target}
            </button>
          ))}
        </div>
      )}
      {selectedJoin && <p className="text-keyword">Join condition highlighted in SQL.</p>}
      <p className="text-dim">Dashed links show join conditions. The map follows the statement at the cursor.</p>
    </div>
  );
}

export function MapDetails({ map }: { map: Exploring }) {
  const selection = map.selection;
  const edge = selection.kind === "foreignKey" ? map.graph.edges.get(selection.id) : undefined;
  const node = selection.kind === "node" ? map.graph.nodes.get(selection.oid) : undefined;
  const incident = node ? [...map.graph.edges.values()].filter((k) => k.source === node.relation.oid || k.target === node.relation.oid) : [];
  const errors = [...map.graph.nodes.values()].filter((n) => n.page.kind === "error");
  return (
    <div
      data-testid="map-details"
      className="max-h-[36%] shrink-0 space-y-2 overflow-y-auto border-t border-border bg-sidebar px-4 py-3 text-ui-11.5 text-muted"
      aria-live="polite"
    >
      {map.limit && (
        <p data-testid="map-limit" className="text-amber">
          Map limit reached ({NODE_LIMIT} tables / {EDGE_LIMIT} relationships). This page was not added. Focus a selected table or reset exploration to
          continue.
        </p>
      )}
      {edge && <ForeignKeyDetail edge={edge} map={map} />}
      {node && (
        <div className="flex items-center justify-between gap-3">
          <span className="font-mono text-fg">
            {node.relation.schema}.{node.relation.name}
          </span>
          <Button className="h-6 px-2 text-ui-11" data-testid="map-focus-selected" onClick={() => openDatabaseMap(map.db, node.relation.oid)}>
            Focus this table
          </Button>
        </div>
      )}
      {node && incident.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {incident.map((key) => (
            <button
              type="button"
              key={key.id}
              data-testid={`map-key-detail-${key.id}`}
              className="rounded border border-control px-2 py-1 text-left text-ui-11 hover:bg-hover"
              onClick={() => selectMap({ kind: "foreignKey", id: key.id })}
            >
              {key.source === key.target ? "↻" : key.source === node.relation.oid ? "→" : "←"} <span className="font-mono">{key.name}</span> ·{" "}
              {key.columns.length} column{key.columns.length === 1 ? "" : "s"}
            </button>
          ))}
        </div>
      )}
      {errors.map((n) => (
        <p key={n.relation.oid} className="text-danger">
          {n.relation.name} · {n.page.kind === "error" ? n.page.message : ""}
        </p>
      ))}
      {map.analysis.kind === "ready" ? (
        <QueryDetail query={map.analysis.query} map={map} />
      ) : map.analysis.kind === "loading" ? (
        <p>Analyzing cursor statement…</p>
      ) : map.analysis.kind === "waiting" || map.analysis.kind === "error" ? (
        <p>{map.analysis.message}</p>
      ) : !edge && !node ? (
        <p>Select a table or relationship for details. Expand a table to follow its declared keys.</p>
      ) : null}
    </div>
  );
}
