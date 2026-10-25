use tokio_postgres::types::ToSql;
use tokio_postgres::{Client, Row, Transaction};

use crate::model::{DetailError, ObjectScript, ScriptTarget};
use crate::script::{self, Change, Privilege, postgres as ddl};
use crate::text::quote_ident_if_needed as ident;

const RELATION: &str = "SELECT c.relkind::text, n.nspname, c.relname, c.relpersistence = 'u',
        coalesce(c.reloptions, '{}')::text[] || ARRAY(SELECT 'toast.' || o FROM pg_class t, unnest(t.reloptions) o WHERE t.oid = c.reltoastrelid),
        obj_description(c.oid, 'pg_class'), pg_get_partkeydef(c.oid), pg_get_expr(c.relpartbound, c.oid), pn.nspname, p.relname, c.relispopulated,
        c.relrowsecurity, c.relforcerowsecurity, c.relreplident::text, ts.spcname, CASE WHEN am.amname <> 'heap' THEN am.amname END,
        EXISTS (SELECT 1 FROM pg_seclabel l WHERE l.classoid = 'pg_class'::regclass AND l.objoid = c.oid),
        fs.srvname, coalesce(ft.ftoptions, '{}'),
        ARRAY(SELECT hn.nspname::text FROM pg_inherits h JOIN pg_class hc ON hc.oid = h.inhparent JOIN pg_namespace hn ON hn.oid = hc.relnamespace
              WHERE h.inhrelid = c.oid AND NOT c.relispartition ORDER BY h.inhseqno),
        ARRAY(SELECT hc.relname::text FROM pg_inherits h JOIN pg_class hc ON hc.oid = h.inhparent
              WHERE h.inhrelid = c.oid AND NOT c.relispartition ORDER BY h.inhseqno),
        CASE WHEN c.reloftype <> 0 THEN format_type(c.reloftype, NULL) END,
        ARRAY(SELECT p.pubname::text FROM pg_publication_rel pr JOIN pg_publication p ON p.oid = pr.prpubid WHERE pr.prrelid = c.oid ORDER BY 1),
        (SELECT ic.relname FROM pg_index ri JOIN pg_class ic ON ic.oid = ri.indexrelid
          WHERE ri.indrelid = c.oid AND ri.indisreplident AND EXISTS (SELECT 1 FROM pg_inherits h WHERE h.inhrelid = ri.indexrelid))
     FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
     LEFT JOIN pg_inherits i ON i.inhrelid = c.oid AND c.relispartition
     LEFT JOIN pg_class p ON p.oid = i.inhparent LEFT JOIN pg_namespace pn ON pn.oid = p.relnamespace
     LEFT JOIN pg_tablespace ts ON ts.oid = c.reltablespace
     LEFT JOIN pg_am am ON am.oid = c.relam
     LEFT JOIN pg_foreign_table ft ON ft.ftrelid = c.oid LEFT JOIN pg_foreign_server fs ON fs.oid = ft.ftserver
     WHERE c.oid = $1";

struct Queries {
    columns: String,
    statistics: String,
}

