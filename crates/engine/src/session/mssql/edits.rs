use super::{MsSession, number};
use crate::model::*;
use crate::session::{Edit, QueryEvent, error};
use crate::sql::tsql::quote_ident;
use futures_util::StreamExt;

fn parameter(cell: &CellWrite, relation: &Relation, parameters: &mut Vec<Option<String>>) -> Result<String, QueryError> {
    let column = relation.columns.iter().find(|c| c.name == cell.column).ok_or_else(|| error(format!("Unknown column {}", cell.column)))?;
    let native = column.type_display.as_str();
    let base = native.split('(').next().unwrap_or(native);
    parameters.push(cell.value.clone());
    let parameter = format!("@P{}", parameters.len());
    if matches!(base, "char" | "varchar" | "nchar" | "nvarchar" | "text" | "ntext") {
        if matches!(base, "nchar" | "nvarchar") {
            let bound = native.strip_suffix(')').and_then(|s| s.split_once('(')).and_then(|(_, n)| n.parse::<usize>().ok());
            if cell.value.as_ref().is_some_and(|v| bound.is_some_and(|n| v.encode_utf16().count() > n)) {
                return Err(error(format!("Text value exceeds the capacity of {} ({native})", column.name)));
            }
        }
        return Ok(parameter);
    }
    if matches!(base, "binary" | "varbinary" | "image") {
        if let Some(value) = &cell.value {
            let hex = value.strip_prefix("0x").ok_or_else(|| error("Binary values must use 0x-prefixed hexadecimal text"))?;
            if hex.len() % 2 != 0 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(error("Binary values must contain complete hexadecimal bytes"));
            }
            let bound = native.strip_suffix(')').and_then(|s| s.split_once('(')).and_then(|(_, n)| n.parse::<usize>().ok());
            if bound.is_some_and(|n| hex.len() / 2 > n) {
                return Err(error(format!("Binary value exceeds the capacity of {} ({native})", column.name)));
            }
        }
        return Ok(format!("CONVERT(varbinary(max), {parameter}, 1)"));
    }
    let ty = match base {
        "tinyint" | "smallint" | "int" | "bigint" | "decimal" | "numeric" | "money" | "smallmoney" | "float" | "real" | "bit" | "date" | "time" | "datetime" | "smalldatetime"
        | "datetime2" | "datetimeoffset" | "uniqueidentifier" | "xml" => native,
        _ => return Err(error(format!("Staged writes for SQL Server type {native} are unsupported"))),
    };
    Ok(format!("CONVERT({ty}, {parameter})"))
}

fn key(key: &[CellWrite], relation: &Relation, parameters: &mut Vec<Option<String>>) -> Result<String, QueryError> {
    key.iter()
        .map(|c| {
            if c.value.is_none() { Ok(format!("{} IS NULL", quote_ident(&c.column))) } else { Ok(format!("{} = {}", quote_ident(&c.column), parameter(c, relation, parameters)?)) }
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.join(" AND "))
}

pub(in crate::session) fn plan(changes: &TableChanges, relation: &Relation) -> Result<Vec<Edit>, ApplyError> {
    let table = format!("{}.{}", quote_ident(&changes.schema), quote_ident(&changes.table));
    let output = "OUTPUT CAST(1 AS tinyint) INTO @l1_changed(value)";
    let mut edits = Vec::new();
    let mut add = |kind: ChangeKind, index: usize, key_cells: &[CellWrite], cells: &[CellWrite]| -> Result<(), ApplyError> {
        let change = ChangeRef { kind, index: u32::try_from(index).map_err(|_| ApplyError::NotConfigured)? };
        let mut parameters = Vec::new();
        let sql = (|| -> Result<String, QueryError> {
            match kind {
                ChangeKind::Delete => Ok(format!("DELETE FROM {table} {output} WHERE {}", key(key_cells, relation, &mut parameters)?)),
                ChangeKind::Update => {
                    let assignments = cells
                        .iter()
                        .map(|c| Ok(format!("{} = {}", quote_ident(&c.column), parameter(c, relation, &mut parameters)?)))
                        .collect::<Result<Vec<_>, QueryError>>()?
                        .join(", ");
                    Ok(format!("UPDATE {table} SET {assignments} {output} WHERE {}", key(key_cells, relation, &mut parameters)?))
                }
                ChangeKind::Insert if cells.is_empty() => Ok(format!("INSERT INTO {table} {output} DEFAULT VALUES")),
                ChangeKind::Insert => {
                    let columns = cells.iter().map(|c| quote_ident(&c.column)).collect::<Vec<_>>().join(", ");
                    let values = cells.iter().map(|c| parameter(c, relation, &mut parameters)).collect::<Result<Vec<_>, _>>()?.join(", ");
                    Ok(format!("INSERT INTO {table} ({columns}) {output} VALUES ({values})"))
                }
            }
        })()
        .map_err(|error| ApplyError::Failed { change, error })?;
        edits.push(Edit {
            change,
            sql: format!("DECLARE @l1_changed TABLE(value tinyint NOT NULL); {sql}; SELECT COUNT_BIG(*) AS [__l1_affected_rows] FROM @l1_changed"),
            parameters,
        });
        Ok(())
    };
    for (i, cells) in changes.deletes.iter().enumerate() {
        add(ChangeKind::Delete, i, cells, &[])?;
    }
    for (i, update) in changes.updates.iter().enumerate() {
        if !update.set.is_empty() {
            add(ChangeKind::Update, i, &update.key, &update.set)?;
        }
    }
    for (i, cells) in changes.inserts.iter().enumerate() {
        add(ChangeKind::Insert, i, &[], cells)?;
    }
    Ok(edits)
}

impl MsSession {
    pub(in crate::session) async fn execute_edit(&mut self, edit: &Edit) -> Result<u64, QueryError> {
        let mut stream = self.query_parameters(&edit.sql, &edit.parameters);
        let mut is_count = false;
        let mut count = None;
        while let Some(event) = stream.next().await {
            match event? {
                QueryEvent::Columns(columns) => {
                    is_count = columns.len() == 1 && columns[0].name == "__l1_affected_rows";
                    count = None;
                }
                QueryEvent::Row(row) if is_count => count = Some(number::<u64>(&row, 0)?),
                _ => {}
            }
        }
        count.ok_or_else(|| error("SQL Server did not return the staged edit's exact affected-row count"))
    }
}
