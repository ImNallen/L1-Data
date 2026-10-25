use sha2::{Digest, Sha256};

use crate::model::{ConnectionConfig, ConsoleConfigView, Environment, WriteConfirmation, WriteDigest};

const SHOWN: usize = 3;

/// A guarded action sends `statements` in order, each marked with whether it writes. On a Production connection
/// it runs only when `confirmed` is the digest of these statements on this connection and console target.
pub fn check(connection: &ConnectionConfig, target: &ConsoleConfigView, statements: &[(&str, bool)], confirmed: Option<&WriteDigest>) -> Result<(), WriteConfirmation> {
    if connection.environment != Some(Environment::Production) {
        return Ok(());
    }
    let writes: Vec<&str> = statements.iter().filter(|(_, writes)| *writes).map(|(sql, _)| *sql).collect();
    if writes.is_empty() {
        return Ok(());
    }
    let sent: Vec<&str> = statements.iter().map(|(sql, _)| *sql).collect();
    let json = serde_json::to_vec(&(connection, target, sent)).expect("model types serialize to JSON");
    let digest = WriteDigest(format!("{:x}", Sha256::digest(json)));
    if confirmed == Some(&digest) {
        return Ok(());
    }
    Err(WriteConfirmation {
        connection: connection.name.clone(),
        database: target.database.clone(),
        writes: writes.len().try_into().unwrap_or(u32::MAX),
        statements: writes.iter().take(SHOWN).map(|sql| sql.trim().to_owned()).collect(),
        digest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ConnectionId, DatabaseKind, SslMode, TxnMode};
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

    fn target(mode: TxnMode) -> ConsoleConfigView {
        ConsoleConfigView { connection_id: ConnectionId(Uuid::from_u128(1)), database: "shop".into(), schema: None, mode }
    }

    const WORK: &[(&str, bool)] = &[("select 1", false), (" insert into t values (1) ", true), ("delete from t", true)];

    #[test]
    fn only_a_write_on_production_asks() {
        let production = connection(Some(Environment::Production));
        let target = target(TxnMode::AutoCommit);
        assert!(check(&production, &target, &[("select 1", false)], None).is_ok());
        for environment in [None, Some(Environment::Local), Some(Environment::Staging)] {
            assert!(check(&connection(environment), &target, WORK, None).is_ok(), "{environment:?}");
        }
        let asked = check(&production, &target, WORK, None).unwrap_err();
        assert_eq!((asked.connection.as_str(), asked.database.as_str(), asked.writes), ("prod", "shop", 2));
        assert_eq!(asked.statements, ["insert into t values (1)", "delete from t"]);
    }

    #[test]
    fn the_digest_holds_only_for_the_same_work_and_target() {
        let production = connection(Some(Environment::Production));
        let auto = target(TxnMode::AutoCommit);
        let digest = check(&production, &auto, WORK, None).unwrap_err().digest;
        assert!(check(&production, &auto, WORK, Some(&digest)).is_ok());

        let edited = [("select 1", false), (" insert into t values (2) ", true), ("delete from t", true)];
        assert!(check(&production, &auto, &edited, Some(&digest)).is_err(), "the statements changed");
        let read_changed = [("select 2", false), (" insert into t values (1) ", true), ("delete from t", true)];
        assert!(check(&production, &auto, &read_changed, Some(&digest)).is_err(), "a statement that reads changed");
        assert!(check(&production, &target(TxnMode::Manual), WORK, Some(&digest)).is_err(), "the mode changed");
        let other_db = ConsoleConfigView { database: "reporting".into(), ..auto.clone() };
        assert!(check(&production, &other_db, WORK, Some(&digest)).is_err(), "the database changed");
        let other_host = ConnectionConfig { host: "db2".into(), ..production.clone() };
        assert!(check(&other_host, &auto, WORK, Some(&digest)).is_err(), "the connection changed");
    }

    #[test]
    fn the_shown_statements_stop_at_three() {
        let work: Vec<(&str, bool)> = (0..5).map(|_| ("delete from t", true)).collect();
        let asked = check(&connection(Some(Environment::Production)), &target(TxnMode::AutoCommit), &work, None).unwrap_err();
        assert_eq!((asked.writes, asked.statements.len()), (5, 3));
    }
}
