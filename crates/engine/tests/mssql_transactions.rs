#![cfg(feature = "mssql-tests")]

use engine::model::*;
use engine::{AgentPath, Engine, EngineConfig, MemoryKeyStore};
use futures_util::{FutureExt, future::BoxFuture};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;

struct Harness {
    engine: Arc<Engine>,
    connection: ConnectionId,
    manual: TabId,
    observer: TabId,
    table: String,
    objects: Vec<(&'static str, String)>,
    _dir: tempfile::TempDir,
}

impl Harness {
    async fn open() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let engine = Engine::open(EngineConfig {
            data_dir: dir.path().into(),
            keys: Arc::new(MemoryKeyStore::default()),
            agent_path: AgentPath::Fixed("".into()),
        }).await.unwrap();
        let connection = engine.save_connection(ConnectionDraft {
            database_kind: DatabaseKind::SqlServer,
            id: None,
            name: "Manual transaction regression".into(),
            host: std::env::var("L1_DATA_TEST_MSSQL_HOST").unwrap_or_else(|_| "localhost".into()),
            port: std::env::var("L1_DATA_TEST_MSSQL_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(1434),
            database: "shop".into(),
            user: "sa".into(),
            password: PasswordInput::Set(std::env::var("L1_DATA_TEST_MSSQL_PASSWORD").unwrap_or_else(|_| "L1Data_dev_Str0ng".into())),
            ssl_mode: SslMode::Require,
            ca_cert_path: None,
            color: None,
        }).await.unwrap().id;
        engine.connect(connection).await.unwrap();
        let manual = TabId(uuid::Uuid::new_v4());
        let observer = TabId(uuid::Uuid::new_v4());
        let h = Self {
            engine, connection, manual, observer,
            table: format!("##l1_manual_{}", uuid::Uuid::new_v4().simple()),
            objects: Vec::new(),
            _dir: dir,
        };
        h.configure(manual, TxnMode::Manual).await;
        h.configure(observer, TxnMode::AutoCommit).await;
        h.ok(observer, &format!("CREATE TABLE [{}] (id int PRIMARY KEY, n int); INSERT INTO [{}] VALUES(1,1)", h.table, h.table)).await;
        h
    }

    async fn configure(&self, tab: TabId, mode: TxnMode) {
        self.engine.console_configure(tab, ConsoleConfig {
            connection_id: self.connection, database: "shop".into(), schema: None, mode,
        }).await.unwrap();
    }

    fn state(&self, tab: TabId) -> ConsoleState {
        self.engine.boot().consoles.into_iter().find(|s| s.tab_id == tab).unwrap()
    }

    async fn run(&self, tab: TabId, sql: &str) -> ConsoleState {
        let accepted = self.engine.console_run(tab, RunRequest::Script { text: sql.into(), target: RunTarget::All }).await.unwrap();
        self.finish(tab, accepted.run_id).await
    }

