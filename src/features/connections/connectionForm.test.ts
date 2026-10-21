import { expect, it } from "vitest";
import type { ConnectionConfig } from "../../ipc/bindings";
import { initialFields, isDirty } from "./connectionForm";

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
};

const savedFields = { databaseKind: "Postgres", caCertPath: "", name: "shop", host: "db.internal", port: "5433", database: "shop", user: "l1data", sslMode: "Require", color: "red" } as const;

const newFields = { databaseKind: "Postgres", caCertPath: "", name: "", host: "localhost", port: "5432", database: "postgres", user: "postgres", sslMode: "Prefer", color: null } as const;

it("reads an untouched new form as clean", () => {
  expect(isDirty(undefined, { fields: { ...newFields }, password: { kind: "clear" }, url: "" })).toBe(false);
});

it("reads a changed host as dirty", () => {
  expect(isDirty(undefined, { fields: { ...newFields, host: "db.internal" }, password: { kind: "clear" }, url: "" })).toBe(true);
});

it("reads a new form with only a URL typed as dirty", () => {
  expect(isDirty(undefined, { fields: { ...newFields }, password: { kind: "clear" }, url: "postgres://" })).toBe(true);
});

it("reads a kept saved password as clean and a cleared one as dirty", () => {
  expect(isDirty(saved, { fields: { ...savedFields }, password: { kind: "keep" }, url: "" })).toBe(false);
  expect(isDirty(saved, { fields: { ...savedFields }, password: { kind: "clear" }, url: "" })).toBe(true);
});

it("compares color against the normalized id", () => {
  expect(isDirty(saved, { fields: { ...savedFields }, password: { kind: "keep" }, url: "" })).toBe(false);
  expect(isDirty({ ...saved, color: "#ef4444" }, { fields: { ...savedFields, color: null }, password: { kind: "keep" }, url: "" })).toBe(false);
});

it("starts SQL Server with SQL login, master and verified TLS", () => {
  expect(initialFields(undefined, "SqlServer")).toMatchObject({ databaseKind: "SqlServer", port: "1433", database: "master", user: "sa", sslMode: "VerifyFull", caCertPath: "" });
  const fields = initialFields({ ...saved, databaseKind: "SqlServer", port: 1434, database: "reporting", sslMode: "Require" });
  expect(fields).toMatchObject({ databaseKind: "SqlServer", port: "1434", database: "reporting", sslMode: "Require" });
  expect(isDirty(undefined, { fields: initialFields(undefined, "SqlServer"), password: { kind: "clear" }, url: "" })).toBe(true);
});
