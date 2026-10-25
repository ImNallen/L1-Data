#![cfg(feature = "mssql-tests")]
use engine::model::*;
use futures_util::FutureExt;
use std::panic::AssertUnwindSafe;
use engine::{AgentPath, Engine, EngineConfig, MemoryKeyStore};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

struct Harness {
    engine: Arc<Engine>,
    events: broadcast::Receiver<AppEvent>,
    connection: ConnectionId,
    tab: TabId,
    _dir: tempfile::TempDir,
}
impl Harness {
    async fn open(mode: TxnMode) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let engine = Engine::open(EngineConfig { data_dir: dir.path().into(), keys: Arc::new(MemoryKeyStore::default()), agent_path: AgentPath::Fixed("".into()), az_binary: None, user_known_hosts: None, updates: Arc::new(engine::ScriptedTransport::named("off").unwrap()) }).await.unwrap();
        let events = engine.subscribe();
        let config = engine
            .save_connection(ConnectionDraft {
                database_kind: DatabaseKind::SqlServer,
                id: None,
                name: "SQL Server test".into(),
                host: std::env::var("L1_DATA_TEST_MSSQL_HOST").unwrap_or_else(|_| "localhost".into()),
                port: std::env::var("L1_DATA_TEST_MSSQL_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(1434),
                database: "shop".into(),
                user: "sa".into(),
                password: PasswordInput::Set(std::env::var("L1_DATA_TEST_MSSQL_PASSWORD").unwrap_or_else(|_| "L1Data_dev_Str0ng".into())),
                ssl_mode: SslMode::Require,
                ca_cert_path: None,
                color: None,
                ssh: None,
                environment: None,
                sql_server_auth: None,
            })
            .await
            .unwrap();
        engine.connect(config.id).await.unwrap();
        let tab = TabId(uuid::Uuid::new_v4());
        engine.console_configure(tab, ConsoleConfig { connection_id: config.id, database: "shop".into(), schema: Some("lab".into()), mode }).await.unwrap();
        Self { engine, events, connection: config.id, tab, _dir: dir }
    }
    fn db(&self) -> DatabaseRef {
        DatabaseRef { connection_id: self.connection, database: "shop".into() }
    }
    async fn start(&self, sql: &str) -> RunAccepted {
        self.engine.console_run(self.tab, RunRequest::Script { text: sql.into(), target: RunTarget::All }, None).await.unwrap()
    }
    async fn finish(&mut self, id: RunId) -> ConsoleState {
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if let AppEvent::ConsoleChanged(state) = self.events.recv().await.unwrap() {
                    if state.tab_id == self.tab && matches!(state.run, RunState::Finished { run_id, .. } if run_id==id) {
                        return state;
                    }
                }
            }
        })
        .await
        .expect("Engine run did not finish")
    }
    async fn run(&mut self, sql: &str) -> ConsoleState {
        let id = self.start(sql).await.run_id;
        self.finish(id).await
    }
    async fn ok(&mut self, sql: &str) -> ConsoleState {
        let state = self.run(sql).await;
        assert!(matches!(state.run, RunState::Finished { outcome: RunOutcome::Ok, .. }), "{sql}: {:?}", state.run);
        state
    }
    async fn logged(&mut self, sql: &str) -> (ConsoleState, Vec<OutputBody>) {
        let id = self.start(sql).await.run_id;
        let (tab, events) = (self.tab, &mut self.events);
        let mut log = Vec::new();
        let state = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                match events.recv().await.unwrap() {
                    AppEvent::ConsoleOutput { tab_id, entries } if tab_id == tab => log.extend(entries.into_iter().filter(|e| e.run_id == id).map(|e| e.body)),
                    AppEvent::ConsoleChanged(state) if state.tab_id == tab && matches!(state.run, RunState::Finished { run_id, .. } if run_id == id) => return state,
                    _ => {}
                }
            }
        })
        .await
        .expect("Engine run did not finish");
        (state, log)
    }
    fn rowsets(state: &ConsoleState) -> Vec<&ResultMeta> {
        let RunState::Finished { statements, .. } = &state.run else { panic!("not finished") };
        statements
            .iter()
            .flat_map(|s| &s.outputs)
            .filter_map(|o| match o {
                RunOutput::Rows { result, .. } => Some(result),
                _ => None,
            })
            .collect()
    }
    fn rows(&self, meta: &ResultMeta) -> Vec<Vec<Option<String>>> {
        self.engine.result_page(meta.r#ref, 0, 100).unwrap().rows
    }
}

