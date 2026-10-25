use tokio_postgres::{Client, Row, Transaction};

use crate::model::{DetailError, ObjectScript, ScriptTarget};
use crate::script::{self, postgres as ddl};

const RELATION: &str = "SELECT c.relkind::text, n.nspname, c.relname, c.relpersistence = 'u', coalesce(c.reloptions, '{}')::text[],
        obj_description(c.oid, 'pg_class'), pg_get_partkeydef(c.oid), pg_get_expr(c.relpartbound, c.oid), pn.nspname, p.relname, c.relispopulated
     FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
     LEFT JOIN pg_inherits i ON i.inhrelid = c.oid AND c.relispartition
     LEFT JOIN pg_class p ON p.oid = i.inhparent LEFT JOIN pg_namespace pn ON pn.oid = p.relnamespace
     WHERE c.oid = $1";

const COLUMNS: &str = "SELECT a.attname, coalesce(owned_serial.name, format_type(a.atttypid, a.atttypmod)), a.attnotnull, a.attidentity::text, a.attgenerated::text,
        CASE WHEN owned_serial.name IS NULL THEN pg_get_expr(d.adbin, d.adrelid) END,
        CASE WHEN a.attcollation <> t.typcollation THEN cn.nspname END, CASE WHEN a.attcollation <> t.typcollation THEN co.collname END,
        col_description(a.attrelid, a.attnum),
        format_type(s.seqtypid, NULL), s.seqstart, s.seqincrement, s.seqmin, s.seqmax, s.seqcache, s.seqcycle
     FROM pg_attribute a
     JOIN pg_type t ON t.oid = a.atttypid
     LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
     LEFT JOIN pg_collation co ON co.oid = a.attcollation
     LEFT JOIN pg_namespace cn ON cn.oid = co.collnamespace
     LEFT JOIN LATERAL (
        SELECT sc.oid FROM pg_depend dep JOIN pg_class sc ON sc.oid = dep.objid AND sc.relkind = 'S'
        WHERE dep.classid = 'pg_class'::regclass AND dep.refclassid = 'pg_class'::regclass
          AND dep.refobjid = a.attrelid AND dep.refobjsubid = a.attnum AND dep.deptype IN ('a', 'i')
        LIMIT 1
     ) seq ON true
     LEFT JOIN pg_sequence s ON s.seqrelid = seq.oid AND a.attidentity <> ''
     LEFT JOIN LATERAL (
        SELECT CASE a.atttypid WHEN 'int2'::regtype THEN 'smallserial' WHEN 'int4'::regtype THEN 'serial' WHEN 'int8'::regtype THEN 'bigserial' END AS name
        WHERE a.attidentity = '' AND seq.oid IS NOT NULL
          AND pg_get_expr(d.adbin, d.adrelid) = 'nextval(' || quote_literal(seq.oid::regclass::text) || '::regclass)'
     ) owned_serial ON true
     WHERE a.attrelid = $1 AND a.attnum > 0 AND NOT a.attisdropped
     ORDER BY a.attnum";

/// A partition's copies of its parent's constraints, and a foreign key's clones for the
/// partitions it references, come with the parent.
const CONSTRAINTS: &str = "SELECT k.conname, pg_get_constraintdef(k.oid) FROM pg_constraint k
     WHERE k.conrelid = $1 AND k.contype IN ('p', 'u', 'f', 'c', 'x') AND k.conislocal AND k.conparentid = 0
     ORDER BY position(k.contype IN 'pufcx'), k.conname";

const INDEXES: &str = "SELECT pg_get_indexdef(i.indexrelid) FROM pg_index i JOIN pg_class ic ON ic.oid = i.indexrelid
     WHERE i.indrelid = $1
       AND NOT EXISTS (SELECT 1 FROM pg_constraint k WHERE k.conindid = i.indexrelid AND k.conrelid = i.indrelid AND k.contype IN ('p', 'u', 'x'))
       AND NOT EXISTS (SELECT 1 FROM pg_inherits h WHERE h.inhrelid = i.indexrelid)
     ORDER BY ic.relname";

