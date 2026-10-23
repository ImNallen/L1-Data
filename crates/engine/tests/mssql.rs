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
        let engine = Engine::open(EngineConfig { data_dir: dir.path().into(), keys: Arc::new(MemoryKeyStore::default()), agent_path: AgentPath::Fixed("".into()), user_known_hosts: None, updates: Arc::new(engine::ScriptedTransport::named("off").unwrap()) }).await.unwrap();
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
        self.engine.console_run(self.tab, RunRequest::Script { text: sql.into(), target: RunTarget::All }).await.unwrap()
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
    let accepted = h.engine.console_run(h.tab, request).await.unwrap();
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
    h.engine.export_csv(result.r#ref, path.clone()).await.unwrap();
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
    h.engine.console_commit(h.tab).await.unwrap();
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
    let rejected = h.engine.console_run(h.tab, RunRequest::Script { text: "CREATE TABLE #must_not_exist(id int)\nGO\nUSE reporting".into(), target: RunTarget::All }).await;
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
    let run = h.engine.console_run(h.tab, RunRequest::TablePage { schema: "dbo".into(), table: table.clone(), page: 0, query: TableQuery::default() }).await.unwrap();
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
            RunRequest::TablePage { schema: "lab".into(), table: "awkward names".into(), page: 0, query: TableQuery { order_by: descending.sql, ..TableQuery::default() } },
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
