use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use futures_util::StreamExt;

use crate::console::{self, loss_line, statement_line, Console, ConsoleInner, Orphaned, Work};
use crate::model::*;
use crate::results::{Caps, Origin, Rerun, ResultSet};
use crate::session::{self, QueryEvent, Session};
use crate::sql::StmtSpan;
use crate::store::HistoryInsert;
use crate::writes;
use crate::Shared;

pub(crate) enum ExplainMode {
    Explain,
    Analyze,
}

impl RunMode {
    pub(crate) fn explain(self) -> Option<ExplainMode> {
        match self {
            RunMode::Run => None,
            RunMode::Explain => Some(ExplainMode::Explain),
            RunMode::ExplainAnalyze => Some(ExplainMode::Analyze),
        }
    }
}

#[derive(Debug)]
enum Bracket {
    IdleExplain { body: String },
    IdleAnalyze { body: String },
    ActiveExplain { body: String },
    ActiveAnalyze { body: String },
}

impl Bracket {
    fn new(mode: ExplainMode, txn: TxnState, statement: &str) -> Self {
        let body = match mode {
            ExplainMode::Explain => format!("EXPLAIN (FORMAT JSON) {statement}"),
            ExplainMode::Analyze => format!("EXPLAIN (FORMAT JSON, ANALYZE, BUFFERS) {statement}"),
        };
        match (txn, mode) {
            (TxnState::Idle, ExplainMode::Explain) => Bracket::IdleExplain { body },
            (TxnState::Idle, ExplainMode::Analyze) => Bracket::IdleAnalyze { body },
            (TxnState::Active, ExplainMode::Explain) => Bracket::ActiveExplain { body },
            (TxnState::Active, ExplainMode::Analyze) => Bracket::ActiveAnalyze { body },
            (TxnState::Aborted | TxnState::Open, _) => {
                unreachable!("aborted and open are refused before a bracket")
            }
        }
    }

    fn body(&self) -> &str {
        match self {
            Bracket::IdleExplain { body }
            | Bracket::IdleAnalyze { body }
            | Bracket::ActiveExplain { body }
            | Bracket::ActiveAnalyze { body } => body,
        }
    }

    fn open(&self) -> &str {
        match self {
            Bracket::IdleExplain { .. } => "BEGIN READ ONLY",
            Bracket::IdleAnalyze { .. } => "BEGIN",
            Bracket::ActiveExplain { .. } => "SAVEPOINT l1_explain; SET LOCAL transaction_read_only = on",
            Bracket::ActiveAnalyze { .. } => "SAVEPOINT l1_explain",
        }
    }

    fn close(&self) -> &str {
        match self {
            Bracket::IdleExplain { .. } | Bracket::IdleAnalyze { .. } => "ROLLBACK",
            Bracket::ActiveExplain { .. } | Bracket::ActiveAnalyze { .. } => {
                "ROLLBACK TO SAVEPOINT l1_explain; RELEASE SAVEPOINT l1_explain"
            }
        }
    }

    fn restores(&self) -> TxnState {
        match self {
            Bracket::IdleExplain { .. } | Bracket::IdleAnalyze { .. } => TxnState::Idle,
            Bracket::ActiveExplain { .. } | Bracket::ActiveAnalyze { .. } => TxnState::Active,
        }
    }

    fn sent(&self) -> [&str; 3] {
        [self.open(), self.body(), self.close()]
    }
}

#[derive(Debug)]
pub(crate) struct Prepared {
    bracket: Bracket,
    checked: writes::WritesChecked,
}

