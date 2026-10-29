import type { MapCursor, MapForeignKey, MapNeighborhood, MapRelation } from "../../ipc/bindings";

export const NODE_LIMIT = 160;
export const EDGE_LIMIT = 512;
export const CARD_WIDTH = 224;
export const CARD_HEIGHT = 168;
export type Point = { x: number; y: number };
export type Viewport = Point & { scale: number };
export type Page =
  | { kind: "unloaded" }
  | { kind: "loading" }
  | { kind: "complete" }
  | { kind: "more"; after: MapCursor }
  | { kind: "error"; message: string; after: MapCursor | null };
export type MapNode = { relation: MapRelation; position: Point; page: Page };
export type Graph = { nodes: Map<number, MapNode>; edges: Map<number, MapForeignKey> };

export function emptyGraph(): Graph {
  return { nodes: new Map(), edges: new Map() };
}

export function mergeNeighborhood(graph: Graph, page: MapNeighborhood, focus: number): { kind: "merged"; graph: Graph } | { kind: "limit" } {
  const relations = new Map(page.relations.map((r) => [r.oid, r]));
  relations.set(page.root.oid, page.root);
  if (
    new Set([...graph.nodes.keys(), ...relations.keys()]).size > NODE_LIMIT ||
    new Set([...graph.edges.keys(), ...page.foreignKeys.map((k) => k.id)]).size > EDGE_LIMIT
  ) {
    return { kind: "limit" };
  }
  const nodes = new Map(graph.nodes);
  const edges = new Map(graph.edges);
  const origin = nodes.get(focus)?.position ?? { x: 0, y: 0 };
  const sides = new Map<number, number>();
  for (const edge of page.foreignKeys) {
    edges.set(edge.id, edge);
    if (edge.source !== focus) sides.set(edge.source, -1);
    if (edge.target !== focus && !sides.has(edge.target)) sides.set(edge.target, 1);
  }
  const occupied = [...nodes.values()].map((n) => n.position);
  for (const relation of [...relations.values()].sort((a, b) => a.oid - b.oid)) {
    const held = nodes.get(relation.oid);
    if (held) {
      nodes.set(relation.oid, { ...held, relation: { ...relation, columns: [...new Set([...held.relation.columns, ...relation.columns])] } });
      continue;
    }
    const side = relation.oid === focus ? 0 : (sides.get(relation.oid) ?? 1);
    const position = { x: origin.x + side * (CARD_WIDTH + 100), y: origin.y };
    let lane = 0;
    while (occupied.some((p) => Math.abs(p.x - position.x) < CARD_WIDTH + 30 && Math.abs(p.y - position.y) < CARD_HEIGHT + 28)) {
      lane += 1;
      position.y = origin.y + Math.ceil(lane / 2) * (lane % 2 === 1 ? -1 : 1) * (CARD_HEIGHT + 40);
    }
    occupied.push(position);
    nodes.set(relation.oid, { relation, position, page: { kind: "unloaded" } });
  }
  const root = nodes.get(page.root.oid);
  if (root) nodes.set(page.root.oid, { ...root, page: page.after ? { kind: "more", after: page.after } : { kind: "complete" } });
  return { kind: "merged", graph: { nodes, edges } };
}

export function fitViewport(graph: Graph, width: number, height: number): Viewport {
  const positions = [...graph.nodes.values()].map((n) => n.position);
  if (!positions.length) return { x: width / 2 - CARD_WIDTH / 2, y: height / 2 - CARD_HEIGHT / 2, scale: 1 };
  const left = Math.min(...positions.map((p) => p.x));
  const top = Math.min(...positions.map((p) => p.y));
  const w = Math.max(...positions.map((p) => p.x)) + CARD_WIDTH - left;
  const h = Math.max(...positions.map((p) => p.y)) + CARD_HEIGHT - top;
  const scale = Math.min(1, Math.max(0.001, Math.min((width - 72) / w, (height - 72) / h)));
  return { x: (width - w * scale) / 2 - left * scale, y: (height - h * scale) / 2 - top * scale, scale };
}

export function zoomViewport(view: Viewport, factor: number, center: Point): Viewport {
  const scale = Math.max(0.001, Math.min(2.5, view.scale * factor));
  const ratio = scale / view.scale;
  return { scale, x: center.x - (center.x - view.x) * ratio, y: center.y - (center.y - view.y) * ratio };
}
