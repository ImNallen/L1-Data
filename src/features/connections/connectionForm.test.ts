import { expect, it } from "vitest";
import type { ConnectionConfig } from "../../ipc/bindings";
import { initialFields, initialSshSecret, isDirty, sshValid, toSshDraft, type FormState, type PasswordEdit } from "./connectionForm";

const saved: ConnectionConfig = {
  id: "c1",
  databaseKind: "Postgres",
  name: "shop",
  host: "db.internal",
  port: 5433,
  database: "shop",
  user: "l1data",
  sslMode: "Require",
  caCertPath: null,
  hasPassword: true,
  color: "red",
  ssh: null,
  environment: null,
};

const savedFields = { databaseKind: "Postgres", caCertPath: "", name: "shop", host: "db.internal", port: "5433", database: "shop", user: "l1data", sslMode: "Require", color: "red", sshEnabled: false, sshHost: "", sshPort: "22", sshUser: "", sshAuth: "Password", sshKeyPath: "", sshAgentSocket: "" } as const;

const newFields = { databaseKind: "Postgres", caCertPath: "", name: "", host: "localhost", port: "5432", database: "postgres", user: "postgres", sslMode: "Prefer", color: null, sshEnabled: false, sshHost: "", sshPort: "22", sshUser: "", sshAuth: "Password", sshKeyPath: "", sshAgentSocket: "" } as const;

it("reads an untouched new form as clean", () => {
  expect(isDirty(undefined, { fields: { ...newFields }, password: { kind: "clear" }, sshSecret: { kind: "clear" }, url: "" })).toBe(false);
});

it("reads a changed host as dirty", () => {
  expect(isDirty(undefined, { fields: { ...newFields, host: "db.internal" }, password: { kind: "clear" }, sshSecret: { kind: "clear" }, url: "" })).toBe(true);
});

it("reads a new form with only a URL typed as dirty", () => {
  expect(isDirty(undefined, { fields: { ...newFields }, password: { kind: "clear" }, sshSecret: { kind: "clear" }, url: "postgres://" })).toBe(true);
});

it("reads a kept saved password as clean and a cleared one as dirty", () => {
  expect(isDirty(saved, { fields: { ...savedFields }, password: { kind: "keep" }, sshSecret: { kind: "clear" }, url: "" })).toBe(false);
  expect(isDirty(saved, { fields: { ...savedFields }, password: { kind: "clear" }, sshSecret: { kind: "clear" }, url: "" })).toBe(true);
});

it("compares color against the normalized id", () => {
  expect(isDirty(saved, { fields: { ...savedFields }, password: { kind: "keep" }, sshSecret: { kind: "clear" }, url: "" })).toBe(false);
  expect(isDirty({ ...saved, color: "#ef4444" }, { fields: { ...savedFields, color: null }, password: { kind: "keep" }, sshSecret: { kind: "clear" }, url: "" })).toBe(false);
});

it("starts SQL Server with SQL login, master and verified TLS", () => {
  expect(initialFields(undefined, "SqlServer")).toMatchObject({ databaseKind: "SqlServer", port: "1433", database: "master", user: "sa", sslMode: "VerifyFull", caCertPath: "" });
  const fields = initialFields({ ...saved, databaseKind: "SqlServer", port: 1434, database: "reporting", sslMode: "Require" });
  expect(fields).toMatchObject({ databaseKind: "SqlServer", port: "1434", database: "reporting", sslMode: "Require" });
  expect(isDirty(undefined, { fields: initialFields(undefined, "SqlServer"), password: { kind: "clear" }, sshSecret: { kind: "clear" }, url: "" })).toBe(true);
});

const tunneled: ConnectionConfig = { ...saved, ssh: { host: "bastion", port: 2222, user: "tunnel", auth: { kind: "Key", path: "~/.ssh/id_ed25519" }, hasSecret: true } };
const keep: PasswordEdit = { kind: "keep" };
const clear: PasswordEdit = { kind: "clear" };

function form(over: Partial<FormState["fields"]>, sshSecret: PasswordEdit = keep): FormState {
  return { fields: { ...initialFields(tunneled), ...over }, password: keep, sshSecret, url: "" };
}

it("opens a saved tunnel clean and saves it unchanged with its passphrase kept", () => {
  const start = form({});
  expect(isDirty(tunneled, start)).toBe(false);
  expect(toSshDraft(start.fields, start.sshSecret)).toEqual({ host: "bastion", port: 2222, user: "tunnel", auth: { kind: "Key", path: "~/.ssh/id_ed25519" }, secret: "Keep" });
});

it("sends no tunnel when it is off, and turning a saved one off is an edit", () => {
  expect(toSshDraft(initialFields(saved), clear)).toBeNull();
  expect(isDirty(tunneled, form({ sshEnabled: false }))).toBe(true);
});

it("keeps a saved secret only under the auth kind it was saved for", () => {
  expect(initialSshSecret(tunneled, "Key")).toEqual(keep);
  expect(initialSshSecret(tunneled, "Password")).toEqual(clear);
  expect(initialSshSecret({ ...tunneled, ssh: { ...tunneled.ssh!, hasSecret: false } }, "Key")).toEqual(clear);
  expect(isDirty(tunneled, form({ sshAuth: "Password" }, initialSshSecret(tunneled, "Password")))).toBe(true);
  expect(isDirty(tunneled, form({ sshAuth: "Key" }, initialSshSecret(tunneled, "Key")))).toBe(false);
});

it("sends the secret edit for password and key auth, and none for the agent", () => {
  const password = form({ sshAuth: "Password" }, { kind: "set", value: "hunter2" }).fields;
  expect(toSshDraft(password, { kind: "set", value: "hunter2" })).toMatchObject({ auth: { kind: "Password" }, secret: { Set: "hunter2" } });
  expect(toSshDraft(form({}).fields, clear)).toMatchObject({ auth: { kind: "Key", path: "~/.ssh/id_ed25519" }, secret: "Clear" });
  expect(toSshDraft(form({ sshAuth: "Agent" }).fields, keep)).toMatchObject({ auth: { kind: "Agent", socket: null }, secret: "Clear" });
  expect(toSshDraft(form({ sshAuth: "Agent", sshAgentSocket: " /tmp/agent.sock " }).fields, keep)).toMatchObject({ auth: { kind: "Agent", socket: "/tmp/agent.sock" } });
});

it("needs a host, a port, a user, and a key path for key auth before a tunnel can dial", () => {
  expect(sshValid(initialFields(saved))).toBe(true);
  expect(sshValid(form({}).fields)).toBe(true);
  expect(sshValid(form({ sshHost: " " }).fields)).toBe(false);
  expect(sshValid(form({ sshPort: "70000" }).fields)).toBe(false);
  expect(sshValid(form({ sshUser: "" }).fields)).toBe(false);
  expect(sshValid(form({ sshKeyPath: "" }).fields)).toBe(false);
  expect(sshValid(form({ sshKeyPath: "", sshAuth: "Agent" }).fields)).toBe(true);
});
