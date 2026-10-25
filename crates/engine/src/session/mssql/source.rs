use super::catalog::display;
use super::{MsSession, native_object_id, number, required};
use crate::model::*;
use crate::script::{self, tsql as ddl};
use crate::sql::tsql::quote_literal;

fn failed(e: QueryError) -> DetailError {
    DetailError::Failed { message: e.message }
}

fn refused(message: impl Into<String>) -> DetailError {
    DetailError::Failed { message: message.into() }
}

fn bit(row: &[Option<String>], i: usize) -> Result<bool, DetailError> {
    Ok(number::<u8>(row, i).map_err(failed)? != 0)
}

fn text(row: &[Option<String>], i: usize) -> Result<String, DetailError> {
    Ok(required(row, i).map_err(failed)?.to_owned())
}

impl MsSession {
    pub(in crate::session) async fn script(&mut self, target: &ScriptTarget) -> Result<ObjectScript, DetailError> {
        match target {
            ScriptTarget::Relation { oid } => self.relation_script(native_object_id(*oid)).await,
            ScriptTarget::Routine { oid } => self.module_script(&format!("o.object_id={}", native_object_id(*oid))).await,
            ScriptTarget::Trigger { relation_oid, name } => {
                self.module_script(&format!("o.parent_object_id={} AND o.type='TR' AND o.name={}", native_object_id(*relation_oid), quote_literal(name))).await
            }
            ScriptTarget::Index { relation_oid, name } => {
                let oid = native_object_id(*relation_oid);
                let table = self.rows(&format!("SELECT s.name,o.name FROM sys.objects o JOIN sys.schemas s ON s.schema_id=o.schema_id WHERE o.object_id={oid}")).await.map_err(failed)?;
                let table = table.first().ok_or(DetailError::Gone)?;
                let indexes = self.indexes(oid, &format!("i.name={}", quote_literal(name))).await?;
                let index = indexes.first().ok_or(DetailError::Gone)?;
                Ok(ObjectScript { title: script::title(name), text: ddl::index(index, &ddl::qualified(&text(table, 0)?, &text(table, 1)?)) })
            }
        }
    }

    async fn relation_script(&mut self, oid: i32) -> Result<ObjectScript, DetailError> {
        let rows = self.rows(&format!("SELECT s.name,o.name,o.type,t.uses_ansi_nulls FROM sys.objects o JOIN sys.schemas s ON s.schema_id=o.schema_id LEFT JOIN sys.tables t ON t.object_id=o.object_id WHERE o.object_id={oid}")).await.map_err(failed)?;
        let r = rows.first().ok_or(DetailError::Gone)?;
        match required(r, 2).map_err(failed)?.trim() {
            "V" => self.module_script(&format!("o.object_id={oid}")).await,
            "U" => {
                let name = text(r, 1)?;
                let table = ddl::Table {
                    schema: text(r, 0)?,
                    name: name.clone(),
                    ansi_nulls: bit(r, 3)?,
                    columns: self.columns(oid).await?,
                    keys: self.keys(oid).await?,
                    foreign_keys: self.foreign_keys(oid).await?,
                    checks: self.rows(&format!("SELECT name,definition FROM sys.check_constraints WHERE parent_object_id={oid} ORDER BY name")).await.map_err(failed)?.iter().map(|r| Ok((text(r, 0)?, text(r, 1)?))).collect::<Result<_, DetailError>>()?,
                    indexes: self.indexes(oid, "i.is_primary_key=0 AND i.is_unique_constraint=0 AND i.type IN (1,2)").await?,
                    triggers: self.modules(&format!("o.parent_object_id={oid} AND o.type='TR'")).await?.into_iter().map(|(_, m)| m).collect(),
                };
                Ok(ObjectScript { title: script::title(&name), text: ddl::table(&table) })
            }
            _ => Err(refused("Only tables and views can be scripted.")),
        }
    }

    async fn module_script(&mut self, filter: &str) -> Result<ObjectScript, DetailError> {
        let (name, module) = self.modules(filter).await?.into_iter().next().ok_or(DetailError::Gone)?;
        Ok(ObjectScript { title: script::title(&name), text: ddl::module(&module) })
    }

