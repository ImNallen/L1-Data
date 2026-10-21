use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use tokio::sync::{Mutex, watch};

use crate::Shared;
use crate::model::*;
use crate::results::{Caps, ResultSet};
use crate::session::{CancelHandle, Session, QueryEvent};
use crate::sql::{Invalidates, StmtKind, StmtSpan};
use crate::store::HistoryInsert;
use crate::text::{OffsetMap, synthesize_tag};
use crate::txn::{StmtOutcome, TransactionPlan, txn_after};

const STREAM_EMIT_INTERVAL: Duration = Duration::from_millis(100);

pub struct Console {
    pub tab: TabId,
    pub inner: Mutex<ConsoleInner>,
    pub results: std::sync::Mutex<Vec<Arc<ResultSet>>>,
    pub latest: std::sync::Mutex<ConsoleState>,
    pub closed: AtomicBool,
}

pub struct ConsoleInner {
    pub config: ConsoleConfigView,
    pub session: SessionSlot,
    pub txn: TxnState,
    pub txn_depth: Option<u32>,
    pub run: RunState,
    pub rev: u64,
    refreshes: Option<PendingRefreshes>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Owed {
    pub catalog: bool,
    pub databases: bool,
}

impl Owed {
    pub fn add(&mut self, invalidates: Invalidates) {
        match invalidates {
            Invalidates::Nothing => {}
            Invalidates::Catalog => self.catalog = true,
            Invalidates::DatabaseList => self.databases = true,
            Invalidates::Both => {
                self.catalog = true;
                self.databases = true;
            }
        }
    }
}

struct PendingRefreshes {
    db: DatabaseRef,
    policy: RefreshPolicy,
    owed: Owed,
}

#[derive(PartialEq, Eq)]
enum RefreshPolicy {
    Commit,
    NativeEnd,
}

impl ConsoleInner {
    fn invalidate(&mut self, database_kind: DatabaseKind, invalidates: Invalidates) {
        if invalidates == Invalidates::Nothing { return; }
        let db = self.config.db();
        let pending = self.refreshes.get_or_insert_with(|| PendingRefreshes {
            db,
            policy: match database_kind {
                DatabaseKind::Postgres => RefreshPolicy::Commit,
                DatabaseKind::SqlServer => RefreshPolicy::NativeEnd,
            },
            owed: Owed::default(),
        });
        pending.owed.add(invalidates);
    }

    pub fn finish_refreshes(&mut self, committed: bool) -> Option<(DatabaseRef, Owed)> {
        if self.txn != TxnState::Idle { return None; }
        let pending = self.refreshes.take()?;
        // Native batches may have committed DDL before opening the transaction that ended.
        (committed || pending.policy == RefreshPolicy::NativeEnd).then_some((pending.db, pending.owed))
    }
}

pub enum SessionSlot {
    Empty,
    Idle(Session),
    Busy {
        run_id: RunId,
        backend_pid: i32,
        cancel: CancelHandle,
        stop: Arc<AtomicBool>,
        closed: watch::Receiver<Option<String>>,
    },
    Lost {
        reason: String,
        transaction: TransactionLoss,
    },
}

/// Disconnect or close reassigned the slot while a task held the session. The task's edits no
/// longer apply, and it must stop.
pub struct Orphaned;

impl Console {
    pub fn new(tab: TabId, config: ConsoleConfigView) -> Self {
        let inner = ConsoleInner {
            config,
            session: SessionSlot::Empty,
            txn: TxnState::Idle,
            txn_depth: None,
            run: RunState::Idle,
            rev: 0,
            refreshes: None,
        };
        let latest = ConsoleState {
            tab_id: tab,
            rev: 0,
            config: inner.config.clone(),
            session: SessionState::None,
            txn: TxnState::Idle,
            txn_depth: None,
            run: RunState::Idle,
        };
        Self {
            tab,
            inner: Mutex::new(inner),
            results: std::sync::Mutex::new(Vec::new()),
            latest: std::sync::Mutex::new(latest),
            closed: AtomicBool::new(false),
        }
    }

