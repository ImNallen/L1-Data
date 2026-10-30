// @vitest-environment jsdom
import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it } from "vitest";
import { ask, confirm, useWorkspace } from "../state/workspace";
import { Overlays } from "./Overlays";

afterEach(() => {
  cleanup();
  useWorkspace.setState(useWorkspace.getInitialState(), true);
});

it("opens a destructive confirm on Cancel, so Enter keeps the data", async () => {
  const user = userEvent.setup();
  render(<Overlays />);
  let answer: Promise<boolean> | undefined;
  act(() => {
    answer = confirm({ title: "Delete connection?", body: "This removes prod.", confirmLabel: "Delete" });
  });
  expect(await screen.findByRole("alertdialog", { name: "Delete connection?" })).toBe(screen.getByTestId("confirm-dialog"));
  expect(document.activeElement).toBe(screen.getByTestId("confirm-cancel"));

  await user.keyboard("{Enter}");
  await expect(answer).resolves.toBe(false);
  expect(screen.queryByTestId("confirm-dialog")).toBeNull();
});

it("still opens on the primary choice when a question has one beside a danger choice", async () => {
  render(<Overlays />);
  act(() => {
    void ask({
      testId: "ask",
      title: "Save changes?",
      body: "The file has unsaved changes.",
      choices: [
        { id: "discard", label: "Discard", variant: "danger" },
        { id: "save", label: "Save", variant: "primary" },
      ],
    });
  });
  expect(await screen.findByTestId("ask-dialog")).toBeTruthy();
  expect(document.activeElement).toBe(screen.getByTestId("ask-save"));
});

it("says a toast through a live region that is mounted before the toast and stays after it", () => {
  render(<Overlays />);
  const region = screen.getByRole("status");
  expect(region.textContent).toBe("");
  act(() => useWorkspace.getState().notify("Query copied"));
  expect([screen.getByRole("status"), region.textContent, screen.getByTestId("toast").textContent]).toEqual([region, "Query copied", "Query copied"]);
});