impl Queries {
    fn for_version(version: i32) -> Queries {
        let compression = |alias: &str| if version >= 140000 { format!("{alias}.attcompression::text") } else { "''".into() };
        let target = if version >= 130000 { "NULLIF(s.stxstattarget, -1)::int4" } else { "NULL::int4" };
        Queries {
            columns: format!(
                "SELECT a.attname, coalesce(serial.name, format_type(a.atttypid, a.atttypmod)), a.attnotnull, a.attidentity::text, a.attgenerated::text,
        CASE WHEN serial.name IS NULL THEN pg_get_expr(d.adbin, d.adrelid) END,
        CASE WHEN a.attcollation <> t.typcollation THEN cn.nspname END, CASE WHEN a.attcollation <> t.typcollation THEN co.collname END,
        col_description(a.attrelid, a.attnum),
        ident.nspname, ident.relname, format_type(ident.seqtypid, NULL), ident.seqstart, ident.seqincrement, ident.seqmin, ident.seqmax, ident.seqcache, ident.seqcycle,
        a.attislocal, NULLIF(a.attstattarget, -1)::int4, a.attstorage::text, t.typstorage::text, {compression}, coalesce(a.attoptions, '{{}}'), coalesce(a.attfdwoptions, '{{}}'),
        pa.position, pa.def, pa.attnotnull, pa.attstorage::text, pa.attcompression, serial.seq
     FROM pg_attribute a
     JOIN pg_class c ON c.oid = a.attrelid
     JOIN pg_type t ON t.oid = a.atttypid
     LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
     LEFT JOIN pg_collation co ON co.oid = a.attcollation
     LEFT JOIN pg_namespace cn ON cn.oid = co.collnamespace
     LEFT JOIN LATERAL (
        SELECT sn.nspname, sc.relname, s.* FROM pg_depend dep JOIN pg_class sc ON sc.oid = dep.objid AND sc.relkind = 'S'
        JOIN pg_namespace sn ON sn.oid = sc.relnamespace JOIN pg_sequence s ON s.seqrelid = sc.oid
        WHERE a.attidentity <> '' AND dep.classid = 'pg_class'::regclass AND dep.refclassid = 'pg_class'::regclass
          AND dep.refobjid = a.attrelid AND dep.refobjsubid = a.attnum AND dep.deptype = 'i'
     ) ident ON true
     LEFT JOIN LATERAL (
        SELECT CASE a.atttypid WHEN 'int2'::regtype THEN 'smallserial' WHEN 'int4'::regtype THEN 'serial' WHEN 'int8'::regtype THEN 'bigserial' END AS name, sc.relname AS seq
        FROM pg_depend dep JOIN pg_class sc ON sc.oid = dep.objid AND sc.relkind = 'S' JOIN pg_sequence s ON s.seqrelid = sc.oid
        WHERE a.attidentity = '' AND a.attnotnull AND a.atttypid IN ('int2'::regtype, 'int4'::regtype, 'int8'::regtype)
          AND dep.classid = 'pg_class'::regclass AND dep.refclassid = 'pg_class'::regclass
          AND dep.refobjid = a.attrelid AND dep.refobjsubid = a.attnum AND dep.deptype = 'a'
          AND pg_get_expr(d.adbin, d.adrelid) = 'nextval(' || quote_literal(sc.oid::regclass::text) || '::regclass)'
          AND sc.relnamespace = c.relnamespace AND sc.relname = c.relname || '_' || a.attname || '_seq'
          AND s.seqtypid = a.atttypid AND s.seqstart = 1 AND s.seqincrement = 1 AND s.seqmin = 1 AND s.seqcache = 1 AND NOT s.seqcycle
          AND s.seqmax = CASE a.atttypid WHEN 'int2'::regtype THEN 32767 WHEN 'int4'::regtype THEN 2147483647 ELSE 9223372036854775807 END
     ) serial ON true
     LEFT JOIN LATERAL (
        SELECT (h.inhseqno::int8 << 16) + pa.attnum AS position, pg_get_expr(pd.adbin, pd.adrelid) AS def, pa.attnotnull, pa.attstorage, {parent_compression} AS attcompression
        FROM pg_inherits h JOIN pg_attribute pa ON pa.attrelid = h.inhparent AND pa.attname = a.attname AND NOT pa.attisdropped
        LEFT JOIN pg_attrdef pd ON pd.adrelid = pa.attrelid AND pd.adnum = pa.attnum
        WHERE h.inhrelid = a.attrelid AND a.attinhcount > 0 ORDER BY h.inhseqno LIMIT 1
     ) pa ON true
     WHERE a.attrelid = $1 AND a.attnum > 0 AND NOT a.attisdropped
     ORDER BY a.attnum",
                compression = compression("a"),
                parent_compression = compression("pa"),
            ),
            statistics: format!(
                "SELECT n.nspname, s.stxname, pg_get_statisticsobjdef(s.oid), {target}, obj_description(s.oid, 'pg_statistic_ext')
                 FROM pg_statistic_ext s JOIN pg_namespace n ON n.oid = s.stxnamespace WHERE s.stxrelid = $1 ORDER BY s.stxname"
            ),
        }
    }
}

