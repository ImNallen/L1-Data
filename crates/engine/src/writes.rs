use sha2::{Digest, Sha256};

use crate::model::{ConnectionConfig, ConsoleConfigView, DatabaseKind, Environment, ExecutionUnitKind, WriteConfirmation, WriteDigest};
use crate::text;

const SHOWN: usize = 3;
const SHOWN_CHARS: usize = 2000;

/// Where a guarded action runs. Saving a connection does not move the sessions it already opened, so `stored`, the
/// connection as saved now, and `connected`, the copy its live instance connected with, can differ. A session opened
/// on Production keeps asking until it reconnects when the edit also points the connection at another server.
pub struct Target<'a> {
    pub stored: &'a ConnectionConfig,
    pub connected: &'a ConnectionConfig,
    pub console: &'a ConsoleConfigView,
}

/// A guarded action sends `statements` in order. On Production, when any statement writes, it runs only when
/// `confirmed` is the digest of these statements on this target.
pub fn check<'a>(target: &Target<'a>, statements: &[&str], confirmed: Option<&WriteDigest>) -> Result<(), WriteConfirmation> {
    let production = |c: &ConnectionConfig| c.environment == Some(Environment::Production);
    let server = |c: &'a ConnectionConfig| (c.host.as_str(), c.port, c.ssh.as_ref());
    let moved = server(target.stored) != server(target.connected);
    if !production(target.stored) && !(moved && production(target.connected)) {
        return Ok(());
    }
    let kind = target.stored.database_kind;
    let writes: Vec<&str> = statements.iter().copied().filter(|sql| text::is_write(sql, kind)).collect();
    if writes.is_empty() {
        return Ok(());
    }
    let mut hasher = Sha256::new();
    serde_json::to_writer(&mut hasher, &(target.stored, target.connected, target.console, statements)).expect("model types serialize to JSON");
    let digest = WriteDigest(format!("{:x}", hasher.finalize()));
    if confirmed == Some(&digest) {
        return Ok(());
    }
    Err(WriteConfirmation {
        connection: target.stored.name.clone(),
        database: target.console.database.clone(),
        unit: match kind {
            DatabaseKind::Postgres => ExecutionUnitKind::Statement,
            DatabaseKind::SqlServer => ExecutionUnitKind::Batch,
        },
        writes: writes.len().try_into().unwrap_or(u32::MAX),
        statements: writes.iter().take(SHOWN).map(|sql| shown(sql)).collect(),
        digest,
    })
}