    pub fn result_set(&self, r: ResultRef) -> Option<Arc<ResultSet>> {
        self.results.lock().unwrap().iter().find(|s| s.r#ref == r).cloned()
    }

    pub fn snapshot(&self, inner: &ConsoleInner) -> ConsoleState {
        ConsoleState {
            tab_id: self.tab,
            rev: inner.rev,
            config: inner.config.clone(),
            session: match &inner.session {
                SessionSlot::Empty => SessionState::None,
                SessionSlot::Idle(s) => SessionState::Open { backend_pid: s.backend_pid },
                SessionSlot::Busy { backend_pid, .. } => SessionState::Open { backend_pid: *backend_pid },
                SessionSlot::Lost { reason, transaction } => SessionState::Lost { reason: reason.clone(), transaction: *transaction },
            },
            txn: inner.txn,
            txn_depth: inner.txn_depth,
            run: inner.run.clone(),
        }
    }

    pub fn publish(&self, inner: &mut ConsoleInner, shared: &Shared) -> ConsoleState {
        inner.rev += 1;
        let state = self.snapshot(inner);
        *self.latest.lock().unwrap() = state.clone();
        if !self.closed.load(Ordering::Relaxed) {
            shared.emit(AppEvent::ConsoleChanged(state.clone()));
        }
        state
    }

    pub fn output(&self, shared: &Shared, run_id: RunId, body: OutputBody) {
        self.output_all(shared, run_id, vec![body]);
    }

    pub fn output_all(&self, shared: &Shared, run_id: RunId, bodies: Vec<OutputBody>) {
        if bodies.is_empty() || self.closed.load(Ordering::Relaxed) {
            return;
        }
        let at_ms = now_ms();
        shared.emit(AppEvent::ConsoleOutput {
            tab_id: self.tab,
            entries: bodies.into_iter().map(|body| OutputEntry { at_ms, run_id, body }).collect(),
        });
    }

    pub fn take_session(&self, inner: &mut ConsoleInner, run_id: RunId) -> Option<(Session, Arc<AtomicBool>)> {
        let SessionSlot::Idle(_) = inner.session else { return None };
        let SessionSlot::Idle(session) = std::mem::replace(&mut inner.session, SessionSlot::Empty) else { unreachable!() };
        let stop = Arc::new(AtomicBool::new(false));
        inner.session = SessionSlot::Busy {
            run_id,
            backend_pid: session.backend_pid,
            cancel: session.cancel_handle(),
            stop: stop.clone(),
            closed: session.closed_watch(),
        };
        Some((session, stop))
    }

    /// The one way a task that took the session edits console state. The edit applies and is
    /// published only while `run_id` still owns the slot; otherwise the task is `Orphaned`.
    pub async fn edit<T>(&self, shared: &Shared, run_id: RunId, edit: impl FnOnce(&mut ConsoleInner) -> T) -> Result<T, Orphaned> {
        let mut inner = self.inner.lock().await;
        if !matches!(inner.session, SessionSlot::Busy { run_id: r, .. } if r == run_id) {
            return Err(Orphaned);
        }
        let out = edit(&mut inner);
        self.publish(&mut inner, shared);
        Ok(out)
    }

    /// Puts the session back after a run or a transaction command and applies its final `txn`.
    pub fn return_session(inner: &mut ConsoleInner, session: Session, txn: TxnState, lost: Option<String>) -> Option<(DatabaseRef, Owed)> {
        inner.txn = txn;
        inner.txn_depth = session.txn_depth();
        match lost {
            Some(reason) => {
                let transaction = session.transaction_loss(txn);
                inner.txn = TxnState::Idle;
                inner.txn_depth = None;
                inner.session = SessionSlot::Lost { reason, transaction };
                inner.finish_refreshes(false)
            }
            None => {
                inner.session = SessionSlot::Idle(session);
                None
            }
        }
    }

    /// Cancels any run and drops the session; the console itself stays. A task holding the
    /// session finds itself `Orphaned` at its next edit, so the run is finished here: the
    /// statement in flight is Cancelled and the rest Skipped.
    pub async fn drop_session(&self, shared: &Arc<Shared>) {
        let mut inner = self.inner.lock().await;
        let session = std::mem::replace(&mut inner.session, SessionSlot::Empty);
        if let SessionSlot::Busy { stop, .. } = &session {
            stop.store(true, Ordering::Relaxed);
        }
        inner.txn = TxnState::Idle;
        inner.txn_depth = None;
        let due = inner.finish_refreshes(false);
        if let RunState::Running { run_id, started_at_ms, statements } = &mut inner.run {
            let mut in_flight = true;
            for s in statements.iter_mut() {
                if matches!(s.status, StmtStatus::Pending | StmtStatus::Preparing | StmtStatus::Running) {
                    for output in &mut s.outputs {
                        if let RunOutput::Rows { state, .. } = output {
                            if *state == RowsState::Streaming { *state = RowsState::Cancelled; }
                        }
                    }
                    s.status = if in_flight { StmtStatus::Cancelled } else { StmtStatus::Skipped };
                    in_flight = false;
                }
            }
            inner.run = RunState::Finished {
                run_id: *run_id,
                elapsed_ms: (now_ms() - *started_at_ms).clamp(0, u32::MAX as i64) as u32,
                statements: std::mem::take(statements),
                outcome: RunOutcome::Cancelled,
            };
        }
        self.publish(&mut inner, shared);
        drop(inner);
        match session {
            SessionSlot::Busy { cancel, mut closed, .. } => {
                let worker = shared.clone();
                shared.spawn(async move {
                    cancel.cancel().await;
                    if let Some((db, owed)) = due {
                        // The orphaned task must release its session before metadata can see its final DDL.
                        while closed.changed().await.is_ok() {}
                        worker.schedule_refreshes(db, owed);
                    }
                });
            }
            session => {
                drop(session);
                if let Some((db, owed)) = due { shared.schedule_refreshes(db, owed); }
            }
        }
    }

    pub fn statements_mut(inner: &mut ConsoleInner) -> &mut Vec<StatementRun> {
        match &mut inner.run {
            RunState::Running { statements, .. } | RunState::Finished { statements, .. } => statements,
            RunState::Idle => unreachable!("run state is set before the run task starts"),
        }
    }
}

pub struct RunPlan {
    pub run_id: RunId,
    pub text: String,
    pub statements: Vec<PlannedStmt>,
    pub txn: TxnState,
    pub schema: Option<String>,
    /// The database the session is on; catalog refreshes and history rows name it.
    pub database: String,
    pub connection: ConnectionConfig,
    pub table: Option<Relation>,
}

pub struct PlannedStmt {
    pub span: StmtSpan,
    pub kind: StmtKind,
    pub transaction: TransactionPlan,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn elapsed_ms(since: Instant) -> u32 {
    since.elapsed().as_millis().min(u32::MAX as u128) as u32
}

/// Every console write goes through `Console::edit`, so an orphaned task returns at its next
/// write and drops the session with it.
pub async fn run_task(console: Arc<Console>, shared: Arc<Shared>, mut session: Session, plan: RunPlan, stop: Arc<AtomicBool>) -> Result<(), Orphaned> {
    let started = Instant::now();
    let run_id = plan.run_id;
    let map = OffsetMap::new(&plan.text);
    let settings = shared.settings();
    let caps = Caps { max_rows: settings.max_rows, max_bytes: settings.max_result_bytes };
    let mut txn = plan.txn;
    let mut outcome = RunOutcome::Ok;
    let mut fatal = None;
    if session.search_path != plan.schema {
        if let Err(error) = session.set_search_path(plan.schema.as_deref()).await {
            console.output(&shared, run_id, OutputBody::Error { statement: None, error });
        }
    }
    let mut index = 0;
    while index < plan.statements.len() {
        let stmt = &plan.statements[index];
        let i = index as u16;
        if stop.load(Ordering::Relaxed) {
            set_status(&console, &shared, run_id, i, StmtStatus::Cancelled).await?;
            outcome = RunOutcome::Cancelled;
            break;
        }
        let sql = stmt.span.text(&plan.text);
        if stmt.transaction == TransactionPlan::BeginIfIdle && txn == TxnState::Idle {
            match session.batch("BEGIN").await {
                Ok(()) => {
                    txn = TxnState::Active;
                    console.edit(&shared, run_id, |inner| { inner.txn = txn; inner.txn_depth = session.txn_depth(); }).await?;
                    console.output(&shared, run_id, OutputBody::Info { message: "BEGIN (implicit)".into() });
                }
                Err(error) => {
                    set_status(&console, &shared, run_id, i, StmtStatus::Failed { error: error.clone() }).await?;
                    console.output(&shared, run_id, OutputBody::Error { statement: Some(i), error });
                    outcome = RunOutcome::Failed;
                    break;
                }
            }
        }
        console.output(&shared, run_id, statement_line(i, sql));
        console.edit(&shared, run_id, |inner| {
            if plan.connection.database_kind == DatabaseKind::SqlServer {
                inner.invalidate(DatabaseKind::SqlServer, crate::sql::tsql::invalidates(sql));
            }
            Console::statements_mut(inner)[index].status = StmtStatus::Preparing;
        }).await?;
        let unit_started = Instant::now();
        let result = run_outputs(&console, &shared, &mut session, &plan, index, &caps, &stop).await?;
        let cancelled = stop.load(Ordering::Relaxed) && result.is_err();
        let result = result.map_err(|mut e| {
            e.position = e.position.map(|p| DocOffset(map.to_utf16(stmt.span.start).0.saturating_add(p.0)));
            e
        });
        if let Err(e) = &result { if e.severity == "FATAL" { fatal = Some(e.message.clone()); } }
        txn = session.observed_txn().unwrap_or_else(|| txn_after(txn, &stmt.kind, if result.is_ok() { StmtOutcome::Ok } else { StmtOutcome::Err }));
        let (status, history) = match &result {
            Ok((rows, tags)) => (StmtStatus::Finished, if let Some(rows) = rows { HistoryOutcome::Rows { rows: *rows } } else { HistoryOutcome::Command { tag: tags.join("; ") } }),
            Err(_) if cancelled => (StmtStatus::Cancelled, HistoryOutcome::Cancelled),
            Err(error) => (StmtStatus::Failed { error: error.clone() }, HistoryOutcome::Error { message: error.message.clone() }),
        };
        let due = console.edit(&shared, run_id, |inner| {
            inner.txn = txn;
            inner.txn_depth = session.txn_depth();
            Console::statements_mut(inner)[index].status = status;
            if plan.connection.database_kind == DatabaseKind::Postgres && result.is_ok() {
                inner.invalidate(DatabaseKind::Postgres, stmt.kind.invalidates());
            }
            inner.finish_refreshes(true)
        }).await?;
        if let Some((db, owed)) = due { shared.schedule_refreshes(db, owed); }
        let mut lines = notice_lines(&mut session);
        if let Err(error) = &result { lines.push(OutputBody::Error { statement: Some(i), error: error.clone() }); }
        console.output_all(&shared, run_id, lines);
        shared.record_history(HistoryInsert {
            database_kind: plan.connection.database_kind,
            at_ms: now_ms(), connection_id: plan.connection.id, connection_name: plan.connection.name.clone(), database: plan.database.clone(),
            sql: sql.to_owned(), elapsed_ms: elapsed_ms(unit_started), outcome: history,
        });
        index += 1;
        if cancelled { outcome = RunOutcome::Cancelled; break; }
        if result.is_err() { outcome = RunOutcome::Failed; if settings.stop_on_error { break; } }
        if fatal.is_some() || session.closed_reason().is_some() { break; }
    }
    let lost = session.closed_reason_settled(fatal).await;
    let due = console.edit(&shared, run_id, |inner| {
        let statements = Console::statements_mut(inner);
        for s in statements.iter_mut().skip(index) { if matches!(s.status, StmtStatus::Pending) { s.status = StmtStatus::Skipped; } }
        let statements = statements.clone();
        if let Some(reason) = &lost {
            let message = match session.transaction_loss(txn) {
                TransactionLoss::RolledBack => format!("Transaction rolled back: connection lost ({reason})"),
                TransactionLoss::None => format!("Session reset: connection lost ({reason})"),
                TransactionLoss::Unknown => format!("Session reset: earlier commits remain committed; the last transaction outcome may be unknown ({reason})"),
            };
            console.output(&shared, run_id, OutputBody::Info { message });
        }
        let due = Console::return_session(inner, session, txn, lost);
        inner.run = RunState::Finished { run_id, elapsed_ms: elapsed_ms(started), statements, outcome };
        due
    }).await?;
    if let Some((db, owed)) = due { shared.schedule_refreshes(db, owed); }
    Ok(())
}

async fn set_status(console: &Console, shared: &Shared, run_id: RunId, index: u16, status: StmtStatus) -> Result<(), Orphaned> {
    console.edit(shared, run_id, |inner| Console::statements_mut(inner)[index as usize].status = status).await
}

pub fn notice_lines(session: &mut Session) -> Vec<OutputBody> { session.drain_notices() }

async fn run_outputs(console: &Console, shared: &Shared, session: &mut Session, plan: &RunPlan, index: usize, caps: &Caps, stop: &AtomicBool) -> Result<Result<(Option<u64>, Vec<String>), QueryError>, Orphaned> {
    let sql = plan.statements[index].span.text(&plan.text);
    let run_id = plan.run_id;
    let started = Instant::now();
    let mut last_emit = Instant::now();
    let mut active: Option<(Arc<ResultSet>, usize)> = None;
    let mut outputs = Vec::new();
    let mut rows: Option<u64> = None;
    let mut tags = Vec::new();
    let mut failure = None;
    let mut stream = session.query(sql);
    while let Some(event) = stream.next().await {
        match event {
            Err(error) => { failure = Some(error); break; }
            Ok(QueryEvent::Prepared) if stop.load(Ordering::Relaxed) => { failure = Some(crate::session::error("Cancelled")); break; }
            Ok(QueryEvent::Prepared) => {}
            Ok(QueryEvent::Columns(mut columns)) => {
                if let Some((set, out)) = active.take() { finish_rows(console, shared, index, &set, &mut outputs[out], started, &mut rows); }
                if let Some(table) = &plan.table && plan.connection.database_kind == DatabaseKind::SqlServer {
                    if columns.len() != table.columns.len() || columns.iter().zip(&table.columns).any(|(actual, known)| actual.name != known.name || actual.r#type.category != known.category) {
                        failure = Some(crate::session::error("Table shape changed. Refresh the catalog before browsing again."));
                        break;
                    }
                    for (actual, known) in columns.iter_mut().zip(&table.columns) {
                        actual.source = Some(ColumnSource { table_oid: table.oid, attnum: known.attnum });
                        actual.r#type = SqlTypeRef { display: known.type_display.clone(), category: known.category };
                    }
                }
                let ordinal = match u16::try_from(console.results.lock().unwrap().len()) { Ok(n) => n, Err(_) => { failure = Some(crate::session::error("Too many result sets")); break; } };
                let db = DatabaseRef { connection_id: plan.connection.id, database: plan.database.clone() };
                let set = Arc::new(ResultSet::new(ResultRef { tab_id: console.tab, run_id, index: ordinal }, db, plan.connection.database_kind, columns));
                console.results.lock().unwrap().push(set.clone());
                active = Some((set.clone(), outputs.len()));
                outputs.push(RunOutput::Rows { result: set.meta(), state: RowsState::Streaming });
                console.edit(shared, run_id, |inner| { let stmt = &mut Console::statements_mut(inner)[index]; stmt.status = StmtStatus::Running; stmt.outputs = outputs.clone(); }).await?;
            }
            Ok(QueryEvent::Row(cells)) => {
                if let Some((set, _)) = &active { set.push_row(cells.iter().map(|v| v.as_deref()), caps); }
                else { failure = Some(crate::session::error("Driver returned a row without result metadata")); break; }
            }
            Ok(QueryEvent::Complete { affected_rows }) => {
                if let Some((set, out)) = active.take() { finish_rows(console, shared, index, &set, &mut outputs[out], started, &mut rows); }
                else {
                    let tag = affected_rows.map_or_else(|| "Batch completed (affected rows unavailable)".into(), |n| synthesize_tag(&plan.statements[index].kind, n));
                    let elapsed_ms = elapsed_ms(started);
                    console.output(shared, run_id, OutputBody::Command { statement: index as u16, tag: tag.clone(), elapsed_ms });
                    tags.push(tag.clone());
                    outputs.push(RunOutput::Command { tag, affected_rows, elapsed_ms });
                }
            }
            Ok(QueryEvent::Notice(body)) => console.output(shared, run_id, body),
        }
        if last_emit.elapsed() >= STREAM_EMIT_INTERVAL {
            last_emit = Instant::now();
            if let Some((set, out)) = &active { set.set_elapsed(elapsed_ms(started)); outputs[*out] = RunOutput::Rows { result: set.meta(), state: RowsState::Streaming }; }
            console.edit(shared, run_id, |inner| Console::statements_mut(inner)[index].outputs = outputs.clone()).await?;
        }
    }
    drop(stream);
    if let Some((set, out)) = active {
        if failure.is_none() { finish_rows(console, shared, index, &set, &mut outputs[out], started, &mut rows); }
        else { set.set_elapsed(elapsed_ms(started)); outputs[out] = RunOutput::Rows { result: set.meta(), state: if stop.load(Ordering::Relaxed) { RowsState::Cancelled } else { RowsState::Failed } }; }
    }
    console.edit(shared, run_id, |inner| Console::statements_mut(inner)[index].outputs = outputs).await?;
    Ok(match failure { Some(error) => Err(error), None => Ok((rows, tags)) })
}

fn finish_rows(console: &Console, shared: &Shared, index: usize, set: &ResultSet, output: &mut RunOutput, started: Instant, rows: &mut Option<u64>) {
    set.set_elapsed(elapsed_ms(started));
    let meta = set.meta();
    *rows = Some(rows.unwrap_or(0) + meta.rows_seen);
    console.output(shared, set.r#ref.run_id, OutputBody::Rows { statement: index as u16, rows: meta.rows_seen, elapsed_ms: meta.elapsed_ms, truncated: meta.truncated.is_some() });
    *output = RunOutput::Rows { result: meta, state: RowsState::Complete };
}

/// Caps the logged text so one huge pasted statement cannot bloat the webview's 5,000-entry log.
const STATEMENT_TEXT_CAP: usize = 2_000;

pub fn statement_line(statement: u16, sql: &str) -> OutputBody {
    let text = match sql.char_indices().nth(STATEMENT_TEXT_CAP) {
        Some((cut, _)) => format!("{}…", &sql[..cut]),
        None => sql.to_owned(),
    };
    OutputBody::Statement { statement, text }
}