const OWNED_SEQUENCES: &str = "SELECT sn.nspname, sc.relname, a.attname, format_type(s.seqtypid, NULL), s.seqstart, s.seqincrement, s.seqmin, s.seqmax, s.seqcache, s.seqcycle
     FROM pg_depend dep JOIN pg_class sc ON sc.oid = dep.objid AND sc.relkind = 'S'
     JOIN pg_namespace sn ON sn.oid = sc.relnamespace JOIN pg_sequence s ON s.seqrelid = sc.oid
     JOIN pg_attribute a ON a.attrelid = dep.refobjid AND a.attnum = dep.refobjsubid
     WHERE dep.classid = 'pg_class'::regclass AND dep.refclassid = 'pg_class'::regclass AND dep.refobjid = $1 AND dep.deptype = 'a'
     ORDER BY sc.relname";

/// A partition's copies of its parent's constraints, and a foreign key's clones for the
/// partitions it references, come with the parent.
const CONSTRAINTS: &str = "SELECT k.conname, pg_get_constraintdef(k.oid), obj_description(k.oid, 'pg_constraint'),
        k.contype = 'f' AND k.confrelid = k.conrelid
          AND NOT EXISTS (SELECT 1 FROM pg_constraint u WHERE u.conindid = k.conindid AND u.conrelid = k.conrelid AND u.contype IN ('p', 'u'))
     FROM pg_constraint k
     WHERE k.conrelid = $1 AND k.contype IN ('p', 'u', 'f', 'c', 'x') AND k.conislocal AND k.conparentid = 0
     ORDER BY position(k.contype IN 'pufcx'), k.conname";

/// A partition's copy of its parent's index comes with the parent.
const INDEXES: &str = "SELECT ic.relname,
        CASE WHEN NOT EXISTS (SELECT 1 FROM pg_constraint k WHERE k.conindid = i.indexrelid AND k.conrelid = i.indrelid AND k.contype IN ('p', 'u', 'x'))
             THEN pg_get_indexdef(i.indexrelid) END,
        obj_description(i.indexrelid, 'pg_class'), i.indisclustered, i.indisreplident, ts.spcname,
        ARRAY(SELECT a.attnum::int4 FROM pg_attribute a WHERE a.attrelid = i.indexrelid AND NULLIF(a.attstattarget, -1) IS NOT NULL ORDER BY a.attnum),
        ARRAY(SELECT NULLIF(a.attstattarget, -1)::int4 FROM pg_attribute a WHERE a.attrelid = i.indexrelid AND NULLIF(a.attstattarget, -1) IS NOT NULL ORDER BY a.attnum),
        NOT i.indisvalid, ic.relkind = 'I'
     FROM pg_index i JOIN pg_class ic ON ic.oid = i.indexrelid LEFT JOIN pg_tablespace ts ON ts.oid = ic.reltablespace
     WHERE i.indrelid = $1 AND NOT EXISTS (SELECT 1 FROM pg_inherits h WHERE h.inhrelid = i.indexrelid)
     ORDER BY ic.relname";

/// A partition's clone of its parent's trigger comes with the parent, so only a clone whose
/// state or comment the replay would not reproduce is read, to set them again.
const TRIGGERS: &str = "SELECT t.tgname, pg_get_triggerdef(t.oid), t.tgenabled::text, obj_description(t.oid, 'pg_trigger'), p.oid IS NOT NULL
     FROM pg_trigger t
     LEFT JOIN pg_depend d ON d.classid = 'pg_trigger'::regclass AND d.objid = t.oid AND d.refclassid = 'pg_trigger'::regclass AND d.deptype = 'P'
     LEFT JOIN pg_trigger p ON p.oid = d.refobjid
     WHERE t.tgrelid = $1
       AND (p.oid IS NULL AND NOT t.tgisinternal
            OR p.oid IS NOT NULL AND (t.tgenabled <> 'O' OR t.tgenabled <> p.tgenabled OR obj_description(t.oid, 'pg_trigger') IS NOT NULL))
     ORDER BY t.tgname";

