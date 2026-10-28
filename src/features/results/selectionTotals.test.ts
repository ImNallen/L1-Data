import { expect, it } from "vitest";
import type { CellAggregate } from "../../ipc/bindings";
import { acceptSelectionTotals, formatSelectionTotals } from "./selectionTotals";

it("drops a selection total for another result or an older epoch", () => {
  const shown = "q1:1:0";
  const later = { key: shown, epoch: 2, text: "Sum later" };
  const earlier = { key: shown, epoch: 1, text: "Sum earlier" };
  expect(acceptSelectionTotals(later, earlier, shown)).toBe(later);
  expect(acceptSelectionTotals(earlier, later, shown)).toEqual(later);
  expect(acceptSelectionTotals(later, { key: "q1:2:0", epoch: 9, text: "other" }, shown)).toBe(later);
  expect(acceptSelectionTotals(null, later, shown)).toEqual(later);
});

it("groups the cell counts and leaves the sum strings untouched", () => {
  const numeric: CellAggregate = { kind: "Numeric", count: 2, nonNull: 2, sum: "1000", avg: "2000.50", min: "3000", max: "4000" };
  expect(formatSelectionTotals({ kind: "Counts", count: 1000, nonNull: 2 }, false)).toBe("1,000 cells · 2 non-null");
  expect(formatSelectionTotals(numeric, true)).toBe("2 cells · 2 non-null · Sum 1000 · Avg 2000.50 · Min 3000 · Max 4000 · excludes pending edits");
});