const TRIGGERS: &str = "SELECT t.tgname, pg_get_triggerdef(t.oid), t.tgenabled::text FROM pg_trigger t
     WHERE t.tgrelid = $1 AND NOT t.tgisinternal
       AND NOT EXISTS (SELECT 1 FROM pg_depend d WHERE d.classid = 'pg_trigger'::regclass AND d.objid = t.oid
                       AND d.refclassid = 'pg_trigger'::regclass AND d.deptype = 'P')
     ORDER BY t.tgname";

const PARTITIONS: &str = "SELECT n.nspname, c.relname, pg_get_expr(c.relpartbound, c.oid), pg_get_partkeydef(c.oid)
     FROM pg_inherits i JOIN pg_class c ON c.oid = i.inhrelid JOIN pg_namespace n ON n.oid = c.relnamespace
     WHERE i.inhparent = $1 AND c.relispartition
     ORDER BY pg_get_expr(c.relpartbound, c.oid) = 'DEFAULT', c.relname";

const ROUTINE: &str = "SELECT p.prokind::text, p.proname, CASE WHEN p.prokind <> 'a' THEN pg_get_functiondef(p.oid) END,
        obj_description(p.oid, 'pg_proc'), n.nspname, pg_get_function_identity_arguments(p.oid)
     FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace WHERE p.oid = $1";

/// With `search_path` only `pg_catalog`, the server prints every name it puts in an expression or
/// a definition with its schema.
pub async fn load(client: &mut Client, target: &ScriptTarget) -> Result<ObjectScript, DetailError> {
    let tx = client.build_transaction().read_only(true).start().await.map_err(failed)?;
    tx.batch_execute("SET LOCAL search_path = pg_catalog").await.map_err(failed)?;
    let script = match target {
        ScriptTarget::Relation { oid } => relation(&tx, *oid).await,
        ScriptTarget::Routine { oid } => routine(&tx, *oid).await,
        ScriptTarget::Index { relation_oid, name } => index(&tx, *relation_oid, name).await,
        ScriptTarget::Trigger { relation_oid, name } => trigger(&tx, *relation_oid, name).await,
    };
    tx.rollback().await.map_err(failed)?;
    script
}

fn failed(e: tokio_postgres::Error) -> DetailError {
    DetailError::Failed { message: super::query_failure(e, None).message }
}

fn refused(message: &str) -> DetailError {
    DetailError::Failed { message: message.into() }
}

async fn relation(tx: &Transaction<'_>, oid: u32) -> Result<ObjectScript, DetailError> {
    let params: &[&(dyn tokio_postgres::types::ToSql + Sync)] = &[&oid];
    let r = tx.query_opt(RELATION, params).await.map_err(failed)?.ok_or(DetailError::Gone)?;
    let (schema, name): (String, String) = (r.get(1), r.get(2));
    let options: Vec<String> = r.get(4);
    let comment: Option<String> = r.get(5);
    let text = match r.get::<_, &str>(0) {
        "r" | "p" => {
            let (columns, constraints, indexes, triggers, partitions) = tokio::try_join!(
                tx.query(COLUMNS, params),
                tx.query(CONSTRAINTS, params),
                tx.query(INDEXES, params),
                tx.query(TRIGGERS, params),
                tx.query(PARTITIONS, params),
            )
            .map_err(failed)?;
            let partition_of = r.get::<_, Option<String>>(7).map(|bound| ddl::PartitionOf { schema: r.get(8), name: r.get(9), bound });
            ddl::table(&ddl::Table {
                unlogged: r.get(3),
                options,
                comment,
                columns: columns.iter().map(column).collect(),
                constraints: constraints.iter().map(|k| (k.get(0), k.get(1))).collect(),
                partition_key: r.get(6),
                partition_of,
                partitions: partitions.iter().map(|p| ddl::Partition { schema: p.get(0), name: p.get(1), bound: p.get(2), partition_key: p.get(3) }).collect(),
                indexes: indexes.iter().map(|i| i.get(0)).collect(),
                triggers: triggers.iter().map(trigger_row).collect(),
                schema: schema.clone(),
                name: name.clone(),
            })
        }
        kind @ ("v" | "m") => {
            let (query, indexes, triggers) = tokio::try_join!(tx.query_one("SELECT pg_get_viewdef($1::oid, true)", params), tx.query(INDEXES, params), tx.query(TRIGGERS, params)).map_err(failed)?;
            ddl::view(&ddl::View {
                schema: schema.clone(),
                name: name.clone(),
                materialized: kind == "m",
                populated: r.get(10),
                options,
                query: query.get(0),
                comment,
                indexes: indexes.iter().map(|i| i.get(0)).collect(),
                triggers: triggers.iter().map(trigger_row).collect(),
            })
        }
        "f" => return Err(refused("Foreign tables cannot be scripted yet.")),
        _ => return Err(refused("Only tables and views can be scripted.")),
    };
    Ok(script::object_script(&name, script::Ddl::complete(text)))
}