    async fn modules(&mut self, filter: &str) -> Result<Vec<(String, ddl::Module)>, DetailError> {
        let rows = self
            .rows(&format!(
                "SELECT s.name,o.name,OBJECT_DEFINITION(o.object_id),m.uses_ansi_nulls,m.uses_quoted_identifier,tr.is_disabled,ps.name,po.name
                 FROM sys.objects o JOIN sys.schemas s ON s.schema_id=o.schema_id
                 LEFT JOIN sys.sql_modules m ON m.object_id=o.object_id
                 LEFT JOIN sys.triggers tr ON tr.object_id=o.object_id
                 LEFT JOIN sys.objects po ON po.object_id=o.parent_object_id LEFT JOIN sys.schemas ps ON ps.schema_id=po.schema_id
                 WHERE {filter} ORDER BY o.name"
            ))
            .await
            .map_err(failed)?;
        rows.iter()
            .map(|r| {
                let qualified = ddl::qualified(&text(r, 0)?, &text(r, 1)?);
                let Some(definition) = r[2].clone() else {
                    return Err(refused(format!("SQL Server has no readable definition for {qualified}. The module is encrypted or CLR, or this login lacks VIEW DEFINITION permission.")));
                };
                let disabled = match r[5].as_deref() {
                    Some("1") => Some((qualified, ddl::qualified(&text(r, 6)?, &text(r, 7)?))),
                    _ => None,
                };
                Ok((text(r, 1)?, ddl::Module { definition, ansi_nulls: bit(r, 3)?, quoted_identifier: bit(r, 4)?, disabled }))
            })
            .collect()
    }

    async fn columns(&mut self, oid: i32) -> Result<Vec<ddl::Column>, DetailError> {
        let rows = self
            .rows(&format!(
                "SELECT c.name,COALESCE(t.name,ut.name),ut.is_user_defined,SCHEMA_NAME(ut.schema_id),ut.name,c.max_length,c.precision,c.scale,c.is_nullable,
                    CASE WHEN c.collation_name<>CONVERT(sysname,DATABASEPROPERTYEX(DB_NAME(),'Collation')) THEN c.collation_name END,
                    CONVERT(nvarchar(40),ic.seed_value),CONVERT(nvarchar(40),ic.increment_value),cc.definition,cc.is_persisted,dc.name,dc.definition
                 FROM sys.columns c
                 JOIN sys.types ut ON ut.user_type_id=c.user_type_id
                 LEFT JOIN sys.types t ON t.user_type_id=c.system_type_id AND t.system_type_id=t.user_type_id
                 LEFT JOIN sys.identity_columns ic ON ic.object_id=c.object_id AND ic.column_id=c.column_id
                 LEFT JOIN sys.computed_columns cc ON cc.object_id=c.object_id AND cc.column_id=c.column_id
                 LEFT JOIN sys.default_constraints dc ON dc.object_id=c.default_object_id
                 WHERE c.object_id={oid} ORDER BY c.column_id"
            ))
            .await
            .map_err(failed)?;
        rows.iter()
            .map(|r| {
                let type_name = if bit(r, 2)? {
                    ddl::qualified(&text(r, 3)?, &text(r, 4)?)
                } else {
                    display(required(r, 1).map_err(failed)?, number(r, 5).map_err(failed)?, number(r, 6).map_err(failed)?, number(r, 7).map_err(failed)?)
                };
                let identity = r[10].clone().zip(r[11].clone());
                let computed = match &r[12] {
                    Some(definition) => Some(ddl::Computed { definition: definition.clone(), persisted: bit(r, 13)? }),
                    None => None,
                };
                Ok(ddl::Column { name: text(r, 0)?, type_name, collation: r[9].clone(), nullable: bit(r, 8)?, identity, computed, default: r[14].clone().zip(r[15].clone()) })
            })
            .collect()
    }

