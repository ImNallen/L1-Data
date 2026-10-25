use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(3);

/// The PATH the engine looks up a child program on (an agent CLI, or `az`), and the PATH that
/// program runs with. `LoginShell` is the app's: a Finder-launched app gets launchd's PATH, which
/// misses where `claude`, `node`, and `az` live, so the login shell's PATH comes first, then the
/// inherited one, then the usual install directories. `Fixed` is exactly the given PATH, with no
/// shell and nothing added; tests pass an empty one, so nothing installed on the machine runs.
#[derive(Debug, Clone)]
pub enum AgentPath {
    LoginShell,
    Fixed(OsString),
}

pub(crate) async fn search_path(agent_path: &AgentPath, home: &Path) -> OsString {
    match agent_path {
        AgentPath::Fixed(path) => path.clone(),
        AgentPath::LoginShell => {
            let inherited = std::env::var_os("PATH").unwrap_or_default();
            let shell_path = login_shell_path().await.unwrap_or_default();
            let mut dirs: Vec<PathBuf> = std::env::split_paths(&shell_path).chain(std::env::split_paths(&inherited)).filter(|p| !p.as_os_str().is_empty()).collect();
            for extra in [home.join(".local/bin"), home.join(".claude/local"), PathBuf::from("/opt/homebrew/bin"), PathBuf::from("/usr/local/bin")] {
                if !dirs.contains(&extra) {
                    dirs.push(extra);
                }
            }
            std::env::join_paths(&dirs).unwrap_or(inherited)
        }
    }
}

pub(crate) fn dirs(path: &OsString) -> Vec<PathBuf> {
    std::env::split_paths(path).filter(|p| p.is_absolute()).collect()
}

pub(crate) fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

async fn login_shell_path() -> Option<OsString> {
    let shell = std::env::var_os("SHELL")?;
    let output = tokio::time::timeout(LOGIN_SHELL_TIMEOUT, tokio::process::Command::new(shell).args(["-lc", "printf %s \"$PATH\""]).kill_on_drop(true).output()).await.ok()?.ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let path = text.lines().last()?.trim();
    (!path.is_empty()).then(|| OsString::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_path_entry_is_not_searched() {
        let path = std::env::join_paths(["/opt/bin", ".", "", "node_modules/.bin", "/usr/bin"]).unwrap();
        assert_eq!(dirs(&path), [PathBuf::from("/opt/bin"), PathBuf::from("/usr/bin")]);
    }
}
