use std::fs::File;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::sync::{mpsc, watch};

use super::{Prepared, csv};
use crate::{
    Shared,
    console::{Console, STREAM_EMIT_INTERVAL, SessionSlot, Work},
    model::*,
    session::{CopyChunk, Session},
};

pub(super) enum Fence {
    Auto,
    Manual,
    Savepoint,
}

impl Fence {
    pub fn new(txn: TxnState, mode: TxnMode) -> Self {
        if txn == TxnState::Active {
            Self::Savepoint
        } else if mode == TxnMode::Manual {
            Self::Manual
        } else {
            Self::Auto
        }
    }
    fn open(&self) -> &'static str {
        match self {
            Self::Savepoint => "SAVEPOINT l1_import",
            _ => "BEGIN",
        }
    }
    fn finish(&self) -> Option<&'static str> {
        match self {
            Self::Auto => Some("COMMIT"),
            Self::Manual => None,
            Self::Savepoint => Some("RELEASE SAVEPOINT l1_import"),
        }
    }
    fn undo(&self) -> &'static str {
        match self {
            Self::Savepoint => "ROLLBACK TO SAVEPOINT l1_import; RELEASE SAVEPOINT l1_import",
            _ => "ROLLBACK",
        }
    }
    pub fn statements<'a>(&self, lock: &'a str, identity: &'a str, copy: &'a str) -> Vec<&'a str> {
        std::iter::once(self.open())
            .chain([lock, identity, copy])
            .chain(self.finish())
            .chain(std::iter::once(self.undo()))
            .collect()
    }
}

fn produce(
    prepared: &Prepared,
    request: &ImportRequest,
    stop: &AtomicBool,
    tx: &mpsc::Sender<Result<CopyChunk, ImportError>>,
) -> Result<(), ImportError> {
    let mut parser = csv::Parser::new(File::open(&prepared.snapshot.path).map_err(csv::io)?)?;
    let mut rows = 0;
    let mut bytes = 0;
    let mut data = Vec::with_capacity(256 * 1024);
    if request.header {
        bytes = parser.next(stop)?.map_or(0, |r| r.bytes);
    }
    while let Some(record) = parser.next(stop)? {
        if stop.load(Ordering::Relaxed) {
            return Err(ImportError::Cancelled);
        }
        csv::encode(&record.fields, &request.mapping, &mut data);
        rows += 1;
        bytes = record.bytes;
        if data.len() >= 256 * 1024 {
            tx.blocking_send(Ok(CopyChunk {
                data: std::mem::replace(&mut data, Vec::with_capacity(256 * 1024)),
                bytes,
                rows,
            }))
            .map_err(|_| ImportError::Cancelled)?;
        }
    }
    if !data.is_empty() || rows == 0 {
        tx.blocking_send(Ok(CopyChunk { data, bytes, rows }))
            .map_err(|_| ImportError::Cancelled)?;
    }
    Ok(())
}

