//! One result to one file. Owns the INSERT target resolution and the flow of bytes from the arena
//! to the file. Byte formats live in `encode.rs`, file replacement in `files::Replacement`.

mod encode;

use std::sync::Arc;

use crate::Shared;
use crate::files::Replacement;
use crate::model::*;
use crate::results::ResultSet;
use crate::text::quote_ident_in;
use encode::{Encoder, Format, InsertTarget};

/// Bytes handed to the file at a time.
const CHUNK: usize = 256 * 1024;

pub(crate) async fn export(shared: &Arc<Shared>, r: ResultRef, format: ExportFormat, scope: ExportScope, path: String) -> Result<ExportSummary, ExportError> {
    let set = shared.result_set(r).ok_or(ExportError::Gone)?;
    if scope == ExportScope::All {
        return Err(ExportError::NotRerunnable);
    }
    let encoder = Encoder::new(resolve(shared, &set, format).await, set.database_kind, &set.columns);
    blocking(shared, move || write_loaded(&set, encoder, &path)).await
}

async fn blocking<T: Send + 'static>(shared: &Shared, f: impl FnOnce() -> Result<T, ExportError> + Send + 'static) -> Result<T, ExportError> {
    shared.runtime.spawn_blocking(f).await.unwrap_or_else(|e| Err(ExportError::Io { message: e.to_string() }))
}

fn io(e: std::io::Error) -> ExportError {
    ExportError::Io { message: e.to_string() }
}

async fn resolve(shared: &Arc<Shared>, set: &ResultSet, format: ExportFormat) -> Format {
    match format {
        ExportFormat::Csv => Format::Csv,
        ExportFormat::Json => Format::Json,
        ExportFormat::Markdown => Format::Markdown,
        ExportFormat::SqlInsert => Format::Insert(insert_target(shared, set).await),
    }
}

/// The one table every column comes from, each from a different column of it, named by the
/// catalog of the result's own database and with the table's own column names, so `SELECT id AS
/// order_id` still inserts into `id`. A table page qualifies on both drivers. Anything else,
/// including a temp table or a catalog that cannot load, inserts into the placeholder.
async fn insert_target(shared: &Arc<Shared>, set: &ResultSet) -> InsertTarget {
    let dialect = set.database_kind;
    let placeholder = || InsertTarget::placeholder(dialect, &set.columns);
    let Some(oid) = single_source(&set.columns) else { return placeholder() };
    let Ok(catalog) = shared.catalog(&set.db).await else { return placeholder() };
    let named = catalog.relation_by_oid(oid).and_then(|(schema, relation)| {
        let columns = set
            .columns
            .iter()
            .map(|c| {
                let attnum = c.source?.attnum;
                relation.columns.iter().find(|k| k.attnum == attnum).map(|k| quote_ident_in(dialect, &k.name))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(InsertTarget { table: format!("{}.{}", quote_ident_in(dialect, schema), quote_ident_in(dialect, &relation.name)), columns })
    });
    named.unwrap_or_else(placeholder)
}

fn single_source(columns: &[ColumnMeta]) -> Option<u32> {
    let oid = columns.first()?.source?.table_oid;
    let mut attnums = std::collections::HashSet::new();
    columns.iter().all(|c| c.source.is_some_and(|s| s.table_oid == oid && attnums.insert(s.attnum))).then_some(oid)
}

fn write_loaded(set: &ResultSet, mut encoder: Encoder, path: &str) -> Result<ExportSummary, ExportError> {
    let mut file = Replacement::begin(path).map_err(io)?;
    let mut buf = Vec::with_capacity(CHUNK + 4096);
    encoder.begin(&mut buf);
    let (rows, truncated) = set
        .each_row(|cells| {
            encoder.row(cells.iter().copied(), &mut buf);
            if buf.len() >= CHUNK {
                file.write_all(&buf)?;
                buf.clear();
            }
            Ok(())
        })
        .map_err(io)?;
    encoder.finish(&mut buf);
    file.write_all(&buf).map_err(io)?;
    let bytes = file.commit().map_err(io)?;
    Ok(ExportSummary { rows, bytes, truncated })
}
