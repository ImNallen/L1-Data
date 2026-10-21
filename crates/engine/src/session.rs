mod postgres;

use std::pin::Pin;
use futures_util::Stream;
use tokio::sync::watch;
use crate::connection::{ConnectParams, DatabaseInfo};
use crate::model::*;
use crate::sql::StmtKind;

pub struct Session {
    driver: Driver,
    pub backend_pid: i32,
    pub server: ServerInfo,
    pub search_path: Option<String>,
}

enum Driver {
    Postgres(postgres::PgSession),
}

#[derive(Clone)]
pub struct CancelHandle(postgres::PgCancel);
impl CancelHandle {
    pub async fn cancel(&self) { self.0.cancel().await; }
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

pub fn parse_postgres_url(url: &str, name: &str) -> Result<ConnectionDraft, String> { postgres::draft_from_url(url, name) }

impl Session {
    pub async fn open(p: &ConnectParams, schema: Option<&str>, timezone: &str) -> Result<Self, ConnectError> {
        match p.database_kind {
            DatabaseKind::Postgres => {
                let driver = postgres::PgSession::open(p, schema, timezone).await?;
                Ok(Self { backend_pid: driver.backend_pid, server: driver.server.clone(), search_path: driver.search_path.clone(), driver: Driver::Postgres(driver) })
            }
            DatabaseKind::SqlServer => Err(ConnectError::Network { message: "SQL Server driver is not connected yet".into() }),
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

    pub fn txn_depth(&self) -> Option<u32> { None }
    pub fn observed_txn(&self) -> Option<TxnState> { None }
    pub fn cancel_handle(&self) -> CancelHandle { match &self.driver { Driver::Postgres(p) => CancelHandle(p.cancel_handle()) } }
    pub fn closed_reason(&self) -> Option<String> { match &self.driver { Driver::Postgres(p) => p.closed_reason() } }
    pub fn closed_watch(&self) -> watch::Receiver<Option<String>> { match &self.driver { Driver::Postgres(p) => p.closed_watch() } }
    pub async fn closed_reason_settled(&mut self, fatal: Option<String>) -> Option<String> { match &mut self.driver { Driver::Postgres(p) => p.closed_reason_settled(fatal).await } }
    pub async fn set_search_path(&mut self, schema: Option<&str>) -> Result<(), QueryError> {
        match &mut self.driver { Driver::Postgres(p) => p.set_search_path(schema).await.map_err(|e| postgres::query_failure(e, None))? }
        self.search_path = schema.map(str::to_owned);
        Ok(())
    }
    pub fn drain_notices(&mut self) -> Vec<OutputBody> { match &mut self.driver { Driver::Postgres(p) => p.drain_notices().iter().map(postgres::notice).collect() } }
    pub async fn batch(&mut self, sql: &str) -> Result<(), QueryError> { match &mut self.driver { Driver::Postgres(p) => p.batch(sql).await } }
    pub async fn execute_edit(&mut self, sql: &str) -> Result<u64, QueryError> { match &mut self.driver { Driver::Postgres(p) => p.execute_edit(sql).await } }
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryStream<'a> { match &mut self.driver { Driver::Postgres(p) => p.query(sql) } }
    pub async fn list_databases(&mut self) -> Result<(Vec<DatabaseInfo>, String), QueryError> { match &mut self.driver { Driver::Postgres(p) => p.list_databases().await } }
    pub async fn count_schemas(&mut self) -> Result<u32, QueryError> { match &mut self.driver { Driver::Postgres(p) => p.count_schemas().await } }
    pub async fn catalog(&mut self, db: DatabaseRef, version: u64, extra: &[String]) -> Result<Catalog, QueryError> { match &mut self.driver { Driver::Postgres(p) => p.catalog(db, version, extra).await } }
    pub async fn detail(&mut self, db: DatabaseRef, oid: u32, version: u64) -> Result<Option<RelationDetail>, QueryError> { match &mut self.driver { Driver::Postgres(p) => p.detail(db, oid, version).await } }
    pub async fn count(&mut self, sql: &str) -> Result<u64, QueryError> { match &mut self.driver { Driver::Postgres(p) => p.count(sql).await } }
    pub async fn has_writes(&mut self) -> bool { match &mut self.driver { Driver::Postgres(p) => p.has_writes().await } }
    pub async fn finish_transaction(&mut self, commit: bool) -> Result<(), QueryError> { self.batch(if commit { "COMMIT" } else { "ROLLBACK" }).await }
    pub fn implicit_begin(&self, mode: TxnMode, txn: TxnState, kind: &StmtKind, _sql: &str) -> bool { crate::txn::needs_implicit_begin(mode, txn, kind) }
}
