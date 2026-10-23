import type { ConnectionConfig, DatabaseKind, PasswordInput, SshAuth, SshTunnelDraft, SslMode } from "../../ipc/bindings";
import { connectionColorId, type ConnectionColorId } from "./connectionColors";

export type SshAuthKind = SshAuth["kind"];

export type Fields = {
  databaseKind: DatabaseKind;
  name: string;
  host: string;
  port: string;
  database: string;
  user: string;
  sslMode: SslMode;
  caCertPath: string;
  color: ConnectionColorId | null;
  sshEnabled: boolean;
  sshHost: string;
  sshPort: string;
  sshUser: string;
  sshAuth: SshAuthKind;
  sshKeyPath: string;
  sshAgentSocket: string;
};

export type PasswordEdit = { kind: "keep" } | { kind: "set"; value: string } | { kind: "clear" };

export type FormState = { fields: Fields; password: PasswordEdit; sshSecret: PasswordEdit; url: string };

export function initialFields(c: ConnectionConfig | undefined, databaseKind: DatabaseKind = c?.databaseKind ?? "Postgres"): Fields {
  const sqlServer = databaseKind === "SqlServer";
  const ssh = c?.ssh;
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
    sshEnabled: ssh != null,
    sshHost: ssh?.host ?? "",
    sshPort: String(ssh?.port ?? 22),
    sshUser: ssh?.user ?? "",
    sshAuth: ssh?.auth.kind ?? "Password",
    sshKeyPath: ssh?.auth.kind === "Key" ? ssh.auth.path : "",
    sshAgentSocket: ssh?.auth.kind === "Agent" ? (ssh.auth.socket ?? "") : "",
  };
}

export function initialPassword(c: ConnectionConfig | undefined): PasswordEdit {
  return c?.hasPassword ? { kind: "keep" } : { kind: "clear" };
}

/** The engine keeps a saved SSH secret only under the auth kind it was saved for, so any other kind starts empty. */
export function initialSshSecret(c: ConnectionConfig | undefined, auth: SshAuthKind | undefined = c?.ssh?.auth.kind): PasswordEdit {
  return c?.ssh?.hasSecret && c.ssh.auth.kind === auth ? { kind: "keep" } : { kind: "clear" };
}

/** Whether the form differs from what `existing` would open it with. */
export function isDirty(existing: ConnectionConfig | undefined, form: FormState): boolean {
  const start = initialFields(existing);
  const fieldChanged = (Object.keys(start) as (keyof Fields)[]).some((k) => start[k] !== form.fields[k]);
  // The initial edits are never `set`, so comparing kinds is enough.
  return fieldChanged || form.password.kind !== initialPassword(existing).kind || form.sshSecret.kind !== initialSshSecret(existing).kind || form.url !== "";
}

export function parsePort(text: string): number | null {
  const port = Number(text);
  return Number.isInteger(port) && port > 0 && port < 65_536 ? port : null;
}

export function toPasswordInput(edit: PasswordEdit): PasswordInput {
  switch (edit.kind) {
    case "keep":
      return "Keep";
    case "clear":
      return "Clear";
    case "set":
      return { Set: edit.value };
    default: {
      const unreachable: never = edit;
      return unreachable;
    }
  }
}

/** The tunnel the form describes, or `null` when it is off. Agent auth sends `Clear`, since it takes no secret. */
export function toSshDraft(fields: Fields, sshSecret: PasswordEdit): SshTunnelDraft | null {
  if (!fields.sshEnabled) return null;
  const tunnel = { host: fields.sshHost.trim(), port: parsePort(fields.sshPort) ?? 22, user: fields.sshUser.trim() };
  switch (fields.sshAuth) {
    case "Password":
      return { ...tunnel, auth: { kind: "Password" }, secret: toPasswordInput(sshSecret) };
    case "Key":
      return { ...tunnel, auth: { kind: "Key", path: fields.sshKeyPath.trim() }, secret: toPasswordInput(sshSecret) };
    case "Agent":
      return { ...tunnel, auth: { kind: "Agent", socket: fields.sshAgentSocket.trim() || null }, secret: "Clear" };
    default: {
      const unreachable: never = fields.sshAuth;
      return unreachable;
    }
  }
}

/** Whether the tunnel fields are complete enough to dial. An off tunnel always is. */
export function sshValid(fields: Fields): boolean {
  if (!fields.sshEnabled) return true;
  return fields.sshHost.trim() !== "" && parsePort(fields.sshPort) !== null && fields.sshUser.trim() !== "" && (fields.sshAuth !== "Key" || fields.sshKeyPath.trim() !== "");
}
