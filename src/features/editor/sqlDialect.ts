import { MSSQL, PostgreSQL, type SQLDialect } from "@codemirror/lang-sql";
import type { DatabaseKind } from "../../ipc/bindings";

export const SQL_DIALECT: Record<DatabaseKind, SQLDialect> = { Postgres: PostgreSQL, SqlServer: MSSQL };
