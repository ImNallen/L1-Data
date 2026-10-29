mod csv;
mod run;

use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use sha2::{Digest, Sha256};
use tempfile::TempPath;
use tokio::sync::{Notify, watch};

use crate::{
    Shared,
    console::{Console, SessionSlot},
    model::*,
};

pub struct Draft {
    pub id: ImportSourceId,
    config: ConsoleConfigView,
    state: watch::Sender<DraftState>,
    stop: Arc<AtomicBool>,
    cancel: Arc<Notify>,
}

#[derive(Clone)]
enum DraftState {
    Preparing,
    Ready(Arc<Prepared>),
    Failed(ImportError),
}

impl Drop for Draft {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.state
            .send_replace(DraftState::Failed(ImportError::Gone));
        self.cancel.notify_one();
    }
}

struct Snapshot {
    path: TempPath,
    hash: String,
    bytes: u64,
    records: u64,
    first: Vec<Option<String>>,
    sample: Vec<Vec<Option<String>>>,
    truncated: bool,
}

struct Prepared {
    snapshot: Snapshot,
    target: ImportTarget,
    columns: Vec<ColumnInfo>,
    relation: String,
    config: ConsoleConfigView,
    saved: ConnectionConfig,
}

fn signature(relation: &Relation) -> String {
    serde_json::to_string(&(relation.oid, relation.kind, &relation.columns))
        .expect("catalog serializes")
}

fn same_server(a: &ConnectionConfig, b: &ConnectionConfig) -> bool {
    let endpoint = |c: &ConnectionConfig| {
        serde_json::to_string(&(
            c.database_kind,
            &c.host,
            c.port,
            &c.user,
            c.ssl_mode,
            &c.ca_cert_path,
            &c.ssh,
        ))
        .expect("connection serializes")
    };
    endpoint(a) == endpoint(b)
}

fn bounded(value: &str, left: &mut usize, truncated: &mut bool) -> String {
    let mut end = value.len().min(256).min(*left);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    *left -= end;
    *truncated |= end < value.len();
    value[..end].to_owned()
}

fn snapshot(path: &str, stop: &AtomicBool) -> Result<Snapshot, ImportError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let mut source = options.open(path).map_err(csv::io)?;
    if !source.metadata().map_err(csv::io)?.is_file() {
        return Err(ImportError::Io {
            message: "Choose a regular CSV file".into(),
        });
    }
    let mut file = tempfile::NamedTempFile::new().map_err(csv::io)?;
    let mut hash = Sha256::new();
    let mut bytes = 0;
    let mut buffer = vec![0; 256 * 1024];
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err(ImportError::Cancelled);
        }
        let n = source.read(&mut buffer).map_err(csv::io)?;
        if n == 0 {
            break;
        }
        file.write_all(&buffer[..n]).map_err(csv::io)?;
        hash.update(&buffer[..n]);
        bytes += n as u64;
    }
    file.flush().map_err(csv::io)?;
    let path = file.into_temp_path();
    let mut parser = csv::Parser::new(File::open(&path).map_err(csv::io)?)?;
    let mut records = 0;
    let mut first = Vec::new();
    let mut sample = Vec::new();
    let mut remaining = 64 * 1024;
    let mut truncated = false;
    while let Some(record) = parser.next(stop)? {
        if records == 0 {
            first = record.fields.clone();
        } else if record.fields.len() != first.len() {
            return Err(ImportError::Csv {
                line: record.line,
                record: record.number,
                message: "Record has a different number of fields".into(),
            });
        }
        if sample.len() < 11 && remaining > 0 {
            sample.push(
                record
                    .fields
                    .iter()
                    .map(|v| {
                        v.as_ref()
                            .map(|v| bounded(v, &mut remaining, &mut truncated))
                    })
                    .collect(),
            );
        } else {
            truncated = true;
        }
        records += 1;
    }
    if stop.load(Ordering::Relaxed) {
        return Err(ImportError::Cancelled);
    }
    Ok(Snapshot {
        path,
        hash: format!("{:x}", hash.finalize()),
        bytes,
        records,
        first,
        sample,
        truncated,
    })
}