pub(crate) fn prepare(
    mode: ExplainMode,
    kind: DatabaseKind,
    txn: TxnState,
    text: &str,
    spans: &[StmtSpan],
    target: &writes::Target,
    confirmed: Option<&WriteDigest>,
) -> Result<Prepared, RunError> {
    if kind != DatabaseKind::Postgres {
        return Err(unsupported("Explain is only for PostgreSQL."));
    }
    if spans.is_empty() {
        return Err(RunError::NoStatement);
    }
    if spans.len() != 1 {
        return Err(unsupported("Explain one statement."));
    }
    let span = StmtSpan { start: spans[0].start, end: spans[0].end };
    let statement = &text[span.start..span.end];
    if starts_with_explain(statement) {
        return Err(unsupported("The statement already starts with EXPLAIN."));
    }
    match txn {
        TxnState::Aborted => return Err(unsupported("The transaction must be rolled back first.")),
        TxnState::Open => return Err(unsupported("Explain is only for PostgreSQL.")),
        TxnState::Idle | TxnState::Active => {}
    }
    let bracket = Bracket::new(mode, txn, statement);
    let sent = bracket.sent();
    match writes::check(target, &sent, confirmed) {
        Ok(checked) => Ok(Prepared { bracket, checked }),
        Err(confirmation) => Err(RunError::ConfirmWrites { confirmation }),
    }
}

fn unsupported(what: &str) -> RunError {
    RunError::Unsupported { what: what.to_string() }
}

