use super::{MsSession, catalog, number, object_id, required};
use crate::model::*;
use crate::session::{Edit, QueryEvent, error};
use crate::sql::mssql_value::{Input, base_type, edit_input_sql, input_sql};
use crate::sql::tsql::{quote_ident, quote_literal};
use futures_util::StreamExt;
use std::collections::HashMap;

fn parameter(cell: &CellWrite, relation: &Relation, collations: Option<&HashMap<String, String>>, parameters: &mut Vec<Option<String>>) -> Result<String, QueryError> {
    let column = relation.columns.iter().find(|c| c.name == cell.column).ok_or_else(|| error(format!("Unknown column {}", cell.column)))?;
    parameters.push(cell.value.clone());
    if let Some(collations) = collations {
        edit_input_sql(column, parameters.len(), cell.value.as_deref(), collations.get(&cell.column).map(String::as_str), false).map_err(error)
    } else {
        input_sql(column, Input::Parameter { index: parameters.len(), value: cell.value.as_deref() }).map_err(error)
    }
}

fn key(key: &[CellWrite], relation: &Relation, collations: Option<&HashMap<String, String>>, parameters: &mut Vec<Option<String>>) -> Result<String, QueryError> {
    key.iter()
        .map(|c| {
            if c.value.is_none() { Ok(format!("{} IS NULL", quote_ident(&c.column))) } else { Ok(format!("{} = {}", quote_ident(&c.column), parameter(c, relation, collations, parameters)?)) }
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.join(" AND "))
}

pub(in crate::session) fn plan(changes: &TableChanges, relation: &Relation) -> Result<Vec<Edit>, ApplyError> {
    lower(changes, relation, None)
}

fn lower(changes: &TableChanges, relation: &Relation, collations: Option<&HashMap<String, String>>) -> Result<Vec<Edit>, ApplyError> {
    let table = format!("{}.{}", quote_ident(&changes.schema), quote_ident(&changes.table));
    let output = "OUTPUT CAST(1 AS tinyint) INTO @l1_changed(value)";
    let mut edits = Vec::new();
    let mut add = |kind: ChangeKind, index: usize, key_cells: &[CellWrite], cells: &[CellWrite]| -> Result<(), ApplyError> {
        let change = ChangeRef { kind, index: u32::try_from(index).map_err(|_| ApplyError::NotConfigured)? };
        let mut parameters = Vec::new();
        let sql = (|| -> Result<String, QueryError> {
            match kind {
                ChangeKind::Delete => Ok(format!("DELETE FROM {table} {output} WHERE {}", key(key_cells, relation, collations, &mut parameters)?)),
                ChangeKind::Update => {
                    let assignments = cells
                        .iter()
                        .map(|c| Ok(format!("{} = {}", quote_ident(&c.column), parameter(c, relation, collations, &mut parameters)?)))
                        .collect::<Result<Vec<_>, QueryError>>()?
                        .join(", ");
                    Ok(format!("UPDATE {table} SET {assignments} {output} WHERE {}", key(key_cells, relation, collations, &mut parameters)?))
                }
                ChangeKind::Insert if cells.is_empty() => Ok(format!("INSERT INTO {table} {output} DEFAULT VALUES")),
                ChangeKind::Insert => {
                    let columns = cells.iter().map(|c| quote_ident(&c.column)).collect::<Vec<_>>().join(", ");
                    let values = cells.iter().map(|c| parameter(c, relation, collations, &mut parameters)).collect::<Result<Vec<_>, _>>()?.join(", ");
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
    pub(in crate::session) async fn prepare_edits(&mut self, changes: &TableChanges, relation: &Relation) -> Result<Vec<Edit>, ApplyError> {
        let transaction = |error| ApplyError::Transaction { error };
        let table = format!("{}.{}", quote_ident(&changes.schema), quote_ident(&changes.table));
        // A real scan takes the object lock even for an empty table. Sch-M cannot pass it before the writes finish.
        self.rows(&format!("SELECT TOP (1) 1 FROM {table} WITH (TABLOCKX, HOLDLOCK)")).await.map_err(transaction)?;
        let rows = self.rows(&format!("SELECT o.object_id,o.type,c.column_id,c.name,COALESCE(t.name,declared.name),c.max_length,c.precision,c.scale,c.is_nullable,c.is_identity,c.is_computed,c.generated_always_type,
            CASE WHEN EXISTS (SELECT 1 FROM sys.indexes i JOIN sys.index_columns ic ON ic.object_id=i.object_id AND ic.index_id=i.index_id WHERE i.object_id=c.object_id AND i.is_primary_key=1 AND ic.column_id=c.column_id AND ic.key_ordinal>0) THEN 1 ELSE 0 END,c.collation_name
            FROM sys.all_objects o JOIN sys.all_columns c ON c.object_id=o.object_id JOIN sys.types declared ON declared.user_type_id=c.user_type_id
            LEFT JOIN sys.types t ON t.user_type_id=c.system_type_id AND t.system_type_id=t.user_type_id
            WHERE o.object_id=OBJECT_ID({}) ORDER BY c.column_id", quote_literal(&table))).await.map_err(transaction)?;
        let drift = || transaction(error("Table identity, column types, or primary key changed. Refresh the catalog and reload the table before applying edits."));
        if relation.kind != RelKind::Table || rows.len() != relation.columns.len() || rows.is_empty() {
            return Err(drift());
        }
        let mut collations = HashMap::new();
        for (row, cached) in rows.iter().zip(&relation.columns) {
            let native = required(row, 4).map_err(transaction)?;
            let display = catalog::display(native, number(row, 5).map_err(transaction)?, number(row, 6).map_err(transaction)?, number(row, 7).map_err(transaction)?);
            let writable = number::<u8>(row, 9).map_err(transaction)? == 0 && number::<u8>(row, 10).map_err(transaction)? == 0
                && number::<u8>(row, 11).map_err(transaction)? == 0 && !matches!(native, "timestamp" | "rowversion");
            if object_id(row, 0).map_err(transaction)? != relation.oid || required(row, 1).map_err(transaction)?.trim() != "U"
                || number::<i16>(row, 2).map_err(transaction)? != cached.attnum || required(row, 3).map_err(transaction)? != cached.name
                || display != cached.type_display || (number::<u8>(row, 8).map_err(transaction)? == 0) != cached.not_null
                || writable != cached.writable || (number::<u8>(row, 12).map_err(transaction)? != 0) != cached.pk {
                return Err(drift());
            }
            if let Some(collation) = &row[13] { collations.insert(cached.name.clone(), collation.clone()); }
        }
        for (index, key) in changes.deletes.iter().enumerate() {
            self.validate_cells(key, true, relation, &collations, ChangeRef { kind: ChangeKind::Delete, index: index as u32 }).await?;
        }
        for (index, update) in changes.updates.iter().enumerate() {
            let change = ChangeRef { kind: ChangeKind::Update, index: index as u32 };
            self.validate_cells(&update.key, true, relation, &collations, change).await?;
            self.validate_cells(&update.set, false, relation, &collations, change).await?;
        }
        for (index, cells) in changes.inserts.iter().enumerate() {
            self.validate_cells(cells, false, relation, &collations, ChangeRef { kind: ChangeKind::Insert, index: index as u32 }).await?;
        }
        lower(changes, relation, Some(&collations))
    }

    async fn validate_cells(&mut self, cells: &[CellWrite], key: bool, relation: &Relation, collations: &HashMap<String, String>, change: ChangeRef) -> Result<(), ApplyError> {
        let failed = |error| ApplyError::Failed { change, error };
        for cell in cells {
            let column = relation.columns.iter().find(|c| c.name == cell.column).ok_or_else(|| failed(error("Unknown edit column")))?;
            let text = matches!(base_type(column), "char" | "varchar" | "nchar" | "nvarchar" | "text" | "ntext");
            if !key && !text { continue; }
            let mut expression = edit_input_sql(column, 1, cell.value.as_deref(), collations.get(&cell.column).map(String::as_str), true).map_err(|e| failed(error(e)))?;
            if text { expression = format!("CONVERT(nvarchar(max), {expression})"); }
            if matches!(base_type(column), "money" | "smallmoney") { expression = format!("CONVERT(decimal(19,4), {expression})"); }
            let sql = format!("SELECT {expression}");
            let mut stream = self.query_parameters(&sql, std::slice::from_ref(&cell.value));
            let mut returned = None;
            while let Some(event) = stream.next().await {
                if let QueryEvent::Row(row) = event.map_err(failed)? { returned = row.into_iter().next(); }
            }
            let exact = returned.as_ref() == Some(&cell.value);
            let padded = !key && matches!(base_type(column), "char" | "nchar") && cell.value.as_ref().zip(returned.as_ref().and_then(Option::as_ref))
                .is_some_and(|(original, converted)| converted.strip_prefix(original).is_some_and(|suffix| suffix.bytes().all(|b| b == b' ')));
            if !exact && !padded {
                return Err(failed(error(format!("{} for column {} cannot be converted to {} without loss. Reload the table or use a value representable by the column.",
                    if key { "Original key" } else { "Text value" }, column.name, column.type_display))));
            }
        }
        Ok(())
    }

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
