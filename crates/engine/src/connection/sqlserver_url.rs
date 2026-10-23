use crate::model::{ConnectionDraft, DatabaseKind, PasswordInput, SslMode};
use percent_encoding::percent_decode_str;

fn decode(value: &str) -> Result<String, String> {
    let decoded = percent_decode_str(value).decode_utf8().map_err(|_| "SQL Server URL contains invalid UTF-8".to_owned())?;
    if decoded.chars().any(char::is_control) {
        return Err("SQL Server URL values cannot contain control characters".into());
    }
    Ok(decoded.into_owned())
}

pub(super) fn parse(input: &str, name: &str) -> Result<ConnectionDraft, String> {
    if input.chars().any(char::is_control) {
        return Err("SQL Server URL cannot contain control characters".into());
    }
    for (i, byte) in input.bytes().enumerate() {
        if byte == b'%' && !input.as_bytes().get(i + 1..i + 3).is_some_and(|pair| pair.iter().all(u8::is_ascii_hexdigit)) {
            return Err("SQL Server URL contains invalid percent encoding".into());
        }
    }
    let (_, authority) = input.split_once("://").ok_or("Use mssql:// or sqlserver:// with a TCP host")?;
    let raw_path = authority.split(['?', '#']).next().unwrap_or_default().split_once('/').map(|(_, p)| p).unwrap_or_default();
    if raw_path.contains('/') || matches!(raw_path, "." | "..") {
        return Err("SQL Server URL must contain a single database name; percent-encode slashes in that name".into());
    }
    let url = url::Url::parse(input).map_err(|e| format!("Invalid SQL Server URL: {e}"))?;
    if url.fragment().is_some() {
        return Err("SQL Server URL fragments are unsupported".into());
    }
    let host = url.host_str().filter(|h| !h.is_empty()).ok_or("SQL Server URL needs a TCP host")?;
    if host.contains([',', '\\', '%']) {
        return Err("SQL Server URL requires one TCP host; named instances and multi-host URLs are unsupported".into());
    }
    let host = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host).to_owned();
    let port = url.port().unwrap_or(1433);
    if port == 0 {
        return Err("SQL Server port must be between 1 and 65535".into());
    }
    let mut ssl_mode = SslMode::VerifyFull;
    let mut seen_ssl = false;
    if let Some(query) = url.query().filter(|q| !q.is_empty()) {
        for pair in query.split('&') {
            let (key, value) = pair.split_once('=').ok_or("SQL Server URL options need a name and value")?;
            let key = decode(&key.replace('+', " "))?;
            let value = decode(&value.replace('+', " "))?;
            if key != "sslmode" {
                return Err(format!("Unsupported SQL Server URL option: {key}"));
            }
            if seen_ssl {
                return Err("SQL Server URL repeats sslmode".into());
            }
            seen_ssl = true;
            ssl_mode = match value.as_str() {
                "require" => SslMode::Require,
                "verify-full" => SslMode::VerifyFull,
                _ => return Err("SQL Server sslmode must be require or verify-full".into()),
            };
        }
    }
    let database = if raw_path.is_empty() { "master".into() } else { decode(raw_path)? };
    Ok(ConnectionDraft {
        id: None,
        database_kind: DatabaseKind::SqlServer,
        name: name.into(),
        host,
        port,
        database,
        user: decode(url.username())?,
        password: url.password().map(decode).transpose()?.map_or(PasswordInput::Clear, PasswordInput::Set),
        ssl_mode,
        ca_cert_path: None,
        color: None,
        ssh: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::draft_from_url;

    #[test]
    fn both_schemes_default_to_encrypted_sql_server_and_decode_unicode_values() {
        for scheme in ["mssql", "sqlserver", "SQLSERVER"] {
            let draft = draft_from_url(&format!("{scheme}://localhost"), "local").unwrap();
            assert_eq!((draft.database_kind, draft.port, draft.ssl_mode), (DatabaseKind::SqlServer, 1433, SslMode::VerifyFull));
            assert_eq!(draft.database, "master");
            assert_eq!(draft.name, "local");
        }
        let draft = draft_from_url("sqlserver://a%40b:p%3A%2F%40%F0%9F%98%80@localhost:1434/shop%20%2F%20%E4%B8%96%E7%95%8C?sslmode=require", "test").unwrap();
        assert_eq!((draft.host.as_str(), draft.port, draft.database.as_str(), draft.user.as_str()), ("localhost", 1434, "shop / 世界", "a@b"));
        assert!(matches!(draft.password, PasswordInput::Set(p) if p == "p:/@😀"));
        assert_eq!(draft.ssl_mode, SslMode::Require);
        assert_eq!(draft_from_url("mssql://sa@localhost/shop?sslmode=verify-full", "").unwrap().ssl_mode, SslMode::VerifyFull);
        assert_eq!(draft_from_url("mssql://[::1]:1434/shop", "").unwrap().host, "::1");
    }

    #[test]
    fn unsupported_or_unrepresentable_options_are_rejected() {
        for url in [
            "mssql://localhost/shop?sslmode=disable",
            "mssql://localhost/shop?sslmode=verify-ca",
            "mssql://localhost/shop?sslrootcert=ca.pem",
            "mssql://localhost/shop?encrypt=false",
            "mssql://localhost/shop?integratedSecurity=true",
            "mssql://localhost/shop?applicationName=test",
            "mssql://localhost/shop?sslmode=require&sslmode=verify-full",
            "mssql://localhost/shop#ignored",
            "mssql://localhost/a/b",
            "mssql://localhost/a/../shop",
            "mssql://localhost:0/shop",
            "mssql://sa:%FF@localhost/shop",
            "mssql://sa:bad%2@localhost/shop",
            "mssql://localhost/shop%00",
            "mssql://localhost,other/shop",
            "mssql://localhost/shop?sslmode",
            "mssql:shop",
        ] {
            assert!(draft_from_url(url, "").is_err(), "{url}");
        }
    }
}
