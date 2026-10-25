import { afterEach, expect, it, vi } from "vitest";
import { commands, type DatabaseRef } from "../../ipc/bindings";
import { useWorkspace } from "../../state/workspace";
import { dropEditor, textOf } from "../editor/editorStates";
import { openObjectScript } from "./ConnectionTree";

const shop: DatabaseRef = { connectionId: "pg", database: "shop" };

afterEach(() => {
  for (const id of useWorkspace.getState().order) dropEditor(id);
  vi.restoreAllMocks();
  useWorkspace.setState(useWorkspace.getInitialState(), true);
});

it("opens the script in a new tab on the object's database, titled by the engine", async () => {
  const text = "CREATE TABLE public.orders (\n    id bigint NOT NULL\n);\n";
  const call = vi.spyOn(commands, "objectScript").mockResolvedValueOnce({ status: "ok", data: { title: "orders.sql", text } });
  await openObjectScript(shop, { kind: "Relation", oid: 42 });
  expect(call).toHaveBeenCalledWith(shop, { kind: "Relation", oid: 42 });
  const { active, tabs } = useWorkspace.getState();
  expect(tabs[active!]).toMatchObject({ title: "orders.sql", console: { db: shop }, table: null, file: null });
  expect(textOf(active!)).toBe(text);
});

it("shows why an object cannot be scripted and opens no tab", async () => {
  vi.spyOn(commands, "objectScript").mockResolvedValueOnce({ status: "error", error: { kind: "Failed", message: "Aggregates cannot be scripted yet." } });
  await openObjectScript(shop, { kind: "Routine", oid: 7 });
  const { order, toast } = useWorkspace.getState();
  expect(order).toEqual([]);
  expect(toast?.text).toBe("Aggregates cannot be scripted yet.");
});
