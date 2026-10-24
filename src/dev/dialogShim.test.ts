import { afterEach, expect, it, vi } from "vitest";
import { createDialogShim } from "./dialogShim";

afterEach(() => vi.restoreAllMocks());

it("answers dialogs from the queue, oldest first, in the shape each dialog returns", () => {
  const d = createDialogShim();
  d.shim.answer(["/q/a.sql", "/q/b.sql"], "/q/c.sql", "/q/new.sql", ["/q/x.sql", "/q/y.sql"], null);
  expect([
    d.reply("plugin:dialog|open", { options: { multiple: true } }),
    d.reply("plugin:dialog|open", { options: { multiple: true } }),
    d.reply("plugin:dialog|save", { options: { defaultPath: "Untitled-1.sql" } }),
    d.reply("plugin:dialog|save", { options: {} }),
    d.reply("plugin:dialog|open", { options: { multiple: true } }),
  ]).toEqual([["/q/a.sql", "/q/b.sql"], ["/q/c.sql"], "/q/new.sql", "/q/x.sql", null]);
  expect(d.shim.calls.map((c) => c.cmd)).toEqual(["open", "open", "save", "save", "open"]);
  expect(d.shim.calls[2]?.options).toEqual({ defaultPath: "Untitled-1.sql" });
});

it("answers Cancel and warns when nothing is queued", () => {
  const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
  const d = createDialogShim();
  expect(d.reply("plugin:dialog|save", { options: {} })).toBeNull();
  expect(warn).toHaveBeenCalledOnce();
});

it("leaves every other command to the bridge", () => {
  const d = createDialogShim();
  expect([d.handles("plugin:dialog|open"), d.handles("plugin:dialog|save"), d.handles("file_open"), d.handles("plugin:dialog|message")]).toEqual([true, true, false, false]);
});
