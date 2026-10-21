mod mssql;
mod postgres;

use crate::connection::{ConnectParams, DatabaseInfo};
use crate::model::*;
use futures_util::Stream;
use std::pin::Pin;
use tokio::sync::watch;

pub const SQL_SERVER_EDITS_UNSUPPORTED: &str = "SQL Server staged grid edits are unavailable because server-side code can end the edit transaction. Use the SQL editor in Auto-commit mode with explicit transaction statements.";

pub struct Session {
    driver: Driver,
    pub backend_pid: i32,
    pub server: ServerInfo,
    pub search_path: Option<String>,
}

enum Driver {
    Postgres(postgres::PgSession),
    SqlServer(mssql::MsSession),
}

#[derive(Clone)]
pub struct CancelHandle(CancelDriver);
#[derive(Clone)]
enum CancelDriver {
    Postgres(postgres::PgCancel),
    SqlServer(mssql::MsCancel),
}
impl CancelHandle {
    pub async fn cancel(&self) {
        match &self.0 {
            CancelDriver::Postgres(p) => p.cancel().await,
            CancelDriver::SqlServer(s) => s.cancel().await,
        }
    }
}

pub enum QueryEvent {
    Prepared,
    Columns(Vec<ColumnMeta>),
    Row(Vec<Option<String>>),
    Complete { affected_rows: Option<u64> },
    Notice(OutputBody),
}
pub type QueryStream<'a> = Pin<Box<dyn Stream<Item = Result<QueryEvent, QueryError>> + Send + 'a>>;

pub fn error(message: impl Into<String>) -> QueryError {
    QueryError { severity: "ERROR".into(), sqlstate: None, message: message.into(), detail: None, hint: None, context: None, position: None }
}

pub fn parse_postgres_url(url: &str, name: &str) -> Result<ConnectionDraft, String> {
    postgres::draft_from_url(url, name)
}

impl Session {
    pub fn database_kind(&self) -> DatabaseKind {
        match &self.driver {
            Driver::Postgres(_) => DatabaseKind::Postgres,
            Driver::SqlServer(_) => DatabaseKind::SqlServer,
        }
    }

    pub async fn open(p: &ConnectParams, schema: Option<&str>, timezone: &str) -> Result<Self, ConnectError> {
        match p.database_kind {
            DatabaseKind::Postgres => {
                let driver = postgres::PgSession::open(p, schema, timezone).await?;
                Ok(Self { backend_pid: driver.backend_pid, server: driver.server.clone(), search_path: driver.search_path.clone(), driver: Driver::Postgres(driver) })
            }
            DatabaseKind::SqlServer => {
                let driver = mssql::MsSession::open(p).await?;
                Ok(Self { backend_pid: driver.backend_pid, server: driver.server.clone(), search_path: schema.map(str::to_owned), driver: Driver::SqlServer(driver) })
            }
        }
    }

    pub async fn open_assistant(p: &ConnectParams, timezone: &str) -> Result<Self, ConnectError> {
        if p.database_kind != DatabaseKind::Postgres {
            return Err(ConnectError::Server(error("SQL Server model queries are disabled because server-enforced read-only transactions are unavailable.")));
        }
        let mut session = Self::open(p, None, timezone).await?;
        session.batch("SET application_name = 'L1 Data assistant'; SET default_transaction_read_only = on").await.map_err(ConnectError::Server)?;
        Ok(session)
    }