pub async fn prepare(
    s: &Arc<Shared>,
    tab: TabId,
    target: ImportTarget,
    file: String,
) -> Result<ImportSourceId, ImportError> {
    let console = s.console(tab).ok_or(ImportError::NotConfigured)?;
    let mut inner = console.inner.lock().await;
    if console.closed.load(Ordering::Relaxed) {
        return Err(ImportError::Gone);
    }
    if matches!(inner.session, SessionSlot::Busy { .. }) {
        return Err(ImportError::Busy);
    }
    let config = inner.config.clone();
    let saved = s
        .config(config.connection_id)
        .ok_or(ImportError::NotConfigured)?;
    if saved.database_kind != DatabaseKind::Postgres {
        return Err(ImportError::Unsupported);
    }
    if config.db() != target.db {
        return Err(ImportError::TargetChanged);
    }
    let id = ImportSourceId(uuid::Uuid::new_v4());
    let stop = Arc::new(AtomicBool::new(false));
    let cancel = Arc::new(Notify::new());
    let (state, _) = watch::channel(DraftState::Preparing);
    inner.import = Some(Draft {
        id,
        config: config.clone(),
        state: state.clone(),
        stop: stop.clone(),
        cancel: cancel.clone(),
    });
    let shared = s.clone();
    s.spawn(async move {
        let stopping = stop.clone();
        let scanned = shared
            .runtime
            .spawn_blocking(move || snapshot(&file, &stopping))
            .await;
        let result = async {
            let snapshot = scanned.map_err(|e| ImportError::Io {
                message: e.to_string(),
            })??;
            if stop.load(Ordering::Relaxed) {
                return Err(ImportError::Cancelled);
            }
            shared
                .connect(config.connection_id)
                .await
                .map_err(|error| ImportError::Connect { error })?;
            let catalog = shared
                .catalog(&target.db)
                .await
                .map_err(|_| ImportError::TargetChanged)?;
            let relation = catalog
                .relation(&target.schema, &target.table)
                .ok_or(ImportError::TargetChanged)?;
            if relation.oid != target.oid {
                return Err(ImportError::TargetChanged);
            }
            if !matches!(relation.kind, RelKind::Table | RelKind::PartitionedTable) {
                return Err(ImportError::Unsupported);
            }
            Ok(Prepared {
                snapshot,
                target,
                columns: relation
                    .columns
                    .iter()
                    .filter(|c| c.writable)
                    .cloned()
                    .collect(),
                relation: signature(relation),
                config,
                saved,
            })
        };
        let result = tokio::select! { _ = cancel.notified() => return, result = result => result };
        if !stop.load(Ordering::Relaxed) {
            state.send_replace(match result {
                Ok(ready) => DraftState::Ready(Arc::new(ready)),
                Err(error) => DraftState::Failed(error),
            });
        }
    });
    Ok(id)
}

async fn ready(console: &Console, source: ImportSourceId) -> Result<Arc<Prepared>, ImportError> {
    let mut state = {
        let inner = console.inner.lock().await;
        let draft = inner
            .import
            .as_ref()
            .filter(|d| d.id == source)
            .ok_or(ImportError::Gone)?;
        draft.state.subscribe()
    };
    loop {
        let value = state.borrow_and_update().clone();
        match value {
            DraftState::Ready(ready) => return Ok(ready),
            DraftState::Failed(error) => return Err(error),
            DraftState::Preparing => state.changed().await.map_err(|_| ImportError::Gone)?,
        }
    }
}

