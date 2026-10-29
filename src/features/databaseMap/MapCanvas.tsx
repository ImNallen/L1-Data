import { FocusIcon, MaximizeIcon, MinusIcon, PlusIcon } from "lucide-react";
import { useEffect, useRef, type KeyboardEvent, type PointerEvent } from "react";
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
  type Exploring,
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

function Edge({
  source,
  target,
  offset,
  self,
  label,
  selected,
  query,
  testId,
  onSelect,
}: {
  source: Point;
  target: Point;
  offset: number;
  self: boolean;
  label: string;
  selected: boolean;
  query?: boolean;
  testId: string;
  onSelect: () => void;
}) {
  const { path, label: at } = route(source, target, offset, self);
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
        {label}
      </text>
    </g>
  );
}

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

function TableCard({ node, map, beginDrag }: { node: MapNode; map: Exploring; beginDrag: (event: PointerEvent<HTMLButtonElement>, oid: number) => void }) {
  const { relation, position } = node;
  const selected = map.selection.kind === "node" && map.selection.oid === relation.oid;
  const occurrences = map.analysis.kind === "ready" ? map.analysis.query.occurrences.filter((o) => occurrenceOid(o) === relation.oid) : [];
  const tentative = occurrences.some((o) => o.resolution.kind === "Tentative");
  const matched = occurrences.length > 0;
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
        className="flex cursor-grab flex-col gap-1 border-b border-border px-3 py-2.5 text-left outline-none focus-visible:bg-selected active:cursor-grabbing"
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
          <span>{relation.oid === map.root ? "FOCUS" : "TABLE"}</span>
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
          {tentative ? "Name match" : matched ? `${occurrences.length} SQL reference${occurrences.length === 1 ? "" : "s"}` : "Declared metadata"}
        </span>
        <button
          type="button"
          data-testid={`map-expand-${relation.oid}`}
          className="shrink-0 rounded px-1.5 py-1 text-ui-10.5 text-fg hover:bg-hover disabled:text-dim"
          disabled={node.page.kind === "complete" || node.page.kind === "loading" || map.limit}
          onClick={() => expandMapNode(relation.oid)}
        >
          {nodePageLabel(node)}
        </button>
      </div>
    </article>
  );
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
    if (box) setMapViewport(zoomViewport(map.viewport, factor, { x: box.width / 2, y: box.height / 2 }));
  };
  const beginDrag = (event: PointerEvent<HTMLButtonElement>, oid: number) => {
    if (event.button !== 0) return;
    const node = map.graph.nodes.get(oid);
    if (!node) return;
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = { kind: "node", oid, start: { x: event.clientX, y: event.clientY }, position: node.position, scale: map.viewport.scale };
  };
  const query = map.analysis.kind === "ready" ? map.analysis.query : null;
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
          drag.current = { kind: "pan", start: { x: event.clientX, y: event.clientY }, viewport: map.viewport };
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
          setMapViewport(zoomViewport(map.viewport, event.deltaY > 0 ? 0.92 : 1.08, { x: event.clientX - box.left, y: event.clientY - box.top }));
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
            setMapViewport({ ...map.viewport, x: map.viewport.x + d.x, y: map.viewport.y + d.y });
          }
        }}
      >
        <div className="absolute origin-top-left" style={{ transform: `translate(${map.viewport.x}px, ${map.viewport.y}px) scale(${map.viewport.scale})` }}>
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
                  testId={`map-fk-${edge.id}`}
                  selected={map.selection.kind === "foreignKey" && map.selection.id === edge.id}
                  onSelect={() => selectMap({ kind: "foreignKey", id: edge.id })}
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
                  selected={map.selection.kind === "join" && map.selection.index === index}
                  onSelect={() => selectMap({ kind: "join", index })}
                />
              ) : null;
            })}
          </svg>
          {[...map.graph.nodes.values()].map((node) => (
            <TableCard key={node.relation.oid} node={node} map={map} beginDrag={beginDrag} />
          ))}
        </div>
      </div>
      <div className="absolute right-3 bottom-3 flex items-center gap-1 rounded-lg border border-control bg-raised p-1 shadow-[var(--shadow-float)]">
        <IconButton data-testid="map-zoom-out" title="Zoom out" aria-label="Zoom out" onClick={() => zoom(1 / 1.2)}>
          <MinusIcon size={14} />
        </IconButton>
        <span className="w-10 text-center text-ui-11 text-muted tabular-nums">
          {map.viewport.scale < 0.01 ? (map.viewport.scale * 100).toFixed(1) : Math.round(map.viewport.scale * 100)}%
        </span>
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
