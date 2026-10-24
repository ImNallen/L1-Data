import type { SavedQuery, SavedQueryId } from "../../ipc/bindings";
import { matchRanges, parseQuery, startsWords, type TextRange } from "../../ui/textMatch";

export type Ranked = { query: SavedQuery; nameMatch: TextRange[] };

const Band = { NameWordStarts: 0, Name: 1, SqlOnly: 2 } as const;
type Band = (typeof Band)[keyof typeof Band];

export function rankSaved(items: readonly SavedQuery[], text: string): Ranked[] {
  const tokens = parseQuery(text);
  if (tokens.length === 0) return items.map((query) => ({ query, nameMatch: [] }));
  const banded: { band: Band; ranked: Ranked }[] = [];
  for (const query of items) {
    const name = query.name.toLowerCase();
    const sql = query.sql.toLowerCase();
    if (!tokens.every((t) => name.includes(t) || sql.includes(t))) continue;
    const inName = tokens.filter((t) => name.includes(t));
    const band = inName.length < tokens.length ? Band.SqlOnly : startsWords(tokens, query.name) ? Band.NameWordStarts : Band.Name;
    banded.push({ band, ranked: { query, nameMatch: matchRanges(inName, query.name) ?? [] } });
  }
  return banded.sort((a, b) => a.band - b.band).map((b) => b.ranked);
}

export type SavedAction = "primary" | "open" | "copy" | "next" | "previous";

export function savedKey(e: Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey" | "isComposing">, searchHasSelection: boolean): SavedAction | null {
  if (e.isComposing || e.shiftKey || e.altKey) return null;
  const mod = e.metaKey || e.ctrlKey;
  if (e.key === "Enter") return mod ? "open" : "primary";
  if (mod && e.key.toLowerCase() === "c" && !searchHasSelection) return "copy";
  if (!mod && e.key === "ArrowDown") return "next";
  if (!mod && e.key === "ArrowUp") return "previous";
  return null;
}

export function keepSelection(ranked: readonly Ranked[], selected: SavedQueryId | null, previousIndex: number): SavedQueryId | null {
  if (ranked.some((r) => r.query.id === selected)) return selected;
  return ranked[Math.min(previousIndex, ranked.length - 1)]?.query.id ?? null;
}

export function step(ranked: readonly Ranked[], from: SavedQueryId | null, dir: "next" | "previous"): SavedQueryId | null {
  const index = ranked.findIndex((r) => r.query.id === from);
  const next = index === -1 ? 0 : Math.max(0, Math.min(ranked.length - 1, index + (dir === "next" ? 1 : -1)));
  return ranked[next]?.query.id ?? null;
}