pub(super) async fn run(
    s: &Arc<Shared>,
    console: Arc<Console>,
    run_id: RunId,
    mut session: Session,
    stop: Arc<AtomicBool>,
    prepared: Arc<Prepared>,
    request: ImportRequest,
    sql: String,
    lock: String,
    identity: String,
    fence: Fence,
    unasked: bool,
) -> Result<ImportSummary, ImportError> {
    let backend_pid = session.backend_pid;
    let (tx, rx) = mpsc::channel(4);
    let producer_stop = stop.clone();
    let source = prepared.clone();
    let producer = s.runtime.spawn_blocking(move || {
        let result = produce(&source, &request, &producer_stop, &tx);
        if let Err(error) = &result {
            let _ = tx.blocking_send(Err(error.clone()));
        }
        result
    });
    let (progress, mut changes) = watch::channel((0, 0));
    let mut opened = false;
    let mut commit_unknown = false;
    let mut outcome = async {
        if stop.load(Ordering::Relaxed) { return Err(ImportError::Cancelled); }
        session.batch(fence.open()).await.map_err(|error| ImportError::Query { error })?;
        opened = true;
        if stop.load(Ordering::Relaxed) { return Err(ImportError::Cancelled); }
        session.batch(&lock).await.map_err(|error| ImportError::Query { error })?;
        if session.count(&identity).await.map_err(|error| ImportError::Query { error })? != u64::from(prepared.target.oid) { return Err(ImportError::TargetChanged); }
        let copying = session.import_csv(&sql, rx, &stop, progress, |notice| console.output(s, run_id, notice));
        tokio::pin!(copying);
        let mut tick = tokio::time::interval(STREAM_EMIT_INTERVAL);
        let rows = loop {
            tokio::select! {
                result = &mut copying => break result?,
                _ = tick.tick() => {
                    let (bytes, rows) = *changes.borrow_and_update();
                    if console.edit(s, run_id, |inner| {
                        if let SessionSlot::Busy { work: Work::Import { progress, .. }, .. } = &mut inner.session { progress.bytes = bytes; progress.rows = rows; }
                    }).await.is_err() { stop.store(true, Ordering::Relaxed); }
                }
            }
        };
        Ok(rows)
    }.await;
    if stop.load(Ordering::Relaxed)
        && matches!(&outcome, Err(ImportError::Query { error }) if error.sqlstate.as_deref() == Some("57014"))
    {
        outcome = Err(ImportError::Cancelled);
    }
    if outcome.is_err() {
        stop.store(true, Ordering::Relaxed);
    }
    let produced = producer
        .await
        .map_err(|e| ImportError::Io {
            message: e.to_string(),
        })
        .and_then(|r| r);
    let mut outcome = outcome.and_then(|rows| produced.map(|()| rows));
    if outcome.is_ok() && stop.load(Ordering::Relaxed) {
        outcome = Err(ImportError::Cancelled);
    }
    if outcome.is_ok() {
        if console
            .edit(s, run_id, |inner| {
                if let SessionSlot::Busy {
                    work: Work::Import { progress, .. },
                    ..
                } = &mut inner.session
                {
                    let (bytes, rows) = *changes.borrow();
                    progress.bytes = bytes;
                    progress.rows = rows;
                    progress.finishing = true;
                }
            })
            .await
            .is_err()
        {
            outcome = Err(ImportError::Cancelled);
        }
    }
    if outcome.is_ok() && stop.load(Ordering::Relaxed) {
        outcome = Err(ImportError::Cancelled);
    }
    if outcome.is_ok() {
        if let Some(finish) = fence.finish() {
            if let Err(error) = session.batch(finish).await {
                commit_unknown = matches!(fence, Fence::Auto)
                    && error.sqlstate.as_deref().is_none_or(|state| {
                        state.starts_with("08") || state == "40003"
                    });
                outcome = Err(if commit_unknown {
                    ImportError::OutcomeUnknown { error }
                } else {
                    ImportError::Query { error }
                });
            }
        }
    }
    let mut cleanup_failed = false;
    if outcome.is_err() && opened {
        let cleanup = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            session.batch(fence.undo()),
        )
        .await;
        if !matches!(cleanup, Ok(Ok(()))) {
            cleanup_failed = true;
            let message = match cleanup {
                Ok(Err(error)) => error.message,
                _ => "Rollback timed out".into(),
            };
            outcome = Err(ImportError::OutcomeUnknown {
                error: crate::session::error(format!(
                    "Import failed and cleanup could not be verified: {message}. The session was retired."
                )),
            });
        }
    }
    if commit_unknown || cleanup_failed {
        session.retire("CSV import transaction outcome is unknown".into());
    }
    let after = match (&fence, &outcome) {
        (Fence::Savepoint, _) | (Fence::Manual, Ok(_)) => TxnState::Active,
        _ => TxnState::Idle,
    };
    let lost = session.closed_reason_settled(None).await;
    let notices = crate::console::notice_lines(&mut session);
    let ended = opened
        && !matches!(fence, Fence::Savepoint)
        && (matches!(fence, Fence::Auto) || outcome.is_err())
        && !commit_unknown
        && !cleanup_failed;
    let line = match &outcome {
        Ok(rows) => OutputBody::Notice {
            severity: "INFO".into(),
            message: format!(
                "{rows} rows imported into {}.{} ({})",
                prepared.target.schema,
                prepared.target.table,
                if after == TxnState::Idle {
                    "committed"
                } else {
                    "pending commit"
                }
            ),
            detail: None,
        },
        Err(error) => OutputBody::Error {
            statement: None,
            error: match error {
                ImportError::Query { error } | ImportError::OutcomeUnknown { error } => {
                    error.clone()
                }
                ImportError::Cancelled => {
                    crate::session::error("CSV import cancelled; no import rows kept")
                }
                _ => crate::session::error(format!("CSV import failed: {error:?}")),
            },
        },
    };
    let returned = console
        .edit(s, run_id, |inner| {
            if outcome.is_ok() && unasked && after == TxnState::Active {
                inner.unasked.add(&sql);
            }
            if ended {
                inner.unasked = WriteList::default();
            }
            let due = Console::return_session(inner, session, after, lost);
            if commit_unknown || cleanup_failed {
                if let SessionSlot::Lost { transaction, .. } = &mut inner.session {
                    *transaction = TransactionLoss::Unknown;
                }
            }
            due
        })
        .await;
    match returned {
        Err(_) => return Err(ImportError::Gone),
        Ok(due) => {
            console.output_all(
                s,
                run_id,
                notices.into_iter().chain(std::iter::once(line)).collect(),
            );
            if let Some((db, owed)) = due {
                s.schedule_refreshes(db, owed);
            }
        }
    }
    outcome.map(|rows| ImportSummary {
        backend_pid,
        rows,
        committed: after == TxnState::Idle,
    })
}
