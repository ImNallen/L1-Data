import { describe, expect, it } from "vitest";
import type { SavedQuery } from "../../ipc/bindings";
import { keepSelection, rankSaved, savedKey, step } from "./savedList";

const query = (id: number, name: string, sql = "select 1"): SavedQuery => ({ id, name, sql, databaseKind: "Postgres", excerpt: sql, createdAtMs: 1, usedAtMs: 1 });

const items = [
  query(1, "Revenue by region", "select region, sum(total) from orders group by region"),
  query(2, "Active sessions", "select pid, state from pg_stat_activity"),
  query(3, "Inactive accounts", "select * from accounts where last_login < now() - interval '1 year'"),
  query(4, "Locks", "select * from pg_locks join pg_stat_activity using (pid)"),
];
const names = (text: string) => rankSaved(items, text).map((r) => r.query.name);

describe("rankSaved", () => {
  it("keeps the engine's order with no search words", () => {
    expect(names("  ")).toEqual(["Revenue by region", "Active sessions", "Inactive accounts", "Locks"]);
  });

  it("ranks names whose words start with the search above names that only contain it, then SQL-only matches", () => {
    expect(names("act")).toEqual(["Active sessions", "Inactive accounts", "Locks"]);
  });

  it("needs every word, each in the name or the SQL, ignoring case", () => {
    expect(names("LOCKS activity")).toEqual(["Locks"]);
    expect(names("sessions pid")).toEqual(["Active sessions"]);
    expect(names("region zebra")).toEqual([]);
  });

  it("marks where the words matched the name, and only the words the name holds", () => {
    const [active] = rankSaved(items, "sess");
    expect(active?.nameMatch).toEqual([{ start: 7, end: 11 }]);
    const [locks] = rankSaved(items, "locks pid");
    expect(locks?.nameMatch).toEqual([{ start: 0, end: 5 }]);
  });
});

describe("selection", () => {
  const ranked = rankSaved(items, "");

  it("keeps a listed selection and moves a missing one to the entry now at its place, else the last", () => {
    expect(keepSelection(ranked, 3, 0)).toBe(3);
    expect(keepSelection(ranked, 9, 1)).toBe(2);
    expect(keepSelection(ranked, 9, 7)).toBe(4);
    expect(keepSelection(ranked, null, 0)).toBe(1);
    expect(keepSelection([], 3, 0)).toBeNull();
  });

  it("steps within the list and starts at the top from no selection", () => {
    expect([step(ranked, 1, "next"), step(ranked, 4, "next"), step(ranked, 1, "previous"), step(ranked, 3, "previous")]).toEqual([2, 4, 1, 2]);
    expect(step(ranked, null, "next")).toBe(1);
    expect(step([], null, "next")).toBeNull();
  });
});

describe("saved keys", () => {
  const key = (key: string, modifiers: Partial<Parameters<typeof savedKey>[0]> = {}, selection = false) =>
    savedKey({ key, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, isComposing: false, ...modifiers }, selection);

  it("uses with Enter, opens with Mod-Enter, copies with Mod-C, and moves with the arrows", () => {
    expect([key("Enter"), key("Enter", { metaKey: true }), key("Enter", { ctrlKey: true }), key("c", { metaKey: true }), key("ArrowDown"), key("ArrowUp")]).toEqual([
      "primary",
      "open",
      "open",
      "copy",
      "next",
      "previous",
    ]);
  });

  it("leaves a search selection's copy, composition, and other chords alone", () => {
    expect(key("c", { metaKey: true }, true)).toBeNull();
    expect(key("Enter", { isComposing: true })).toBeNull();
    expect([key("Enter", { shiftKey: true }), key("Enter", { altKey: true }), key("Backspace", { metaKey: true }), key("F2"), key("Escape")]).toEqual([null, null, null, null, null]);
  });
});