#[tokio::test]
async fn real_engine_streams_a_multi_result_batch_and_keeps_zero_row_sets() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    let state =
        h.ok("DECLARE @value int = 42; SELECT @value AS [a]]b], N'世界; GO' AS label; SELECT 9 AS empty WHERE 1=0; SELECT @value + 1 AS answer\nGO\nSELECT DB_NAME() AS db").await;
    let RunState::Finished { statements, .. } = &state.run else { unreachable!() };
    assert_eq!(statements.len(), 2);
    let sets = Harness::rowsets(&state);
    assert_eq!(sets.len(), 4);
    assert_eq!(sets.iter().map(|s| s.r#ref.index).collect::<Vec<_>>(), [0, 1, 2, 3]);
    assert_eq!(h.rows(sets[0]), vec![vec![Some("42".into()), Some("世界; GO".into())]]);
    assert_eq!(sets[0].columns[0].name, "a]b");
    assert!(sets[0].columns.iter().all(|c| c.source.is_none()));
    assert_eq!(sets[1].row_count, 0);
    assert_eq!(h.rows(sets[2])[0][0].as_deref(), Some("43"));
    assert_eq!(h.rows(sets[3])[0][0].as_deref(), Some("shop"));
    assert_eq!((state.txn, state.txn_depth), (TxnState::Idle, Some(0)));
    assert_eq!(sets[0].database_kind, DatabaseKind::SqlServer);
}

fn lines(log: &[OutputBody]) -> Vec<String> {
    log.iter()
        .filter_map(|body| match body {
            OutputBody::Notice { severity, message, .. } => Some(format!("{severity}: {message}")),
            OutputBody::Rows { rows, .. } => Some(format!("rows {rows}")),
            OutputBody::Command { tag, .. } => Some(tag.clone()),
            OutputBody::Error { error, .. } => Some(format!("{}: {}", error.severity, error.message)),
            _ => None,
        })
        .collect()
}

fn summary(state: &ConsoleState) -> Vec<String> {
    let RunState::Finished { statements, .. } = &state.run else { panic!("not finished") };
    statements
        .iter()
        .flat_map(|s| &s.outputs)
        .filter_map(|o| match o {
            RunOutput::Command { tag, .. } => Some(tag.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn print_and_low_severity_raiserror_reach_the_output_log_in_server_order() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    let (_, log) = h.logged("PRINT 'hello'").await;
    assert_eq!(lines(&log), ["NOTICE: hello", "Commands completed successfully"]);

    let (state, log) = h.logged("PRINT '1'; SELECT 2 AS two; PRINT '3';").await;
    assert_eq!(lines(&log), ["NOTICE: 1", "rows 1", "NOTICE: 3"]);
    assert_eq!(Harness::rowsets(&state).len(), 1);

    let (_, log) = h.logged("RAISERROR('informational', 10, 1); RAISERROR('careful', 5, 1); PRINT 'after'").await;
    assert_eq!(lines(&log), ["NOTICE: informational", "WARNING: careful", "NOTICE: after", "Commands completed successfully"]);

    let (state, log) = h.logged("PRINT 'a'; RAISERROR('boom', 16, 1); PRINT 'b'").await;
    assert_eq!(lines(&log), ["NOTICE: a", "ERROR: boom", "NOTICE: b"]);
    assert!(matches!(state.run, RunState::Finished { outcome: RunOutcome::Failed, .. }), "{:?}", state.run);
    let RunState::Finished { statements, .. } = &state.run else { unreachable!() };
    assert!(matches!(&statements[0].status, StmtStatus::Failed { error } if error.message == "boom"), "{:?}", statements[0].status);

    let (_, log) = h.logged("RAISERROR('e1', 16, 1); RAISERROR('e2', 16, 1)").await;
    assert_eq!(lines(&log), ["ERROR: e1", "ERROR: e2"]);

    let (_, log) = h.logged("CREATE TABLE #u (id int PRIMARY KEY); INSERT #u VALUES (1); INSERT #u VALUES (1); INSERT #u VALUES (2); DROP TABLE #u").await;
    let log = lines(&log);
    assert_eq!(log.len(), 4, "{log:?}");
    assert_eq!(log[0], "1 row affected");
    assert!(log[1].starts_with("ERROR: Violation of PRIMARY KEY constraint"), "{log:?}");
    assert_eq!(log[2..], ["NOTICE: The statement has been terminated.", "1 row affected"]);
}

#[tokio::test]
async fn a_fatal_error_in_the_batch_retires_the_session() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    let (state, log) = h.logged("PRINT 'before'; RAISERROR('fatal', 20, 1) WITH LOG; PRINT 'never'").await;
    assert_eq!(lines(&log)[..2], ["NOTICE: before", "FATAL: fatal"]);
    assert!(!lines(&log).contains(&"NOTICE: never".into()), "{log:?}");
    assert!(log.iter().any(|body| matches!(body, OutputBody::Info { message } if message.starts_with("Session reset"))), "{log:?}");
    let RunState::Finished { statements, .. } = &state.run else { unreachable!() };
    assert!(matches!(&statements[0].status, StmtStatus::Failed { error } if error.severity == "FATAL" && error.message == "fatal"), "{:?}", statements[0].status);
    h.ok("SELECT 1 AS one").await;
}

fn rowset_states(state: &ConsoleState) -> Vec<(RowsState, u64)> {
    let RunState::Finished { statements, .. } = &state.run else { panic!("not finished") };
    statements
        .iter()
        .flat_map(|s| &s.outputs)
        .filter_map(|o| match o {
            RunOutput::Rows { result, state } => Some((*state, result.rows_seen)),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn an_error_cuts_off_only_the_result_set_it_ends() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    let (state, log) = h.logged("SELECT x FROM (VALUES (1),(2),(0),(3)) v(x) WHERE 10 / x > 0").await;
    assert_eq!(rowset_states(&state), [(RowsState::Failed, 2)]);
    assert_eq!(lines(&log), ["ERROR: Divide by zero error encountered."]);
    assert!(matches!(state.run, RunState::Finished { outcome: RunOutcome::Failed, .. }), "{:?}", state.run);

    let (state, log) = h.logged("SELECT 1 AS a; SELECT 1/0 AS b; SELECT 3 AS c").await;
    assert_eq!(rowset_states(&state), [(RowsState::Complete, 1), (RowsState::Failed, 0), (RowsState::Complete, 1)]);
    assert_eq!(lines(&log), ["rows 1", "ERROR: Divide by zero error encountered.", "rows 1"]);
}

#[tokio::test]
async fn a_nowait_message_streams_while_the_batch_waits_and_survives_cancel() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    let id = h.start("RAISERROR('early', 0, 1) WITH NOWAIT; WAITFOR DELAY '00:00:30'; PRINT 'never'").await.run_id;
    let (tab, events) = (h.tab, &mut h.events);
    let early = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let AppEvent::ConsoleOutput { tab_id, entries } = events.recv().await.unwrap()
                && tab_id == tab
                && let Some(entry) = entries.into_iter().find(|e| e.run_id == id && matches!(&e.body, OutputBody::Notice { .. }))
            {
                return entry.body;
            }
        }
    })
    .await
    .expect("The NOWAIT message did not arrive while the batch waited");
    assert!(matches!(early, OutputBody::Notice { message, .. } if message == "early"));
    h.engine.console_cancel(h.tab).await;
    let state = h.finish(id).await;
    assert!(matches!(state.run, RunState::Finished { outcome: RunOutcome::Cancelled, .. }), "{:?}", state.run);
}

async fn notices_read_slowly(events: &mut broadcast::Receiver<AppEvent>, until: impl Fn(&AppEvent) -> bool) -> (Vec<String>, u64) {
    let (mut notices, mut lagged) = (Vec::new(), 0);
    loop {
        match events.recv().await {
            Ok(event) if until(&event) => return (notices, lagged),
            Ok(AppEvent::ConsoleOutput { entries, .. }) => {
                notices.extend(entries.into_iter().filter_map(|e| match e.body {
                    OutputBody::Notice { message, .. } => Some(message),
                    _ => None,
                }));
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            Ok(_) => {}
            Err(broadcast::error::RecvError::Lagged(n)) => lagged += n,
            Err(e) => panic!("{e:?}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_nowait_flood_reaches_a_slow_reader_whole() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    let id = h.start("DECLARE @i int = 0; WHILE @i < 20000 BEGIN RAISERROR('%d', 0, 1, @i) WITH NOWAIT; SET @i += 1; END").await.run_id;
    let tab = h.tab;
    let (notices, lagged) = notices_read_slowly(&mut h.events, |event| matches!(event, AppEvent::ConsoleChanged(s) if s.tab_id == tab && matches!(s.run, RunState::Finished { run_id, .. } if run_id == id))).await;
    assert_eq!(lagged, 0);
    assert_eq!(notices.len(), 20_000);
    assert!(notices.iter().enumerate().all(|(i, n)| *n == i.to_string()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_export_sends_the_notices_of_its_rerun_in_batches() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    let state = h.ok("DECLARE @i int = 0; WHILE @i < 5000 BEGIN PRINT @i; SET @i += 1; END; SELECT 1 AS a").await;
    let meta = Harness::rowsets(&state)[0].clone();
    let mut events = h.engine.subscribe();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.csv");
    h.engine.result_export(meta.r#ref, ExportFormat::Csv, ExportScope::All, path.display().to_string()).await.unwrap();
    let (mut notices, mut lagged) = (0, 0);
    loop {
        match events.try_recv() {
            Ok(AppEvent::ConsoleOutput { entries, .. }) => notices += entries.iter().filter(|e| matches!(e.body, OutputBody::Notice { .. })).count(),
            Ok(_) => {}
            Err(broadcast::error::TryRecvError::Lagged(n)) => lagged += n,
            Err(broadcast::error::TryRecvError::Empty) => break,
            Err(e) => panic!("{e:?}"),
        }
    }
    assert_eq!((notices, lagged), (5_000, 0));
}

#[tokio::test]
async fn sql_server_reports_each_statement_count_like_ssms() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    let (state, log) = h
        .logged("CREATE TABLE #counted (id int); DECLARE @n int = 0; INSERT #counted VALUES (1),(2),(3),(4),(5); SET @n = 1; UPDATE #counted SET id = id + 1; SELECT id FROM #counted WHERE id > 4; DELETE #counted WHERE id = 2; DROP TABLE #counted")
        .await;
    assert_eq!(lines(&log), ["5 rows affected", "5 rows affected", "rows 2", "1 row affected"]);
    assert_eq!(summary(&state), ["5 rows affected", "1 row affected"]);

    let (state, log) = h.logged("DECLARE @i int = 0; WHILE @i < 3 BEGIN PRINT @i; SET @i += 1; END").await;
    assert_eq!(lines(&log), ["NOTICE: 0", "NOTICE: 1", "NOTICE: 2", "Commands completed successfully"]);
    assert_eq!(summary(&state), ["Commands completed successfully"]);

    let (state, _) = h.logged("CREATE TABLE #loop (id int); DECLARE @i int = 0; WHILE @i < 50 BEGIN INSERT #loop VALUES (@i); SET @i += 1; END; DROP TABLE #loop").await;
    assert_eq!(summary(&state), ["1 row affected"]);

    let (state, log) = h.logged("SET NOCOUNT ON; CREATE TABLE #quiet (id int); INSERT #quiet VALUES (1); DROP TABLE #quiet; SET NOCOUNT OFF").await;
    assert_eq!(lines(&log), ["Commands completed successfully"]);
    assert_eq!(summary(&state), ["Commands completed successfully"]);
}

#[tokio::test]
async fn catalogs_details_browse_counts_and_exact_text_use_sql_server() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    let catalog = h.engine.catalog(h.db()).await.unwrap();
    assert_eq!(catalog.database_kind, DatabaseKind::SqlServer);
    assert_eq!(catalog.default_schema.as_deref(), Some("dbo"));
    let relation = catalog.schemas.iter().find(|s| s.name == "dbo").unwrap().relations.iter().find(|r| r.name == "orders").unwrap();
    assert_eq!(relation.qualified_name, "[dbo].[orders]");
    assert!(!relation.columns.iter().find(|c| c.name == "id").unwrap().writable);
    let detail = h.engine.relation_detail(h.db(), relation.oid).await.unwrap();
    assert!(detail.keys.iter().any(|k| matches!(k.kind, KeyKind::Primary)));
    assert!(detail.keys.iter().any(|k| matches!(k.kind, KeyKind::Foreign { .. })));
    assert!(detail.keys.iter().any(|k| matches!(k.kind, KeyKind::Check { .. })));
    assert!(detail.defaults.iter().any(|d| d.column == "status"));
    assert!(detail.indexes.iter().any(|i| i.name == "IX_orders_customer"));
    let request = RunRequest::TablePage { schema: "lab".into(), table: "type_zoo".into(), page: 0, query: TableQuery::default() };
    let accepted = h.engine.console_run(h.tab, request, None).await.unwrap();
    let state = h.finish(accepted.run_id).await;
    assert!(matches!(state.run, RunState::Finished { outcome: RunOutcome::Ok, .. }), "{:?}", state.run);
    let sets = Harness::rowsets(&state);
    let result = sets[0];
    assert!(result.columns.iter().all(|c| c.source.is_some()));
    let rows = h.rows(result);
    let cell = |name: &str| {
        let i = result.columns.iter().position(|c| c.name == name).unwrap();
        h.engine.result_cell(result.r#ref, 0, i as u16).unwrap()
    };
    assert_eq!(cell("big").as_deref(), Some("9223372036854775807"));
    assert_eq!(cell("precise").as_deref(), Some("12345678901234567890.123456789012"));
    assert_eq!(cell("money_value").as_deref(), Some("12.3456"));
    assert_eq!(cell("bytes").as_deref(), Some("0x0001FEFF"));
    assert_eq!(cell("datetime_value").as_deref(), Some("2026-10-21 12:34:56.123"));
    assert_eq!(cell("time_value").as_deref(), Some("12:34:56.1234567"));
    assert_eq!(cell("offset_value").as_deref(), Some("2026-10-21 12:34:56.1234567+02:00"));
    assert_eq!(cell("null_value"), None);
    assert_eq!(cell("text_value").unwrap(), "😀".repeat(3000));
    let text_index = result.columns.iter().position(|c| c.name == "text_value").unwrap();
    assert!(rows[0][text_index].as_ref().unwrap().chars().count() < 3000);
    let path = h._dir.path().join("exact.csv");
    h.engine.result_export(result.r#ref, ExportFormat::Csv, ExportScope::Loaded, path.display().to_string()).await.unwrap();
    let csv = std::fs::read_to_string(path).unwrap();
    assert!(csv.contains("12345678901234567890.123456789012"));
    assert!(csv.contains(&"😀".repeat(3000)));
    let query = TableQuery { filters: vec![ColumnFilter { column: "a]b".into(), op: FilterOp::Contains, value: "%_[".into() }], ..Default::default() };
    assert_eq!(h.engine.exact_row_count(h.tab, h.db(), "lab".into(), "awkward names".into(), query).await.unwrap(), 1);
    for injection in ["1=1 DELETE FROM dbo.orders", "1=1; SELECT 2", "1=1 -- ignored"] {
        let query = TableQuery { r#where: injection.into(), ..Default::default() };
        assert!(h.engine.exact_row_count(h.tab, h.db(), "dbo".into(), "orders".into(), query).await.is_err(), "{injection}");
    }
}

#[tokio::test]
async fn raw_money_refuses_without_returning_an_approximation_and_partial_results_survive() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    let state = h.run("SELECT 17 AS before_money; SELECT CAST(922337203685477.5807 AS money) AS unsafe_money").await;
    let RunState::Finished { outcome, statements, .. } = &state.run else { unreachable!() };
    assert_eq!(*outcome, RunOutcome::Failed);
    assert!(matches!(&statements[0].status,StmtStatus::Failed{error} if error.message.contains("decimal(19,4)")));
    assert_eq!(Harness::rowsets(&state).len(), 1);
    assert_eq!(h.rows(Harness::rowsets(&state)[0])[0][0].as_deref(), Some("17"));
    let state = h.ok("SELECT CAST(CAST(922337203685477.5807 AS money) AS decimal(19,4)) AS exact").await;
    assert_eq!(h.rows(Harness::rowsets(&state)[0])[0][0].as_deref(), Some("922337203685477.5807"));
    let state = h.run("SELECT 5 AS partial; RAISERROR('expected failure',16,1)").await;
    assert_eq!(h.rows(Harness::rowsets(&state)[0])[0][0].as_deref(), Some("5"));
    let RunState::Finished { statements, .. } = &state.run else { unreachable!() };
    assert!(matches!(&statements[0].status,StmtStatus::Failed{error} if error.sqlstate.is_none() && error.detail.as_ref().is_some_and(|s|s.contains("50000"))));
}

#[tokio::test]
async fn transactions_are_observed_and_commit_button_finishes_all_nesting() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    h.ok("CREATE TABLE #tx (id int)").await;
    let state = h.ok("BEGIN TRANSACTION; BEGIN TRANSACTION; INSERT INTO #tx VALUES (1)").await;
    assert_eq!((state.txn, state.txn_depth), (TxnState::Open, None));
    let state = h.ok("COMMIT TRANSACTION").await;
    assert_eq!((state.txn, state.txn_depth), (TxnState::Open, None));
    let state = h.run("RAISERROR('recoverable',16,1)").await;
    assert_eq!((state.txn, state.txn_depth), (TxnState::Open, None));
    h.engine.console_rollback(h.tab).await.unwrap();
    let state = h.ok("SELECT COUNT_BIG(*) FROM #tx").await;
    assert_eq!(h.rows(Harness::rowsets(&state)[0])[0][0].as_deref(), Some("0"));
    h.ok("BEGIN TRANSACTION; BEGIN TRANSACTION; INSERT INTO #tx VALUES (2)").await;
    h.engine.console_commit(h.tab, None).await.unwrap();
    let state = h.ok("SELECT @@TRANCOUNT, COUNT_BIG(*) FROM #tx").await;
    assert_eq!(h.rows(Harness::rowsets(&state)[0])[0], vec![Some("0".into()), Some("1".into())]);
    h.ok("SET XACT_ABORT ON; BEGIN TRANSACTION").await;
    let state = h.run("BEGIN TRY SELECT CONVERT(int,N'bad') END TRY BEGIN CATCH SELECT XACT_STATE() END CATCH").await;
    let sets = Harness::rowsets(&state);
    assert_eq!(h.rows(sets.last().unwrap())[0][0].as_deref(), Some("-1"));
    assert!(
        matches!(&state.run, RunState::Finished { outcome: RunOutcome::Failed, statements, .. } if matches!(&statements[0].status, StmtStatus::Failed { error } if error.detail.as_ref().is_some_and(|d| d.contains("3998"))))
    );
    assert_eq!((state.txn, state.txn_depth), (TxnState::Idle, Some(0)));
    let state = h.ok("SELECT @@TRANCOUNT").await;
    assert_eq!(state.txn, TxnState::Idle);
}

#[tokio::test]
async fn every_batch_is_validated_before_any_execution_and_dynamic_use_stays_scoped() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    let rejected = h.engine.console_run(h.tab, RunRequest::Script { text: "CREATE TABLE #must_not_exist(id int)\nGO\nUSE reporting".into(), target: RunTarget::All }, None).await;
    assert!(matches!(rejected, Err(RunError::Unsupported { .. })));
    let state = h.ok("SELECT OBJECT_ID('tempdb..#must_not_exist')").await;
    assert_eq!(h.rows(Harness::rowsets(&state)[0])[0][0], None);
    let state = h.run("EXEC ('USE reporting; SELECT 1')").await;
    // Dynamic EXEC scopes USE locally. The tab must still target shop after it returns.
    assert!(matches!(state.session, SessionState::Open { .. }));
    let state = h.ok("SELECT DB_NAME()").await;
    assert_eq!(h.rows(Harness::rowsets(&state)[0])[0][0].as_deref(), Some("shop"));
}

#[tokio::test]
async fn cancelling_waitfor_retires_the_session_and_stops_the_server_request() {
    for sql in ["WAITFOR DELAY '00:00:30'; SELECT 99", "SELECT 1 AS first; WAITFOR DELAY '00:00:30'; SELECT 99"] {
        let mut h = Harness::open(TxnMode::AutoCommit).await;
        // SQL Server gives a closed session's session_id to later logins; connection_id is never reused.
        let warm = h.ok("BEGIN TRANSACTION; SELECT connection_id FROM sys.dm_exec_connections WHERE session_id = @@SPID").await;
        let connection = h.rows(Harness::rowsets(&warm)[0])[0][0].clone().unwrap();
        let id = h.start(sql).await.run_id;
        tokio::time::sleep(Duration::from_millis(150)).await;
        tokio::time::timeout(Duration::from_secs(1), h.engine.console_cancel(h.tab)).await.unwrap();
        let state = h.finish(id).await;
        assert!(matches!(state.run, RunState::Finished { outcome: RunOutcome::Cancelled, .. }), "{:?}", state.run);
        assert!(matches!(state.session, SessionState::Lost { transaction: TransactionLoss::Unknown, .. }), "{:?}", state.session);
        let state = h.ok(&format!("SELECT COUNT_BIG(*) FROM sys.dm_exec_requests WHERE connection_id = '{connection}'")).await;
        assert_eq!(h.rows(Harness::rowsets(&state)[0])[0][0].as_deref(), Some("0"));
    }
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    let id = h.start("WAITFOR DELAY '00:00:30'").await.run_id;
    h.engine.console_cancel(h.tab).await;
    let state = h.finish(id).await;
    assert!(matches!(state.session, SessionState::Lost { .. }), "{:?}", state.session);
}

#[tokio::test]
async fn native_writes_keep_precision_defaults_trigger_results_and_metadata() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    let table = format!("l1_edit_{}", uuid::Uuid::new_v4().simple());
    let trigger = format!("{table}_trigger");
    h.ok(&format!("CREATE TABLE dbo.[{table}] (id int IDENTITY PRIMARY KEY, [a]]b] nvarchar(200), n decimal(38,12), bytes varbinary(max), price money, state int DEFAULT 7, computed AS (state+1), version rowversion)")).await;
    let outcome = AssertUnwindSafe(async {
        h.ok(&format!("CREATE TRIGGER dbo.[{trigger}] ON dbo.[{table}] AFTER INSERT,UPDATE,DELETE AS BEGIN SET NOCOUNT ON; SELECT 123 AS trigger_row; END")).await;
        h.engine.refresh_catalog(h.db()).await.unwrap();
        let catalog = h.engine.catalog(h.db()).await.unwrap();
        let relation = catalog.schemas.iter().flat_map(|s| &s.relations).find(|r| r.name == table).unwrap();
        for col in ["id", "computed", "version"] {
            assert!(!relation.columns.iter().find(|c| c.name == col).unwrap().writable);
        }
        let detail = h.engine.relation_detail(h.db(), relation.oid).await.unwrap();
        assert!(matches!(&detail.triggers[0].implementation,TriggerImplementation::SqlModule {definition:Some(definition)} if definition.contains("NOCOUNT")));
        h.ok(&format!("INSERT INTO dbo.[{table}] ([a]]b],n,bytes,price) VALUES(N'O''Neil 世界 😀 %_[',12345678901234567890.123456789012,0x0001FEFF,922337203685477.5807)")).await;
        let state = h.ok(&format!("SELECT id,[a]]b],n,bytes,CAST(price AS decimal(19,4)),state,computed FROM dbo.[{table}]")).await;
        let rows = h.rows(Harness::rowsets(&state)[0]);
        assert_eq!(
            rows[0],
            vec![
                Some("1".into()),
                Some("O'Neil 世界 😀 %_[".into()),
                Some("12345678901234567890.123456789012".into()),
                Some("0x0001FEFF".into()),
                Some("922337203685477.5807".into()),
                Some("7".into()),
                Some("8".into())
            ]
        );
        let state = h.ok(&format!("UPDATE dbo.[{table}] SET [a]]b]=N'changed',n=NULL WHERE id=1; SELECT [a]]b],n FROM dbo.[{table}]")).await;
        let sets = Harness::rowsets(&state);
        assert_eq!(sets.len(), 2);
        assert_eq!(h.rows(sets[0]), vec![vec![Some("123".into())]]);
        assert_eq!(h.rows(sets[1]), vec![vec![Some("changed".into()), None]]);
        h.ok(&format!("BEGIN TRANSACTION; UPDATE dbo.[{table}] SET [a]]b]=N'uncommitted' WHERE id=1")).await;
        h.engine.console_rollback(h.tab).await.unwrap();
        let state = h.ok(&format!("SELECT [a]]b] FROM dbo.[{table}]")).await;
        assert_eq!(h.rows(Harness::rowsets(&state)[0])[0][0].as_deref(), Some("changed"));
        h.ok(&format!("DELETE FROM dbo.[{table}] WHERE id=1")).await;
        let state = h.ok(&format!("SELECT COUNT_BIG(*) FROM dbo.[{table}]")).await;
        assert_eq!(h.rows(Harness::rowsets(&state)[0])[0][0].as_deref(), Some("0"));
    }).catch_unwind().await;
    h.ok(&format!("DROP TABLE dbo.[{table}]")).await;
    let state = h.ok(&format!("SELECT COUNT_BIG(*) FROM sys.objects WHERE name IN (N'{table}',N'{trigger}')")).await;
    assert_eq!(h.rows(Harness::rowsets(&state)[0]), vec![vec![Some("0".into())]]);
    if let Err(panic) = outcome { std::panic::resume_unwind(panic); }

}

#[tokio::test]
async fn connection_kind_is_immutable_and_history_records_sql_server() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    let old = h.engine.boot().connections.into_iter().find(|c| c.id == h.connection).unwrap();
    let changed = ConnectionDraft {
        database_kind: DatabaseKind::Postgres,
        id: Some(h.connection),
        name: old.name,
        host: old.host,
        port: 5432,
        database: old.database,
        user: old.user,
        ssl_mode: SslMode::Disable,
        ca_cert_path: None,
        password: PasswordInput::Clear,
        color: None,
        ssh: None,
        environment: None,
        sql_server_auth: None,
    };
    assert!(matches!(h.engine.save_connection(changed).await, Err(SaveError::Invalid { .. })));
    assert!(h.engine.boot().connections.iter().find(|c| c.id == h.connection).unwrap().has_password);
    h.ok("SELECT 1 AS history_example").await;
    let history = h.engine.history(HistoryQuery { text: Some("history_example".into()), connection_id: None, before_id: None, limit: 10 });
    assert_eq!(history.entries[0].database_kind, DatabaseKind::SqlServer);
    let page =
        h.engine.history_browse(HistoryBrowseQuery { text: Some("history_example".into()), filter: HistoryFilter::All, before_id: None, limit: 10, revision: None }).unwrap();
    assert_eq!(page.groups[0].database_kind, DatabaseKind::SqlServer);
    let group = h.engine.history_group(HistoryGroupQuery { id: page.groups[0].id, before_id: None, limit: 10, revision: page.revision }).unwrap();
    assert_eq!(group.database_kind, DatabaseKind::SqlServer);
}

#[tokio::test]
async fn clr_columns_stay_in_the_catalog_and_browse_refuses_unsupported_values() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    let table = format!("l1_clr_{}", uuid::Uuid::new_v4().simple());
    h.ok(&format!("CREATE TABLE dbo.[{table}] (id int PRIMARY KEY, path hierarchyid, location geography); INSERT INTO dbo.[{table}] VALUES (1, hierarchyid::GetRoot(), geography::Point(59.3, 18.1, 4326))")).await;
    h.engine.refresh_catalog(h.db()).await.unwrap();
    let catalog = h.engine.catalog(h.db()).await.unwrap();
    let relation = catalog.schemas.iter().flat_map(|s| &s.relations).find(|r| r.name == table).unwrap();
    assert_eq!(relation.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["id", "path", "location"]);
    for column in &relation.columns[1..] {
        assert_eq!(column.category, TypeCategory::Other);
    }
    assert_eq!(relation.columns[1].type_display, "hierarchyid");
    assert_eq!(relation.columns[2].type_display, "geography");
    let run = h.engine.console_run(h.tab, RunRequest::TablePage { schema: "dbo".into(), table: table.clone(), page: 0, query: TableQuery::default() }, None).await.unwrap();
    let state = h.finish(run.run_id).await;
    assert!(
        matches!(&state.run, RunState::Finished { outcome: RunOutcome::Failed, statements, .. } if matches!(&statements[0].status, StmtStatus::Failed { error } if error.message.contains("CLR"))),
        "{:?}",
        state.run
    );
    assert!(Harness::rowsets(&state).is_empty());
    h.ok(&format!("DROP TABLE dbo.[{table}]")).await;
}

#[tokio::test]
async fn database_catalogs_stay_scoped_and_header_sort_sql_runs_with_bracket_names() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    let shop = h.engine.catalog(h.db()).await.unwrap();
    let reporting_db = DatabaseRef { connection_id: h.connection, database: "reporting".into() };
    h.engine.edit_view(h.connection, ViewEdit::Database { name: "reporting".into(), shown: true }).await.unwrap();
    let reporting = h.engine.catalog(reporting_db.clone()).await.unwrap();
    let customers =
        |catalog: &Catalog| catalog.schemas.iter().flat_map(|s| &s.relations).find(|r| r.name == "customers").unwrap().columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
    assert_eq!(customers(&shop), ["id", "name", "region"]);
    assert_eq!(customers(&reporting), ["id", "label"]);
    let oid = reporting.schemas.iter().flat_map(|s| &s.relations).find(|r| r.name == "customers").unwrap().oid;
    assert_eq!(h.engine.relation_detail(reporting_db.clone(), oid).await.unwrap().db, reporting_db);
    let script = h.engine.table_script(h.db(), "lab", "awkward names").unwrap();
    assert_eq!(script.text, "SELECT * FROM [lab].[awkward names]");
    assert_eq!(script.accepted_texts, ["SELECT * FROM [lab].[awkward names]"]);
    let sorted = h.engine.table_order(h.db(), "lab", "awkward names", "", TableOrderEdit::Cycle { column: "a]b".into(), additive: false }).unwrap();
    assert_eq!(sorted.sql, "[a]]b] ASC");
    let descending = h.engine.table_order(h.db(), "lab", "awkward names", &sorted.sql, TableOrderEdit::Cycle { column: "a]b".into(), additive: false }).unwrap();
    assert_eq!(descending.sql, "[a]]b] DESC");
    let advanced = "COALESCE([a]]b], N'') DESC";
    let inspected = h.engine.table_order(h.db(), "lab", "awkward names", advanced, TableOrderEdit::Cycle { column: "a]b".into(), additive: false }).unwrap();
    assert_eq!(inspected.sql, advanced);
    assert!(inspected.keys.is_none());
    let accepted = h
        .engine
        .console_run(
            h.tab,
            RunRequest::TablePage { schema: "lab".into(), table: "awkward names".into(), page: 0, query: TableQuery { order_by: descending.sql, ..TableQuery::default() } }, None,
        )
        .await
        .unwrap();
    let state = h.finish(accepted.run_id).await;
    assert!(matches!(state.run, RunState::Finished { outcome: RunOutcome::Ok, .. }), "{:?}", state.run);
    let set = Harness::rowsets(&state)[0];
    assert_eq!(h.rows(set)[0][1].as_deref(), Some("100%_[value]"));
    assert_eq!(set.db, h.db());
}

#[tokio::test]
async fn system_schema_views_keep_lossless_opaque_ids_in_catalog_and_detail() {
    let h = Harness::open(TxnMode::AutoCommit).await;
    h.engine.edit_view(h.connection, ViewEdit::ShowSystemSchemas { database: "shop".into(), show: true }).await.unwrap();
    for name in ["sys", "INFORMATION_SCHEMA"] {
        h.engine.edit_view(h.connection, ViewEdit::Schema { database: "shop".into(), name: name.into(), shown: true }).await.unwrap();
    }
    h.engine.refresh_catalog(h.db()).await.unwrap();
    let catalog = h.engine.catalog(h.db()).await.unwrap();
    for name in ["sys", "INFORMATION_SCHEMA"] {
        let schema = catalog.schemas.iter().find(|s| s.name == name).unwrap();
        let view = schema.relations.iter().find(|r| r.oid > i32::MAX as u32 && !r.columns.is_empty()).expect("System views with negative native IDs must load");
        assert_eq!(view.kind, RelKind::View);
        let detail = h.engine.relation_detail(h.db(), view.oid).await.unwrap();
        assert_eq!(detail.oid, view.oid);
        assert_eq!(detail.db, h.db());
    }
}

#[tokio::test]
async fn ddl_committed_before_a_later_batch_error_refreshes_the_catalog() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    h.engine.catalog(h.db()).await.unwrap();
    let table = format!("l1_partial_{}", uuid::Uuid::new_v4().simple());
    let state = h.run(&format!("CREATE TABLE dbo.[{table}] (id int); RAISERROR('after committed DDL',16,1)")).await;
    assert!(matches!(state.run, RunState::Finished { outcome: RunOutcome::Failed, .. }));
    assert_eq!(state.txn, TxnState::Idle);
    let refreshed = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let catalog = h.engine.catalog(h.db()).await.unwrap();
            if catalog.schemas.iter().flat_map(|s| &s.relations).any(|r| r.name == table) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    h.ok(&format!("DROP TABLE dbo.[{table}]")).await;
    assert!(refreshed.is_ok(), "The catalog stayed stale after the batch committed DDL before its error");
}

impl Harness {
    fn console(&self) -> ConsoleState {
        self.engine.boot().consoles.into_iter().find(|c| c.tab_id == self.tab).expect("console")
    }

    async fn export(&self, meta: &ResultMeta, format: ExportFormat, scope: ExportScope, name: &str) -> (ExportSummary, String) {
        let path = self._dir.path().join(name);
        let summary = self.engine.result_export(meta.r#ref, format, scope, path.display().to_string()).await.unwrap();
        (summary, std::fs::read_to_string(path).unwrap())
    }
}

#[tokio::test]
async fn all_rows_exports_the_chosen_result_set_of_a_batch_and_keeps_the_session() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    h.engine.set_settings(Settings { max_rows: 5, ..h.engine.settings() });
    let state = h
        .ok("SELECT N'first' AS head; SELECT TOP (40) ROW_NUMBER() OVER (ORDER BY (SELECT NULL)) AS n FROM sys.all_objects ORDER BY n; SELECT N'last' AS tail")
        .await;
    let sets = Harness::rowsets(&state);
    assert_eq!(sets.len(), 3);
    assert_eq!((sets[1].row_count, sets[1].rows_seen, &sets[1].all_rows), (5, 40, &AllRows::Statement));
    let pid = match state.session { SessionState::Open { backend_pid } => backend_pid, other => panic!("{other:?}") };

    let (summary, csv) = h.export(sets[1], ExportFormat::Csv, ExportScope::All, "middle.csv").await;
    let expected: String = std::iter::once("n".to_owned()).chain((1..=40).map(|n| n.to_string())).map(|l| l + "\r\n").collect();
    assert_eq!((summary.rows, csv), (40, expected), "only the second result set, every row");
    let (_, tail) = h.export(sets[2], ExportFormat::Json, ExportScope::All, "tail.json").await;
    assert_eq!(tail, "[\n  {\"tail\":\"last\"}\n]\n");
    let after = h.console();
    assert_eq!((after.work, after.export, after.txn), (None, None, TxnState::Idle));
    assert!(matches!(after.session, SessionState::Open { backend_pid } if backend_pid == pid), "the export ran on the tab's own session and kept it");
}

#[tokio::test]
async fn all_rows_refuses_a_batch_that_controls_a_transaction() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    let state = h.ok("BEGIN TRANSACTION; SELECT 1 AS x; COMMIT").await;
    let meta = Harness::rowsets(&state)[0].clone();
    assert!(matches!(&meta.all_rows, AllRows::Refused { reason } if reason.contains("transaction")), "{:?}", meta.all_rows);
    let path = h._dir.path().join("never.csv");
    assert!(matches!(h.engine.result_export(meta.r#ref, ExportFormat::Csv, ExportScope::All, path.display().to_string()).await, Err(ExportError::NotRerunnable)));
    assert!(!path.exists());
}

#[tokio::test]
async fn sql_server_inserts_round_trip_every_value_under_another_dateformat() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    let source = format!("l1_export_src_{}", uuid::Uuid::new_v4().simple());
    let copy = format!("l1_export_copy_{}", uuid::Uuid::new_v4().simple());
    let columns = "id int, big bigint, exact decimal(38,12), flag bit, label nvarchar(50), ascii varchar(20), old datetime, small smalldatetime, day date, \
                   tick time(7), modern datetime2(7), moment datetimeoffset(7), uid uniqueidentifier, bytes varbinary(16), ratio float, single real";
    h.ok(&format!("CREATE TABLE dbo.[{source}] ({columns}); CREATE TABLE dbo.[{copy}] ({columns})")).await;
    let outcome = AssertUnwindSafe(async {
        h.ok(&format!(
            "INSERT INTO dbo.[{source}] VALUES \
             (1, 9223372036854775807, 12345678901234567890123456.789012345678, 1, N'O''Brien 😀 | x', 'plain', '2024-01-02T03:04:05.123', '2024-01-02T03:04:00', '2024-01-02', \
              '03:04:05.1234567', '2024-01-02T03:04:05.1234567', '2024-01-02T03:04:05.1234567+02:00', 'abcd1234-5678-90ab-cdef-1234567890ab', 0x0001FEFF, 1e300, 1.5), \
             (2, -1, -0.000000000001, 0, N'', '', NULL, NULL, NULL, NULL, NULL, NULL, NULL, 0x, -2.5e-300, NULL), \
             (3, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL)"
        ))
        .await;
        let state = h.ok(&format!("SELECT * FROM dbo.[{source}] ORDER BY id")).await;
        let meta = Harness::rowsets(&state)[0].clone();
        let (summary, script) = h.export(&meta, ExportFormat::SqlInsert, ExportScope::All, "values.sql").await;
        assert_eq!(summary.rows, 3);
        assert!(script.starts_with("INSERT INTO target_table ([id], [big], "), "{script}");
        h.ok("SET DATEFORMAT dmy").await;
        h.ok(&script.replace("target_table", &format!("dbo.[{copy}]"))).await;
        let differ = |a: &str, b: &str| format!("SELECT COUNT_BIG(*) FROM (SELECT * FROM dbo.[{a}] EXCEPT SELECT * FROM dbo.[{b}]) d");
        let state = h.ok(&format!("{}; {}; SELECT COUNT_BIG(*) FROM dbo.[{copy}]", differ(&source, &copy), differ(&copy, &source))).await;
        let counts: Vec<_> = Harness::rowsets(&state).iter().map(|s| h.rows(s)[0][0].clone()).collect();
        assert_eq!(counts, [Some("0".into()), Some("0".into()), Some("3".into())], "the script rebuilds every row exactly");
    })
    .catch_unwind()
    .await;
    h.ok(&format!("DROP TABLE dbo.[{source}]; DROP TABLE dbo.[{copy}]")).await;
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

#[tokio::test]
async fn esc_during_a_sql_server_all_rows_export_resets_the_session_and_leaves_the_file() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    let state = h.ok("SELECT 1 AS n UNION ALL SELECT 2; WAITFOR DELAY '00:00:02'; SELECT 3 AS done").await;
    let meta = Harness::rowsets(&state)[0].clone();
    let out = h._dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    let path = out.join("n.csv");
    std::fs::write(&path, "old").unwrap();
    let export = {
        let (engine, path) = (h.engine.clone(), path.display().to_string());
        tokio::spawn(async move { engine.result_export(meta.r#ref, ExportFormat::Csv, ExportScope::All, path).await })
    };
    tokio::time::timeout(Duration::from_secs(10), async {
        while h.console().work != Some(ConsoleWork::Export) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the export takes the session");
    tokio::time::sleep(Duration::from_millis(500)).await;
    h.engine.console_cancel(h.tab).await;

    assert!(matches!(export.await.unwrap(), Err(ExportError::Cancelled)));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "old");
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 1, "no temp file left beside it");
    let after = h.console();
    assert_eq!((after.work, after.export), (None, None));
    assert!(matches!(&after.session, SessionState::Lost { reason, .. } if reason.contains("Cancelled")), "{:?}", after.session);
    h.ok("SELECT 1").await;
}

#[tokio::test]
async fn insert_exports_replay_identity_and_leave_out_computed_and_rowversion_columns() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    let source = format!("l1_export_ident_{}", uuid::Uuid::new_v4().simple());
    let copy = format!("l1_export_ident_copy_{}", uuid::Uuid::new_v4().simple());
    let columns = "id int IDENTITY PRIMARY KEY, label nvarchar(20) NOT NULL, doubled AS (LEN(label) * 2), ver rowversion";
    h.ok(&format!("CREATE TABLE dbo.[{source}] ({columns}); CREATE TABLE dbo.[{copy}] ({columns})")).await;
    let outcome = AssertUnwindSafe(async {
        h.ok(&format!("INSERT INTO dbo.[{source}] (label) VALUES (N'a'), (N'bb'), (N'ccc'); DELETE FROM dbo.[{source}] WHERE id = 2")).await;
        h.engine.refresh_catalog(h.db()).await.unwrap();
        let page = h.engine.console_run(h.tab, RunRequest::TablePage { schema: "dbo".into(), table: source.clone(), page: 0, query: TableQuery::default() }, None).await.unwrap();
        let state = h.finish(page.run_id).await;
        let meta = Harness::rowsets(&state)[0].clone();
        let (summary, script) = h.export(&meta, ExportFormat::SqlInsert, ExportScope::All, "ident.sql").await;
        assert_eq!(summary.rows, 2);
        let table = format!("[dbo].[{source}]");
        assert_eq!(
            script,
            format!("SET IDENTITY_INSERT {table} ON;\nINSERT INTO {table} ([id], [label]) VALUES (1, N'a');\nINSERT INTO {table} ([id], [label]) VALUES (3, N'ccc');\nSET IDENTITY_INSERT {table} OFF;\n")
        );
        h.ok(&script.replace(&table, &format!("dbo.[{copy}]"))).await;
        let differ = |a: &str, b: &str| format!("SELECT COUNT_BIG(*) FROM (SELECT id, label, doubled FROM dbo.[{a}] EXCEPT SELECT id, label, doubled FROM dbo.[{b}]) d");
        let state = h.ok(&format!("{}; {}; SELECT COUNT_BIG(*) FROM dbo.[{copy}]", differ(&source, &copy), differ(&copy, &source))).await;
        let counts: Vec<_> = Harness::rowsets(&state).iter().map(|s| h.rows(s)[0][0].clone()).collect();
        assert_eq!(counts, [Some("0".into()), Some("0".into()), Some("2".into())], "the script rebuilds every row with its identity value");
    })
    .catch_unwind()
    .await;
    h.ok(&format!("DROP TABLE dbo.[{source}]; DROP TABLE dbo.[{copy}]")).await;
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

async fn set_environment(h: &Harness, environment: Option<Environment>) {
    let old = h.engine.boot().connections.into_iter().find(|c| c.id == h.connection).unwrap();
    let draft = ConnectionDraft {
        database_kind: DatabaseKind::SqlServer,
        id: Some(h.connection),
        name: old.name,
        host: old.host,
        port: old.port,
        database: old.database,
        user: old.user,
        ssl_mode: old.ssl_mode,
        ca_cert_path: None,
        password: PasswordInput::Keep,
        color: None,
        ssh: None,
        environment,
    };
    h.engine.save_connection(draft).await.unwrap();
}

async fn asked_to_run(h: &Harness, text: &str, confirmed: Option<WriteDigest>) -> WriteConfirmation {
    match h.engine.console_run(h.tab, RunRequest::Script { text: text.into(), target: RunTarget::All }, confirmed).await {
        Err(RunError::ConfirmWrites { confirmation }) => confirmation,
        other => panic!("{text}: expected ConfirmWrites, got {other:?}"),
    }
}

#[tokio::test]
async fn a_batch_that_runs_exec_on_production_waits_for_the_digest() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    set_environment(&h, Some(Environment::Production)).await;
    let text = "CREATE TABLE #p1_guard(id int)\nGO\nSELECT 1\nGO\nEXEC sp_who";
    let request = || RunRequest::Script { text: text.into(), target: RunTarget::All };
    let asked = asked_to_run(&h, text, None).await;
    assert_eq!((asked.connection.as_str(), asked.database.as_str(), asked.sends.count), ("SQL Server test", "shop", 2));
    assert_eq!(asked.sends.statements, ["CREATE TABLE #p1_guard(id int)", "EXEC sp_who"]);
    let state = h.ok("SELECT OBJECT_ID('tempdb..#p1_guard')").await;
    assert_eq!(h.rows(Harness::rowsets(&state)[0])[0][0], None, "a refused batch runs nothing");

    let id = h.engine.console_run(h.tab, request(), Some(asked.digest)).await.unwrap().run_id;
    let state = h.finish(id).await;
    assert!(matches!(state.run, RunState::Finished { outcome: RunOutcome::Ok, .. }), "{:?}", state.run);
    let state = h.ok("SELECT OBJECT_ID('tempdb..#p1_guard')").await;
    assert!(h.rows(Harness::rowsets(&state)[0])[0][0].is_some());
}

#[tokio::test]
async fn every_batch_on_production_asks_while_the_transaction_holds_writes_that_ran_before() {
    let mut h = Harness::open(TxnMode::AutoCommit).await;
    h.ok("CREATE TABLE #held (id int)").await;
    assert_eq!(h.ok("BEGIN TRAN; INSERT INTO #held VALUES (1)").await.txn, TxnState::Open);
    set_environment(&h, Some(Environment::Production)).await;
    for text in ["IF @@TRANCOUNT > 0 COMMIT TRANSACTION", "SELECT 1\nCOMMIT", "PRINT 'x'; COMMIT TRAN", "SELECT @@TRANCOUNT"] {
        asked_to_run(&h, text, None).await;
        assert_eq!(h.engine.boot().consoles.into_iter().find(|c| c.tab_id == h.tab).unwrap().txn, TxnState::Open, "{text}: a refused batch sends nothing");
    }
    let asked = asked_to_run(&h, "IF @@TRANCOUNT > 0 COMMIT TRANSACTION", None).await;
    let id = h.engine.console_run(h.tab, RunRequest::Script { text: "IF @@TRANCOUNT > 0 COMMIT TRANSACTION".into(), target: RunTarget::All }, Some(asked.digest)).await.unwrap().run_id;
    assert_eq!(h.finish(id).await.txn, TxnState::Idle);
    let state = h.ok("SELECT @@TRANCOUNT, COUNT_BIG(*) FROM #held").await;
    assert_eq!(h.rows(Harness::rowsets(&state)[0])[0], vec![Some("0".into()), Some("1".into())], "the confirmed batch committed the held write");
}
