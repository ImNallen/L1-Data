import { expect, it } from "vitest";
import type { ApproveError, ChatError, SendError } from "./bindings";
import { applyErrorText, approveErrorText, chatErrorText, configureErrorText, connectErrorText, runErrorText, sendErrorText } from "./errors";

it("keeps the engine's actionable capability refusal intact", () => {
  const what = "SQL Server Manual mode is unavailable. Select Auto-commit and use explicit transaction statements.";
  expect(configureErrorText({ kind: "Unsupported", what })).toBe(what);
  expect(applyErrorText({ kind: "Unsupported", what })).toBe(what);
  expect(runErrorText({ kind: "Unsupported", what })).toBe(`Cannot run this SQL. ${what}`);
});

it("tells the user to verify unknown edit outcomes before retrying", () => {
  const error = { message: "Connection closed during commit", severity: "ERROR", sqlstate: null, detail: null, hint: null, context: null, position: null };
  expect(applyErrorText({ kind: "OutcomeUnknown", error })).toBe(
    "The outcome of these edits is unknown. Verify the saved data before retrying. Connection closed during commit",
  );
  expect(applyErrorText({ kind: "Transaction", error })).not.toContain("Nothing was saved");
});

it("says what to do about every way a send can fail", () => {
  const errors: SendError[] = [
    { kind: "Busy" },
    { kind: "EmptyMessage" },
    { kind: "ProviderOff", provider: "Claude" },
    { kind: "UnknownModel", model: "opus-4" },
    { kind: "AttachmentGone" },
    { kind: "Store", message: "database is locked" },
  ];
  expect(errors.map(sendErrorText)).toEqual([
    "The assistant is still answering. Stop it or wait for it to finish.",
    "Type a message first.",
    "Claude is turned off. Turn it on in Settings → Providers.",
    "The model opus-4 is no longer offered. Pick another model.",
    "The attached result is gone. Run the query again, then attach it.",
    "The chat could not be saved. database is locked",
  ]);
});

it("says why a chat could not be opened or configured, and why an approval missed", () => {
  const errors: ChatError[] = [
    { kind: "UnknownModel", model: "opus-4" },
    { kind: "UnsupportedEffort", effort: "max" },
    { kind: "Store", message: "disk full" },
  ];
  expect(errors.map(chatErrorText)).toEqual([
    "The model opus-4 is no longer offered. Pick another model.",
    "This model has no max effort level.",
    "The chat could not be saved. disk full",
  ]);
  const late: ApproveError = { kind: "NotPending" };
  expect(approveErrorText(late)).toBe("This query was already answered, stopped, or expired.");
});

it("names the host key an SSH server offered, and the fix for a changed one", () => {
  const server = { host: "bastion", port: 2222, algorithm: "ssh-ed25519", fingerprint: "SHA256:abc" };
  expect(connectErrorText({ kind: "Ssh", error: { kind: "HostKeyUnknown", ...server, trust: 1 } })).toBe("SSH: bastion:2222 offered an unknown ssh-ed25519 host key SHA256:abc.");
  expect(connectErrorText({ kind: "Ssh", error: { kind: "HostKeyChanged", ...server, knownAt: "/Users/a/.ssh/known_hosts:4" } })).toContain(
    "ssh-keygen -R '[bastion]:2222' -f '/Users/a/.ssh/known_hosts'",
  );
  expect(connectErrorText({ kind: "Ssh", error: { kind: "MissingSecret" } })).toMatch(/^SSH: /);
});