    async fn finish(&self, tab: TabId, run: RunId) -> ConsoleState {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let state = self.state(tab);
                if matches!(state.run, RunState::Finished { run_id, .. } if run_id == run) {
                    return state;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.expect("Manual transaction regression did not finish")
    }

    async fn ok(&self, tab: TabId, sql: &str) -> ConsoleState {
        let state = self.run(tab, sql).await;
        assert!(matches!(state.run, RunState::Finished { outcome: RunOutcome::Ok, .. }), "{sql}: {:?}", state.run);
        state
    }

    async fn scalar(&self, tab: TabId, sql: &str) -> String {
        let state = self.ok(tab, sql).await;
        let RunState::Finished { statements, .. } = state.run else { unreachable!() };
        let result = statements.iter().flat_map(|s| &s.outputs).find_map(|o| match o {
            RunOutput::Rows { result, .. } => Some(result.r#ref),
            _ => None,
        }).unwrap();
        self.engine.result_page(result, 0, 1).unwrap().rows[0][0].clone().unwrap()
    }

    async fn rejected(&self, sql: &str) {
        let before = self.state(self.manual);
        let result = self.engine.console_run(self.manual, RunRequest::Script { text: sql.into(), target: RunTarget::All }).await;
        assert!(matches!(result, Err(RunError::Unsupported { ref what }) if what.contains("Manual") && what.contains("Auto-commit")), "{sql}: {result:?}");
        let after = self.state(self.manual);
        assert_eq!((after.rev, after.txn, after.txn_depth), (before.rev, before.txn, before.txn_depth), "Rejected SQL changed console state");
    }

    fn procedure(&mut self) -> String {
        self.object("PROCEDURE")
    }

    fn object(&mut self, kind: &'static str) -> String {
        let name = format!("l1_manual_{}", uuid::Uuid::new_v4().simple());
        self.objects.push((kind, name.clone()));
        name
    }

    async fn cleanup(self) {
        for tab in [self.manual, self.observer] {
            let _ = self.engine.console_rollback(tab).await;
            self.engine.console_close(tab, true).await.unwrap();
        }
        self.configure(self.observer, TxnMode::AutoCommit).await;
        for (kind, name) in self.objects.iter().rev() {
            self.ok(self.observer, &format!("DROP {kind} IF EXISTS dbo.[{name}]")).await;
        }
        self.ok(self.observer, &format!("DROP TABLE IF EXISTS [{}]", self.table)).await;
        self.engine.shutdown().await;
    }
}

async fn check(test: impl for<'a> FnOnce(&'a mut Harness) -> BoxFuture<'a, ()>) {
    let mut h = Harness::open().await;
    let result = AssertUnwindSafe(test(&mut h)).catch_unwind().await;
    h.cleanup().await;
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

fn transaction(state: &ConsoleState, depth: u32) {
    assert_eq!((state.txn, state.txn_depth), (if depth == 0 { TxnState::Idle } else { TxnState::Active }, Some(depth)));
}

#[tokio::test]
async fn module_bodies_remain_rollbackable_and_conditional_control_cannot_commit_a_write() {
    check(|h| async move {
        for body in ["SELECT 1", "BEGIN TRANSACTION; SELECT 1; COMMIT TRANSACTION"] {
            let name = h.procedure();
            let state = h.ok(h.manual, &format!("CREATE PROCEDURE dbo.[{name}] AS BEGIN {body}; END")).await;
            transaction(&state, 1);
            h.engine.console_rollback(h.manual).await.unwrap();
            transaction(&h.state(h.manual), 0);
            assert_eq!(h.scalar(h.observer, &format!("SELECT COUNT_BIG(*) FROM sys.procedures WHERE name=N'{name}'")).await, "0");
        }
        h.rejected(&format!("IF 1=0 ROLLBACK TRANSACTION; UPDATE [{}] SET n=2 WHERE id=1", h.table)).await;
        assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}] WHERE id=1", h.table)).await, "1");
        assert!(matches!(h.engine.console_rollback(h.manual).await, Err(TxnError::NoTransaction)));
    }.boxed()).await;
}

#[tokio::test]
async fn manual_refuses_every_unsafe_selected_batch_before_any_write() {
    check(|h| async move {
        let update = format!("UPDATE [{}] SET n=2 WHERE id=1", h.table);
        for sql in [
            format!("IF 1=0 BEGIN TRANSACTION; {update}"),
            format!("IF 1=1 BEGIN TRANSACTION; {update}"),
            format!("IF 1=0 COMMIT TRANSACTION; {update}"),
            format!("IF 1=1 COMMIT TRANSACTION; {update}"),
            format!("IF 1=0 ROLLBACK TRANSACTION; {update}"),
            format!("IF 1=1 ROLLBACK TRANSACTION; {update}"),
            format!("BEGIN TRANSACTION; {update}; COMMIT TRANSACTION"),
            format!("{update}; COMMIT TRANSACTION"),
            format!("ROLLBACK TRANSACTION; {update}"),
            format!("{update}\nGO\nIF 1=0 ROLLBACK TRANSACTION"),
            format!("{update}\nGO\nCOMMIT TRANSACTION\nGO\nIF 1=0 ROLLBACK TRANSACTION"),
            format!("{update}\nGO\nSET IMPLICIT_TRANSACTIONS OFF"),
            format!("{update}\nGO\nSET ANSI_DEFAULTS OFF"),
            format!("{update}\nGO\nSET NOEXEC ON"),
            format!("SET ANSI_PADDING, IMPLICIT_TRANSACTIONS ON; {update}"),
            format!("{update}\nGO\nEXEC(N'ROLLBACK TRANSACTION; {update}')"),
        ] {
            h.rejected(&sql).await;
            assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "1", "{sql}");
        }
        for option in ["IMPLICIT_TRANSACTIONS", "ANSI_DEFAULTS", "XACT_ABORT", "REMOTE_PROC_TRANSACTIONS", "NOEXEC", "PARSEONLY", "FMTONLY", "SHOWPLAN_ALL", "SHOWPLAN_TEXT", "SHOWPLAN_XML"] {
            for value in ["ON", "OFF"] {
                h.rejected(&format!("{update}\nGO\nSET {option} {value}")).await;
                assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "1");
            }
        }
        let name = h.procedure();
        h.ok(h.observer, &format!("CREATE PROCEDURE dbo.[{name}] @n int AS IF @@TRANCOUNT > 0 ROLLBACK TRANSACTION; UPDATE [{}] SET n=@n", h.table)).await;
        for call in [format!("EXEC dbo.[{name}] @n=2"), format!("dbo.[{name}] @n=2"), format!("{name} @n=2")] {
            h.rejected(&call).await;
            h.rejected(&format!("{update}\nGO\n{call}")).await;
            assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "1");
        }
        let selected = format!("{update}\nGO\nIF 1=0 ROLLBACK TRANSACTION");
        let request = RunRequest::Script {
            target: RunTarget::Selection { from: DocOffset(0), to: DocOffset(selected.encode_utf16().count() as u32) },
            text: selected,
        };
        assert!(matches!(h.engine.console_run(h.manual, request).await, Err(RunError::Unsupported { .. })));
        assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "1");
        let accepted = h.engine.console_run(h.manual, RunRequest::Script {
            text: format!("{update}\nGO\nIF 1=0 ROLLBACK TRANSACTION"), target: RunTarget::Cursor { offset: DocOffset(0) },
        }).await.unwrap();
        transaction(&h.finish(h.manual, accepted.run_id).await, 1);
        h.engine.console_rollback(h.manual).await.unwrap();
        assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "1");
        h.ok(h.manual, &update).await;
        h.rejected(&format!("COMMIT TRANSACTION; {update}")).await;
        transaction(&h.state(h.manual), 1);
        h.engine.console_rollback(h.manual).await.unwrap();
        assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "1");
    }.boxed()).await;
}