pub async fn preview(
    s: &Arc<Shared>,
    tab: TabId,
    source: ImportSourceId,
    header: bool,
) -> Result<ImportPreview, ImportError> {
    let console = s.console(tab).ok_or(ImportError::Gone)?;
    let prepared = ready(&console, source).await?;
    let inner = console.inner.lock().await;
    if !inner
        .import
        .as_ref()
        .is_some_and(|d| d.id == source && d.config == inner.config)
    {
        return Err(ImportError::Gone);
    }
    let snap = &prepared.snapshot;
    let mut remaining = 32 * 1024;
    let mut truncated = snap.truncated;
    let fields = snap
        .first
        .iter()
        .enumerate()
        .map(|(i, v)| {
            if header {
                bounded(v.as_deref().unwrap_or(""), &mut remaining, &mut truncated)
            } else {
                format!("Column {}", i + 1)
            }
        })
        .collect();
    let mapping = snap
        .first
        .iter()
        .enumerate()
        .map(|(i, value)| {
            if header {
                value
                    .as_ref()
                    .filter(|name| {
                        snap.first
                            .iter()
                            .filter(|v| v.as_ref() == Some(name))
                            .count()
                            == 1
                    })
                    .and_then(|name| prepared.columns.iter().find(|c| &c.name == name))
                    .map(|c| c.name.clone())
            } else {
                prepared.columns.get(i).map(|c| c.name.clone())
            }
        })
        .collect();
    Ok(ImportPreview {
        source,
        target: prepared.target.clone(),
        bytes: snap.bytes,
        rows: snap.records.saturating_sub(u64::from(header)),
        fields,
        columns: prepared.columns.clone(),
        mapping,
        sample: snap
            .sample
            .iter()
            .skip(usize::from(header))
            .take(10)
            .cloned()
            .collect(),
        truncated,
    })
}

pub async fn discard(
    s: &Arc<Shared>,
    tab: TabId,
    source: ImportSourceId,
) -> Result<(), ImportError> {
    if let Some(console) = s.console(tab) {
        let mut inner = console.inner.lock().await;
        if inner.import.as_ref().is_some_and(|d| d.id == source) {
            inner.import = None;
        }
    }
    Ok(())
}

