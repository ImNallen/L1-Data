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
    async fn with_az(scenario: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let az = bin.join("az");
        let log = dir.path().join("az.log");
        std::fs::write(&az, format!("#!/bin/sh\nexec '{FAKE_AZ}' --fake-scenario {scenario} --fake-log '{}' \"$@\"\n", log.display())).unwrap();
        std::fs::set_permissions(&az, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self::open(dir, AgentPath::Fixed(bin.into())).await
    }

    async fn without_az() -> Self {
        Self::open(tempfile::tempdir().unwrap(), AgentPath::Fixed("".into())).await
    }

    async fn open(dir: tempfile::TempDir, agent_path: AgentPath) -> Self {
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
        ssl_mode: SslMode::VerifyFull,
        ca_cert_path: None,
        color: None,
        ssh: None,
        environment: None,
        sql_server_auth: Some(SqlServerAuth::EntraAzureCli),
    }
}

#[tokio::test]
async fn an_entra_connection_needs_no_user_and_saves_no_password() {
    let h = Harness::with_az("ok").await;
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
    let h = Harness::with_az("ok").await;
    let id = h.engine.save_connection(entra("127.0.0.1", refused_port())).await.unwrap().id;
    for _ in 0..3 {
        assert!(matches!(h.engine.connect(id).await, Err(ConnectError::Network { .. })));
    }
    assert_eq!(h.calls(), 1, "{}", std::fs::read_to_string(h.log()).unwrap());
}

#[tokio::test]
async fn a_token_inside_the_refresh_margin_is_fetched_again() {
    let h = Harness::with_az("short-expiry").await;
    let id = h.engine.save_connection(entra("127.0.0.1", refused_port())).await.unwrap().id;
    for _ in 0..2 {
        assert!(matches!(h.engine.connect(id).await, Err(ConnectError::Network { .. })));
    }
    assert_eq!(h.calls(), 2);
}

#[tokio::test]
async fn a_signed_out_cli_fails_as_auth_naming_az_login() {
    let h = Harness::with_az("signed-out").await;
    let Err(ConnectError::Auth { message }) = h.engine.test_connection(entra("127.0.0.1", refused_port())).await else { panic!("expected Auth") };
    assert!(message.contains("Run `az login`") && message.contains("Please run 'az login'"), "{message}");
    assert_eq!(h.calls(), 1);
}

#[tokio::test]
async fn a_missing_cli_fails_as_auth_naming_the_install_and_az_login() {
    let h = Harness::without_az().await;
    let Err(ConnectError::Auth { message }) = h.engine.test_connection(entra("127.0.0.1", refused_port())).await else { panic!("expected Auth") };
    assert!(message.contains("Install the Azure CLI, run `az login`"), "{message}");
}

#[tokio::test]
async fn an_entra_token_is_never_sent_over_tls_that_skips_verification() {
    let h = Harness::with_az("ok").await;
    let Err(ConnectError::Tls { message }) = h.engine.test_connection(ConnectionDraft { ssl_mode: SslMode::Require, ..entra("127.0.0.1", refused_port()) }).await else { panic!("expected Tls") };
    assert!(message.starts_with("Choose Verify full"), "{message}");
    assert_eq!(h.calls(), 0, "the token is not even fetched");
}

#[tokio::test]
async fn a_sql_login_never_runs_az() {
    let h = Harness::with_az("ok").await;
    let draft = ConnectionDraft { user: "sa".into(), sql_server_auth: None, ..entra("127.0.0.1", refused_port()) };
    assert!(matches!(h.engine.test_connection(draft).await, Err(ConnectError::Network { .. })));
    assert_eq!(h.calls(), 0);
}

#[cfg(feature = "mssql-tests")]
#[tokio::test]
async fn the_dev_servers_self_signed_certificate_stops_an_entra_sign_in_before_the_token_is_sent() {
    let h = Harness::with_az("ok").await;
    let host = std::env::var("L1_DATA_TEST_MSSQL_HOST").unwrap_or_else(|_| "localhost".into());
    let port = std::env::var("L1_DATA_TEST_MSSQL_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(1434);
    let id = h.engine.save_connection(entra(&host, port)).await.unwrap().id;
    assert!(matches!(h.engine.connect(id).await, Err(ConnectError::Tls { .. })));
}
