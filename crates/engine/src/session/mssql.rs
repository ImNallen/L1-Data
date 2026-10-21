mod catalog;
mod detail;
pub(super) mod edits;
mod values;

use super::{QueryEvent, QueryStream, error};
use crate::connection::{CONNECT_TIMEOUT, ConnectParams};
use crate::model::*;
use futures_util::{StreamExt, TryStreamExt};
use std::time::Duration;
use tiberius::{AuthMethod, Client, Config, EncryptionLevel, QueryItem};
use tokio::net::TcpStream;
use tokio::sync::watch;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

type TdsClient = Client<Compat<TcpStream>>;
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const ATTENTION_TIMEOUT: Duration = Duration::from_millis(300);

pub(super) struct MsSession {
    client: Option<TdsClient>,
    pub backend_pid: i32,
    pub server: ServerInfo,
    database: String,
    pub depth: u32,
    pub txn: TxnState,
    cancel: watch::Sender<u64>,
    closed: watch::Sender<Option<String>>,
}

#[derive(Clone)]
pub(super) struct MsCancel(watch::Sender<u64>);
impl MsCancel {
    pub async fn cancel(&self) {
        self.0.send_modify(|revision| *revision = revision.wrapping_add(1));
    }
}

struct Lease<'a> {
    owner: &'a mut MsSession,
    client: Option<TdsClient>,
}
impl Lease<'_> {
    fn client(&mut self) -> &mut TdsClient {
        self.client.as_mut().expect("lease owns the client")
    }
    fn restore(mut self, depth: u32, txn: TxnState) {
        self.owner.depth = depth;
        self.owner.txn = txn;
        self.owner.client = self.client.take();
    }
    fn retire(&mut self, reason: String) {
        self.owner.closed.send_replace(Some(reason));
        self.client.take();
    }
}
impl Drop for Lease<'_> {
    fn drop(&mut self) {
        if let Some(mut client) = self.client.take() {
            self.owner.closed.send_replace(Some("SQL Server operation was abandoned; the session and any open transaction were discarded.".into()));
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    let _ = tokio::time::timeout(ATTENTION_TIMEOUT, client.cancel_query()).await;
                });
            }
        }
    }
}

pub(super) fn query_error(e: tiberius::error::Error) -> QueryError {
    match e {
        tiberius::error::Error::Server(e) => QueryError {
            severity: if e.class() >= 20 { "FATAL" } else { "ERROR" }.into(),
            sqlstate: None,
            message: e.message().to_owned(),
            detail: Some(format!("SQL Server error {}, state {}, line {}", e.code(), e.state(), e.line())),
            hint: None,
            context: (!e.procedure().is_empty()).then(|| e.procedure().to_owned()),
            position: None,
        },
        e => QueryError { severity: "FATAL".into(), ..error(e.to_string()) },
    }
}

fn config(p: &ConnectParams) -> Result<Config, ConnectError> {
    let mut config = Config::new();
    config.host(&p.host);
    config.port(p.port);
    config.database(&p.database);
    config.application_name("L1 Data");
    config.command_timeout(None);
    config.authentication(AuthMethod::sql_server(&p.user, p.password.as_deref().unwrap_or("")));
    config.encryption(EncryptionLevel::Required);
    match (p.ssl_mode, &p.ca_cert_path) {
        (SslMode::Require, None) => config.trust_cert(),
        (SslMode::VerifyFull, None) => {}
        (SslMode::VerifyFull, Some(_)) => {
            return Err(ConnectError::Tls { message: "Custom SQL Server CA files are not supported by this adapter yet. Use system-trusted certificates with VerifyFull.".into() });
        }
        _ => {
            return Err(ConnectError::Tls {
                message:
                    "SQL Server supports VerifyFull, or Require for encrypted connections without certificate verification. Other SSL modes and CA combinations are unsupported."
                        .into(),
            });
        }
    }
    Ok(config)
}

