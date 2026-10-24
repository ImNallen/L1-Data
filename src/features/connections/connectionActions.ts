import { commands, type ConnectError, type ConnectionId, type DatabaseRef, type SshError, type ViewEdit } from "../../ipc/bindings";
import { catalogErrorText, connectErrorText, disconnectErrorText } from "../../ipc/errors";
import { confirm, useWorkspace } from "../../state/workspace";

const ws = () => useWorkspace.getState();

export type UnknownHostKey = Extract<SshError, { kind: "HostKeyUnknown" }>;

/** The bastion key a Test or Connect can trust and retry with. Changed and revoked keys are not offered. */
export function unknownHostKey(e: ConnectError): UnknownHostKey | null {
  return e.kind === "Ssh" && e.error.kind === "HostKeyUnknown" ? e.error : null;
}

export async function connect(id: ConnectionId): Promise<void> {
  const res = await commands.connectionConnect(id);
  if (res.status === "ok") return;
  const name = ws().configs[id]?.name ?? "Connection";
  const key = unknownHostKey(res.error);
  if (!key) {
    ws().notify(`${name}: ${connectErrorText(res.error)}`);
    return;
  }
  const trust = await confirm({
    title: `Trust the host key of ${key.host}:${key.port}?`,
    body: `${name} connects through this SSH server, which offered an unknown ${key.algorithm} key ${key.fingerprint}. Trust it only if it matches the server's key.`,
    confirmLabel: "Trust and connect",
  });
  if (!trust) return;
  const trusted = await commands.sshTrustHostKey(key.trust);
  if (trusted.status === "error") ws().notify(`${name}: SSH: ${trusted.error}`);
  else await connect(id);
}

export async function disconnect(id: ConnectionId): Promise<void> {
  const res = await commands.connectionDisconnect(id, false);
  if (res.status === "ok") return;
  if (res.error.kind !== "OpenTransactions") {
    ws().notify(disconnectErrorText(res.error));
    return;
  }
  const rollBack = await confirm({
    title: "Disconnect with open transactions?",
    body: `${disconnectErrorText(res.error)} Disconnecting rolls it back.`,
    confirmLabel: "Roll back and disconnect",
  });
  if (rollBack) await commands.connectionDisconnect(id, true);
}

export async function confirmDelete(id: ConnectionId): Promise<void> {
  const name = ws().configs[id]?.name ?? "this connection";
  const ok = await confirm({ title: `Delete ${name}?`, body: "This removes the saved connection and its stored password. Query history is kept.", confirmLabel: "Delete" });
  if (!ok) return;
  const res = await commands.connectionDelete(id);
  if (res.status === "error") ws().notify(disconnectErrorText(res.error));
  else ws().removeConfig(id);
}

export async function refreshCatalog(db: DatabaseRef): Promise<void> {
  const res = await commands.catalogRefresh(db);
  if (res.status === "error") ws().notify(res.error.kind === "Failed" ? res.error.message : "Connect first to refresh the catalog.");
}

export async function refreshConnection(id: ConnectionId): Promise<void> {
  const res = await commands.connectionRefresh(id);
  if (res.status === "error") ws().notify(catalogErrorText(res.error));
}

export async function editView(id: ConnectionId, edit: ViewEdit): Promise<void> {
  const res = await commands.connectionEditView(id, edit);
  if (res.status === "error") ws().notify(catalogErrorText(res.error));
}

export function probeDatabases(id: ConnectionId, names: string[]): void {
  void commands.connectionProbeDatabases(id, names).catch((e: unknown) => ws().notify(String(e)));
}