#[tokio::test]
async fn manual_keeps_scope_native_nesting_and_restarts_after_transaction_end() {
    check(|h| async move {
        transaction(&h.ok(h.manual, "BEGIN TRANSACTION").await, 1);
        transaction(&h.ok(h.manual, "BEGIN TRANSACTION").await, 2);
        let sql = format!("DECLARE @n int=3; UPDATE [{}] SET n=@n; SELECT @n", h.table);
        assert_eq!(h.scalar(h.manual, &sql).await, "3");
        transaction(&h.ok(h.manual, "COMMIT TRANSACTION").await, 1);
        h.engine.console_rollback(h.manual).await.unwrap();
        transaction(&h.state(h.manual), 0);
        assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "1");

        transaction(&h.ok(h.manual, "BEGIN TRAN; BEGIN TRAN;").await, 2);
        h.ok(h.manual, &format!("UPDATE [{}] SET n=4", h.table)).await;
        h.engine.console_commit(h.manual).await.unwrap();
        transaction(&h.state(h.manual), 0);
        assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "4");
        assert_eq!(h.scalar(h.observer, "SELECT @@TRANCOUNT").await, "0");
        assert!(matches!(h.engine.console_commit(h.manual).await, Err(TxnError::NoTransaction)));

        let script = format!("UPDATE [{}] SET n=5\nGO\nCOMMIT TRANSACTION\nGO\nUPDATE [{}] SET n=6", h.table, h.table);
        transaction(&h.ok(h.manual, &script).await, 1);
        h.engine.console_rollback(h.manual).await.unwrap();
        assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "5");

        let script = format!("UPDATE [{}] SET n=7\nGO\nROLLBACK TRANSACTION\nGO\nUPDATE [{}] SET n=8", h.table, h.table);
        transaction(&h.ok(h.manual, &script).await, 1);
        h.engine.console_rollback(h.manual).await.unwrap();
        assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "5");

        h.ok(h.manual, &format!("UPDATE [{}] SET n=9", h.table)).await;
        transaction(&h.ok(h.manual, "SAVE TRANSACTION [point]").await, 1);
        h.ok(h.manual, &format!("UPDATE [{}] SET n=10", h.table)).await;
        transaction(&h.ok(h.manual, "ROLLBACK TRANSACTION [point]").await, 1);
        h.engine.console_commit(h.manual).await.unwrap();
        assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "9");
    }.boxed()).await;
}

