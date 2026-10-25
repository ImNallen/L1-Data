use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::model::ConnectError;
use crate::shell_path::{self, AgentPath, is_executable};

const AZ_BINARY_ENV: &str = "L1_DATA_AZ_BINARY";
const RESOURCE: &str = "https://database.windows.net/";
const REFRESH_BEFORE: Duration = Duration::from_secs(5 * 60);
const AZ_TIMEOUT: Duration = Duration::from_secs(30);
const AZ_NAMES: &[&str] = if cfg!(windows) { &["az.cmd", "az.exe"] } else { &["az"] };

pub(crate) struct AzCli {
    agent_path: AgentPath,
    binary: Option<PathBuf>,
    path: tokio::sync::OnceCell<OsString>,
    cached: tokio::sync::Mutex<Option<Cached>>,
}

struct Cached {
    access: String,
    refresh_at: SystemTime,
}

#[derive(Debug, PartialEq)]
struct Token {
    access: String,
    expires_at: Option<SystemTime>,
}

impl std::fmt::Debug for AzCli {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AzCli")
    }
}

impl AzCli {
    pub(crate) fn new(agent_path: AgentPath) -> Self {
        let binary = matches!(agent_path, AgentPath::LoginShell).then(|| std::env::var_os(AZ_BINARY_ENV).map(PathBuf::from)).flatten();
        Self { agent_path, binary, path: tokio::sync::OnceCell::new(), cached: tokio::sync::Mutex::new(None) }
    }

    pub(crate) async fn token(&self) -> Result<String, ConnectError> {
        let mut cached = self.cached.lock().await;
        if let Some(c) = cached.as_ref().filter(|c| SystemTime::now() < c.refresh_at) {
            return Ok(c.access.clone());
        }
        let token = self.fetch().await?;
        *cached = token.expires_at.and_then(|at| at.checked_sub(REFRESH_BEFORE)).filter(|at| SystemTime::now() < *at).map(|refresh_at| Cached { access: token.access.clone(), refresh_at });
        Ok(token.access)
    }

    pub(crate) async fn forget(&self, access: &str) {
        let mut cached = self.cached.lock().await;
        if cached.as_ref().is_some_and(|c| c.access == access) {
            *cached = None;
        }
    }

    async fn fetch(&self) -> Result<Token, ConnectError> {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let path = self.path.get_or_init(|| shell_path::search_path(&self.agent_path, &home)).await;
        let program = self.program(path)?;
        let run = tokio::process::Command::new(&program)
            .args(["account", "get-access-token", "--resource", RESOURCE, "--output", "json"])
            .env("PATH", path)
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output();
        let output = tokio::time::timeout(AZ_TIMEOUT, run)
            .await
            .map_err(|_| auth(format!("The Azure CLI did not answer within {} s. Run `az login`, then try again.", AZ_TIMEOUT.as_secs())))?
            .map_err(|e| auth(format!("Could not run the Azure CLI at {}: {e}. Install the Azure CLI, run `az login`, then try again.", program.display())))?;
        if !output.status.success() {
            let said = String::from_utf8_lossy(&output.stderr).lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default().chars().take(300).collect::<String>();
            return Err(auth(format!("The Azure CLI has no token for Azure SQL. Run `az login`, then try again. az said: {said}")));
        }
        parse(&output.stdout).map_err(|e| auth(format!("The Azure CLI's token could not be read: {e}")))
    }

    fn program(&self, path: &OsString) -> Result<PathBuf, ConnectError> {
        let missing = |at: String| auth(format!("Microsoft Entra ID sign-in uses the Azure CLI, which was not found {at}. Install the Azure CLI, run `az login`, then try again."));
        if let Some(binary) = &self.binary {
            return if is_executable(binary) { Ok(binary.clone()) } else { Err(missing(format!("at {} ({AZ_BINARY_ENV})", binary.display()))) };
        }
        shell_path::dirs(path).iter().flat_map(|dir| AZ_NAMES.iter().map(move |name| dir.join(name))).find(|p| is_executable(p)).ok_or_else(|| missing("on the PATH".into()))
    }
}

