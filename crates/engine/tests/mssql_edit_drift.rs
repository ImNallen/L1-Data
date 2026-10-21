#![cfg(feature = "mssql-tests")]

use engine::model::*;
use engine::{AgentPath, Engine, EngineConfig, MemoryKeyStore};
use futures_util::{FutureExt, future::BoxFuture};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;

struct Harness {
    engine: Arc<Engine>,
    db: DatabaseRef,
    editor: TabId,
    ddl: TabId,
    observer: TabId,
    other: ConnectionId,
    objects: Vec<(&'static str, String)>,
    _dir: tempfile::TempDir,
}

fn draft() -> ConnectionDraft {
    ConnectionDraft {
        database_kind: DatabaseKind::SqlServer,
        id: None,
        name: "SQL Server edit conversion regression".into(),
        host: std::env::var("L1_DATA_TEST_MSSQL_HOST").unwrap_or_else(|_| "localhost".into()),
        port: std::env::var("L1_DATA_TEST_MSSQL_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(1434),
        database: "shop".into(),
        user: "sa".into(),
        password: PasswordInput::Set(std::env::var("L1_DATA_TEST_MSSQL_PASSWORD").unwrap_or_else(|_| "L1Data_dev_Str0ng".into())),
        ssl_mode: SslMode::Require,
        ca_cert_path: None,
        color: None,
    }
}

impl Harness {
    async fn open() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let engine = Engine::open(EngineConfig {
            data_dir: dir.path().into(), keys: Arc::new(MemoryKeyStore::default()), agent_path: AgentPath::Fixed("".into()),
        }).await.unwrap();
        let connection = engine.save_connection(draft()).await.unwrap().id;
        let other = engine.save_connection(draft()).await.unwrap().id;
        engine.connect(connection).await.unwrap();
        engine.connect(other).await.unwrap();
        let h = Self {
            engine, db: DatabaseRef { connection_id: connection, database: "shop".into() }, other,
            editor: TabId(uuid::Uuid::new_v4()), ddl: TabId(uuid::Uuid::new_v4()), observer: TabId(uuid::Uuid::new_v4()),
            objects: vec![], _dir: dir,
        };
        h.configure(TxnMode::AutoCommit).await;
        for tab in [h.ddl, h.observer] {
            h.engine.console_configure(tab, ConsoleConfig { connection_id: other, database: "shop".into(), schema: None, mode: TxnMode::AutoCommit }).await.unwrap();
        }
        h
    }

    async fn configure(&self, mode: TxnMode) {
        self.engine.console_configure(self.editor, ConsoleConfig { connection_id: self.db.connection_id, database: "shop".into(), schema: None, mode }).await.unwrap();
    }

    fn object(&mut self, kind: &'static str) -> String {
        let name = format!("l1_edit_drift_{}", uuid::Uuid::new_v4().simple());
        self.objects.push((kind, name.clone()));
        name
    }

    fn state(&self, tab: TabId) -> ConsoleState {
        self.engine.boot().consoles.into_iter().find(|s| s.tab_id == tab).unwrap()
    }

    fn pid(&self, tab: TabId) -> i32 {
        let SessionState::Open { backend_pid } = self.state(tab).session else { panic!("session not open") };
        backend_pid
    }

    async fn finish(&self, tab: TabId, id: RunId) -> ConsoleState {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let state = self.state(tab);
                if matches!(state.run, RunState::Finished { run_id, .. } if run_id == id) {
                    return state;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.expect("SQL Server edit regression did not finish")
    }

    fn rows(&self, state: &ConsoleState) -> Vec<Vec<Option<String>>> {
        let RunState::Finished { statements, .. } = &state.run else { panic!("run not finished") };
        statements.iter().flat_map(|s| &s.outputs).filter_map(|o| match o {
            RunOutput::Rows { result, .. } => Some(self.engine.result_page(result.r#ref, 0, 100).unwrap().rows),
            _ => None,
        }).flatten().collect()
    }

    async fn run(&self, tab: TabId, sql: &str) -> ConsoleState {
        let accepted = self.engine.console_run(tab, RunRequest::Script { text: sql.into(), target: RunTarget::All }).await.unwrap();
        self.finish(tab, accepted.run_id).await
    }

    async fn sql(&self, tab: TabId, sql: &str) -> Vec<Vec<Option<String>>> {
        let state = self.run(tab, sql).await;
        assert!(matches!(state.run, RunState::Finished { outcome: RunOutcome::Ok, .. }), "{sql}: {:?}", state.run);
        self.rows(&state)
    }

    async fn page(&self, table: &str) -> Vec<Vec<Option<String>>> {
        let accepted = self.engine.console_run(self.editor, RunRequest::TablePage { schema: "dbo".into(), table: table.into(), page: 0, query: TableQuery::default() }).await.unwrap();
        let state = self.finish(self.editor, accepted.run_id).await;
        assert!(matches!(state.run, RunState::Finished { outcome: RunOutcome::Ok, .. }), "{:?}", state.run);
        self.rows(&state)
    }

    async fn refresh(&self) {
        self.engine.refresh_catalog(self.db.clone()).await.unwrap();
    }

    async fn cleanup(&self) {
        for tab in [self.editor, self.ddl, self.observer] {
            let _ = self.engine.console_cancel(tab).await;
            let _ = self.engine.console_rollback(tab).await;
            self.engine.console_close(tab, true).await.unwrap();
        }
        self.engine.console_configure(self.observer, ConsoleConfig { connection_id: self.other, database: "shop".into(), schema: None, mode: TxnMode::AutoCommit }).await.unwrap();
        for (kind, name) in self.objects.iter().rev() {
            assert_eq!(self.sql(self.observer, &format!("DROP {kind} IF EXISTS dbo.[{name}]; SELECT OBJECT_ID(N'dbo.{name}')")).await, vec![vec![None]]);
        }
        self.engine.shutdown().await;
    }
}

async fn check(test: impl for<'a> FnOnce(&'a mut Harness) -> BoxFuture<'a, ()>) {
    static DATABASE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = DATABASE.lock().await;
    let mut h = Harness::open().await;
    let outcome = AssertUnwindSafe(test(&mut h)).catch_unwind().await;
    h.cleanup().await;
    if let Err(panic) = outcome { std::panic::resume_unwind(panic); }
}

fn cell(column: &str, value: &str) -> CellWrite {
    CellWrite { column: column.into(), value: Some(value.into()) }
}

fn changes(table: &str) -> TableChanges {
    TableChanges { schema: "dbo".into(), table: table.into(), inserts: vec![], updates: vec![], deletes: vec![] }
}

fn update(table: &str, key: &str, column: &str, value: &str) -> TableChanges {
    let mut edit = changes(table);
    edit.updates.push(RowUpdate { key: vec![cell("id", key)], set: vec![cell(column, value)] });
    edit
}

fn loss_refused(result: Result<ApplySummary, ApplyError>) {
    assert!(matches!(result, Err(ApplyError::Failed { ref error, .. }) if error.message.contains("without loss") || error.message.contains("capacity")), "{result:?}");
}

fn drift_refused(result: Result<ApplySummary, ApplyError>) {
    assert!(matches!(result, Err(ApplyError::Transaction { ref error }) if error.message.contains("changed")), "{result:?}");
}

#[tokio::test]
async fn decimal_scale_drift_cannot_update_or_delete_another_grid_row() {
    check(|h| async move {
        let table = h.object("TABLE");
        let pk = format!("{table}_pk");
        h.sql(h.ddl, &format!("CREATE TABLE dbo.[{table}] (id decimal(8,2) NOT NULL CONSTRAINT [{pk}] PRIMARY KEY, note nvarchar(40)); INSERT INTO dbo.[{table}] VALUES(1.23,N'original')")).await;
        h.refresh().await;
        h.sql(h.ddl, &format!("ALTER TABLE dbo.[{table}] DROP CONSTRAINT [{pk}]; ALTER TABLE dbo.[{table}] ALTER COLUMN id decimal(8,3) NOT NULL; ALTER TABLE dbo.[{table}] ADD CONSTRAINT [{pk}] PRIMARY KEY(id); INSERT INTO dbo.[{table}] VALUES(1.234,N'new precision')")).await;
        let expected = vec![vec![Some("1.230".into()), Some("original".into())], vec![Some("1.234".into()), Some("new precision".into())]];
        let grid = h.page(&table).await;
        assert_eq!(grid, expected);
        let key = grid[1][0].as_deref().unwrap();
        let applied = h.engine.table_apply(h.editor, update(&table, key, "note", "wrong update")).await;
        let persisted = h.sql(h.observer, &format!("SELECT id,note FROM dbo.[{table}] ORDER BY id")).await;
        eprintln!("decimal drift apply={applied:?}; persisted={persisted:?}");
        assert_eq!(persisted, expected, "a selected key must never round to another row");
        assert!(applied.is_err(), "stale native type must refuse before writing");
        let mut edit = changes(&table);
        edit.deletes.push(vec![cell("id", key)]);
        assert!(h.engine.table_apply(h.editor, edit).await.is_err());
        assert_eq!(h.sql(h.observer, &format!("SELECT id,note FROM dbo.[{table}] ORDER BY id")).await, expected);
        h.refresh().await;
        assert!(h.engine.table_apply(h.editor, update(&table, key, "note", "exact selected row")).await.unwrap().committed);
        assert_eq!(h.sql(h.observer, &format!("SELECT id,note FROM dbo.[{table}] ORDER BY id")).await, vec![expected[0].clone(), vec![Some("1.234".into()), Some("exact selected row".into())]]);
    }.boxed()).await;
}

#[tokio::test]
async fn non_unicode_assignments_cannot_replace_characters() {
    check(|h| async move {
        let table = h.object("TABLE");
        h.sql(h.ddl, &format!("CREATE TABLE dbo.[{table}] (id int PRIMARY KEY, note varchar(20) COLLATE SQL_Latin1_General_CP1_CI_AS); INSERT INTO dbo.[{table}] VALUES(1,'original')")).await;
        h.refresh().await;
        let applied = h.engine.table_apply(h.editor, update(&table, "1", "note", "漢字")).await;
        let persisted = h.sql(h.observer, &format!("SELECT note FROM dbo.[{table}]")).await;
        eprintln!("codepage apply={applied:?}; persisted={persisted:?}");
        assert_eq!(persisted, vec![vec![Some("original".into())]], "Unicode text must survive conversion or refuse");
        assert!(applied.is_err());
    }.boxed()).await;
}

#[tokio::test]
async fn ansi_warnings_off_cannot_truncate_assignments() {
    check(|h| async move {
        let table = h.object("TABLE");
        h.sql(h.ddl, &format!("CREATE TABLE dbo.[{table}] (id int PRIMARY KEY, note varchar(3)); INSERT INTO dbo.[{table}] VALUES(1,'old')")).await;
        h.refresh().await;
        h.sql(h.editor, "SET ANSI_WARNINGS OFF").await;
        let applied = h.engine.table_apply(h.editor, update(&table, "1", "note", "abcdef")).await;
        let persisted = h.sql(h.observer, &format!("SELECT note FROM dbo.[{table}]")).await;
        eprintln!("warnings-off apply={applied:?}; persisted={persisted:?}");
        assert_eq!(persisted, vec![vec![Some("old".into())]], "warning settings cannot allow silent truncation");
        assert!(applied.is_err());
    }.boxed()).await;
}

#[tokio::test]
async fn temporal_scale_drift_cannot_update_or_delete_another_grid_row() {
    check(|h| async move {
        for (native, prefix, offset) in [("time", "03:04:05", ""), ("datetime2", "2024-01-02 03:04:05", ""), ("datetimeoffset", "2024-01-02 03:04:05", "+02:00")] {
            let table = h.object("TABLE");
            let pk = format!("{table}_pk");
            let first = format!("{prefix}.2300000{offset}");
            let selected = format!("{prefix}.2340000{offset}");
            h.sql(h.ddl, &format!("CREATE TABLE dbo.[{table}] (id {native}(2) NOT NULL CONSTRAINT [{pk}] PRIMARY KEY, note nvarchar(40)); INSERT INTO dbo.[{table}] VALUES(CONVERT({native}(2),N'{first}',121),N'original')")).await;
            h.refresh().await;
            h.sql(h.ddl, &format!("ALTER TABLE dbo.[{table}] DROP CONSTRAINT [{pk}]; ALTER TABLE dbo.[{table}] ALTER COLUMN id {native}(3) NOT NULL; ALTER TABLE dbo.[{table}] ADD CONSTRAINT [{pk}] PRIMARY KEY(id); INSERT INTO dbo.[{table}] VALUES(CONVERT({native}(3),N'{selected}',121),N'new precision')")).await;
            let expected = vec![vec![Some(first), Some("original".into())], vec![Some(selected), Some("new precision".into())]];
            let grid = h.page(&table).await;
            assert_eq!(grid, expected, "{native}");
            let key = grid[1][0].as_deref().unwrap();
            let applied = h.engine.table_apply(h.editor, update(&table, key, "note", "wrong update")).await;
            let persisted = h.sql(h.observer, &format!("SELECT id,note FROM dbo.[{table}] ORDER BY id")).await;
            eprintln!("{native} drift apply={applied:?}; persisted={persisted:?}");
            assert_eq!(persisted, expected, "a temporal key must never round to another row");
            drift_refused(applied);
            let mut edit = changes(&table);
            edit.deletes.push(vec![cell("id", key)]);
            drift_refused(h.engine.table_apply(h.editor, edit).await);
            assert_eq!(h.sql(h.observer, &format!("SELECT id,note FROM dbo.[{table}] ORDER BY id")).await, expected);
            h.refresh().await;
            assert!(h.engine.table_apply(h.editor, update(&table, key, "note", "selected")).await.unwrap().committed);
            assert_eq!(h.sql(h.observer, &format!("SELECT id,note FROM dbo.[{table}] ORDER BY id")).await, vec![expected[0].clone(), vec![grid[1][0].clone(), Some("selected".into())]]);
        }
    }.boxed()).await;
}

#[tokio::test]
async fn refreshed_metadata_cannot_round_old_grid_keys_in_any_native_family() {
    check(|h| async move {
        for (before, after, existing, selected) in [
            ("decimal(8,3)", "decimal(8,2)", "1.230", "1.234"),
            ("time(3)", "time(2)", "03:04:05.2300000", "03:04:05.2340000"),
            ("datetime2(3)", "datetime2(2)", "2024-01-02 03:04:05.2300000", "2024-01-02 03:04:05.2340000"),
            ("datetimeoffset(3)", "datetimeoffset(2)", "2024-01-02 03:04:05.2300000+02:00", "2024-01-02 03:04:05.2340000+02:00"),
            ("decimal(8,2)", "int", "1.00", "1.23"),
            ("varchar(50)", "uniqueidentifier", "abcd1234-5678-90ab-cdef-1234567890ab", "abcd1234-5678-90ab-cdef-1234567890ab-extra"),
            ("varchar(8)", "varchar(3)", "abc", "abcdef"),
            ("varbinary(4)", "varbinary(2)", "0x0102", "0x010203"),
        ] {
            let table = h.object("TABLE");
            let pk = format!("{table}_pk");
            let literal = |value: &str| if before.starts_with("varbinary") { value.to_owned() } else { format!("N'{value}'") };
            h.sql(h.ddl, &format!("CREATE TABLE dbo.[{table}] (id {before} NOT NULL CONSTRAINT [{pk}] PRIMARY KEY, note nvarchar(40)); INSERT INTO dbo.[{table}] VALUES(CONVERT({before},{},121),N'existing'),(CONVERT({before},{},121),N'selected')", literal(existing), literal(selected))).await;
            h.refresh().await;
            let grid = h.page(&table).await;
            let key = grid.iter().find(|r| r[1].as_deref() == Some("selected")).unwrap()[0].as_deref().unwrap();
            h.sql(h.ddl, &format!("DELETE FROM dbo.[{table}] WHERE note=N'selected'; ALTER TABLE dbo.[{table}] DROP CONSTRAINT [{pk}]; ALTER TABLE dbo.[{table}] ALTER COLUMN id {after} NOT NULL; ALTER TABLE dbo.[{table}] ADD CONSTRAINT [{pk}] PRIMARY KEY(id)")).await;
            h.refresh().await;
            let expected = h.sql(h.observer, &format!("SELECT id,note FROM dbo.[{table}]")).await;
            let applied = h.engine.table_apply(h.editor, update(&table, key, "note", "wrong row")).await;
            let persisted = h.sql(h.observer, &format!("SELECT id,note FROM dbo.[{table}]")).await;
            eprintln!("old grid {before} -> {after} apply={applied:?}; persisted={persisted:?}");
            assert_eq!(persisted, expected, "{before} -> {after}");
            loss_refused(applied);
            let mut edit = changes(&table);
            edit.deletes.push(vec![cell("id", key)]);
            loss_refused(h.engine.table_apply(h.editor, edit).await);
            assert_eq!(h.sql(h.observer, &format!("SELECT id,note FROM dbo.[{table}]")).await, expected);
        }
    }.boxed()).await;
}

#[tokio::test]
async fn text_families_validate_inserts_and_assignments_and_preserve_fixed_padding() {
    check(|h| async move {
        let table = h.object("TABLE");
        h.sql(h.ddl, &format!("CREATE TABLE dbo.[{table}] (id int PRIMARY KEY, narrow varchar(3) COLLATE SQL_Latin1_General_CP1_CI_AS, fixed char(6) COLLATE SQL_Latin1_General_CP1_CI_AS, legacy text COLLATE SQL_Latin1_General_CP1_CI_AS, wide nchar(6), unlimited ntext); INSERT INTO dbo.[{table}] VALUES(1,'old','old','old',N'old',N'old')")).await;
        h.refresh().await;
        h.sql(h.editor, "SET ANSI_WARNINGS OFF").await;
        for column in ["narrow", "fixed", "legacy"] {
            loss_refused(h.engine.table_apply(h.editor, update(&table, "1", column, "漢字")).await);
            let mut edit = changes(&table);
            edit.inserts.push(vec![cell("id", "2"), cell(column, "漢字")]);
            loss_refused(h.engine.table_apply(h.editor, edit).await);
        }
        for column in ["narrow", "fixed", "wide"] {
            loss_refused(h.engine.table_apply(h.editor, update(&table, "1", column, "abcdefghi")).await);
        }
        assert_eq!(h.sql(h.observer, &format!("SELECT id,narrow,fixed,CONVERT(nvarchar(max),legacy),wide,CONVERT(nvarchar(max),unlimited) FROM dbo.[{table}]")).await,
            vec![vec![Some("1".into()), Some("old".into()), Some("old   ".into()), Some("old".into()), Some("old   ".into()), Some("old".into())]]);
        let mut edit = update(&table, "1", "narrow", "é");
        edit.updates[0].set.extend([cell("fixed", "café"), cell("legacy", "café"), cell("wide", "漢字"), cell("unlimited", "漢字 😀")]);
        assert!(h.engine.table_apply(h.editor, edit).await.unwrap().committed);
        let mut edit = changes(&table);
        edit.inserts.push(vec![cell("id", "2"), cell("narrow", "abc"), cell("fixed", "x"), cell("legacy", "é"), cell("wide", "😀"), cell("unlimited", "Unicode")]);
        assert!(h.engine.table_apply(h.editor, edit).await.unwrap().committed);
        assert_eq!(h.sql(h.observer, &format!("SELECT id,narrow,fixed,CONVERT(nvarchar(max),legacy),wide,CONVERT(nvarchar(max),unlimited) FROM dbo.[{table}] ORDER BY id")).await,
            vec![vec![Some("1".into()), Some("é".into()), Some("café  ".into()), Some("café".into()), Some("漢字    ".into()), Some("漢字 😀".into())],
                vec![Some("2".into()), Some("abc".into()), Some("x     ".into()), Some("é".into()), Some("😀    ".into()), Some("Unicode".into())]]);
    }.boxed()).await;
}

#[tokio::test]
async fn live_utf8_collation_preserves_unicode_and_checks_byte_capacity() {
    check(|h| async move {
        let supported = h.sql(h.observer, "SELECT COUNT_BIG(*) FROM sys.fn_helpcollations() WHERE name=N'Latin1_General_100_CI_AS_SC_UTF8'").await;
        if supported[0][0].as_deref() == Some("0") { eprintln!("UTF8 collations unavailable on this SQL Server"); return; }
        let table = h.object("TABLE");
        h.sql(h.ddl, &format!("CREATE TABLE dbo.[{table}] (id int PRIMARY KEY, note varchar(6) COLLATE Latin1_General_100_CI_AS_SC_UTF8, fixed char(6) COLLATE Latin1_General_100_CI_AS_SC_UTF8); INSERT INTO dbo.[{table}] VALUES(1,'old','old')")).await;
        h.refresh().await;
        h.sql(h.editor, "SET ANSI_WARNINGS OFF").await;
        loss_refused(h.engine.table_apply(h.editor, update(&table, "1", "note", "漢字!")).await);
        let mut edit = update(&table, "1", "note", "漢字");
        edit.updates[0].set.push(cell("fixed", "é"));
        assert!(h.engine.table_apply(h.editor, edit).await.unwrap().committed);
        assert_eq!(h.sql(h.observer, &format!("SELECT CONVERT(nvarchar(max),note),DATALENGTH(note),CONVERT(nvarchar(max),fixed),DATALENGTH(fixed) FROM dbo.[{table}]")).await,
            vec![vec![Some("漢字".into()), Some("6".into()), Some("é    ".into()), Some("6".into())]]);
        assert!(h.engine.table_apply(h.editor, update(&table, "1", "note", "😀")).await.unwrap().committed);
        assert_eq!(h.sql(h.observer, &format!("SELECT CONVERT(nvarchar(max),note),DATALENGTH(note) FROM dbo.[{table}]")).await, vec![vec![Some("😀".into()), Some("4".into())]]);
    }.boxed()).await;
}

#[tokio::test]
async fn column_codepage_is_read_live_for_assignments_and_old_keys() {
    check(|h| async move {
        let table = h.object("TABLE");
        let pk = format!("{table}_pk");
        h.sql(h.ddl, &format!("CREATE TABLE dbo.[{table}] (id varchar(20) COLLATE Chinese_PRC_CI_AS NOT NULL CONSTRAINT [{pk}] PRIMARY KEY, note varchar(20) COLLATE SQL_Latin1_General_CP1_CI_AS); INSERT INTO dbo.[{table}] VALUES(N'漢字','old')")).await;
        h.refresh().await;
        let key = h.page(&table).await[0][0].clone().unwrap();
        assert_eq!(key, "漢字");
        h.sql(h.ddl, &format!("ALTER TABLE dbo.[{table}] ALTER COLUMN note varchar(20) COLLATE Chinese_PRC_CI_AS NULL")).await;
        assert!(h.engine.table_apply(h.editor, update(&table, &key, "note", "漢字")).await.unwrap().committed);
        assert_eq!(h.sql(h.observer, &format!("SELECT CONVERT(nvarchar(max),note) FROM dbo.[{table}]")).await, vec![vec![Some("漢字".into())]]);
        h.sql(h.ddl, &format!("ALTER TABLE dbo.[{table}] DROP CONSTRAINT [{pk}]; ALTER TABLE dbo.[{table}] ALTER COLUMN id varchar(20) COLLATE SQL_Latin1_General_CP1_CI_AS NOT NULL; ALTER TABLE dbo.[{table}] ADD CONSTRAINT [{pk}] PRIMARY KEY(id)")).await;
        h.refresh().await;
        let expected = h.sql(h.observer, &format!("SELECT id,CONVERT(nvarchar(max),note) FROM dbo.[{table}]")).await;
        assert_eq!(expected[0][0].as_deref(), Some("??"));
        loss_refused(h.engine.table_apply(h.editor, update(&table, &key, "note", "wrong row")).await);
        let mut edit = changes(&table);
        edit.deletes.push(vec![cell("id", &key)]);
        loss_refused(h.engine.table_apply(h.editor, edit).await);
        assert_eq!(h.sql(h.observer, &format!("SELECT id,CONVERT(nvarchar(max),note) FROM dbo.[{table}]")).await, expected);
    }.boxed()).await;
}

#[tokio::test]
async fn every_conversion_is_checked_before_any_mutation_and_savepoint_failure_keeps_prior_work() {
    check(|h| async move {
        let sequence = h.object("SEQUENCE");
        let table = h.object("TABLE");
        let trigger = format!("{table}_trigger");
        h.sql(h.ddl, &format!("CREATE SEQUENCE dbo.[{sequence}] AS bigint START WITH 100; CREATE TABLE dbo.[{table}] (id int PRIMARY KEY, note varchar(3)); INSERT INTO dbo.[{table}] VALUES(1,'old'),(2,'old')")).await;
        h.sql(h.ddl, &format!("CREATE TRIGGER dbo.[{trigger}] ON dbo.[{table}] AFTER INSERT,UPDATE,DELETE AS BEGIN SET NOCOUNT ON; DECLARE @attempt bigint; SELECT @attempt=NEXT VALUE FOR dbo.[{sequence}]; END")).await;
        h.refresh().await;
        h.sql(h.editor, "SET ANSI_WARNINGS OFF").await;
        let attempts = format!("SELECT CONVERT(bigint,last_used_value) FROM sys.sequences WHERE object_id=OBJECT_ID(N'dbo.{sequence}')");
        assert_eq!(h.sql(h.observer, &attempts).await, vec![vec![None]]);
        for manual in [false, true] {
            if manual {
                h.configure(TxnMode::Manual).await;
                h.sql(h.editor, &format!("UPDATE dbo.[{table}] SET note='pre' WHERE id=1")).await;
            }
            let before_attempts = h.sql(h.observer, &attempts).await;
            let mut edit = update(&table, "1", "note", "abcdef");
            edit.deletes.push(vec![cell("id", "2")]);
            edit.inserts.push(vec![cell("id", "3"), cell("note", "new")]);
            loss_refused(h.engine.table_apply(h.editor, edit).await);
            assert_eq!(h.sql(h.observer, &attempts).await, before_attempts, "validation must happen before the first DELETE, including work rolled back later");
            assert_eq!((h.state(h.editor).txn, h.state(h.editor).txn_depth), (if manual { TxnState::Active } else { TxnState::Idle }, Some(u32::from(manual))));
            if manual { h.engine.console_commit(h.editor).await.unwrap(); }
            assert_eq!(h.sql(h.observer, &format!("SELECT id,note FROM dbo.[{table}] ORDER BY id")).await,
                vec![vec![Some("1".into()), Some(if manual { "pre" } else { "old" }.into())], vec![Some("2".into()), Some("old".into())]]);
        }
    }.boxed()).await;
}

#[tokio::test]
async fn schema_drift_refuses_before_any_mutation_and_preserves_the_outer_transaction() {
    check(|h| async move {
        let sequence = h.object("SEQUENCE");
        let table = h.object("TABLE");
        let prior = h.object("TABLE");
        let pk = format!("{table}_pk");
        let trigger = format!("{table}_trigger");
        h.sql(h.ddl, &format!("CREATE SEQUENCE dbo.[{sequence}] AS bigint START WITH 100; CREATE TABLE dbo.[{table}] (id decimal(8,2) NOT NULL CONSTRAINT [{pk}] PRIMARY KEY, note nvarchar(40)); INSERT INTO dbo.[{table}] VALUES(1.23,N'original'); CREATE TABLE dbo.[{prior}] (n int); INSERT INTO dbo.[{prior}] VALUES(1)")).await;
        h.sql(h.ddl, &format!("CREATE TRIGGER dbo.[{trigger}] ON dbo.[{table}] AFTER INSERT,UPDATE,DELETE AS BEGIN SET NOCOUNT ON; DECLARE @attempt bigint; SELECT @attempt=NEXT VALUE FOR dbo.[{sequence}]; END")).await;
        h.refresh().await;
        h.sql(h.ddl, &format!("ALTER TABLE dbo.[{table}] DROP CONSTRAINT [{pk}]; ALTER TABLE dbo.[{table}] ALTER COLUMN id decimal(8,3) NOT NULL; ALTER TABLE dbo.[{table}] ADD CONSTRAINT [{pk}] PRIMARY KEY(id); INSERT INTO dbo.[{table}] VALUES(1.234,N'new precision')")).await;
        let attempts = format!("SELECT CONVERT(bigint,last_used_value) FROM sys.sequences WHERE object_id=OBJECT_ID(N'dbo.{sequence}')");
        let baseline = h.sql(h.observer, &attempts).await;
        assert_eq!(baseline, vec![vec![Some("100".into())]], "the sentinel must observe a real write");
        h.configure(TxnMode::Manual).await;
        h.sql(h.editor, &format!("UPDATE dbo.[{prior}] SET n=8")).await;
        let mut edit = update(&table, "1.234", "note", "wrong row");
        edit.deletes.push(vec![cell("id", "1.230")]);
        edit.inserts.push(vec![cell("id", "2.00"), cell("note", "new")]);
        drift_refused(h.engine.table_apply(h.editor, edit).await);
        assert_eq!(h.sql(h.observer, &attempts).await, baseline, "schema drift must refuse before even a rollbackable first DELETE");
        assert_eq!((h.state(h.editor).txn, h.state(h.editor).txn_depth), (TxnState::Active, Some(1)));
        h.engine.console_commit(h.editor).await.unwrap();
        assert_eq!(h.sql(h.observer, &format!("SELECT n FROM dbo.[{prior}]")).await, vec![vec![Some("8".into())]]);
        assert_eq!(h.sql(h.observer, &format!("SELECT id,note FROM dbo.[{table}] ORDER BY id")).await,
            vec![vec![Some("1.230".into()), Some("original".into())], vec![Some("1.234".into()), Some("new precision".into())]]);
    }.boxed()).await;
}

#[tokio::test]
async fn unchanged_native_keys_round_trip_and_only_the_selected_row_changes() {
    check(|h| async move {
        for (native, value) in [
            ("bit", "1"), ("bigint", "9223372036854775807"), ("decimal(38,12)", "12345678901234567890.123456789012"),
            ("money", "922337203685477.5807"), ("real", "0.1"), ("float", "0.1"),
            ("uniqueidentifier", "abcd1234-5678-90ab-cdef-1234567890ab"), ("binary(4)", "0x0001FEFF"), ("varbinary(4)", "0x0001FEFF"),
            ("char(4)", "ab"), ("nchar(4)", "漢字"), ("varchar(20)", "café"), ("nvarchar(20)", "漢字 😀"),
        ] {
            let table = h.object("TABLE");
            let literal = if value.starts_with("0x") { value.to_owned() } else { format!("N'{value}'") };
            h.sql(h.ddl, &format!("CREATE TABLE dbo.[{table}] (id {native} PRIMARY KEY, note nvarchar(40)); INSERT INTO dbo.[{table}] VALUES(CONVERT({native},{literal}),N'original')")).await;
            h.refresh().await;
            let key = h.page(&table).await[0][0].clone().unwrap();
            assert!(h.engine.table_apply(h.editor, update(&table, &key, "note", "selected")).await.unwrap().committed, "{native}");
            assert_eq!(h.sql(h.observer, &format!("SELECT note FROM dbo.[{table}]")).await, vec![vec![Some("selected".into())]], "{native}");
            let mut edit = changes(&table);
            edit.deletes.push(vec![cell("id", &key)]);
            assert!(h.engine.table_apply(h.editor, edit).await.unwrap().committed, "{native}");
            assert_eq!(h.sql(h.observer, &format!("SELECT COUNT_BIG(*) FROM dbo.[{table}]")).await, vec![vec![Some("0".into())]], "{native}");
        }
    }.boxed()).await;
}

#[tokio::test]
async fn table_column_and_primary_key_identity_drift_refuse_the_entire_apply() {
    check(|h| async move {
        for drift in ["object", "column", "primary key", "type"] {
            let table = h.object("TABLE");
            let pk = format!("{table}_pk");
            h.sql(h.ddl, &format!("CREATE TABLE dbo.[{table}] (id int NOT NULL CONSTRAINT [{pk}] PRIMARY KEY, note varchar(20) NOT NULL); INSERT INTO dbo.[{table}] VALUES(1,'old')")).await;
            h.refresh().await;
            let ddl = match drift {
                "object" => format!("DROP TABLE dbo.[{table}]; CREATE TABLE dbo.[{table}] (id int NOT NULL CONSTRAINT [{pk}] PRIMARY KEY, note varchar(20) NOT NULL); INSERT INTO dbo.[{table}] VALUES(1,'old')"),
                "column" => format!("ALTER TABLE dbo.[{table}] DROP COLUMN note; ALTER TABLE dbo.[{table}] ADD note varchar(20) NOT NULL DEFAULT 'old'"),
                "primary key" => format!("ALTER TABLE dbo.[{table}] DROP CONSTRAINT [{pk}]; ALTER TABLE dbo.[{table}] ADD CONSTRAINT [{pk}] PRIMARY KEY(note)"),
                _ => format!("ALTER TABLE dbo.[{table}] ALTER COLUMN note varchar(10) NOT NULL"),
            };
            h.sql(h.ddl, &ddl).await;
            drift_refused(h.engine.table_apply(h.editor, update(&table, "1", "note", "changed")).await);
            assert_eq!(h.sql(h.observer, &format!("SELECT id,note FROM dbo.[{table}]")).await, vec![vec![Some("1".into()), Some("old".into())]], "{drift}");
        }
    }.boxed()).await;
}

#[tokio::test]
async fn an_empty_table_is_locked_until_its_manual_transaction_finishes() {
    check(|h| async move {
        let table = h.object("TABLE");
        h.sql(h.ddl, &format!("CREATE TABLE dbo.[{table}] (id int PRIMARY KEY, note varchar(20))")).await;
        h.refresh().await;
        h.configure(TxnMode::Manual).await;
        assert!(!h.engine.table_apply(h.editor, changes(&table)).await.unwrap().committed);
        assert_eq!(h.sql(h.editor, &format!("SELECT COUNT_BIG(*) FROM dbo.[{table}]")).await, vec![vec![Some("0".into())]]);
        let locks = h.sql(h.observer, &format!("SELECT CASE WHEN EXISTS (SELECT 1 FROM sys.dm_tran_locks WHERE request_session_id={} AND resource_type='OBJECT' AND resource_associated_entity_id=OBJECT_ID(N'dbo.{table}') AND resource_database_id=DB_ID() AND request_mode='X' AND request_status='GRANT') THEN 1 ELSE 0 END", h.pid(h.editor))).await;
        assert_eq!(locks, vec![vec![Some("1".into())]], "even an empty table must have a granted object X lock");
        let blocked = h.run(h.ddl, &format!("SET LOCK_TIMEOUT 150; ALTER TABLE dbo.[{table}] ADD added int")).await;
        assert!(matches!(blocked.run, RunState::Finished { outcome: RunOutcome::Failed, .. }), "{:?}", blocked.run);
        let RunState::Finished { statements, .. } = &blocked.run else { unreachable!() };
        assert!(statements.iter().any(|s| matches!(&s.status, StmtStatus::Failed { error } if error.detail.as_deref().is_some_and(|d| d.contains("1222")))), "{statements:?}");
        h.engine.console_rollback(h.editor).await.unwrap();
        h.sql(h.ddl, &format!("ALTER TABLE dbo.[{table}] ADD added int")).await;
        assert_eq!(h.sql(h.observer, &format!("SELECT COUNT_BIG(*) FROM dbo.[{table}]")).await, vec![vec![Some("0".into())]]);
    }.boxed()).await;
}

#[tokio::test]
async fn concurrent_ddl_waits_for_the_checked_edit_and_auto_commit_releases_the_lock() {
    check(|h| async move {
        let table = h.object("TABLE");
        h.sql(h.ddl, &format!("CREATE TABLE dbo.[{table}] (id int PRIMARY KEY, note varchar(20)); INSERT INTO dbo.[{table}] VALUES(1,'old')")).await;
        h.refresh().await;
        h.configure(TxnMode::Manual).await;
        assert!(!h.engine.table_apply(h.editor, update(&table, "1", "note", "manual")).await.unwrap().committed);
        h.sql(h.ddl, "SELECT @@SPID").await;
        let ddl_pid = h.pid(h.ddl);
        let owner_pid = h.pid(h.editor);
        let accepted = h.engine.console_run(h.ddl, RunRequest::Script { text: format!("SET LOCK_TIMEOUT 10000; ALTER TABLE dbo.[{table}] ALTER COLUMN note varchar(30) NULL"), target: RunTarget::All }).await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let waiting = h.sql(h.observer, &format!("SELECT COUNT_BIG(*) FROM sys.dm_exec_requests WHERE session_id={ddl_pid} AND blocking_session_id={owner_pid} AND wait_type=N'LCK_M_SCH_M'")).await;
                if waiting[0][0].as_deref() == Some("1") { break; }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.expect("concurrent DDL must wait on the edit transaction's schema-incompatible table lock");
        h.engine.console_commit(h.editor).await.unwrap();
        let done = h.finish(h.ddl, accepted.run_id).await;
        assert!(matches!(done.run, RunState::Finished { outcome: RunOutcome::Ok, .. }), "{:?}", done.run);
        assert_eq!(h.sql(h.observer, &format!("SELECT note FROM dbo.[{table}]")).await, vec![vec![Some("manual".into())]]);
        h.refresh().await;
        h.configure(TxnMode::AutoCommit).await;
        assert!(h.engine.table_apply(h.editor, update(&table, "1", "note", "auto")).await.unwrap().committed);
        h.sql(h.ddl, &format!("SET LOCK_TIMEOUT 150; ALTER TABLE dbo.[{table}] ALTER COLUMN note varchar(40) NULL")).await;
        assert_eq!(h.sql(h.observer, &format!("SELECT note FROM dbo.[{table}]")).await, vec![vec![Some("auto".into())]]);
        h.refresh().await;
        h.configure(TxnMode::Manual).await;
        assert!(!h.engine.table_apply(h.editor, update(&table, "1", "note", "cancelled")).await.unwrap().committed);
        let owner_pid = h.pid(h.editor);
        let accepted = h.engine.console_run(h.editor, RunRequest::Script { text: "WAITFOR DELAY '00:00:30'".into(), target: RunTarget::All }).await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let waiting = h.sql(h.observer, &format!("SELECT COUNT_BIG(*) FROM sys.dm_exec_requests WHERE session_id={owner_pid} AND command='WAITFOR'")).await;
                if waiting[0][0].as_deref() == Some("1") { break; }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.expect("the edit transaction must still own the session during WAITFOR");
        h.engine.console_cancel(h.editor).await;
        let cancelled = h.finish(h.editor, accepted.run_id).await;
        assert!(matches!(cancelled.session, SessionState::Lost { transaction: TransactionLoss::Unknown, .. }));
        h.sql(h.ddl, &format!("SET LOCK_TIMEOUT 2000; ALTER TABLE dbo.[{table}] ALTER COLUMN note varchar(50) NULL")).await;
        assert_eq!(h.sql(h.observer, &format!("SELECT note FROM dbo.[{table}]")).await, vec![vec![Some("auto".into())]]);
    }.boxed()).await;
}
