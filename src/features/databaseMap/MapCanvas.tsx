import { FocusIcon, MaximizeIcon, MinusIcon, PlusIcon } from "lucide-react";
import { memo, useCallback, useEffect, useRef, type KeyboardEvent, type PointerEvent, type ReactNode } from "react";
import { IconButton, Spinner, cx } from "../../ui/primitives";
import {
  displayedQueryJoins,
  expandMapNode,
  moveMapNode,
  occurrenceOid,
  openDatabaseMap,
  queryJoinEndpoints,
  selectMap,
  setMapViewport,
  useDatabaseMap,
  type Exploring,
  type Selection,
} from "./controller";
import { CARD_HEIGHT, CARD_WIDTH, fitViewport, initialViewport, zoomViewport, type MapNode, type Point, type Viewport } from "./graph";

function activate(event: KeyboardEvent, action: () => void): void {
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    action();
  }
}

function route(source: Point, target: Point, offset: number, self: boolean): { path: string; label: Point } {
  if (self) {
    const x = source.x + CARD_WIDTH;
    const y = source.y + 58;
    const reach = 54 + offset;
    return { path: `M ${x} ${y} C ${x + reach} ${y - 100}, ${x + reach} ${y + 100}, ${x} ${y + 42}`, label: { x: x + reach - 10, y: y - 20 } };
  }
  const right = target.x > source.x;
  const x1 = source.x + (right ? CARD_WIDTH : 0);
  const x2 = target.x + (right ? 0 : CARD_WIDTH);
  const port = Math.min(offset, 72);
  const y1 = source.y + 58 + port;
  const y2 = target.y + 58 + port;
  const bend = (Math.max(48, Math.abs(x2 - x1) / 2) + Math.max(0, offset - 72)) * (right ? 1 : -1);
  return { path: `M ${x1} ${y1} C ${x1 + bend} ${y1}, ${x2 - bend} ${y2}, ${x2} ${y2}`, label: { x: (x1 + x2) / 2, y: (y1 + y2) / 2 - 7 } };
}

// Edges and cards are memoised on plain props: positions keep their identity until that
// node moves, so dragging one card re-renders it and its own edges, not the whole map.
const Edge = memo(function Edge({
  source,
  target,
  offset,
  self,
  label,
  caption,
  selected,
  query,
  testId,
  id,
}: {
  source: Point;
  target: Point;
  offset: number;
  self: boolean;
  label: string;
  caption?: string;
  selected: boolean;
  query?: boolean;
  testId: string;
  id: number;
}) {
  const { path, label: at } = route(source, target, offset, self);
  const onSelect = () => selectMap(query ? { kind: "join", index: id } : { kind: "foreignKey", id });
  return (
    <g
      role="button"
      tabIndex={0}
      aria-label={label}
      aria-pressed={selected}
      data-testid={testId}
      data-map-control="true"
      className="map-edge cursor-pointer outline-none"
      onPointerDown={(e) => e.stopPropagation()}
      onClick={onSelect}
      onKeyDown={(e) => activate(e, onSelect)}
    >
      <title>{label}</title>
      <path d={path} fill="none" stroke="transparent" strokeWidth={16} />
      <path
        className="map-edge-line"
        d={path}
        fill="none"
        stroke={selected ? "var(--color-green)" : query ? "var(--color-keyword)" : "var(--color-dim)"}
        strokeWidth={selected ? 2.5 : 1.5}
        strokeDasharray={query ? "6 5" : undefined}
        markerEnd={query ? "url(#map-query-arrow)" : "url(#map-fk-arrow)"}
      />
      <text
        x={at.x}
        y={at.y}
        textAnchor="middle"
        className="fill-muted font-mono text-[10px]"
        paintOrder="stroke"
        stroke="var(--color-bg)"
        strokeWidth={4}
        strokeLinejoin="round"
      >
        {caption ?? label}
      </text>
    </g>
  );
});

function nodePageLabel(node: MapNode): string {
  switch (node.page.kind) {
    case "unloaded":
      return "Expand";
    case "loading":
      return "Loading…";
    case "complete":
      return "Explored";
    case "more":
      return "More relationships";
    case "error":
      return "Retry";
  }
}

