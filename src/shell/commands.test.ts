import { afterEach, expect, it } from "vitest";
import { useWorkspace } from "../state/workspace";
import { commandForKey, handleGlobalKey } from "./commands";

type Mods = Partial<Record<"metaKey" | "ctrlKey" | "shiftKey" | "altKey", boolean>>;
const chord = (key: string, mods: Mods) => ({ key, metaKey: false, ctrlKey: false, shiftKey: false, altKey: false, ...mods });
const press = (key: string, mods: Mods = { metaKey: true }) => commandForKey(chord(key, mods), true);
const pressOff = (key: string, mods: Mods) => commandForKey(chord(key, mods), false);

afterEach(() => useWorkspace.setState(useWorkspace.getInitialState(), true));

it("maps each shortcut to its command", () => {
  expect(["t", "n", "p", ",", "w"].map((k) => press(k)?.id)).toEqual(["new-query", "new-connection", "search", "settings", "close-tab"]);
  expect(press("T")?.id).toBe("new-query");
});

it("takes Cmd alone on macOS and leaves the Ctrl chords to the editor", () => {
  expect([press("t", { ctrlKey: true }), press("p", { ctrlKey: true }), press("t", { metaKey: true, ctrlKey: true })]).toEqual([undefined, undefined, undefined]);
});

it("takes Ctrl alone off macOS", () => {
  expect([pressOff("t", { ctrlKey: true })?.id, pressOff("w", { ctrlKey: true })?.id]).toEqual(["new-query", "close-tab"]);
  expect([pressOff("t", { metaKey: true }), pressOff("t", { metaKey: true, ctrlKey: true })]).toEqual([undefined, undefined]);
});

function keydown(key: string, mods: Mods, defaultPrevented: boolean) {
  const e = { ...chord(key, mods), defaultPrevented, prevented: false, preventDefault: () => (e.prevented = true) };
  handleGlobalKey(e as unknown as KeyboardEvent);
  return e;
}

it("leaves a chord a focused control already handled, and claims the ones it runs", () => {
  const handled = keydown("n", { ctrlKey: true }, true);
  expect([useWorkspace.getState().overlay, handled.prevented]).toEqual([{ kind: "none" }, false]);
  const ran = keydown("n", { ctrlKey: true }, false);
  expect([useWorkspace.getState().overlay, ran.prevented]).toEqual([{ kind: "connection", editing: null }, true]);
});

it("ignores plain keys and shifted or alt chords", () => {
  expect(press("t", {})).toBeUndefined();
  expect(press("t", { metaKey: true, shiftKey: true })).toBeUndefined();
  expect(press("p", { metaKey: true, altKey: true })).toBeUndefined();
  expect(press("k")).toBeUndefined();
});

it("opens the overlay a command names", () => {
  press("n")?.run();
  expect(useWorkspace.getState().overlay).toEqual({ kind: "connection", editing: null });
});

const searchState = () => {
  const { screen, overlay, layout, searchFocus } = useWorkspace.getState();
  return { screen, overlay, sidebar: layout.sidebar, searchFocus };
};

it("asks the sidebar search box for focus on ⌘P, closing any overlay so the box can take it", () => {
  useWorkspace.getState().open({ kind: "history" });
  press("p")?.run();
  expect(searchState()).toEqual({ screen: { kind: "workspace" }, overlay: { kind: "none" }, sidebar: true, searchFocus: true });
  useWorkspace.getState().searchFocused();
  expect(useWorkspace.getState().searchFocus).toBe(false);
});

it("reveals a hidden sidebar on ⌘P so its search box can mount and take focus", () => {
  useWorkspace.getState().setLayout({ sidebar: false });
  press("p")?.run();
  expect(searchState()).toEqual({ screen: { kind: "workspace" }, overlay: { kind: "none" }, sidebar: true, searchFocus: true });
});

it("stays on settings on ⌘P there and focuses the settings search box", () => {
  useWorkspace.getState().openSettings("queries");
  useWorkspace.getState().setSettingsQuery("row");
  press("p")?.run();
  expect(searchState()).toEqual({ screen: { kind: "settings", section: "queries", query: "row" }, overlay: { kind: "none" }, sidebar: true, searchFocus: true });
});

it("toggles the settings screen on its shortcut, starting at Appearance", () => {
  press(",")?.run();
  expect(useWorkspace.getState().screen).toEqual({ kind: "settings", section: "appearance", query: "" });
  press(",")?.run();
  expect(useWorkspace.getState().screen).toEqual({ kind: "workspace" });
});

it("leaves settings for the workspace before a shortcut opens an overlay there", () => {
  useWorkspace.getState().openSettings("queries");
  press("n")?.run();
  expect([useWorkspace.getState().screen, useWorkspace.getState().overlay]).toEqual([{ kind: "workspace" }, { kind: "connection", editing: null }]);
});

it("closes settings, not the active tab, on the close-tab shortcut", () => {
  useWorkspace.getState().addTab({ id: "t1", title: "a.sql", console: { db: null, mode: "AutoCommit" }, table: null });
  useWorkspace.getState().openSettings();
  press("w")?.run();
  expect([useWorkspace.getState().screen, useWorkspace.getState().order]).toEqual([{ kind: "workspace" }, ["t1"]]);
});

it.each(["w", ",", "t"])("drops the settings search when ⌘%s leaves settings, and keeps the tree search", (key) => {
  useWorkspace.getState().setTreeQuery("order");
  useWorkspace.getState().openSettings("queries");
  useWorkspace.getState().setSettingsQuery("row");
  press(key)?.run();
  press(",")?.run();
  expect([useWorkspace.getState().screen, useWorkspace.getState().treeQuery]).toEqual([{ kind: "settings", section: "appearance", query: "" }, "order"]);
});

it("closes an open overlay when ⌘, opens settings", () => {
  useWorkspace.getState().open({ kind: "history" });
  press(",")?.run();
  expect([useWorkspace.getState().screen, useWorkspace.getState().overlay]).toEqual([{ kind: "settings", section: "appearance", query: "" }, { kind: "none" }]);
});

it("opens a section with a blank search when its nav entry is picked from the results", () => {
  useWorkspace.getState().openSettings("appearance");
  useWorkspace.getState().setSettingsQuery("row limit");
  useWorkspace.getState().openSettings("queries");
  expect(useWorkspace.getState().screen).toEqual({ kind: "settings", section: "queries", query: "" });
});

it("keeps a settings search out of the workspace screen", () => {
  useWorkspace.getState().setSettingsQuery("row");
  expect([useWorkspace.getState().screen, useWorkspace.getState().treeQuery]).toEqual([{ kind: "workspace" }, ""]);
});