fn column(r: &Row) -> ddl::Column {
    let identity = match r.get::<_, &str>(3) {
        "" => None,
        kind => Some(ddl::Identity {
            always: kind == "a",
            type_name: r.get(9),
            start: r.get(10),
            increment: r.get(11),
            min: r.get(12),
            max: r.get(13),
            cache: r.get(14),
            cycle: r.get(15),
        }),
    };
    let generated = match r.get::<_, &str>(4) {
        "s" => Some(ddl::Generated::Stored),
        "v" => Some(ddl::Generated::Virtual),
        _ => None,
    };
    let collation = r.get::<_, Option<String>>(6).zip(r.get::<_, Option<String>>(7));
    ddl::Column { name: r.get(0), type_name: r.get(1), collation, not_null: r.get(2), default: r.get(5), generated, identity, comment: r.get(8) }
}

fn trigger_row(r: &Row) -> ddl::Trigger {
    ddl::Trigger { name: r.get(0), definition: r.get(1), enabled: r.get::<_, &str>(2).chars().next().unwrap_or('O') }
}

async fn routine(tx: &Transaction<'_>, oid: u32) -> Result<ObjectScript, DetailError> {
    let r = tx.query_opt(ROUTINE, &[&oid]).await.map_err(failed)?.ok_or(DetailError::Gone)?;
    let kind: &str = r.get(0);
    let name: String = r.get(1);
    let Some(definition) = r.get::<_, Option<String>>(2) else {
        return Err(refused("Aggregates cannot be scripted yet."));
    };
    let signature = format!("{}({})", ddl::qualified(r.get(4), &name), r.get::<_, &str>(5));
    Ok(script::object_script(&name, script::Ddl::complete(ddl::routine(&definition, kind == "p", &signature, &r.get(3)))))
}

async fn index(tx: &Transaction<'_>, relation: u32, name: &str) -> Result<ObjectScript, DetailError> {
    let sql = "SELECT pg_get_indexdef(i.indexrelid) FROM pg_index i JOIN pg_class ic ON ic.oid = i.indexrelid WHERE i.indrelid = $1 AND ic.relname = $2";
    let r = tx.query_opt(sql, &[&relation, &name]).await.map_err(failed)?.ok_or(DetailError::Gone)?;
    Ok(script::object_script(name, script::Ddl::complete(ddl::script(vec![ddl::index(r.get(0))]))))
}

async fn trigger(tx: &Transaction<'_>, relation: u32, name: &str) -> Result<ObjectScript, DetailError> {
    let sql = "SELECT t.tgname, pg_get_triggerdef(t.oid), t.tgenabled::text, n.nspname, c.relname
               FROM pg_trigger t JOIN pg_class c ON c.oid = t.tgrelid JOIN pg_namespace n ON n.oid = c.relnamespace
               WHERE t.tgrelid = $1 AND t.tgname = $2";
    let r = tx.query_opt(sql, &[&relation, &name]).await.map_err(failed)?.ok_or(DetailError::Gone)?;
    let table = ddl::qualified(r.get(3), r.get(4));
    Ok(script::object_script(name, script::Ddl::complete(ddl::script(ddl::trigger(&trigger_row(&r), &table)))))
}