const TableCard = memo(function TableCard({
  node,
  selected,
  root,
  references,
  tentative,
  limit,
  beginDrag,
}: {
  node: MapNode;
  selected: boolean;
  root: boolean;
  references: number;
  tentative: boolean;
  limit: boolean;
  beginDrag: (event: PointerEvent<HTMLButtonElement>, oid: number) => void;
}) {
  const { relation, position } = node;
  const matched = references > 0;
  return (
    <article
      data-testid={`map-node-${relation.oid}`}
      data-map-control="true"
      data-selected={selected}
      data-query-match={tentative ? "tentative" : matched ? "catalog" : "none"}
      className={cx(
        "absolute flex flex-col overflow-hidden rounded-xl border bg-raised shadow-[var(--shadow-float)]",
        selected ? "border-green" : matched ? "border-keyword" : "border-control",
        tentative && "border-dashed",
      )}
      style={{ left: position.x, top: position.y, width: CARD_WIDTH, height: CARD_HEIGHT }}
    >
      <button
        type="button"
        aria-label={`Select ${relation.schema}.${relation.name}; arrow keys move table`}
        aria-pressed={selected}
        data-testid={`map-select-${relation.oid}`}
        className="flex cursor-grab flex-col gap-1 border-b border-border px-3 py-2.5 text-left focus-visible:bg-selected focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-focus active:cursor-grabbing"
        onPointerDown={(e) => beginDrag(e, relation.oid)}
        onClick={() => selectMap({ kind: "node", oid: relation.oid })}
        onKeyDown={(e) => {
          const delta: Record<string, Point> = {
            ArrowLeft: { x: -24, y: 0 },
            ArrowRight: { x: 24, y: 0 },
            ArrowUp: { x: 0, y: -24 },
            ArrowDown: { x: 0, y: 24 },
          };
          const d = delta[e.key];
          if (d) {
            e.preventDefault();
            moveMapNode(relation.oid, { x: position.x + d.x, y: position.y + d.y });
          }
        }}
      >
        <span className="flex w-full items-center justify-between text-ui-10.5 text-dim">
          <span className="truncate">{relation.schema}</span>
          <span>{root ? "FOCUS" : "TABLE"}</span>
        </span>
        <span className="w-full truncate font-mono text-ui-13 font-medium text-fg" title={relation.name}>
          {relation.name}
        </span>
      </button>
      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-1.5 font-mono text-ui-11 text-muted">
        {relation.columns.length ? (
          relation.columns.map((column) => (
            <div key={column} className="truncate" title={column}>
              {column}
            </div>
          ))
        ) : (
          <span className="font-sans text-dim">No participating FK columns</span>
        )}
      </div>
      <div className="flex items-center justify-between gap-1 border-t border-border px-2 py-1">
        <span className={cx("truncate px-1 text-ui-10.5", tentative ? "text-amber" : "text-dim")}>
          {tentative ? "Name match" : matched ? `${references} SQL reference${references === 1 ? "" : "s"}` : "Declared metadata"}
        </span>
        <button
          type="button"
          data-testid={`map-expand-${relation.oid}`}
          className="shrink-0 rounded px-1.5 py-1 text-ui-10.5 text-fg hover:bg-hover disabled:text-dim"
          disabled={node.page.kind === "complete" || node.page.kind === "loading" || limit}
          onClick={() => expandMapNode(relation.oid)}
        >
          {nodePageLabel(node)}
        </button>
      </div>
    </article>
  );
});

function MapTransform({ children }: { children: ReactNode }) {
  const { x, y, scale } = useDatabaseMap((s) => s.viewport);
  return (
    <div className="absolute origin-top-left" style={{ transform: `translate(${x}px, ${y}px) scale(${scale})` }}>
      {children}
    </div>
  );
}

function ZoomReadout() {
  const scale = useDatabaseMap((s) => s.viewport.scale);
  return <span className="w-10 text-center text-ui-11 text-muted tabular-nums">{scale < 0.01 ? (scale * 100).toFixed(1) : Math.round(scale * 100)}%</span>;
}

function viewportNow(): Viewport {
  return useDatabaseMap.getState().viewport;
}

function selectedOf(selection: Selection, kind: "foreignKey" | "join", value: number): boolean {
  return selection.kind === "foreignKey"
    ? kind === "foreignKey" && selection.id === value
    : selection.kind === "join" && kind === "join" && selection.index === value;
}

