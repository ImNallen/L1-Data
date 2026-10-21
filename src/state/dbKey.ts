import type { ConnectionConfig, ConnectionState, DatabaseRef } from "../ipc/bindings";

/** Map key for a DatabaseRef. NUL cannot appear in a Postgres identifier, so the join is exact. */
export type DbKey = string & { readonly __brand: "DbKey" };

export function dbKey(ref: DatabaseRef): DbKey {
  return `${ref.connectionId}\u0000${ref.database}` as DbKey;
}

export function sameDb(a: DatabaseRef | null | undefined, b: DatabaseRef | null | undefined): boolean {
  return a === b || (!!a && !!b && a.connectionId === b.connectionId && a.database === b.database);
}

export function resolveDb(db: DatabaseRef, state: ConnectionState | undefined): DatabaseRef {
  if (state?.id !== db.connectionId || state.status.kind !== "Connected") return db;
  const { configuredDatabase, home } = state.status.instance;
  return db.database === configuredDatabase && db.database !== home ? { ...db, database: home } : db;
}

export function homeDb(config: ConnectionConfig, state?: ConnectionState): DatabaseRef {
  return resolveDb({ connectionId: config.id, database: config.database }, state);
}