impl MsSession {
    pub(super) async fn open(p: &ConnectParams) -> Result<Self, ConnectError> {
        let cfg = config(p)?;
        let connection = async {
            let tcp = TcpStream::connect((p.host.as_str(), p.port)).await.map_err(|e| ConnectError::Network { message: e.to_string() })?;
            tcp.set_nodelay(true).map_err(|e| ConnectError::Network { message: e.to_string() })?;
            Client::connect(cfg, tcp.compat_write()).await.map_err(|e| match e {
                tiberius::error::Error::Tls(message) => ConnectError::Tls { message },
                tiberius::error::Error::Server(e) if e.code() == 18456 => ConnectError::Auth { message: e.message().to_owned() },
                e => ConnectError::Server(query_error(e)),
            })
        };
        let mut client = tokio::time::timeout(CONNECT_TIMEOUT, connection).await.map_err(|_| ConnectError::Network { message: "SQL Server connection timed out".into() })??;
        let initialize = async {
            let mut stream = native_query(&mut client, "SELECT CONVERT(int, @@SPID), CONVERT(nvarchar(128), SERVERPROPERTY('ProductVersion')), DB_NAME()", &[]);
            let mut rows = Vec::new();
            while let Some(event) = stream.next().await {
                if let QueryEvent::Row(row) = event? {
                    rows.push(row);
                }
            }
            let row = rows.first().ok_or_else(|| error("SQL Server returned no server information"))?;
            let backend_pid = number(row, 0)?;
            let version = required(row, 1)?.to_owned();
            let database = required(row, 2)?.to_owned();
            Ok::<_, QueryError>((backend_pid, version, database))
        };
        let (backend_pid, version, database) = tokio::time::timeout(CONNECT_TIMEOUT, initialize)
            .await
            .map_err(|_| ConnectError::Network { message: "SQL Server initialization timed out".into() })?
            .map_err(ConnectError::Server)?;
        let version_num = version.split('.').next().and_then(|n| n.parse::<u32>().ok()).unwrap_or(0) * 10000;
        let (cancel, _) = watch::channel(0);
        let (closed, _) = watch::channel(None);
        Ok(Self {
            client: Some(client),
            backend_pid,
            server: ServerInfo { version, version_num, encoding: "Unicode".into() },
            database,
            depth: 0,
            txn: TxnState::Idle,
            cancel,
            closed,
        })
    }
    pub(super) fn cancel_handle(&self) -> MsCancel {
        MsCancel(self.cancel.clone())
    }
    pub(super) fn closed_reason(&self) -> Option<String> {
        self.closed.borrow().clone().or_else(|| (*self.cancel.borrow() != 0).then(|| "Cancelled SQL Server session was retired; earlier commits remain committed; the last transaction outcome may be unknown.".into()))
    }
    pub(super) fn closed_watch(&self) -> watch::Receiver<Option<String>> {
        self.closed.subscribe()
    }

    pub(super) fn query<'a>(&'a mut self, sql: &'a str) -> QueryStream<'a> {
        self.query_parameters(sql, &[])
    }

    fn query_parameters<'a>(&'a mut self, sql: &'a str, parameters: &'a [Option<String>]) -> QueryStream<'a> {
        Box::pin(async_stream::stream! {
            let Some(client) = self.client.take() else { yield Err(error(self.closed_reason().unwrap_or_else(|| "SQL Server session is retired".into()))); return; };
            let database = self.database.clone();
            let mut cancel = self.cancel.subscribe();
            let mut lease = Lease { owner: self, client: Some(client) };
            if *cancel.borrow() != 0 {
                lease.retire("Cancelled before SQL Server execution. The session was retired.".into());
                yield Err(error("Cancelled"));
                return;
            }
            let mut cancelled = false;
            let mut failed = None;
            {
                let events = native_query(lease.client(), sql, parameters);
                let mut events = std::pin::pin!(events);
                loop {
                    let next = tokio::select! {
                        biased;
                        _ = cancel.changed() => { cancelled = true; break; },
                        next = events.next() => next,
                    };
                    match next {
                        Some(Ok(event)) => yield Ok(event),
                        Some(Err(e)) => { failed = Some(e); break; }
                        None => break,
                    }
                }
            }
            if cancelled {
                let _ = tokio::time::timeout(ATTENTION_TIMEOUT, lease.client().cancel_query()).await;
                lease.retire("Cancelled SQL Server operation. The session was retired and earlier commits remain committed; the last transaction outcome may be unknown.".into());
                yield Err(error("Cancelled"));
                return;
            }
            if failed.as_ref().is_some_and(|e| e.severity == "FATAL") {
                let e = failed.unwrap();
                lease.retire(e.message.clone());
                yield Err(e);
                return;
            }
            let observed = tokio::select! {
                _ = cancel.changed() => Err(error("Cancelled during SQL Server transaction-state probe")),
                result = tokio::time::timeout(PROBE_TIMEOUT, probe(lease.client(), &database)) => result.unwrap_or_else(|_| Err(error("SQL Server transaction-state probe timed out"))),
            };
            match observed {
                Ok((depth, txn)) => lease.restore(depth, txn),
                Err(e) => {
                    let _ = tokio::time::timeout(ATTENTION_TIMEOUT, lease.client().cancel_query()).await;
                    lease.retire(e.message.clone());
                    if failed.is_none() { failed = Some(e); }
                },
            }
            if let Some(e) = failed { yield Err(e); }
            else { yield Ok(QueryEvent::Complete { affected_rows: None }); }
        })
    }

    pub(super) async fn rows(&mut self, sql: &str) -> Result<Vec<Vec<Option<String>>>, QueryError> {
        let mut stream = self.query(sql);
        let mut rows = Vec::new();
        while let Some(event) = stream.next().await {
            if let QueryEvent::Row(row) = event? {
                rows.push(row);
            }
        }
        Ok(rows)
    }
    pub(super) async fn batch(&mut self, sql: &str) -> Result<(), QueryError> {
        let sql = if sql == "BEGIN" { "BEGIN TRANSACTION" } else { sql };
        let mut stream = self.query(sql);
        while let Some(event) = stream.next().await {
            event?;
        }
        Ok(())
    }
    pub(super) async fn finish_transaction(&mut self, commit: bool) -> Result<(), QueryError> {
        self.batch(if commit { "WHILE @@TRANCOUNT > 0 COMMIT TRANSACTION" } else { "IF @@TRANCOUNT > 0 ROLLBACK TRANSACTION" }).await
    }
    pub(super) async fn count(&mut self, sql: &str) -> Result<u64, QueryError> {
        let rows = self.rows(sql).await?;
        number(rows.first().ok_or_else(|| error("Count returned no row"))?, 0)
    }
}

