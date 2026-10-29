// @vitest-environment jsdom
import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it } from "vitest";
import { ParamsDialog } from "./ParamsDialog";
import { finishParams, forgetParams, promptParams } from "./paramsState";

afterEach(() => {
  cleanup();
  finishParams(null);
  forgetParams("tab");
});

it("keeps text and NULL distinct, remembers accepted values, and cancels on Escape", async () => {
  const user = userEvent.setup();
  render(<ParamsDialog />);
  let answer: ReturnType<typeof promptParams> | undefined;
  act(() => {
    answer = promptParams("tab", "db", [{ name: ":name" }, { name: "$1" }], "dev / shop", () => true);
  });
  expect(await screen.findByTestId("params-dialog")).toBeTruthy();
  await user.type(screen.getByTestId("param-value-:name"), "O'Reilly\\literal");
  await user.click(screen.getByTestId("param-null-$1"));
  await user.click(screen.getByTestId("params-run"));
  await expect(answer).resolves.toEqual([
    { name: ":name", value: "O'Reilly\\literal" },
    { name: "$1", value: null },
  ]);
  act(() => {
    answer = promptParams("tab", "db", [{ name: ":name" }, { name: "$1" }], "dev / shop", () => true);
  });
  expect(await screen.findByTestId("param-value-:name")).toHaveProperty("value", "O'Reilly\\literal");
  expect(screen.getByTestId("param-null-$1")).toHaveProperty("checked", true);
  await user.keyboard("{Escape}");
  await expect(answer).resolves.toBeNull();
});

it("remounts superseded same-name prompts and clears values for a new target", async () => {
  const user = userEvent.setup();
  render(<ParamsDialog />);
  let first: ReturnType<typeof promptParams> | undefined;
  let second: ReturnType<typeof promptParams> | undefined;
  act(() => {
    first = promptParams("tab", "db", [{ name: ":id" }], "dev / shop", () => true);
  });
  await user.type(await screen.findByTestId("param-value-:id"), "unsent");
  act(() => {
    second = promptParams("tab", "other", [{ name: ":id" }], "dev / other", () => true);
  });
  await expect(first).resolves.toBeNull();
  expect(screen.getByTestId("param-value-:id")).toHaveProperty("value", "");
  await user.click(screen.getByTestId("params-run"));
  await expect(second).resolves.toEqual([{ name: ":id", value: "" }]);
});

it("focuses the NULL control when the first remembered value is NULL", async () => {
  render(<ParamsDialog />);
  let answer: ReturnType<typeof promptParams> | undefined;
  act(() => {
    answer = promptParams("tab", "db", [{ name: ":id" }], "dev / shop", () => true);
  });
  act(() => finishParams([{ name: ":id", value: null }]));
  await answer;
  act(() => {
    answer = promptParams("tab", "db", [{ name: ":id" }], "dev / shop", () => true);
  });
  expect(await screen.findByTestId("param-null-:id")).toBe(document.activeElement);
  await userEvent.setup().keyboard("{Escape}");
  await expect(answer).resolves.toBeNull();
});