const RULES: &str = "SELECT r.rulename, pg_get_ruledef(r.oid), r.ev_enabled::text, obj_description(r.oid, 'pg_rewrite') FROM pg_rewrite r
     WHERE r.ev_class = $1 AND r.rulename <> '_RETURN' ORDER BY r.rulename";

const POLICIES: &str = "SELECT p.polname, p.polpermissive, p.polcmd::text,
        ARRAY(SELECT r.rolname::text FROM pg_roles r WHERE r.oid = ANY (p.polroles) ORDER BY r.rolname),
        pg_get_expr(p.polqual, p.polrelid), pg_get_expr(p.polwithcheck, p.polrelid), obj_description(p.oid, 'pg_policy')
     FROM pg_policy p WHERE p.polrelid = $1 ORDER BY p.polname";

const PARTITIONS: &str = "SELECT c.oid FROM pg_inherits i JOIN pg_class c ON c.oid = i.inhrelid
     WHERE i.inhparent = $1 AND c.relispartition
     ORDER BY pg_get_expr(c.relpartbound, c.oid) = 'DEFAULT', c.relname";

const ROUTINE: &str = "SELECT p.prokind::text, p.proname, CASE WHEN p.prokind <> 'a' THEN pg_get_functiondef(p.oid) END,
        obj_description(p.oid, 'pg_proc'), n.nspname, pg_get_function_identity_arguments(p.oid)
     FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace WHERE p.oid = $1";

fn acl_changes(from: &str, acl: &str, default: &str, filter: &str, keys: &str) -> String {
    let side = |exploded: &str, granted: bool| {
        format!("SELECT {keys}, a.grantee, a.privilege_type, a.is_grantable, {granted} FROM {from} CROSS JOIN LATERAL aclexplode({exploded}) a WHERE {filter} AND {acl} IS NOT NULL")
    };
    format!("({} EXCEPT {}) UNION ALL ({} EXCEPT {})", side(acl, true), side(default, true), side(default, false), side(acl, false))
}

fn grants_query(changes: &[String]) -> String {
    format!(
        "SELECT k.s, k.n, k.col, k.privilege_type, CASE WHEN k.grantee = 0 THEN NULL ELSE pg_get_userbyid(k.grantee)::text END, k.is_grantable, k.granted
         FROM ({}) k(s, n, col, grantee, privilege_type, is_grantable, granted)
         ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 5 NULLS FIRST, 4, 7",
        changes.join(" UNION ALL ")
    )
}

fn relation_grants_query() -> String {
    grants_query(&[
        acl_changes("pg_class c", "c.relacl", "acldefault('r', c.relowner)", "c.oid = $1", "NULL::text, NULL::text, NULL::text"),
        acl_changes(
            "pg_attribute at JOIN pg_class ac ON ac.oid = at.attrelid",
            "at.attacl",
            "acldefault('c', ac.relowner)",
            "at.attrelid = $1 AND at.attnum > 0 AND NOT at.attisdropped",
            "NULL::text, NULL::text, at.attname::text",
        ),
        acl_changes(
            "pg_depend dep JOIN pg_class sc ON sc.oid = dep.objid AND sc.relkind = 'S' JOIN pg_namespace sn ON sn.oid = sc.relnamespace",
            "sc.relacl",
            "acldefault('s', sc.relowner)",
            "dep.classid = 'pg_class'::regclass AND dep.refclassid = 'pg_class'::regclass AND dep.refobjid = $1 AND dep.deptype IN ('a', 'i')",
            "sn.nspname::text, sc.relname::text, NULL::text",
        ),
    ])
}