    async fn keys(&mut self, oid: i32) -> Result<Vec<ddl::Key>, DetailError> {
        let rows = self
            .rows(&format!(
                "SELECT k.name,k.type,i.type,c.name,ic.is_descending_key FROM sys.key_constraints k
                 JOIN sys.indexes i ON i.object_id=k.parent_object_id AND i.index_id=k.unique_index_id
                 JOIN sys.index_columns ic ON ic.object_id=i.object_id AND ic.index_id=i.index_id
                 JOIN sys.columns c ON c.object_id=ic.object_id AND c.column_id=ic.column_id
                 WHERE k.parent_object_id={oid} AND ic.key_ordinal>0
                 ORDER BY CASE k.type WHEN 'PK' THEN 0 ELSE 1 END,k.name,ic.key_ordinal"
            ))
            .await
            .map_err(failed)?;
        let mut keys: Vec<ddl::Key> = vec![];
        for r in &rows {
            let name = text(r, 0)?;
            let column = (text(r, 3)?, bit(r, 4)?);
            match keys.last_mut() {
                Some(k) if k.name == name => k.columns.push(column),
                _ => keys.push(ddl::Key { name, primary: required(r, 1).map_err(failed)?.trim() == "PK", clustered: required(r, 2).map_err(failed)? == "1", columns: vec![column] }),
            }
        }
        Ok(keys)
    }

    async fn foreign_keys(&mut self, oid: i32) -> Result<Vec<ddl::ForeignKey>, DetailError> {
        let rows = self
            .rows(&format!(
                "SELECT fk.name,c.name,rs.name,ro.name,rc.name,fk.delete_referential_action_desc,fk.update_referential_action_desc
                 FROM sys.foreign_keys fk JOIN sys.foreign_key_columns fc ON fc.constraint_object_id=fk.object_id
                 JOIN sys.columns c ON c.object_id=fc.parent_object_id AND c.column_id=fc.parent_column_id
                 JOIN sys.objects ro ON ro.object_id=fc.referenced_object_id JOIN sys.schemas rs ON rs.schema_id=ro.schema_id
                 JOIN sys.columns rc ON rc.object_id=fc.referenced_object_id AND rc.column_id=fc.referenced_column_id
                 WHERE fk.parent_object_id={oid} ORDER BY fk.name,fc.constraint_column_id"
            ))
            .await
            .map_err(failed)?;
        let mut keys: Vec<ddl::ForeignKey> = vec![];
        for r in &rows {
            let name = text(r, 0)?;
            let (column, ref_column) = (text(r, 1)?, text(r, 4)?);
            match keys.last_mut() {
                Some(k) if k.name == name => {
                    k.columns.push(column);
                    k.ref_columns.push(ref_column);
                }
                _ => keys.push(ddl::ForeignKey {
                    name,
                    columns: vec![column],
                    ref_schema: text(r, 2)?,
                    ref_table: text(r, 3)?,
                    ref_columns: vec![ref_column],
                    on_delete: text(r, 5)?,
                    on_update: text(r, 6)?,
                }),
            }
        }
        Ok(keys)
    }

    async fn indexes(&mut self, oid: i32, filter: &str) -> Result<Vec<ddl::Index>, DetailError> {
        let rows = self
            .rows(&format!(
                "SELECT i.name,i.is_unique,i.type,i.filter_definition,c.name,ic.is_descending_key,ic.is_included_column
                 FROM sys.indexes i JOIN sys.index_columns ic ON ic.object_id=i.object_id AND ic.index_id=i.index_id
                 JOIN sys.columns c ON c.object_id=ic.object_id AND c.column_id=ic.column_id
                 WHERE i.object_id={oid} AND i.index_id>0 AND i.is_hypothetical=0 AND {filter}
                 ORDER BY i.name,ic.is_included_column,ic.key_ordinal,ic.index_column_id"
            ))
            .await
            .map_err(failed)?;
        let mut indexes: Vec<ddl::Index> = vec![];
        for r in &rows {
            let name = text(r, 0)?;
            if !matches!(required(r, 2).map_err(failed)?, "1" | "2") {
                return Err(refused(format!("Only clustered and nonclustered indexes can be scripted, and {name} is neither.")));
            }
            let index = match indexes.last_mut() {
                Some(i) if i.name == name => i,
                _ => {
                    indexes.push(ddl::Index { name, unique: bit(r, 1)?, clustered: required(r, 2).map_err(failed)? == "1", keys: vec![], include: vec![], filter: r[3].clone() });
                    indexes.last_mut().expect("pushed above")
                }
            };
            if bit(r, 6)? {
                index.include.push(text(r, 4)?);
            } else {
                index.keys.push((text(r, 4)?, bit(r, 5)?));
            }
        }
        Ok(indexes)
    }
}