type Drag = { kind: "pan"; start: Point; viewport: Viewport } | { kind: "node"; start: Point; position: Point; oid: number; scale: number };

export function MapCanvas({ map }: { map: Exploring }) {
  const canvas = useRef<HTMLDivElement>(null);
  const drag = useRef<Drag | null>(null);
  const fitted = useRef<number | null>(null);
  const fit = () => {
    const box = canvas.current?.getBoundingClientRect();
    if (box) setMapViewport(fitViewport(map.graph, box.width, box.height));
  };
  useEffect(() => {
    if (fitted.current !== map.generation && map.graph.nodes.size) {
      fitted.current = map.generation;
      const box = canvas.current?.getBoundingClientRect();
      if (box) setMapViewport(initialViewport(map.graph, map.root, box.width, box.height));
    }
  });
  const zoom = (factor: number) => {
    const box = canvas.current?.getBoundingClientRect();
    if (box) setMapViewport(zoomViewport(viewportNow(), factor, { x: box.width / 2, y: box.height / 2 }));
  };
  const beginDrag = useCallback((event: PointerEvent<HTMLButtonElement>, oid: number) => {
    if (event.button !== 0) return;
    const held = useDatabaseMap.getState().map;
    const node = held.kind === "exploring" ? held.graph.nodes.get(oid) : undefined;
    if (!node) return;
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = { kind: "node", oid, start: { x: event.clientX, y: event.clientY }, position: node.position, scale: viewportNow().scale };
  }, []);
  const query = map.analysis.kind === "ready" ? map.analysis.query : null;
  const references = new Map<number, { count: number; tentative: boolean }>();
  for (const o of query?.occurrences ?? []) {
    const oid = occurrenceOid(o);
    if (oid === null) continue;
    const seen = references.get(oid) ?? { count: 0, tentative: false };
    references.set(oid, { count: seen.count + 1, tentative: seen.tentative || o.resolution.kind === "Tentative" });
  }
  const parallel = new Map<string, number>();
  const offset = (a: number, b: number) => {
    const key = `${Math.min(a, b)}/${Math.max(a, b)}`;
    const index = parallel.get(key) ?? 0;
    parallel.set(key, index + 1);
    return index * 18;
  };
  return (
    <div className="relative min-h-0 flex-1">
      <div
        ref={canvas}
        data-testid="map-canvas"
        tabIndex={0}
        role="region"
        aria-label="Database map. Drag background to pan; use zoom and fit controls."
        className="map-canvas h-full touch-none overflow-hidden outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-focus"
        onPointerDown={(event) => {
          if (event.button !== 0 || (event.target instanceof Element && event.target.closest("[data-map-control]"))) return;
          event.currentTarget.setPointerCapture(event.pointerId);
          drag.current = { kind: "pan", start: { x: event.clientX, y: event.clientY }, viewport: viewportNow() };
        }}
        onPointerMove={(event) => {
          const held = drag.current;
          if (!held) return;
          const dx = event.clientX - held.start.x;
          const dy = event.clientY - held.start.y;
          if (held.kind === "pan") setMapViewport({ ...held.viewport, x: held.viewport.x + dx, y: held.viewport.y + dy });
          else moveMapNode(held.oid, { x: held.position.x + dx / held.scale, y: held.position.y + dy / held.scale });
        }}
        onPointerUp={() => {
          drag.current = null;
        }}
        onPointerCancel={() => {
          drag.current = null;
        }}
        onWheel={(event) => {
          if (event.target instanceof Element && event.target.closest("article")) return;
          const box = event.currentTarget.getBoundingClientRect();
          setMapViewport(zoomViewport(viewportNow(), event.deltaY > 0 ? 0.92 : 1.08, { x: event.clientX - box.left, y: event.clientY - box.top }));
        }}
        onKeyDown={(event) => {
          if (event.target !== event.currentTarget) return;
          if (event.key === "+" || event.key === "=") {
            event.preventDefault();
            zoom(1.2);
          }
          if (event.key === "-") {
            event.preventDefault();
            zoom(1 / 1.2);
          }
          const delta: Record<string, Point> = {
            ArrowLeft: { x: 40, y: 0 },
            ArrowRight: { x: -40, y: 0 },
            ArrowUp: { x: 0, y: 40 },
            ArrowDown: { x: 0, y: -40 },
          };
          const d = delta[event.key];
          if (d) {
            event.preventDefault();
            const view = viewportNow();
            setMapViewport({ ...view, x: view.x + d.x, y: view.y + d.y });
          }
        }}
      >
        <MapTransform>
          <svg width="1" height="1" className="absolute overflow-visible" aria-label="Relationships">
            <defs>
              <marker id="map-fk-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
                <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--color-dim)" />
              </marker>
              <marker id="map-query-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
                <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--color-keyword)" />
              </marker>
            </defs>
            {[...map.graph.edges.values()].map((edge) => {
              const a = map.graph.nodes.get(edge.source);
              const b = map.graph.nodes.get(edge.target);
              return a && b ? (
                <Edge
                  key={edge.id}
                  source={a.position}
                  target={b.position}
                  offset={offset(edge.source, edge.target)}
                  self={edge.source === edge.target}
                  label={edge.name}
                  caption={edge.columns.length === 1 ? `${edge.columns[0]?.source} → ${edge.columns[0]?.target}` : `${edge.columns.length} columns`}
                  testId={`map-fk-${edge.id}`}
                  selected={selectedOf(map.selection, "foreignKey", edge.id)}
                  id={edge.id}
                />
              ) : null;
            })}
            {displayedQueryJoins(map).map((join, index) => {
              const endpoints = query ? queryJoinEndpoints(query, join) : null;
              const a = endpoints ? map.graph.nodes.get(endpoints[0]) : null;
              const b = endpoints ? map.graph.nodes.get(endpoints[1]) : null;
              return a && b ? (
                <Edge
                  key={`sql-${index}`}
                  source={a.position}
                  target={b.position}
                  offset={offset(a.relation.oid, b.relation.oid)}
                  self={a.relation.oid === b.relation.oid}
                  query
                  label={`${join.columns.source} = ${join.columns.target}`}
                  testId={`map-join-${index}`}
                  selected={selectedOf(map.selection, "join", index)}
                  id={index}
                />
              ) : null;
            })}
          </svg>
          {[...map.graph.nodes.values()].map((node) => {
            const refs = references.get(node.relation.oid);
            return (
              <TableCard
                key={node.relation.oid}
                node={node}
                selected={map.selection.kind === "node" && map.selection.oid === node.relation.oid}
                root={node.relation.oid === map.root}
                references={refs?.count ?? 0}
                tentative={refs?.tentative ?? false}
                limit={map.limit}
                beginDrag={beginDrag}
              />
            );
          })}
        </MapTransform>
      </div>
      <div className="absolute right-3 bottom-3 flex items-center gap-1 rounded-lg border border-control bg-raised p-1 shadow-[var(--shadow-float)]">
        <IconButton data-testid="map-zoom-out" title="Zoom out" aria-label="Zoom out" onClick={() => zoom(1 / 1.2)}>
          <MinusIcon size={14} />
        </IconButton>
        <ZoomReadout />
        <IconButton data-testid="map-zoom-in" title="Zoom in" aria-label="Zoom in" onClick={() => zoom(1.2)}>
          <PlusIcon size={14} />
        </IconButton>
        <IconButton data-testid="map-fit" title="Fit map" aria-label="Fit map" onClick={fit}>
          <MaximizeIcon size={14} />
        </IconButton>
        <IconButton data-testid="map-reset" title="Reset to focus" aria-label="Reset to focus" onClick={() => openDatabaseMap(map.db, map.root)}>
          <FocusIcon size={14} />
        </IconButton>
      </div>
      {!map.graph.nodes.size && (
        <div role="status" className="pointer-events-none absolute inset-0 grid place-items-center">
          <span className="flex items-center gap-2 text-ui-12 text-muted">
            <Spinner /> Loading declared relationships…
          </span>
        </div>
      )}
      {map.graph.nodes.size === 1 && map.graph.edges.size === 0 && (
        <p className="pointer-events-none absolute top-3 left-3 text-ui-11 text-muted">No declared foreign keys visible for this table.</p>
      )}
      {map.graph.edges.size > 0 && <p className="pointer-events-none absolute top-3 left-3 text-ui-11 text-dim">Drag to explore · Scroll to zoom</p>}
    </div>
  );
}