fn shown(sql: &str) -> String {
    let sql = sql.trim();
    match sql.char_indices().nth(SHOWN_CHARS) {
        Some((end, _)) => format!("{}…", &sql[..end]),
        None => sql.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ConnectionId, SslMode, TxnMode};
    use uuid::Uuid;

    fn connection(environment: Option<Environment>) -> ConnectionConfig {
        ConnectionConfig {
            database_kind: DatabaseKind::Postgres,
            id: ConnectionId(Uuid::from_u128(1)),
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
            environment,
        }
    }

    fn console(mode: TxnMode) -> ConsoleConfigView {
        ConsoleConfigView { connection_id: ConnectionId(Uuid::from_u128(1)), database: "shop".into(), schema: None, mode }
    }

    fn on<'a>(connection: &'a ConnectionConfig, console: &'a ConsoleConfigView) -> Target<'a> {
        Target { stored: connection, connected: connection, console }
    }

    const WORK: &[&str] = &["select 1", " insert into t values (1) ", "delete from t"];

    #[test]
    fn only_a_write_on_production_asks() {
        let production = connection(Some(Environment::Production));
        let auto = console(TxnMode::AutoCommit);
        assert!(check(&on(&production, &auto), &["select 1", "begin", "commit"], None).is_ok());
        for environment in [None, Some(Environment::Local), Some(Environment::Staging)] {
            assert!(check(&on(&connection(environment), &auto), WORK, None).is_ok(), "{environment:?}");
        }
        let asked = check(&on(&production, &auto), WORK, None).unwrap_err();
        assert_eq!((asked.connection.as_str(), asked.database.as_str(), asked.unit, asked.writes), ("prod", "shop", ExecutionUnitKind::Statement, 2));
        assert_eq!(asked.statements, ["insert into t values (1)", "delete from t"]);
    }

    #[test]
    fn a_session_opened_on_production_asks_until_it_reconnects_when_the_connection_moved() {
        let auto = console(TxnMode::AutoCommit);
        let opened = connection(Some(Environment::Production));
        let relabelled = connection(Some(Environment::Local));
        assert!(check(&Target { stored: &relabelled, connected: &opened, console: &auto }, WORK, None).is_ok(), "same server, new label");
        let moved = ConnectionConfig { host: "localhost".into(), ..relabelled.clone() };
        let asked = check(&Target { stored: &moved, connected: &opened, console: &auto }, WORK, None).unwrap_err();
        assert!(check(&Target { stored: &moved, connected: &opened, console: &auto }, WORK, Some(&asked.digest)).is_ok());
        assert!(check(&Target { stored: &opened, connected: &relabelled, console: &auto }, WORK, None).is_err(), "newly marked Production");
    }

    #[test]
    fn the_digest_holds_only_for_the_same_work_and_target() {
        let production = connection(Some(Environment::Production));
        let auto = console(TxnMode::AutoCommit);
        let digest = check(&on(&production, &auto), WORK, None).unwrap_err().digest;
        assert!(check(&on(&production, &auto), WORK, Some(&digest)).is_ok());

        let edited = ["select 1", " insert into t values (2) ", "delete from t"];
        assert!(check(&on(&production, &auto), &edited, Some(&digest)).is_err(), "a statement that writes changed");
        let read_changed = ["select 2", " insert into t values (1) ", "delete from t"];
        assert!(check(&on(&production, &auto), &read_changed, Some(&digest)).is_err(), "a statement that reads changed");
        let manual = console(TxnMode::Manual);
        assert!(check(&on(&production, &manual), WORK, Some(&digest)).is_err(), "the mode changed");
        let other_db = ConsoleConfigView { database: "reporting".into(), ..auto.clone() };
        assert!(check(&on(&production, &other_db), WORK, Some(&digest)).is_err(), "the database changed");
        let other_schema = ConsoleConfigView { schema: Some("audit".into()), ..auto.clone() };
        assert!(check(&on(&production, &other_schema), WORK, Some(&digest)).is_err(), "the schema changed");
        let other_host = ConnectionConfig { host: "db2".into(), ..production.clone() };
        assert!(check(&on(&other_host, &auto), WORK, Some(&digest)).is_err(), "the connection changed");
    }

    #[test]
    fn the_shown_statements_stop_at_three_and_are_cut_when_long() {
        let long = format!("insert into t values ('{}')", "x".repeat(3 * SHOWN_CHARS));
        let work: Vec<&str> = vec![long.as_str(), "delete from t", "delete from t", "delete from t", "delete from t"];
        let asked = check(&on(&connection(Some(Environment::Production)), &console(TxnMode::AutoCommit)), &work, None).unwrap_err();
        assert_eq!((asked.writes, asked.statements.len()), (5, 3));
        assert_eq!(asked.statements[0].chars().count(), SHOWN_CHARS + 1);
        assert!(asked.statements[0].ends_with('…'));
    }

    #[test]
    fn every_session_handover_for_work_that_can_write_follows_the_check() {
        const CAN_WRITE: [&str; 2] = ["Work::Run)", "Work::Edit)"];
        let mut checked = 0;
        for path in [concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"), concat!(env!("CARGO_MANIFEST_DIR"), "/src/export/mod.rs")] {
            let source = std::fs::read_to_string(path).unwrap();
            for (at, _) in source.match_indices(".take_session(") {
                let call = &source[at..at + source[at..].find(';').unwrap()];
                if !CAN_WRITE.iter().any(|work| call.contains(work)) {
                    continue;
                }
                let function = &source[source[..at].rfind(" fn ").unwrap()..at];
                assert!(function.contains("writes::check("), "{path}: {call} hands over a session with no writes::check before it");
                checked += 1;
            }
        }
        assert_eq!(checked, 2, "console_run and table_apply");
    }

    #[test]
    fn a_sql_server_write_is_a_batch() {
        let production = ConnectionConfig { database_kind: DatabaseKind::SqlServer, ..connection(Some(Environment::Production)) };
        let asked = check(&on(&production, &console(TxnMode::AutoCommit)), &["SELECT 1", "EXEC dbo.purge"], None).unwrap_err();
        assert_eq!((asked.unit, asked.statements.as_slice()), (ExecutionUnitKind::Batch, ["EXEC dbo.purge".to_owned()].as_slice()));
    }
}