fn native_query<'a>(client: &'a mut TdsClient, sql: &'a str, parameters: &'a [Option<String>]) -> QueryStream<'a> {
    Box::pin(async_stream::try_stream! {
        let mut query = tiberius::Query::new(sql);
        for parameter in parameters { query.bind(parameter.clone()); }
        let stream = if parameters.is_empty() { client.simple_query(sql).await } else { query.query(client).await }.map_err(query_error)?;
        let mut stream = std::pin::pin!(stream);
        let mut has_rows = false;
        while let Some(item) = stream.try_next().await.map_err(query_error)? {
            match item {
                QueryItem::Metadata(meta) => {
                    if has_rows { yield QueryEvent::Complete { affected_rows: None }; }
                    let columns = meta.columns().iter().map(values::metadata).collect::<Result<Vec<_>, _>>()?;
                    has_rows = true;
                    yield QueryEvent::Columns(columns);
                }
                QueryItem::Row(row) => {
                    let cells = row.cells().map(|(_, data)| values::text(data)).collect::<Result<Vec<_>, _>>()?;
                    yield QueryEvent::Row(cells);
                }
            }
        }
    })
}

async fn probe(client: &mut TdsClient, database: &str) -> Result<(u32, TxnState), QueryError> {
    let row = client
        .simple_query("SELECT @@TRANCOUNT, XACT_STATE(), DB_NAME()")
        .await
        .map_err(query_error)?
        .into_row()
        .await
        .map_err(query_error)?
        .ok_or_else(|| error("Transaction-state probe returned no row"))?;
    let depth = row.try_get::<i32, _>(0).map_err(query_error)?.ok_or_else(|| error("Missing transaction depth"))?;
    let depth = u32::try_from(depth).map_err(|_| error("Invalid transaction depth"))?;
    let actual = row.try_get::<&str, _>(2).map_err(query_error)?.ok_or_else(|| error("Missing execution database"))?;
    if actual != database {
        return Err(error(format!("Execution database changed to {actual}. The session was retired; select a database through the tab target.")));
    }
    if depth == 0 {
        return Ok((0, TxnState::Idle));
    }
    let state = match row.try_get::<i32, _>(1).map_err(query_error)? {
        Some(1) => TxnState::Active,
        Some(-1) => TxnState::Aborted,
        _ => return Err(error("SQL Server returned an inconsistent transaction state")),
    };
    Ok((depth, state))
}

fn required(row: &[Option<String>], i: usize) -> Result<&str, QueryError> {
    row.get(i).and_then(Option::as_deref).ok_or_else(|| error(format!("Missing SQL Server metadata field {i}")))
}
fn number<T: std::str::FromStr>(row: &[Option<String>], i: usize) -> Result<T, QueryError> {
    required(row, i)?.parse().map_err(|_| error(format!("Invalid SQL Server metadata field {i}")))
}