#[tokio::test]
async fn errors_and_cancellation_leave_no_manual_write_committed() {
    check(|h| async move {
        let state = h.run(h.manual, &format!("UPDATE [{}] SET n=2; RAISERROR('expected failure',16,1)", h.table)).await;
        assert!(matches!(state.run, RunState::Finished { outcome: RunOutcome::Failed, .. }));
        transaction(&state, 1);
        h.engine.console_rollback(h.manual).await.unwrap();
        assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "1");

        let state = h.run(h.manual, &format!("UPDATE [{}] SET n=3; INSERT INTO [{}] VALUES(1,99)", h.table, h.table)).await;
        assert!(matches!(state.run, RunState::Finished { outcome: RunOutcome::Failed, .. }));
        transaction(&state, 1);
        h.engine.console_rollback(h.manual).await.unwrap();
        assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "1");

        h.ok(h.manual, &format!("UPDATE [{}] SET n=4", h.table)).await;
        let SessionState::Open { backend_pid } = h.state(h.manual).session else { panic!("Expected open manual session") };
        let accepted = h.engine.console_run(h.manual, RunRequest::Script {
            text: "WAITFOR DELAY '00:00:30'".into(), target: RunTarget::All,
        }).await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if h.scalar(h.observer, &format!("SELECT COUNT_BIG(*) FROM sys.dm_exec_requests WHERE session_id={backend_pid} AND command='WAITFOR'")).await == "1" {
                    break;
                }
            }
        }).await.unwrap();
        h.engine.console_cancel(h.manual).await;
        let state = h.finish(h.manual, accepted.run_id).await;
        assert!(matches!(state.run, RunState::Finished { outcome: RunOutcome::Cancelled, .. }));
        assert!(matches!(state.session, SessionState::Lost { transaction: TransactionLoss::Unknown, .. }));
        assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "1");
        transaction(&h.ok(h.manual, &format!("UPDATE [{}] SET n=5", h.table)).await, 1);
        h.engine.console_rollback(h.manual).await.unwrap();
        assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "1");
    }.boxed()).await;
}

