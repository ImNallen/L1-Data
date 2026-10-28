import type { CellAggregate } from "../../ipc/bindings";
import { groupDigits } from "./format";

export type SelectionSlot = { key: string; epoch: number; text: string | null };

export function acceptSelectionTotals(current: SelectionSlot | null, next: SelectionSlot, shownKey: string): SelectionSlot | null {
  if (next.key !== shownKey) return current;
  if (current?.key === next.key && next.epoch < current.epoch) return current;
  return next;
}

export function formatSelectionTotals(agg: CellAggregate, pending: boolean): string {
  const counts = `${groupDigits(String(agg.count))} cells · ${groupDigits(String(agg.nonNull))} non-null`;
  const stats = agg.kind === "Numeric" ? ` · Sum ${agg.sum} · Avg ${agg.avg} · Min ${agg.min} · Max ${agg.max}` : "";
  const text = counts + stats;
  return pending ? `${text} · excludes pending edits` : text;
}