fn routine_grants_query() -> String {
    grants_query(&[acl_changes("pg_proc p", "p.proacl", "acldefault('f', p.proowner)", "p.oid = $1", "NULL::text, NULL::text, NULL::text")])
}

fn grant_row(r: &Row) -> Privilege {
    let change = match (r.get::<_, bool>(6), r.get::<_, bool>(5)) {
        (false, _) => Change::Revoked,
        (true, true) => Change::GrantedWithGrantOption,
        (true, false) => Change::Granted,
    };
    Privilege { name: r.get(3), column: r.get::<_, Option<&str>>(2).map(ident), grantee: r.get::<_, Option<&str>>(4).map_or("PUBLIC".into(), ident), change }
}

fn unscripted(r: &Row, grants: &[Row]) -> ddl::Unscripted {
    let mut u = ddl::Unscripted { security_labels: r.get(16), publications: r.get(22), ..Default::default() };
    for g in grants {
        match (g.get::<_, Option<String>>(0), g.get::<_, Option<String>>(1)) {
            (Some(schema), Some(name)) => match u.sequence_privileges.last_mut() {
                Some(s) if s.schema == schema && s.name == name => s.privileges.push(grant_row(g)),
                _ => u.sequence_privileges.push(ddl::SequencePrivileges { schema, name, privileges: vec![grant_row(g)] }),
            },
            _ => u.privileges.push(grant_row(g)),
        }
    }
    u
}

