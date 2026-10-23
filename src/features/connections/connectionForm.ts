import type { ConnectionConfig, DatabaseKind, SshTunnelDraft, SslMode } from "../../ipc/bindings";
import { connectionColorId, type ConnectionColorId } from "./connectionColors";

export type Fields = { databaseKind: DatabaseKind; name: string; host: string; port: string; database: string; user: string; sslMode: SslMode; caCertPath: string; color: ConnectionColorId | null };

export type PasswordEdit = { kind: "keep" } | { kind: "set"; value: string } | { kind: "clear" };

export type FormState = { fields: Fields; password: PasswordEdit; url: string };

export function initialFields(c: ConnectionConfig | undefined, databaseKind: DatabaseKind = c?.databaseKind ?? "Postgres"): Fields {
  const sqlServer = databaseKind === "SqlServer";
  return {
    databaseKind,
    name: c?.name ?? "",
    host: c?.host ?? "localhost",
    port: String(c?.port ?? (sqlServer ? 1433 : 5432)),
    database: c?.database ?? (sqlServer ? "master" : "postgres"),
    user: c?.user ?? (sqlServer ? "sa" : "postgres"),
    sslMode: c?.sslMode ?? (sqlServer ? "VerifyFull" : "Prefer"),
    caCertPath: c?.caCertPath ?? "",
    color: connectionColorId(c?.color ?? null),
  };
}

export function initialPassword(c: ConnectionConfig | undefined): PasswordEdit {
  return c?.hasPassword ? { kind: "keep" } : { kind: "clear" };
}

/** Whether the form differs from what `existing` would open it with. */
export function isDirty(existing: ConnectionConfig | undefined, form: FormState): boolean {
  const start = initialFields(existing);
  const fieldChanged = (Object.keys(start) as (keyof Fields)[]).some((k) => start[k] !== form.fields[k]);
  // The initial edit is never `set`, so comparing kinds is enough.
  return fieldChanged || form.password.kind !== initialPassword(existing).kind || form.url !== "";
}

/** The saved tunnel as a draft that keeps its secret, so a save from this form never drops it. */
export function keptTunnel(c: ConnectionConfig | undefined): SshTunnelDraft | null {
  if (!c?.ssh) return null;
  const { host, port, user, auth } = c.ssh;
  return { host, port, user, auth, secret: "Keep" };
}
