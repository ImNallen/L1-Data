// @vitest-environment jsdom
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AppLayout } from "../../shell/AppLayout";
import { commandForKey } from "../../shell/commands";
import { Overlays } from "../../shell/Overlays";
import { useLive } from "../../state/live";
import { confirm, useWorkspace } from "../../state/workspace";
import { dropEditor } from "../editor/editorStates";
import { SavedPalette } from "../saved/SavedPalette";
import { SettingsDialog } from "./SettingsScreen";

beforeEach(() => {
  useWorkspace.setState((s) => ({ settings: { ...s.settings, panelMotionMs: 0 } }));
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  vi.stubGlobal("matchMedia", () => ({ matches: false, addEventListener() {}, removeEventListener() {} }));
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
});

afterEach(() => {
  cleanup();
  for (const id of useWorkspace.getState().order) dropEditor(id);
  useWorkspace.setState(useWorkspace.getInitialState(), true);
  useLive.setState(useLive.getInitialState(), true);
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

it("preserves the workspace nodes and directs search focus to settings without revealing the sidebar", async () => {
  const user = userEvent.setup();
  render(<AppLayout />);
  const main = screen.getByTestId("no-tabs");
  const tree = screen.getByTestId("conn-tree");
  await user.click(screen.getByTestId("open-settings"));
  expect(screen.getByTestId("no-tabs")).toBe(main);
  expect(screen.getByTestId("conn-tree")).toBe(tree);
  expect(document.activeElement).toBe(screen.getByTestId("settings-search"));

  await user.click(screen.getByTestId("settings-section-queries"));
  act(() => useWorkspace.getState().focusSearch());
  expect(document.activeElement).toBe(screen.getByTestId("settings-search"));
  expect(useWorkspace.getState().searchFocus).toBeNull();
  act(() => useWorkspace.getState().setLayout({ sidebar: false }));
  act(() => useWorkspace.getState().focusSearch());
  expect(useWorkspace.getState().layout.sidebar).toBe(false);
  expect(document.activeElement).toBe(screen.getByTestId("settings-search"));
});

function Dialogs() {
  return (
    <>
      <textarea data-testid="opener" />
      <SettingsDialog />
      <Overlays />
      <SavedPalette />
    </>
  );
}

it("clears search on the first Escape, closes on the next, and returns to the shortcut opener", async () => {
  const user = userEvent.setup();
  render(<Dialogs />);
  const opener = screen.getByTestId("opener");
  opener.focus();
  act(() => useWorkspace.getState().openSettings("queries"));
  await user.type(screen.getByTestId("settings-search"), "row");
  await user.keyboard("{Escape}");
  expect(screen.getByTestId("settings-search")).toHaveProperty("value", "");
  expect(screen.getByTestId("settings-breadcrumb").textContent).toBe("Settings/Queries");
  await user.keyboard("{Escape}");
  expect(screen.queryByTestId("settings-dialog")).toBeNull();
  await waitFor(() => expect(document.activeElement).toBe(opener));
});

it("dismisses Add theme before clearing the settings search, then dismisses the confirmation before settings", async () => {
  const user = userEvent.setup();
  render(<Dialogs />);
  act(() => useWorkspace.getState().openSettings());
  act(() => useWorkspace.getState().setSettingsQuery("theme"));
  await user.click(screen.getByTestId("add-theme"));
  await user.keyboard("{Escape}");
  expect(screen.queryByTestId("add-theme-dialog")).toBeNull();
  expect(screen.getByTestId("settings-search")).toHaveProperty("value", "theme");

  let answer: Promise<boolean> | undefined;
  act(() => {
    answer = confirm({ title: "Delete theme?", body: "Delete the custom theme.", confirmLabel: "Delete" });
  });
  expect(screen.getAllByTestId("confirm-dialog")).toHaveLength(1);
  expect(document.activeElement).toBe(screen.getByTestId("confirm-cancel"));
  await user.keyboard("{Escape}");
  await expect(answer).resolves.toBe(false);
  expect(screen.getByTestId("settings-search")).toHaveProperty("value", "theme");
  await user.keyboard("{Escape}");
  expect(screen.getByTestId("settings-search")).toHaveProperty("value", "");
  await user.keyboard("{Escape}");
  expect(screen.queryByTestId("settings-dialog")).toBeNull();
});

it("hands focus to a new dialog when a command leaves settings", async () => {
  const user = userEvent.setup();
  render(<Dialogs />);
  const opener = screen.getByTestId("opener");
  opener.focus();
  act(() => useWorkspace.getState().openSettings());
  act(() => {
    commandForKey({ key: "e", ctrlKey: true, metaKey: false, altKey: false, shiftKey: false }, false)?.run();
  });
  await user.keyboard("orders");
  expect(document.activeElement).toBe(screen.getByTestId("saved-input"));
  expect(screen.getByTestId("saved-input")).toHaveProperty("value", "orders");
});

it("keeps focus in the new editor when the new-tab shortcut leaves settings", async () => {
  const user = userEvent.setup();
  render(<AppLayout />);
  await user.click(screen.getByTestId("open-settings"));
  await user.keyboard("{Control>}t{/Control}");
  const editor = screen.getByTestId("sql-editor");
  expect(editor.contains(document.activeElement)).toBe(true);
  const input = document.activeElement;
  await user.keyboard("{Control>},{/Control}");
  expect(screen.getByTestId("settings-dialog")).toBeTruthy();
  expect(screen.getByTestId("sql-editor")).toBe(editor);
  await user.keyboard("{Escape}");
  await waitFor(() => expect(document.activeElement).toBe(input));
});
