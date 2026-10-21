import type { ConnectionConfig, DatabaseKind, DatabaseRef } from "../ipc/bindings";

export const DATABASE_LABEL: Record<DatabaseKind, string> = { Postgres: "PostgreSQL", SqlServer: "SQL Server" };

export function databaseKindFor(db: DatabaseRef | null, configs: Record<string, ConnectionConfig>): DatabaseKind {
  return db ? configs[db.connectionId]?.databaseKind ?? "Postgres" : "Postgres";
}