fn object_id(row: &[Option<String>], i: usize) -> Result<u32, QueryError> {
    Ok(u32::from_be_bytes(number::<i32>(row, i)?.to_be_bytes()))
}

fn native_object_id(opaque: u32) -> i32 {
    i32::from_be_bytes(opaque.to_be_bytes())
}

impl MsSession {
    pub(super) fn retire(&mut self, reason: String) {
        self.closed.send_replace(Some(reason));
        self.client.take();
    }
}

#[cfg(all(test, feature = "mssql-tests"))]
mod tests {
    use super::*;

    fn params() -> ConnectParams {
        ConnectParams {
            database_kind: DatabaseKind::SqlServer,
            host: std::env::var("L1_DATA_TEST_MSSQL_HOST").unwrap_or_else(|_| "localhost".into()),
            port: std::env::var("L1_DATA_TEST_MSSQL_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(1434),
            database: "shop".into(),
            user: "sa".into(),
            password: Some(std::env::var("L1_DATA_TEST_MSSQL_PASSWORD").unwrap_or_else(|_| "L1Data_dev_Str0ng".into())),
            ssl_mode: SslMode::Require,
            ca_cert_path: None,
        }
    }

    #[tokio::test]
    async fn a_canonicalized_login_still_retires_a_session_that_changes_database() {
        let mut p = params();
        p.database = "SHOP".into();
        let mut session = MsSession::open(&p).await.unwrap();
        assert_eq!(session.database, "shop");
        let error = session.rows("USE reporting; SELECT DB_NAME()").await.unwrap_err();
        assert!(error.message.contains("Execution database changed to reporting"), "{}", error.message);
        assert!(session.closed_reason().is_some());
    }

    async fn stopped(observer: &mut MsSession, pid: i32) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let count = observer.count(&format!("SELECT COUNT_BIG(*) FROM sys.dm_exec_requests WHERE session_id={pid}")).await.unwrap();
                if count == 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("Abandoned server request did not stop");
    }

    #[tokio::test]
    async fn dropped_streams_and_metadata_futures_retire_the_client_and_stop_native_work() {
        let mut observer = MsSession::open(&params()).await.unwrap();
        let mut session = MsSession::open(&params()).await.unwrap();
        let pid = session.backend_pid;
        {
            let mut stream = session.query("SELECT 1; RAISERROR('flush',0,1) WITH NOWAIT; WAITFOR DELAY '00:00:30'; SELECT 2");
            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    if matches!(stream.next().await.unwrap().unwrap(), QueryEvent::Row(_)) {
                        break;
                    }
                }
            })
            .await
            .expect("First row did not stream before WAITFOR");
        }
        assert!(session.closed_reason().is_some());
        assert!(session.client.is_none());
        stopped(&mut observer, pid).await;
        assert!(session.rows("SELECT 99").await.is_err());

        let mut session = MsSession::open(&params()).await.unwrap();
        let pid = session.backend_pid;
        assert!(tokio::time::timeout(Duration::from_millis(100), session.rows("WAITFOR DELAY '00:00:30'; SELECT 1")).await.is_err());
        assert!(session.client.is_none());
        assert!(session.closed_reason().is_some());
        stopped(&mut observer, pid).await;
    }

    #[tokio::test]
    async fn cancel_before_the_lazy_query_subscribes_is_sticky() {
        let mut session = MsSession::open(&params()).await.unwrap();
        let cancel = session.cancel_handle();
        let mut query = session.query("WAITFOR DELAY '00:00:30'; SELECT 1");
        cancel.cancel().await;
        let event = tokio::time::timeout(Duration::from_secs(1), query.next()).await.unwrap().unwrap();
        assert!(event.is_err());
        drop(query);
        assert!(session.client.is_none());
        assert!(session.closed_reason().is_some());
    }

    #[tokio::test]
    async fn changed_database_is_retired_and_unrepresentable_tls_settings_are_rejected() {
        let mut session = MsSession::open(&params()).await.unwrap();
        assert!(session.rows("USE reporting").await.is_err());
        assert!(session.client.is_none());
        assert!(session.closed_reason().unwrap().contains("Execution database changed"));
        let mut p = params();
        p.ssl_mode = SslMode::Disable;
        assert!(matches!(MsSession::open(&p).await, Err(ConnectError::Tls { .. })));
        p.ssl_mode = SslMode::VerifyFull;
        p.ca_cert_path = Some("not-a-real-ca.pem".into());
        assert!(matches!(MsSession::open(&p).await, Err(ConnectError::Tls { .. })));
    }
}