fn auth(message: String) -> ConnectError {
    ConnectError::Auth { message }
}

/// `expires_on` is POSIX seconds, which az prints from 2.54 on. `expiresOn` is local time with no
/// zone, so it is not read.
fn parse(stdout: &[u8]) -> Result<Token, String> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Reply {
        access_token: String,
        #[serde(rename = "expires_on", default)]
        expires_on: Option<serde_json::Value>,
    }
    let reply: Reply = serde_json::from_slice(stdout).map_err(|e| e.to_string())?;
    if reply.access_token.is_empty() {
        return Err("accessToken is empty".into());
    }
    let seconds = match reply.expires_on {
        Some(serde_json::Value::Number(n)) => n.as_u64(),
        Some(serde_json::Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    };
    Ok(Token { access: reply.access_token, expires_at: seconds.and_then(|s| UNIX_EPOCH.checked_add(Duration::from_secs(s))) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn epoch(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds)
    }

    #[test]
    fn a_reply_parses_to_its_token_and_posix_expiry() {
        let reply = br#"{"accessToken":"eyJ0","expiresOn":"2026-10-25 14:03:12.000000","expires_on":1761400992,"subscription":"s","tenant":"t","tokenType":"Bearer"}"#;
        assert_eq!(parse(reply).unwrap(), Token { access: "eyJ0".into(), expires_at: Some(epoch(1761400992)) });
        assert_eq!(parse(br#"{"accessToken":"a","expires_on":"1761400992"}"#).unwrap().expires_at, Some(epoch(1761400992)));
        assert_eq!(parse(br#"{"accessToken":"a","expiresOn":"2026-10-25 14:03:12.000000"}"#).unwrap().expires_at, None);
        assert_eq!(parse(br#"{"accessToken":"a","expires_on":18446744073709551615}"#).unwrap().expires_at, None, "an expiry past SystemTime is not cached");
        assert!(parse(br#"{"accessToken":"","expires_on":1}"#).is_err());
        assert!(parse(b"ERROR: Please run 'az login'").is_err());
    }

    fn seeded(access: &str, refresh_at: SystemTime) -> AzCli {
        let cli = AzCli::new(AgentPath::Fixed("".into()));
        *cli.cached.try_lock().unwrap() = Some(Cached { access: access.into(), refresh_at });
        cli
    }

    #[tokio::test]
    async fn a_token_before_its_refresh_point_is_served_without_az() {
        let cli = seeded("cached", SystemTime::now() + Duration::from_secs(60));
        assert_eq!(cli.token().await.unwrap(), "cached");
    }

    #[tokio::test]
    async fn a_token_past_its_refresh_point_runs_az_again() {
        let cli = seeded("stale", SystemTime::now() - Duration::from_secs(1));
        let Err(ConnectError::Auth { message }) = cli.token().await else { panic!("az is not on an empty PATH") };
        assert!(message.contains("Install the Azure CLI, run `az login`"), "{message}");
    }

    #[tokio::test]
    async fn forget_drops_only_the_token_it_names() {
        let cli = seeded("current", SystemTime::now() + Duration::from_secs(60));
        cli.forget("older").await;
        assert_eq!(cli.token().await.unwrap(), "current");
        cli.forget("current").await;
        assert!(cli.token().await.is_err());
    }

    #[tokio::test]
    async fn cached_reads_take_under_a_millisecond() {
        let cli = seeded("cached", SystemTime::now() + Duration::from_secs(3600));
        let mut samples = Vec::with_capacity(1000);
        for _ in 0..1000 {
            let started = std::time::Instant::now();
            cli.token().await.unwrap();
            samples.push(started.elapsed());
        }
        samples.sort();
        let (p50, p95) = (samples[499], samples[949]);
        println!("cached token read over 1000 calls: p50 {:?}, p95 {:?}", p50, p95);
        assert!(p95 < Duration::from_millis(1), "p95 {p95:?}");
    }
}