#[tokio::test]
async fn autocommit_retains_native_conditionals_options_and_procedure_calls() {
    check(|h| async move {
        h.configure(h.manual, TxnMode::AutoCommit).await;
        transaction(&h.ok(h.manual, &format!("IF 1=0 ROLLBACK TRANSACTION; UPDATE [{}] SET n=2", h.table)).await, 0);
        assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "2");
        transaction(&h.ok(h.manual, &format!("BEGIN TRANSACTION; BEGIN TRANSACTION; UPDATE [{}] SET n=3; COMMIT TRANSACTION", h.table)).await, 1);
        h.engine.console_rollback(h.manual).await.unwrap();
        assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "2");
        transaction(&h.ok(h.manual, &format!("IF 1=1 BEGIN TRANSACTION; UPDATE [{}] SET n=4; IF 1=1 COMMIT TRANSACTION", h.table)).await, 0);
        assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "4");
        transaction(&h.ok(h.manual, &format!("SET IMPLICIT_TRANSACTIONS ON; UPDATE [{}] SET n=5", h.table)).await, 1);
        h.engine.console_rollback(h.manual).await.unwrap();
        transaction(&h.state(h.manual), 0);
        assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "4");
        transaction(&h.ok(h.manual, "SELECT @@TRANCOUNT, XACT_STATE(), DB_NAME()").await, 0);
        transaction(&h.ok(h.manual, "SET IMPLICIT_TRANSACTIONS OFF").await, 0);

        let name = h.procedure();
        h.ok(h.observer, &format!("CREATE PROCEDURE dbo.[{name}] @n int AS UPDATE [{}] SET n=@n", h.table)).await;
        for call in [format!("EXEC dbo.[{name}] @n=6"), format!("dbo.[{name}] @n=6"), format!("{name} @n=6")] {
            transaction(&h.ok(h.manual, &call).await, 0);
            assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "6");
        }
    }.boxed()).await;
}

#[tokio::test]
async fn inherited_options_keep_truthful_depth_and_cannot_skip_the_manual_wrapper() {
    check(|h| async move {
        for (option, depth) in [("IMPLICIT_TRANSACTIONS", 2), ("ANSI_DEFAULTS", 2), ("XACT_ABORT", 1)] {
            h.configure(h.manual, TxnMode::AutoCommit).await;
            transaction(&h.ok(h.manual, &format!("SET {option} ON")).await, 0);
            h.configure(h.manual, TxnMode::Manual).await;
            transaction(&h.ok(h.manual, &format!("UPDATE [{}] SET n=2", h.table)).await, depth);
            h.engine.console_rollback(h.manual).await.unwrap();
            transaction(&h.state(h.manual), 0);
            assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "1");
            h.configure(h.manual, TxnMode::AutoCommit).await;
            transaction(&h.ok(h.manual, &format!("SET {option} OFF")).await, 0);
        }
        h.ok(h.manual, "SET XACT_ABORT ON").await;
        h.configure(h.manual, TxnMode::Manual).await;
        let failed = h.run(h.manual, &format!("UPDATE [{}] SET n=3; INSERT INTO [{}] VALUES(1,99); UPDATE [{}] SET n=4", h.table, h.table, h.table)).await;
        assert!(matches!(failed.run, RunState::Finished { outcome: RunOutcome::Failed, .. }));
        transaction(&failed, 0);
        assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "1");
        transaction(&h.ok(h.manual, &format!("UPDATE [{}] SET n=5", h.table)).await, 1);
        h.engine.console_rollback(h.manual).await.unwrap();
        assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "1");

        h.configure(h.manual, TxnMode::AutoCommit).await;
        let failed = h.run(h.manual, "SET NOEXEC ON").await;
        assert!(matches!(failed.run, RunState::Finished { outcome: RunOutcome::Failed, .. }));
        assert!(matches!(failed.session, SessionState::Lost { .. }));
        h.configure(h.manual, TxnMode::Manual).await;
        transaction(&h.ok(h.manual, &format!("UPDATE [{}] SET n=6", h.table)).await, 1);
        h.engine.console_rollback(h.manual).await.unwrap();
        assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "1");
    }.boxed()).await;
}

