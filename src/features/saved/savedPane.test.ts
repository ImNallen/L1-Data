import { describe, expect, it } from "vitest";
import type { SavedQuery } from "../../ipc/bindings";
import { listKeysLive, paneReducer, type Pane, type PaneEvent } from "./savedPane";

const query: SavedQuery = {
  id: 7,
  name: "Active sessions",
  sql: "select pid from pg_stat_activity",
  databaseKind: "Postgres",
  excerpt: "",
  createdAtMs: 1,
  usedAtMs: 1,
};
const run = (events: PaneEvent[], from: Pane = { kind: "view" }) => events.reduce(paneReducer, from);
const edit = (closeOnDone = false): PaneEvent => ({ type: "edit", query, focus: "name", closeOnDone });
const rename: PaneEvent = { type: "change", draft: { name: "Sessions" } };

describe("the preview pane", () => {
  it("edits a copy of the query and keeps each field's change", () => {
    const pane = run([edit(), rename, { type: "change", draft: { sql: "select 1" } }]);
    expect(pane).toMatchObject({ kind: "edit", id: 7, base: { name: "Active sessions" }, draft: { name: "Sessions", sql: "select 1" } });
    expect(listKeysLive(pane)).toBe(false);
  });

  it("leaves an unchanged edit on Esc, and asks before it drops a changed one", () => {
    expect(run([edit(), { type: "escape" }])).toEqual({ kind: "view" });
    const asking = run([edit(), rename, { type: "escape" }]);
    expect(asking.kind).toBe("confirmDiscard");
    expect(run([{ type: "escape" }], asking)).toMatchObject({ kind: "edit", draft: { name: "Sessions" } });
    expect(run([{ type: "cancel" }], asking)).toMatchObject({ kind: "edit", draft: { name: "Sessions" } });
    expect(run([{ type: "confirm" }], asking)).toEqual({ kind: "view" });
  });

  it("saves a changed draft once, keeps it on a failure, and leaves on success", () => {
    const saving = run([edit(), rename, { type: "submit" }]);
    expect(saving).toMatchObject({ kind: "edit", saving: true });
    expect(run([rename, { type: "escape" }, { type: "submit" }], saving)).toBe(saving);
    const failed = run([{ type: "failed", message: "That saved query was deleted." }], saving);
    expect(failed).toMatchObject({ kind: "edit", saving: false, error: "That saved query was deleted.", draft: { name: "Sessions" } });
    expect(run([{ type: "change", draft: { name: "S" } }], failed)).toMatchObject({ error: null });
    expect(run([{ type: "saved" }], saving)).toEqual({ kind: "view" });
  });

  it("leaves an unchanged draft on submit without a write", () => {
    expect(run([edit(), { type: "submit" }])).toEqual({ kind: "view" });
  });

  it("closes the palette when the edit that names a new query ends, whichever way it ends", () => {
    expect(run([edit(true), { type: "submit" }])).toEqual({ kind: "done" });
    expect(run([edit(true), rename, { type: "submit" }, { type: "saved" }])).toEqual({ kind: "done" });
    expect(run([edit(true), { type: "escape" }])).toEqual({ kind: "done" });
    expect(run([edit(true), rename, { type: "escape" }, { type: "confirm" }])).toEqual({ kind: "done" });
  });

  it("confirms a delete inline, and drops the confirmation for Esc, Cancel, or another selection", () => {
    const asking = run([{ type: "askDelete", id: 7 }]);
    expect(asking).toEqual({ kind: "confirmDelete", id: 7 });
    expect(listKeysLive(asking)).toBe(false);
    expect(run([{ type: "selected", id: 7 }], asking)).toBe(asking);
    for (const event of [{ type: "escape" }, { type: "cancel" }, { type: "confirm" }, { type: "selected", id: 8 }] satisfies PaneEvent[]) {
      expect(run([event], asking)).toEqual({ kind: "view" });
    }
  });

  it("ignores events that do not apply to the current state", () => {
    expect(run([{ type: "submit" }, { type: "confirm" }, rename])).toEqual({ kind: "view" });
    expect(listKeysLive({ kind: "view" })).toBe(true);
  });
});