/// With `search_path` only `pg_catalog`, the server qualifies every name it prints in an
/// expression or a view, index, constraint, or trigger definition. Routine bodies stay as written.
pub async fn load(client: &mut Client, target: &ScriptTarget) -> Result<ObjectScript, DetailError> {
    let tx = client.build_transaction().read_only(true).start().await.map_err(failed)?;
    let version: i32 = tx.query_one("SELECT current_setting('server_version_num')::int4, set_config('search_path', 'pg_catalog', true)", &[]).await.map_err(failed)?.get(0);
    let queries = Queries::for_version(version);
    let script = match target {
        ScriptTarget::Relation { oid } => relation(&tx, &queries, *oid).await,
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

async fn relation(tx: &Transaction<'_>, queries: &Queries, oid: u32) -> Result<ObjectScript, DetailError> {
    let r = tx.query_opt(RELATION, &[&oid]).await.map_err(failed)?.ok_or(DetailError::Gone)?;
    let name: String = r.get(2);
    let ddl = match r.get::<_, &str>(0) {
        "r" | "p" | "f" => ddl::table(&table(tx, queries, oid, &r).await?),
        kind @ ("v" | "m") => {
            let params: &[&(dyn ToSql + Sync)] = &[&oid];
            let grants = relation_grants_query();
            let (query, columns, indexes, triggers, rules, grants) = tokio::try_join!(
                tx.query_one("SELECT pg_get_viewdef($1::oid, true)", params),
                tx.query(&queries.columns, params),
                tx.query(INDEXES, params),
                tx.query(TRIGGERS, params),
                tx.query(RULES, params),
                tx.query(&grants, params)
            )
            .map_err(failed)?;
            ddl::view(&ddl::View {
                schema: r.get(1),
                name: name.clone(),
                materialized: kind == "m",
                populated: r.get(10),
                access_method: r.get(15),
                options: r.get(4),
                tablespace: r.get(14),
                query: query.get::<_, Option<String>>(0).ok_or(DetailError::Gone)?,
                comment: r.get(5),
                columns: columns.iter().map(column).collect(),
                indexes: indexes.iter().map(index_row).collect(),
                triggers: triggers.iter().map(cloned_trigger_row).collect(),
                rules: rules.iter().map(rule_row).collect(),
                unscripted: unscripted(&r, &grants),
            })
        }
        _ => return Err(refused("Only tables and views can be scripted.")),
    };
    Ok(script::object_script(&name, ddl))
}

async fn table(tx: &Transaction<'_>, queries: &Queries, oid: u32, r: &Row) -> Result<ddl::Table, DetailError> {
    let params: &[&(dyn ToSql + Sync)] = &[&oid];
    let grants = relation_grants_query();
    let (columns, sequences, constraints, indexes, triggers, rules, policies, statistics, partitions, grants) = tokio::try_join!(
        tx.query(&queries.columns, params),
        tx.query(OWNED_SEQUENCES, params),
        tx.query(CONSTRAINTS, params),
        tx.query(INDEXES, params),
        tx.query(TRIGGERS, params),
        tx.query(RULES, params),
        tx.query(POLICIES, params),
        tx.query(&queries.statistics, params),
        tx.query(PARTITIONS, params),
        tx.query(&grants, params),
    )
    .map_err(failed)?;
    let schema: String = r.get(1);
    let serials: Vec<String> = columns.iter().filter_map(|c| c.get::<_, Option<String>>(30)).collect();
    let parent = match r.get::<_, Option<String>>(7) {
        Some(bound) => ddl::Parent::Partition { schema: r.get(8), name: r.get(9), bound },
        None => {
            let (schemas, names): (Vec<String>, Vec<String>) = (r.get(19), r.get(20));
            if schemas.is_empty() { ddl::Parent::None } else { ddl::Parent::Inherits(schemas.into_iter().zip(names).collect()) }
        }
    };
    let mut children = vec![];
    for p in &partitions {
        let child: u32 = p.get(0);
        let row = tx.query_opt(RELATION, &[&child]).await.map_err(failed)?.ok_or(DetailError::Gone)?;
        children.push(Box::pin(table(tx, queries, child, &row)).await?);
    }
    Ok(ddl::Table {
        name: r.get(2),
        unlogged: r.get(3),
        of_type: r.get(21),
        foreign: r.get::<_, Option<String>>(17).map(|server| ddl::Foreign { server, options: r.get(18) }),
        access_method: r.get(15),
        options: r.get(4),
        tablespace: r.get(14),
        comment: r.get(5),
        columns: columns.iter().map(column).collect(),
        owned_sequences: sequences
            .iter()
            .filter(|s| !(s.get::<_, &str>(0) == schema && serials.iter().any(|n| n == s.get::<_, &str>(1))))
            .map(owned_sequence)
            .collect(),
        constraints: constraints.iter().map(constraint_row).collect(),
        parent,
        partition_key: r.get(6),
        partitions: children,
        indexes: indexes.iter().map(index_row).collect(),
        replica_identity: match r.get::<_, &str>(13) {
            "n" => ddl::ReplicaIdentity::Nothing,
            "f" => ddl::ReplicaIdentity::Full,
            "i" => r.get::<_, Option<String>>(23).map_or(ddl::ReplicaIdentity::Index, ddl::ReplicaIdentity::AttachedIndex),
            _ => ddl::ReplicaIdentity::Default,
        },
        triggers: triggers.iter().map(cloned_trigger_row).collect(),
        rules: rules.iter().map(rule_row).collect(),
        row_security: r.get(11),
        force_row_security: r.get(12),
        policies: policies
            .iter()
            .map(|p| ddl::Policy { name: p.get(0), permissive: p.get(1), command: first(p.get(2)), roles: p.get(3), using: p.get(4), check: p.get(5), comment: p.get(6) })
            .collect(),
        statistics: statistics.iter().map(|s| ddl::Statistics { schema: s.get(0), name: s.get(1), definition: s.get(2), target: s.get(3), comment: s.get(4) }).collect(),
        unscripted: unscripted(r, &grants),
        schema,
    })
}

fn first(s: &str) -> char {
    s.chars().next().unwrap_or(' ')
}

fn owned_sequence(r: &Row) -> ddl::Sequence {
    ddl::Sequence {
        schema: r.get(0),
        name: r.get(1),
        column: r.get(2),
        type_name: r.get(3),
        start: r.get(4),
        increment: r.get(5),
        min: r.get(6),
        max: r.get(7),
        cache: r.get(8),
        cycle: r.get(9),
    }
}

fn column(r: &Row) -> ddl::Column {
    let identity = match r.get::<_, &str>(3) {
        "" => None,
        kind => Some(ddl::Identity {
            always: kind == "a",
            sequence: ddl::Sequence {
                schema: r.get(9),
                name: r.get(10),
                column: r.get(0),
                type_name: r.get(11),
                start: r.get(12),
                increment: r.get(13),
                min: r.get(14),
                max: r.get(15),
                cache: r.get(16),
                cycle: r.get(17),
            },
        }),
    };
    let generated = match r.get::<_, &str>(4) {
        "s" => Some(ddl::Generated::Stored),
        "v" => Some(ddl::Generated::Virtual),
        _ => None,
    };
    let inherited = r.get::<_, Option<i64>>(25).map(|position| ddl::Inherited { replay_order: position, default: r.get(26), not_null: r.get(27), storage: r.get(28), compression: r.get(29) });
    ddl::Column {
        name: r.get(0),
        type_name: r.get(1),
        collation: r.get::<_, Option<String>>(6).zip(r.get::<_, Option<String>>(7)),
        not_null: r.get(2),
        default: r.get(5),
        generated,
        identity,
        comment: r.get(8),
        declared: r.get(18),
        inherited,
        statistics: r.get(19),
        storage: r.get(20),
        type_storage: r.get(21),
        compression: r.get(22),
        options: r.get(23),
        fdw_options: r.get(24),
    }
}

fn constraint_row(r: &Row) -> ddl::Constraint {
    ddl::Constraint { name: r.get(0), definition: r.get(1), comment: r.get(2), after_indexes: r.get(3) }
}

fn index_row(r: &Row) -> ddl::Index {
    let (columns, targets): (Vec<i32>, Vec<i32>) = (r.get(6), r.get(7));
    ddl::Index {
        name: r.get(0),
        definition: r.get(1),
        comment: r.get(2),
        clustered: r.get(3),
        replica_identity: r.get(4),
        tablespace: r.get(5),
        statistics: columns.into_iter().zip(targets).map(|(column, target)| ddl::ColumnStatistics { column, target }).collect(),
        invalid: r.get(8),
        partitioned: r.get(9),
    }
}

fn trigger_row(r: &Row) -> ddl::Trigger {
    ddl::Trigger { name: r.get(0), definition: r.get(1), enabled: first(r.get(2)), comment: r.get(3), cloned: false }
}

fn cloned_trigger_row(r: &Row) -> ddl::Trigger {
    ddl::Trigger { cloned: r.get(4), ..trigger_row(r) }
}

fn rule_row(r: &Row) -> ddl::Rule {
    ddl::Rule { name: r.get(0), definition: r.get(1), enabled: first(r.get(2)), comment: r.get(3) }
}

fn keyword(relkind: &str) -> &'static str {
    match relkind {
        "f" => "FOREIGN TABLE",
        "m" => "MATERIALIZED VIEW",
        _ => "TABLE",
    }
}

async fn routine(tx: &Transaction<'_>, oid: u32) -> Result<ObjectScript, DetailError> {
    let grants = routine_grants_query();
    let params: &[&(dyn ToSql + Sync)] = &[&oid];
    let (r, grants) = tokio::try_join!(tx.query_opt(ROUTINE, params), tx.query(&grants, params)).map_err(failed)?;
    let r = r.ok_or(DetailError::Gone)?;
    let kind: &str = r.get(0);
    let name: String = r.get(1);
    let Some(definition) = r.get::<_, Option<String>>(2) else {
        return Err(refused("Aggregates cannot be scripted yet."));
    };
    let signature = format!("{}({})", ddl::qualified(r.get(4), &name), r.get::<_, &str>(5));
    Ok(script::object_script(&name, ddl::routine(&definition, kind == "p", &signature, &r.get(3), &grants.iter().map(grant_row).collect::<Vec<_>>())))
}

async fn index(tx: &Transaction<'_>, relation: u32, name: &str) -> Result<ObjectScript, DetailError> {
    let sql = "SELECT ic.relname,
                      CASE WHEN k.oid IS NULL THEN pg_get_indexdef(i.indexrelid) END,
                      obj_description(i.indexrelid, 'pg_class'), i.indisclustered, i.indisreplident, ts.spcname,
                      ARRAY(SELECT a.attnum::int4 FROM pg_attribute a WHERE a.attrelid = i.indexrelid AND NULLIF(a.attstattarget, -1) IS NOT NULL ORDER BY a.attnum),
                      ARRAY(SELECT NULLIF(a.attstattarget, -1)::int4 FROM pg_attribute a WHERE a.attrelid = i.indexrelid AND NULLIF(a.attstattarget, -1) IS NOT NULL ORDER BY a.attnum),
                      NOT i.indisvalid, ic.relkind = 'I',
                      k.conname, pg_get_constraintdef(k.oid), n.nspname, c.relname, c.relkind::text, obj_description(k.oid, 'pg_constraint')
               FROM pg_index i JOIN pg_class ic ON ic.oid = i.indexrelid JOIN pg_class c ON c.oid = i.indrelid JOIN pg_namespace n ON n.oid = c.relnamespace
               LEFT JOIN pg_tablespace ts ON ts.oid = ic.reltablespace
               LEFT JOIN pg_constraint k ON k.conindid = i.indexrelid AND k.conrelid = i.indrelid AND k.contype IN ('p', 'u', 'x')
               WHERE i.indrelid = $1 AND ic.relname = $2";
    let r = tx.query_opt(sql, &[&relation, &name]).await.map_err(failed)?.ok_or(DetailError::Gone)?;
    let constraint = r.get::<_, Option<String>>(10).map(|name| ddl::Constraint { name, definition: r.get(11), comment: r.get(15), after_indexes: false });
    let (schema, table): (&str, &str) = (r.get(12), r.get(13));
    let ddl = ddl::index_script(&index_row(&r), constraint.as_ref(), keyword(r.get(14)), schema, &ddl::qualified(schema, table));
    Ok(script::object_script(name, ddl))
}

async fn trigger(tx: &Transaction<'_>, relation: u32, name: &str) -> Result<ObjectScript, DetailError> {
    let sql = "SELECT t.tgname, pg_get_triggerdef(t.oid), t.tgenabled::text, obj_description(t.oid, 'pg_trigger'), n.nspname, c.relname, c.relkind::text
               FROM pg_trigger t JOIN pg_class c ON c.oid = t.tgrelid JOIN pg_namespace n ON n.oid = c.relnamespace
               WHERE t.tgrelid = $1 AND t.tgname = $2";
    let r = tx.query_opt(sql, &[&relation, &name]).await.map_err(failed)?.ok_or(DetailError::Gone)?;
    let table = ddl::qualified(r.get(4), r.get(5));
    Ok(script::object_script(name, script::Ddl::complete(ddl::script(ddl::trigger(&trigger_row(&r), keyword(r.get(6)), &table)))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_read_only_the_columns_the_server_has() {
        let v12 = Queries::for_version(120000);
        assert!(!v12.columns.contains("attcompression::text") && !v12.statistics.contains("stxstattarget"), "{}\n{}", v12.columns, v12.statistics);
        let v13 = Queries::for_version(130000);
        assert!(!v13.columns.contains("attcompression::text") && v13.statistics.contains("stxstattarget"));
        let v14 = Queries::for_version(140000);
        assert!(v14.columns.contains("a.attcompression::text") && v14.columns.contains("pa.attcompression::text"));
        assert!(v14.columns.contains("coalesce(a.attoptions, '{}')"), "{}", v14.columns);
    }
}
