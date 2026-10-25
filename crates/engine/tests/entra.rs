#![cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;

use engine::model::*;
use engine::{AgentPath, Engine, EngineConfig, MemoryKeyStore};

const FAKE_AZ: &str = env!("CARGO_BIN_EXE_fake-az");

struct Harness {
    engine: Arc<Engine>,
    dir: tempfile::TempDir,
}

impl Harness {
    /// An engine whose fixed PATH holds an `az` that runs fake-az in `scenario`. `None` leaves the
    /// PATH empty, so there is no `az` at all.
    async fn open(scenario: Option<&str>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        if let Some(scenario) = scenario {
            let az = bin.join("az");
            let log = dir.path().join("az.log");
            std::fs::write(&az, format!("#!/bin/sh\nexec '{FAKE_AZ}' --fake-scenario {scenario} --fake-log '{}' \"$@\"\n", log.display())).unwrap();
            std::fs::set_permissions(&az, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let agent_path = AgentPath::Fixed(if scenario.is_some() { bin.into() } else { "".into() });
        let engine = Engine::open(EngineConfig { data_dir: dir.path().join("data"), keys: Arc::new(MemoryKeyStore::default()), agent_path, user_known_hosts: None, updates: Arc::new(engine::ScriptedTransport::named("off").unwrap()) }).await.unwrap();
        Self { engine, dir }
    }

    fn log(&self) -> PathBuf {
        self.dir.path().join("az.log")
    }

    fn calls(&self) -> usize {
        std::fs::read_to_string(self.log()).map(|log| log.lines().count()).unwrap_or(0)
    }
}

fn refused_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn entra(host: &str, port: u16) -> ConnectionDraft {
    ConnectionDraft {
        database_kind: DatabaseKind::SqlServer,
        id: None,
        name: "azure".into(),
        host: host.into(),
        port,
        database: "shop".into(),
        user: "".into(),
        password: PasswordInput::Set("never saved".into()),
        ssl_mode: SslMode::Require,
        ca_cert_path: None,
        color: None,
        ssh: None,
        environment: None,
        sql_server_auth: Some(SqlServerAuth::EntraAzureCli),
    }
}

#[tokio::test]
async fn an_entra_connection_needs_no_user_and_saves_no_password() {
    let h = Harness::open(Some("ok")).await;
    let saved = h.engine.save_connection(entra("db.example.net", 1433)).await.unwrap();
    assert_eq!((saved.sql_server_auth, saved.has_password, saved.user.as_str()), (Some(SqlServerAuth::EntraAzureCli), false, ""));
    let login = h.engine.save_connection(ConnectionDraft { id: Some(saved.id), sql_server_auth: Some(SqlServerAuth::SqlLogin), ..entra("db.example.net", 1433) }).await;
    assert!(matches!(login, Err(SaveError::Invalid { message }) if message == "user is required"));
    let postgres = ConnectionDraft { database_kind: DatabaseKind::Postgres, user: "postgres".into(), ..entra("localhost", 5432) };
    assert_eq!(h.engine.save_connection(postgres).await.unwrap().sql_server_auth, None);
    assert_eq!(h.calls(), 0, "saving never runs az");
}

#[tokio::test]
async fn sessions_share_one_token_until_its_refresh_point() {
    let h = Harness::open(Some("ok")).await;
    let id = h.engine.save_connection(entra("127.0.0.1", refused_port())).await.unwrap().id;
    for _ in 0..3 {
        assert!(matches!(h.engine.connect(id).await, Err(ConnectError::Network { .. })));
    }
    assert_eq!(h.calls(), 1, "{}", std::fs::read_to_string(h.log()).unwrap());
}

#[tokio::test]
async fn a_token_inside_the_refresh_margin_is_fetched_again() {
    let h = Harness::open(Some("short-expiry")).await;
    let id = h.engine.save_connection(entra("127.0.0.1", refused_port())).await.unwrap().id;
    for _ in 0..2 {
        assert!(matches!(h.engine.connect(id).await, Err(ConnectError::Network { .. })));
    }
    assert_eq!(h.calls(), 2);
}

#[tokio::test]
async fn a_signed_out_cli_fails_as_auth_naming_az_login() {
    let h = Harness::open(Some("signed-out")).await;
    let Err(ConnectError::Auth { message }) = h.engine.test_connection(entra("127.0.0.1", refused_port())).await else { panic!("expected Auth") };
    assert!(message.contains("Run `az login`") && message.contains("Please run 'az login'"), "{message}");
    assert_eq!(h.calls(), 1);
}

#[tokio::test]
async fn a_missing_cli_fails_as_auth_naming_the_install_and_az_login() {
    let h = Harness::open(None).await;
    let Err(ConnectError::Auth { message }) = h.engine.test_connection(entra("127.0.0.1", refused_port())).await else { panic!("expected Auth") };
    assert!(message.contains("Install the Azure CLI, run `az login`"), "{message}");
}

#[tokio::test]
async fn a_sql_login_never_runs_az() {
    let h = Harness::open(Some("ok")).await;
    let draft = ConnectionDraft { user: "sa".into(), sql_server_auth: None, ..entra("127.0.0.1", refused_port()) };
    assert!(matches!(h.engine.test_connection(draft).await, Err(ConnectError::Network { .. })));
    assert_eq!(h.calls(), 0);
}

/// The dev SQL Server has no Entra, so it refuses every token with error 18456. The engine drops
/// a refused token, so the next try runs az again.
#[cfg(feature = "mssql-tests")]
#[tokio::test]
async fn a_token_the_server_refuses_fails_as_auth_and_is_not_reused() {
    let h = Harness::open(Some("ok")).await;
    let host = std::env::var("L1_DATA_TEST_MSSQL_HOST").unwrap_or_else(|_| "localhost".into());
    let port = std::env::var("L1_DATA_TEST_MSSQL_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(1434);
    let id = h.engine.save_connection(entra(&host, port)).await.unwrap().id;
    for _ in 0..2 {
        let Err(ConnectError::Auth { message }) = h.engine.connect(id).await else { panic!("expected Auth") };
        assert!(message.contains("Login failed"), "{message}");
    }
    assert_eq!(h.calls(), 2);
}