    pub fn txn_depth(&self) -> Option<u32> {
        match &self.driver {
            Driver::Postgres(_) => None,
            Driver::SqlServer(s) => s.txn().1,
        }
    }
    pub fn observed_txn(&self) -> Option<TxnState> {
        match &self.driver {
            Driver::Postgres(_) => None,
            Driver::SqlServer(s) => Some(s.txn().0),
        }
    }
    pub fn transaction_loss(&self, txn: TxnState) -> TransactionLoss {
        match &self.driver {
            Driver::SqlServer(_) => TransactionLoss::Unknown,
            Driver::Postgres(_) if txn != TxnState::Idle => TransactionLoss::RolledBack,
            Driver::Postgres(_) => TransactionLoss::None,
        }
    }
    pub fn cancel_handle(&self) -> CancelHandle {
        match &self.driver {
            Driver::Postgres(p) => CancelHandle(CancelDriver::Postgres(p.cancel_handle())),
            Driver::SqlServer(s) => CancelHandle(CancelDriver::SqlServer(s.cancel_handle())),
        }
    }
    pub fn closed_reason(&self) -> Option<String> {
        match &self.driver {
            Driver::Postgres(p) => p.closed_reason(),
            Driver::SqlServer(s) => s.closed_reason(),
        }
    }
    pub fn closed_watch(&self) -> watch::Receiver<Option<String>> {
        match &self.driver {
            Driver::Postgres(p) => p.closed_watch(),
            Driver::SqlServer(s) => s.closed_watch(),
        }
    }
    pub async fn closed_reason_settled(&mut self, fatal: Option<String>) -> Option<String> {
        match &mut self.driver {
            Driver::Postgres(p) => p.closed_reason_settled(fatal).await,
            Driver::SqlServer(s) => s.closed_reason().or(fatal),
        }
    }
    pub async fn set_search_path(&mut self, schema: Option<&str>) -> Result<(), QueryError> {
        match &mut self.driver {
            Driver::Postgres(p) => p.set_search_path(schema).await.map_err(|e| postgres::query_failure(e, None))?,
            Driver::SqlServer(_) => {}
        }
        self.search_path = schema.map(str::to_owned);
        Ok(())
    }
    pub fn drain_notices(&mut self) -> Vec<OutputBody> {
        match &mut self.driver {
            Driver::Postgres(p) => p.drain_notices().iter().map(postgres::notice).collect(),
            Driver::SqlServer(_) => vec![],
        }
    }
    pub async fn batch(&mut self, sql: &str) -> Result<(), QueryError> {
        match &mut self.driver {
            Driver::Postgres(p) => p.batch(sql).await,
            Driver::SqlServer(s) => s.batch(sql).await,
        }
    }
    pub async fn execute_edit(&mut self, edit: &Edit) -> Result<u64, QueryError> {
        match &mut self.driver {
            Driver::Postgres(p) => p.execute_edit(&edit.sql).await,
            Driver::SqlServer(_) => Err(error(SQL_SERVER_EDITS_UNSUPPORTED)),
        }
    }
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryStream<'a> {
        match &mut self.driver {
            Driver::Postgres(p) => p.query(sql),
            Driver::SqlServer(s) => s.query(sql),
        }
    }
    pub async fn list_databases(&mut self) -> Result<(Vec<DatabaseInfo>, String), QueryError> {
        match &mut self.driver {
            Driver::Postgres(p) => p.list_databases().await,
            Driver::SqlServer(s) => {
                let (databases, _) = s.list_databases().await?;
                let rows = s.rows("SELECT DB_NAME()").await?;
                let home = rows.first().and_then(|row| row.first()).and_then(Clone::clone).ok_or_else(|| error("SQL Server returned no database name"))?;
                Ok((databases, home))
            }
        }
    }
    pub async fn count_schemas(&mut self) -> Result<u32, QueryError> {
        match &mut self.driver {
            Driver::Postgres(p) => p.count_schemas().await,
            Driver::SqlServer(s) => s.count_schemas().await,
        }
    }
    pub async fn catalog(&mut self, db: DatabaseRef, version: u64, extra: &[String]) -> Result<Catalog, QueryError> {
        match &mut self.driver {
            Driver::Postgres(p) => p.catalog(db, version, extra).await,
            Driver::SqlServer(s) => s.catalog(db, version, extra).await,
        }
    }
    pub async fn detail(&mut self, db: DatabaseRef, oid: u32, version: u64) -> Result<Option<RelationDetail>, QueryError> {
        match &mut self.driver {
            Driver::Postgres(p) => p.detail(db, oid, version).await,
            Driver::SqlServer(s) => s.detail(db, oid, version).await,
        }
    }
    pub async fn count(&mut self, sql: &str) -> Result<u64, QueryError> {
        match &mut self.driver {
            Driver::Postgres(p) => p.count(sql).await,
            Driver::SqlServer(s) => s.count(sql).await,
        }
    }
    pub async fn has_writes(&mut self) -> bool {
        match &mut self.driver {
            Driver::Postgres(p) => p.has_writes().await,
            Driver::SqlServer(s) => s.txn().0 != TxnState::Idle,
        }
    }
    pub async fn finish_transaction(&mut self, commit: bool) -> Result<(), QueryError> {
        match &mut self.driver {
            Driver::Postgres(p) => p.batch(if commit { "COMMIT" } else { "ROLLBACK" }).await,
            Driver::SqlServer(s) => s.finish_transaction(commit).await,
        }
    }
}

pub struct Edit {
    pub change: ChangeRef,
    pub sql: String,
}

pub fn plan_edits(changes: &TableChanges, relation: &Relation) -> Result<Vec<Edit>, ApplyError> {
    let valid = |cells: &[CellWrite], change| -> Result<(), ApplyError> {
        let mut names = std::collections::HashSet::new();
        for cell in cells {
            if !names.insert(&cell.column) || !relation.columns.iter().any(|c| c.name == cell.column && c.writable) {
                return Err(ApplyError::Failed { change, error: error(format!("Column {} is unknown, repeated, or server-owned", cell.column)) });
            }
        }
        Ok(())
    };
    for (i, cells) in changes.inserts.iter().enumerate() {
        valid(cells, ChangeRef { kind: ChangeKind::Insert, index: i as u32 })?;
    }
    for (i, update) in changes.updates.iter().enumerate() {
        valid(&update.set, ChangeRef { kind: ChangeKind::Update, index: i as u32 })?;
    }
    let mut edits = Vec::new();
    let (schema, table) = (&changes.schema, &changes.table);
    let mut add = |kind, index, sql| edits.push(Edit { change: ChangeRef { kind, index }, sql });
    for (i, key) in changes.deletes.iter().enumerate() {
        add(ChangeKind::Delete, i as u32, crate::text::delete_sql(schema, table, key));
    }
    for (i, u) in changes.updates.iter().enumerate() {
        if !u.set.is_empty() {
            add(ChangeKind::Update, i as u32, crate::text::update_sql(schema, table, &u.key, &u.set));
        }
    }
    for (i, cells) in changes.inserts.iter().enumerate() {
        add(ChangeKind::Insert, i as u32, crate::text::insert_sql(schema, table, cells));
    }
    Ok(edits)
}

impl Session {
    pub fn retire(&mut self, reason: String) {
        match &mut self.driver {
            Driver::Postgres(p) => p.retire(reason),
            Driver::SqlServer(s) => s.retire(reason),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_driver_types_stay_inside_the_private_drivers() {
        fn check(path: &std::path::Path) {
            for entry in std::fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    if path.file_name().unwrap() != "session" {
                        check(&path);
                    }
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let source = std::fs::read_to_string(&path).unwrap();
                    let production = source.split("#[cfg(test)]").next().unwrap();
                    for native in ["tokio_postgres::", "tiberius::"] {
                        assert!(!production.contains(native), "{} names {native}; put native types in a private session driver", path.display());
                    }
                }
            }
        }
        check(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
    }
}