#[tokio::test]
async fn every_module_definition_family_keeps_trailing_control_from_executing() {
    check(|h| async move {
        let trigger_table = h.object("TABLE");
        h.ok(h.observer, &format!("CREATE TABLE dbo.[{trigger_table}](n int)")).await;
        for (keyword, kind, accepts_tail) in [
            ("PROCEDURE", "PROCEDURE", true), ("PROC", "PROCEDURE", true),
            ("VIEW", "VIEW", false), ("FUNCTION", "FUNCTION", false), ("TRIGGER", "TRIGGER", true),
        ] {
            let body = match kind {
                "PROCEDURE" => "AS BEGIN SELECT 1 AS n END".into(),
                "VIEW" => "AS SELECT 1 AS n".into(),
                "FUNCTION" => "() RETURNS int AS BEGIN RETURN 1 END".into(),
                "TRIGGER" => format!("ON dbo.[{trigger_table}] AFTER INSERT AS BEGIN PRINT N'body' END"),
                _ => unreachable!(),
            };
            for verb in ["CREATE", "CREATE OR ALTER"] {
                let name = h.object(kind);
                let sql = format!("{verb} {keyword} dbo.[{name}] {body}; COMMIT TRANSACTION; UPDATE [{}] SET n=2", h.table);
                let state = h.run(h.manual, &sql).await;
                let expected = if accepts_tail { RunOutcome::Ok } else { RunOutcome::Failed };
                assert!(matches!(state.run, RunState::Finished { outcome, .. } if outcome == expected), "{sql}: {:?}", state.run);
                transaction(&state, 1);
                h.engine.console_rollback(h.manual).await.unwrap();
                assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "1", "{sql}");
                assert_eq!(h.scalar(h.observer, &format!("SELECT COUNT_BIG(*) FROM sys.objects WHERE name=N'{name}'")).await, "0", "{sql}");
            }
            let name = h.object(kind);
            h.ok(h.observer, &format!("CREATE {keyword} dbo.[{name}] {body}")).await;
            let definition = format!("SELECT OBJECT_DEFINITION(OBJECT_ID(N'dbo.{name}'))");
            let original = h.scalar(h.observer, &definition).await;
            for verb in ["ALTER", "CREATE OR ALTER"] {
                let sql = format!("{verb} {keyword} dbo.[{name}] {body}; COMMIT TRANSACTION; UPDATE [{}] SET n=2", h.table);
                let state = h.run(h.manual, &sql).await;
                let expected = if accepts_tail { RunOutcome::Ok } else { RunOutcome::Failed };
                assert!(matches!(state.run, RunState::Finished { outcome, .. } if outcome == expected), "{sql}: {:?}", state.run);
                transaction(&state, 1);
                h.engine.console_rollback(h.manual).await.unwrap();
                assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, "1", "{sql}");
                assert_eq!(h.scalar(h.observer, &definition).await, original, "{sql}");
            }
        }
    }.boxed()).await;
}

#[tokio::test]
async fn cancellation_does_not_claim_committed_work_was_rolled_back() {
    check(|h| async move {
        h.configure(h.manual, TxnMode::AutoCommit).await;
        for (sql, expected) in [
            (format!("UPDATE [{}] SET n=2\nGO\nWAITFOR DELAY '00:00:30'", h.table), "2"),
            (format!("BEGIN TRANSACTION; UPDATE [{}] SET n=3; COMMIT TRANSACTION; WAITFOR DELAY '00:00:30'", h.table), "3"),
        ] {
            let accepted = h.engine.console_run(h.manual, RunRequest::Script { text: sql, target: RunTarget::All }).await.unwrap();
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    let SessionState::Open { backend_pid } = h.state(h.manual).session else { continue };
                    if h.scalar(h.observer, &format!("SELECT COUNT_BIG(*) FROM sys.dm_exec_requests WHERE session_id={backend_pid} AND command='WAITFOR'")).await == "1" { break; }
                }
            }).await.unwrap();
            assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, expected);
            h.engine.console_cancel(h.manual).await;
            let state = h.finish(h.manual, accepted.run_id).await;
            assert!(matches!(state.run, RunState::Finished { outcome: RunOutcome::Cancelled, .. }));
            assert!(matches!(state.session, SessionState::Lost { transaction: TransactionLoss::Unknown, .. }), "Cancellation must not claim a confirmed rollback: {:?}", state.session);
            assert_eq!(h.scalar(h.observer, &format!("SELECT n FROM [{}]", h.table)).await, expected);
        }
    }.boxed()).await;
}