fn starts_with_explain(sql: &str) -> bool {
    let rest = skip_noise(sql);
    let Some(head) = rest.get(..7) else {
        return false;
    };
    if !head.eq_ignore_ascii_case("explain") {
        return false;
    }
    !rest.as_bytes().get(7).copied().is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn skip_noise(sql: &str) -> &str {
    let mut rest = sql;
    loop {
        let trimmed = rest.trim_start();
        if let Some(comment) = trimmed.strip_prefix("--") {
            rest = match comment.find('\n') {
                Some(index) => &comment[index + 1..],
                None => "",
            };
            continue;
        }
        if let Some(comment) = trimmed.strip_prefix("/*") {
            rest = match comment.find("*/") {
                Some(index) => &comment[index + 2..],
                None => return "",
            };
            continue;
        }
        return trimmed;
    }
}

pub(crate) struct Launch {
    session: Session,
    stop: Arc<AtomicBool>,
    bracket: Bracket,
    schema: Option<String>,
    run_id: RunId,
    connection: ConnectionConfig,
    database: String,
    entered: TxnState,
}

pub(crate) fn launch(
    console: &Console,
    inner: &mut ConsoleInner,
    run_id: RunId,
    prepared: Prepared,
    schema: Option<String>,
    connection: ConnectionConfig,
    database: String,
    entered: TxnState,
) -> Result<Launch, RunError> {
    let Prepared { bracket, checked } = prepared;
    let Some((session, stop)) = console.take_session(inner, run_id, Work::Run(checked)) else {
        return Err(RunError::Busy);
    };
    Ok(Launch { session, stop, bracket, schema, run_id, connection, database, entered })
}

pub(crate) async fn run(console: Arc<Console>, shared: Arc<Shared>, launch: Launch) -> Result<(), Orphaned> {
    let Launch { mut session, stop, bracket, schema, run_id, connection, database, entered } = launch;
    let body = bracket.body().to_owned();
    let restored = bracket.restores();
    let started = Instant::now();
    let settings = shared.settings();
    let caps = Caps { max_rows: settings.max_rows, max_bytes: settings.max_result_bytes };

    if session.search_path != schema {
        if let Err(error) = session.set_search_path(schema.as_deref()).await {
            return finish(&console, &shared, session, run_id, &body, &connection, &database, started, entered, None, failed(error)).await;
        }
    }
    if let Err(error) = session.batch(bracket.open()).await {
        return finish(&console, &shared, session, run_id, &body, &connection, &database, started, entered, None, failed(error)).await;
    }
    console.output(&shared, run_id, statement_line(0, &body));
    let read = read_plan(&mut session, &body, &stop).await;
    console.output_all(&shared, run_id, console::notice_lines(&mut session));
    if let Err(error) = session.batch(bracket.close()).await {
        let lost = Some(error.message.clone());
        let end = match read {
            Read::Cancelled => cancelled(),
            Read::Failed(error) => failed(error),
            Read::Ready { .. } => failed(error),
        };
        return finish(&console, &shared, session, run_id, &body, &connection, &database, started, entered, lost, end).await;
    }
    match read {
        Read::Cancelled => finish(&console, &shared, session, run_id, &body, &connection, &database, started, restored, None, cancelled()).await,
        Read::Failed(error) => finish(&console, &shared, session, run_id, &body, &connection, &database, started, restored, None, failed(error)).await,
        Read::Ready { cell, columns } => {
            let output = match store_plan(&console, run_id, &schema, &connection, &database, &cell, columns, &caps, started) {
                Ok(output) => output,
                Err(error) => return finish(&console, &shared, session, run_id, &body, &connection, &database, started, restored, None, failed(error)).await,
            };
            finish(&console, &shared, session, run_id, &body, &connection, &database, started, restored, None, ok_plan(output)).await
        }
    }
}

enum Read {
    Ready { cell: String, columns: Vec<ColumnMeta> },
    Failed(QueryError),
    Cancelled,
}

async fn read_plan(session: &mut Session, sql: &str, stop: &AtomicBool) -> Read {
    let mut stream = session.query(sql);
    let mut columns = None;
    let mut cell = None;
    let mut failure = None;
    while let Some(event) = stream.next().await {
        match event {
            Err(error) => {
                failure = Some(error);
                break;
            }
            Ok(QueryEvent::Prepared) if stop.load(Ordering::Relaxed) => {
                failure = Some(session::error("Cancelled"));
                break;
            }
            Ok(QueryEvent::Prepared) | Ok(QueryEvent::Notice(_)) => {}
            Ok(QueryEvent::Columns(next)) => columns = Some(next),
            Ok(QueryEvent::Row(row)) if cell.is_none() => cell = row.into_iter().next().flatten(),
            Ok(QueryEvent::Row(_)) | Ok(QueryEvent::Complete { .. }) => {}
            Ok(QueryEvent::ServerError(error)) => {
                failure = Some(error);
                break;
            }
        }
    }
    drop(stream);
    if stop.load(Ordering::Relaxed) && cell.is_none() {
        return Read::Cancelled;
    }
    if let Some(error) = failure {
        return if stop.load(Ordering::Relaxed) { Read::Cancelled } else { Read::Failed(error) };
    }
    let Some(mut columns) = columns.filter(|columns| !columns.is_empty()) else {
        return Read::Failed(session::error("Driver returned a row without result metadata"));
    };
    let Some(cell) = cell else {
        return Read::Failed(session::error("The result is not a plan."));
    };
    columns.truncate(1);
    Read::Ready { cell, columns }
}

fn store_plan(
    console: &Console,
    run_id: RunId,
    schema: &Option<String>,
    connection: &ConnectionConfig,
    database: &str,
    cell: &str,
    columns: Vec<ColumnMeta>,
    caps: &Caps,
    started: Instant,
) -> Result<RunOutput, QueryError> {
    if cell.len() as u64 > caps.max_bytes {
        return Err(session::error("The plan exceeds the result size limit."));
    }
    let set = Arc::new(ResultSet::new(
        ResultRef { tab_id: console.tab, run_id, index: 0 },
        DatabaseRef { connection_id: connection.id, database: database.to_owned() },
        DatabaseKind::Postgres,
        columns,
        Origin { rerun: Rerun::Refused("Explain is not re-run."), nth: 0, schema: schema.clone(), page: false },
    ));
    set.push_row(std::iter::once(Some(cell)), caps);
    set.set_elapsed(elapsed_ms(started));
    if set.meta().truncated.is_some() {
        return Err(session::error("The plan exceeds the result size limit."));
    }
    console.results.lock().unwrap().push(set.clone());
    Ok(RunOutput::Plan { result: set.meta(), format: PlanFormat::Json })
}

struct End {
    status: StmtStatus,
    outcome: RunOutcome,
    outputs: Vec<RunOutput>,
    history: HistoryOutcome,
    error: Option<QueryError>,
}

fn failed(error: QueryError) -> End {
    End {
        history: HistoryOutcome::Error { message: error.message.clone() },
        status: StmtStatus::Failed { error: error.clone() },
        outcome: RunOutcome::Failed,
        outputs: Vec::new(),
        error: Some(error),
    }
}

fn cancelled() -> End {
    End { status: StmtStatus::Cancelled, outcome: RunOutcome::Cancelled, outputs: Vec::new(), history: HistoryOutcome::Cancelled, error: None }
}

fn ok_plan(output: RunOutput) -> End {
    let rows = match &output {
        RunOutput::Plan { result, .. } => result.row_count,
        _ => 0,
    };
    End { status: StmtStatus::Finished, outcome: RunOutcome::Ok, outputs: vec![output], history: HistoryOutcome::Rows { rows: u64::from(rows) }, error: None }
}

async fn finish(
    console: &Console,
    shared: &Arc<Shared>,
    session: Session,
    run_id: RunId,
    body: &str,
    connection: &ConnectionConfig,
    database: &str,
    started: Instant,
    txn: TxnState,
    lost: Option<String>,
    end: End,
) -> Result<(), Orphaned> {
    if let Some(reason) = &lost {
        console.output(shared, run_id, loss_line(&session, txn, reason));
    }
    if let Some(error) = &end.error {
        console.output(shared, run_id, OutputBody::Error { statement: Some(0), error: error.clone() });
    }
    let elapsed = elapsed_ms(started);
    let End { status, outcome, outputs, history, error: _ } = end;
    let due = console
        .edit(shared, run_id, |inner| {
            let statements = Console::statements_mut(inner);
            if let Some(statement) = statements.get_mut(0) {
                statement.status = status;
                statement.outputs = outputs;
            }
            let due = Console::return_session(inner, session, txn, lost);
            let statements = Console::statements_mut(inner).clone();
            inner.run = RunState::Finished { run_id, elapsed_ms: elapsed, statements, outcome };
            due
        })
        .await?;
    if let Some((db, owed)) = due {
        shared.schedule_refreshes(db, owed);
    }
    shared.record_history(HistoryInsert {
        database_kind: connection.database_kind,
        at_ms: console::now_ms(),
        connection_id: connection.id,
        connection_name: connection.name.clone(),
        database: database.to_owned(),
        sql: body.to_owned(),
        elapsed_ms: elapsed,
        outcome: history,
    });
    Ok(())
}

fn elapsed_ms(since: Instant) -> u32 {
    since.elapsed().as_millis().min(u32::MAX as u128) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn span_of(text: &str) -> StmtSpan {
        StmtSpan { start: 0, end: text.len() }
    }

    struct Prod {
        saved: ConnectionConfig,
        console: ConsoleConfigView,
        unasked: WriteList,
    }

    impl Prod {
        fn new() -> Self {
            let id = ConnectionId(Uuid::from_u128(1));
            Self {
                saved: ConnectionConfig {
                    database_kind: DatabaseKind::Postgres,
                    id,
                    name: "prod".into(),
                    host: "db".into(),
                    port: 5432,
                    database: "shop".into(),
                    user: "app".into(),
                    ssl_mode: SslMode::Prefer,
                    ca_cert_path: None,
                    has_password: false,
                    color: None,
                    ssh: None,
                    environment: Some(Environment::Production),
                    sql_server_auth: None,
                },
                console: ConsoleConfigView { connection_id: id, database: "shop".into(), schema: None, mode: TxnMode::AutoCommit },
                unasked: WriteList::default(),
            }
        }

        fn target(&self) -> writes::Target<'_> {
            writes::Target { saved: &self.saved, opened: &self.saved, console: &self.console, txn: TxnState::Idle, unasked: &self.unasked }
        }
    }

    #[test]
    fn four_brackets_send_these_statements() {
        let text = "select 1";
        let idle_explain = Bracket::new(ExplainMode::Explain, TxnState::Idle, text);
        assert_eq!(idle_explain.sent(), ["BEGIN READ ONLY", "EXPLAIN (FORMAT JSON) select 1", "ROLLBACK",]);
        assert_eq!(idle_explain.restores(), TxnState::Idle);

        let idle_analyze = Bracket::new(ExplainMode::Analyze, TxnState::Idle, text);
        assert_eq!(idle_analyze.sent(), ["BEGIN", "EXPLAIN (FORMAT JSON, ANALYZE, BUFFERS) select 1", "ROLLBACK",]);
        assert_eq!(idle_analyze.restores(), TxnState::Idle);

        let active_explain = Bracket::new(ExplainMode::Explain, TxnState::Active, text);
        assert_eq!(
            active_explain.sent(),
            [
                "SAVEPOINT l1_explain; SET LOCAL transaction_read_only = on",
                "EXPLAIN (FORMAT JSON) select 1",
                "ROLLBACK TO SAVEPOINT l1_explain; RELEASE SAVEPOINT l1_explain",
            ]
        );
        assert_eq!(active_explain.restores(), TxnState::Active);

        let active_analyze = Bracket::new(ExplainMode::Analyze, TxnState::Active, text);
        assert_eq!(
            active_analyze.sent(),
            [
                "SAVEPOINT l1_explain",
                "EXPLAIN (FORMAT JSON, ANALYZE, BUFFERS) select 1",
                "ROLLBACK TO SAVEPOINT l1_explain; RELEASE SAVEPOINT l1_explain",
            ]
        );
        assert_eq!(active_analyze.restores(), TxnState::Active);
    }

    fn expect_unsupported(err: RunError, what: &str) {
        match err {
            RunError::Unsupported { what: got } => assert_eq!(got, what),
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn refusals() {
        let select = "select 1";
        let one = [span_of(select)];
        let prod = Prod::new();
        let target = prod.target();

        let aborted = prepare(ExplainMode::Explain, DatabaseKind::Postgres, TxnState::Aborted, select, &one, &target, None).unwrap_err();
        expect_unsupported(aborted, "The transaction must be rolled back first.");

        let two = [StmtSpan { start: 0, end: 6 }, StmtSpan { start: 6, end: select.len() }];
        let several = prepare(ExplainMode::Explain, DatabaseKind::Postgres, TxnState::Idle, select, &two, &target, None).unwrap_err();
        expect_unsupported(several, "Explain one statement.");

        let explained = "/* c */ explain select 1";
        let already = prepare(ExplainMode::Explain, DatabaseKind::Postgres, TxnState::Idle, explained, &[span_of(explained)], &target, None).unwrap_err();
        expect_unsupported(already, "The statement already starts with EXPLAIN.");

        let sql_server = prepare(ExplainMode::Analyze, DatabaseKind::SqlServer, TxnState::Idle, select, &one, &target, None).unwrap_err();
        expect_unsupported(sql_server, "Explain is only for PostgreSQL.");

        let none = prepare(ExplainMode::Explain, DatabaseKind::Postgres, TxnState::Idle, select, &[], &target, None).unwrap_err();
        assert!(matches!(none, RunError::NoStatement), "got {none:?}");

        let open = prepare(ExplainMode::Explain, DatabaseKind::Postgres, TxnState::Open, select, &one, &target, None).unwrap_err();
        expect_unsupported(open, "Explain is only for PostgreSQL.");
    }

    #[test]
    fn production_update_analyze_confirms_and_explain_does_not() {
        let text = "update t set n = 1";
        let spans = [span_of(text)];
        let prod = Prod::new();
        let target = prod.target();
        let analyze = prepare(ExplainMode::Analyze, DatabaseKind::Postgres, TxnState::Idle, text, &spans, &target, None);
        let Err(RunError::ConfirmWrites { confirmation }) = analyze else {
            panic!("analyze update should confirm, got {analyze:?}");
        };
        let explain = prepare(ExplainMode::Explain, DatabaseKind::Postgres, TxnState::Idle, text, &spans, &target, None);
        assert!(explain.is_ok(), "explain update should pass: {explain:?}");

        let digest = confirmation.digest.clone();
        let active = prepare(ExplainMode::Analyze, DatabaseKind::Postgres, TxnState::Active, text, &spans, &target, Some(&digest));
        assert!(matches!(active, Err(RunError::ConfirmWrites { .. })), "idle digest must not accept active analyze, got {active:?}");
    }
}
