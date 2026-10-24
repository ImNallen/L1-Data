import type { DatabaseKind, SavedQuery, SavedQueryId } from "../../ipc/bindings";

export type Draft = { name: string; sql: string };

export type EditPane = { kind: "edit"; id: SavedQueryId; databaseKind: DatabaseKind; base: Draft; draft: Draft; focus: "name" | "sql"; closeOnDone: boolean; error: string | null; saving: boolean };

export type Pane = { kind: "view" } | EditPane | { kind: "confirmDiscard"; edit: EditPane } | { kind: "confirmDelete"; id: SavedQueryId } | { kind: "done" };

export type PaneEvent =
  | { type: "edit"; query: SavedQuery; focus: "name" | "sql"; closeOnDone?: boolean }
  | { type: "change"; draft: Partial<Draft> }
  | { type: "submit" }
  | { type: "saved" }
  | { type: "failed"; message: string }
  | { type: "escape" }
  | { type: "askDelete"; id: SavedQueryId }
  | { type: "confirm" }
  | { type: "cancel" }
  | { type: "selected"; id: SavedQueryId | null };

export const dirty = (edit: EditPane) => edit.draft.name !== edit.base.name || edit.draft.sql !== edit.base.sql;

const leave = (edit: EditPane): Pane => (edit.closeOnDone ? { kind: "done" } : { kind: "view" });

export function paneReducer(pane: Pane, event: PaneEvent): Pane {
  switch (pane.kind) {
    case "view":
      if (event.type === "edit") {
        const base = { name: event.query.name, sql: event.query.sql };
        return { kind: "edit", id: event.query.id, databaseKind: event.query.databaseKind, base, draft: base, focus: event.focus, closeOnDone: event.closeOnDone ?? false, error: null, saving: false };
      }
      return event.type === "askDelete" ? { kind: "confirmDelete", id: event.id } : pane;
    case "edit":
      if (event.type === "failed") return { ...pane, saving: false, error: event.message };
      if (event.type === "saved") return leave(pane);
      if (pane.saving) return pane;
      if (event.type === "change") return { ...pane, draft: { ...pane.draft, ...event.draft }, error: null };
      if (event.type === "submit") return dirty(pane) ? { ...pane, saving: true, error: null } : leave(pane);
      if (event.type === "escape") return dirty(pane) ? { kind: "confirmDiscard", edit: pane } : leave(pane);
      return pane;
    case "confirmDiscard":
      if (event.type === "confirm") return leave(pane.edit);
      return event.type === "cancel" || event.type === "escape" ? pane.edit : pane;
    case "confirmDelete":
      if (event.type === "confirm" || event.type === "cancel" || event.type === "escape") return { kind: "view" };
      return event.type === "selected" && event.id !== pane.id ? { kind: "view" } : pane;
    case "done":
      return pane;
    default: {
      const unreachable: never = pane;
      return unreachable;
    }
  }
}

export const listKeysLive = (pane: Pane) => pane.kind === "view";