pub async fn import(
    s: &Arc<Shared>,
    tab: TabId,
    request: ImportRequest,
    confirmed: Option<WriteDigest>,
) -> Result<ImportSummary, ImportError> {
    let console = s.console(tab).ok_or(ImportError::NotConfigured)?;
    let prepared = ready(&console, request.source).await?;
    let mut columns = HashSet::new();
    if request.mapping.len() != prepared.snapshot.first.len()
        || !request.mapping.iter().any(Option::is_some)
    {
        return Err(ImportError::Mapping {
            message: "Map at least one source field, with one choice per field".into(),
        });
    }
    for column in request.mapping.iter().flatten() {
        if !columns.insert(column) || !prepared.columns.iter().any(|c| c.name == *column) {
            return Err(ImportError::Mapping {
                message: format!("Column {column} is repeated, unknown, or server-owned"),
            });
        }
    }
    let catalog = s
        .catalog(&prepared.target.db)
        .await
        .map_err(|_| ImportError::TargetChanged)?;
    let relation = catalog
        .relation(&prepared.target.schema, &prepared.target.table)
        .ok_or(ImportError::TargetChanged)?;
    if signature(relation) != prepared.relation {
        return Err(ImportError::TargetChanged);
    }
    let needs_session = matches!(
        console.inner.lock().await.session,
        SessionSlot::Empty | SessionSlot::Lost { .. }
    );
    let fresh = if needs_session {
        Some(
            s.open_tab_session(&prepared.config)
                .await
                .map_err(|error| ImportError::Connect { error })?,
        )
    } else {
        None
    };
    let mut inner = console.inner.lock().await;
    if console.closed.load(Ordering::Relaxed)
        || !inner
            .import
            .as_ref()
            .is_some_and(|d| d.id == request.source)
    {
        return Err(ImportError::Gone);
    }
    if inner.config != prepared.config {
        return Err(ImportError::TargetChanged);
    }
    let current = s
        .config(inner.config.connection_id)
        .ok_or(ImportError::TargetChanged)?;
    if serde_json::to_string(&current).unwrap() != serde_json::to_string(&prepared.saved).unwrap() {
        return Err(ImportError::TargetChanged);
    }
    let current_catalog = s
        .instance(current.id)
        .and_then(|i| i.catalogs().get(&prepared.target.db.database).cloned())
        .ok_or(ImportError::TargetChanged)?;
    if current_catalog.version != catalog.version {
        return Err(ImportError::TargetChanged);
    }
    if let Some(fresh) = fresh {
        if matches!(inner.session, SessionSlot::Empty | SessionSlot::Lost { .. }) {
            inner.session = SessionSlot::Idle(fresh);
        }
    }
    let SessionSlot::Idle(idle) = &inner.session else {
        return Err(ImportError::Busy);
    };
    if idle.database_kind() != DatabaseKind::Postgres || !same_server(idle.opened(), &current) {
        return Err(ImportError::TargetChanged);
    }
    if !matches!(inner.txn, TxnState::Idle | TxnState::Active) {
        return Err(ImportError::TransactionAborted);
    }
    let fence = run::Fence::new(inner.txn, inner.config.mode);
    let names = request
        .mapping
        .iter()
        .flatten()
        .map(|name| crate::text::quote_ident(name))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "COPY {}.{} ({names}) FROM STDIN WITH (FORMAT csv, ENCODING 'UTF8')",
        crate::text::quote_ident(&prepared.target.schema),
        crate::text::quote_ident(&prepared.target.table)
    );
    let qualified = format!(
        "{}.{}",
        crate::text::quote_ident(&prepared.target.schema),
        crate::text::quote_ident(&prepared.target.table)
    );
    let lock = format!("LOCK TABLE {qualified} IN ROW EXCLUSIVE MODE");
    let identity = format!(
        "SELECT E'{}'::regclass::oid::bigint",
        qualified.replace('\\', "\\\\").replace('\'', "''")
    );
    let bound = serde_json::to_string(&(
        csv::VERSION,
        &request,
        &prepared.target,
        &prepared.relation,
        &prepared.snapshot.hash,
        prepared.snapshot.bytes,
        prepared.snapshot.records,
    ))
    .unwrap();
    let target = crate::writes::Target {
        saved: &current,
        opened: idle.opened(),
        console: &inner.config,
        txn: inner.txn,
        unasked: &inner.unasked,
    };
    let checked = crate::writes::check_import(
        &target,
        &sql,
        &fence.statements(&lock, &identity, &sql),
        &bound,
        confirmed.as_ref(),
    )
    .map_err(|confirmation| ImportError::ConfirmWrites { confirmation })?;
    let unasked = checked.writes_stay_unasked();
    let run_id = s.next_run_id();
    let progress = ImportProgress {
        source: request.source,
        target: prepared.target.clone(),
        bytes: 0,
        total_bytes: prepared.snapshot.bytes,
        rows: 0,
        total_rows: prepared
            .snapshot
            .records
            .saturating_sub(u64::from(request.header)),
        finishing: false,
    };
    let (session, stop) = console
        .take_session(
            &mut inner,
            run_id,
            crate::console::Work::Import { checked, progress },
        )
        .ok_or(ImportError::Busy)?;
    inner.import = None;
    console.publish(&mut inner, s);
    drop(inner);
    let (tx, rx) = tokio::sync::oneshot::channel();
    let shared = s.clone();
    s.spawn(async move {
        let result = run::run(
            &shared, console, run_id, session, stop, prepared, request, sql, lock, identity, fence,
            unasked,
        )
        .await;
        let _ = tx.send(result);
    });
    rx.await.unwrap_or(Err(ImportError::Gone))
}
