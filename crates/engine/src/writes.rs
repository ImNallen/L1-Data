use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::model::{ConnectionConfig, ConsoleConfigView, DatabaseKind, Environment, ExecutionUnitKind, TxnState, WriteConfirmation, WriteDigest};
use crate::sql::StmtKind;
use crate::text;

const SHOWN: usize = 3;
const SHOWN_CHARS: usize = 2000;

#[derive(Serialize)]
pub struct Target<'a> {
    pub saved: &'a ConnectionConfig,
    pub opened: &'a ConnectionConfig,
    pub console: &'a ConsoleConfigView,
    pub txn: TxnState,
    pub unasked: &'a UnaskedWrites,
}

#[derive(Debug, Default, Serialize)]
pub struct UnaskedWrites {
    writes: u32,
    shown: Vec<String>,
}

impl UnaskedWrites {
    pub fn add(&mut self, writes: &[&str]) {
        let room = SHOWN.saturating_sub(self.shown.len());
        self.shown.extend(writes.iter().take(room).map(|sql| shown(sql)));
        self.writes = self.writes.saturating_add(count(writes));
    }
}

impl Target<'_> {
    fn on_production(&self) -> bool {
        [self.saved, self.opened].iter().any(|c| c.environment == Some(Environment::Production))
    }

    fn digest(&self, work: &[&str]) -> WriteDigest {
        let mut hasher = Sha256::new();
        serde_json::to_writer(&mut hasher, &(self, work)).expect("model types serialize to JSON");
        WriteDigest::new(format!("{:x}", hasher.finalize()))
    }

    fn ask(&self, writes: u32, statements: Vec<String>, digest: WriteDigest) -> WriteConfirmation {
        WriteConfirmation {
            connection: self.saved.name.clone(),
            database: self.console.database.clone(),
            unit: match self.saved.database_kind {
                DatabaseKind::Postgres => ExecutionUnitKind::Statement,
                DatabaseKind::SqlServer => ExecutionUnitKind::Batch,
            },
            writes,
            statements,
            digest,
        }
    }
}

pub fn check<'s>(target: &Target, statements: &[&'s str], confirmed: Option<&WriteDigest>) -> Result<Vec<&'s str>, WriteConfirmation> {
    let kind = target.saved.database_kind;
    if !target.on_production() {
        return Ok(statements.iter().copied().filter(|sql| text::is_write(sql, kind)).collect());
    }
    let keeps_unasked = |sql: &str| target.unasked.writes > 0 && text::classify(sql, kind) == StmtKind::Commit;
    let writes: Vec<&str> = statements.iter().copied().filter(|sql| text::is_write(sql, kind) || keeps_unasked(sql)).collect();
    if writes.is_empty() {
        return Ok(Vec::new());
    }
    let digest = target.digest(statements);
    if confirmed == Some(&digest) {
        return Ok(Vec::new());
    }
    Err(target.ask(count(&writes), writes.iter().take(SHOWN).map(|sql| shown(sql)).collect(), digest))
}

pub fn check_commit(target: &Target, confirmed: Option<&WriteDigest>) -> Result<(), WriteConfirmation> {
    if !target.on_production() || target.unasked.writes == 0 {
        return Ok(());
    }
    let digest = target.digest(&["COMMIT"]);
    if confirmed == Some(&digest) {
        return Ok(());
    }
    Err(target.ask(target.unasked.writes, target.unasked.shown.clone(), digest))
}

fn count(writes: &[&str]) -> u32 {
    writes.len().try_into().unwrap_or(u32::MAX)
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

    static NONE: UnaskedWrites = UnaskedWrites { writes: 0, shown: Vec::new() };

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
        Target { saved: connection, opened: connection, console, txn: TxnState::Idle, unasked: &NONE }
    }

    const WORK: &[&str] = &["select 1", " insert into t values (1) ", "delete from t"];

    #[test]
    fn only_a_write_on_production_asks() {
        let production = connection(Some(Environment::Production));
        let auto = console(TxnMode::AutoCommit);
        assert!(check(&on(&production, &auto), &["select 1", "begin", "commit"], None).is_ok());
        for environment in [None, Some(Environment::Local), Some(Environment::Staging)] {
            assert_eq!(check(&on(&connection(environment), &auto), WORK, None).unwrap(), &WORK[1..], "{environment:?}");
        }
        let asked = check(&on(&production, &auto), WORK, None).unwrap_err();
        assert_eq!((asked.connection.as_str(), asked.database.as_str(), asked.unit, asked.writes), ("prod", "shop", ExecutionUnitKind::Statement, 2));
        assert_eq!(asked.statements, ["insert into t values (1)", "delete from t"]);
        assert_eq!(check(&on(&production, &auto), WORK, Some(&asked.digest)).unwrap(), Vec::<&str>::new(), "a confirmed write is not unasked");
    }

    #[test]
    fn a_session_opened_on_production_asks_until_it_closes() {
        let auto = console(TxnMode::AutoCommit);
        let production = connection(Some(Environment::Production));
        let local = connection(Some(Environment::Local));
        let session = |saved, opened| Target { saved, opened, console: &auto, txn: TxnState::Idle, unasked: &NONE };
        assert!(check(&session(&local, &production), WORK, None).is_err(), "relabelled on the same server");
        let moved = ConnectionConfig { host: "elsewhere".into(), ..local.clone() };
        assert!(check(&session(&moved, &production), WORK, None).is_err(), "relabelled and moved");
        assert!(check(&session(&production, &local), WORK, None).is_err(), "marked Production after the session opened");
        assert!(check(&session(&local, &moved), WORK, None).is_ok());
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
        let reopened = Target { opened: &other_host, ..on(&production, &auto) };
        assert!(check(&reopened, WORK, Some(&digest)).is_err(), "the session reached another server");
        let open = Target { txn: TxnState::Active, ..on(&production, &auto) };
        assert!(check(&open, WORK, Some(&digest)).is_err(), "the transaction state changed");
    }

    #[test]
    fn a_commit_on_production_asks_for_the_writes_that_ran_unasked() {
        let auto = console(TxnMode::Manual);
        let production = connection(Some(Environment::Production));
        let staging = connection(Some(Environment::Staging));
        let mut unasked = UnaskedWrites::default();
        unasked.add(&check(&on(&staging, &auto), WORK, None).unwrap());
        let open = |saved, unasked| Target { saved, opened: &staging, console: &auto, txn: TxnState::Active, unasked };

        assert!(check_commit(&open(&staging, &unasked), None).is_ok(), "off Production a commit never asks");
        assert!(check_commit(&open(&production, &NONE), None).is_ok(), "nothing ran unasked");
        let asked = check_commit(&open(&production, &unasked), None).unwrap_err();
        assert_eq!((asked.writes, asked.statements.as_slice()), (2, ["insert into t values (1)".to_owned(), "delete from t".to_owned()].as_slice()));
        assert!(check_commit(&open(&production, &unasked), Some(&asked.digest)).is_ok());

        let typed = check(&open(&production, &unasked), &["select 1", "commit"], None).unwrap_err();
        assert_eq!((typed.writes, typed.statements.as_slice()), (1, ["commit".to_owned()].as_slice()), "a typed COMMIT keeps them too");
        assert!(check(&open(&production, &NONE), &["select 1", "commit"], None).is_ok());
    }

    #[test]
    fn unasked_writes_count_every_write_and_keep_the_first_three() {
        let mut unasked = UnaskedWrites::default();
        unasked.add(&["insert into t values (1)", "delete from t"]);
        unasked.add(&["update t set n = 2", "truncate t"]);
        assert_eq!((unasked.writes, unasked.shown.as_slice()), (4, ["insert into t values (1)".to_owned(), "delete from t".to_owned(), "update t set n = 2".to_owned()].as_slice()));
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
    fn a_sql_server_write_is_a_batch() {
        let production = ConnectionConfig { database_kind: DatabaseKind::SqlServer, ..connection(Some(Environment::Production)) };
        let asked = check(&on(&production, &console(TxnMode::AutoCommit)), &["SELECT 1", "EXEC dbo.purge"], None).unwrap_err();
        assert_eq!((asked.unit, asked.statements.as_slice()), (ExecutionUnitKind::Batch, ["EXEC dbo.purge".to_owned()].as_slice()));
    }

    const CAN_WRITE: [&str; 3] = ["Work::Run", "Work::Edit", "Work::Transaction"];

    fn code_without_comments_or_literals(source: &str) -> String {
        let c: Vec<char> = source.chars().collect();
        let at = |i: usize, s: &str| s.chars().enumerate().all(|(k, ch)| c.get(i + k) == Some(&ch));
        let ident = |i: usize| i > 0 && (c[i - 1].is_alphanumeric() || c[i - 1] == '_');
        let mut out = String::with_capacity(source.len());
        let mut i = 0;
        while i < c.len() {
            if at(i, "//") {
                while i < c.len() && c[i] != '\n' {
                    i += 1;
                }
            } else if at(i, "/*") {
                let mut depth = 0;
                while i < c.len() {
                    if at(i, "/*") {
                        depth += 1;
                        i += 2;
                    } else if at(i, "*/") {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
                out.push(' ');
            } else if c[i] == 'r' && (!ident(i) || (c[i - 1] == 'b' && !ident(i - 1))) && c[i + 1..].iter().position(|&ch| ch != '#').is_some_and(|n| c[i + 1 + n] == '"') {
                let hashes = c[i + 1..].iter().take_while(|&&ch| ch == '#').count();
                let close: String = std::iter::once('"').chain(std::iter::repeat_n('#', hashes)).collect();
                i += hashes + 2;
                while !at(i, &close) {
                    i += 1;
                }
                i += close.len();
                out.push_str("\"\"");
            } else if c[i] == '"' {
                i += 1;
                while c[i] != '"' {
                    i += if c[i] == '\\' { 2 } else { 1 };
                }
                i += 1;
                out.push_str("\"\"");
            } else if c[i] == '\'' && c.get(i + 1) == Some(&'\\') {
                i += 3;
                while c[i] != '\'' {
                    i += 1;
                }
                i += 1;
                out.push_str("' '");
            } else if c[i] == '\'' && c.get(i + 2) == Some(&'\'') {
                i += 3;
                out.push_str("' '");
            } else {
                out.push(c[i]);
                i += 1;
            }
        }
        out
    }

    fn call_text(call: &str) -> &str {
        let mut depth = 0;
        for (i, ch) in call.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' if depth == 1 => return &call[..=i],
                ')' => depth -= 1,
                _ => {}
            }
        }
        panic!("unclosed call: {call}")
    }

    fn uses_check_result(function: &str) -> bool {
        function.match_indices("writes::check").any(|(at, _)| {
            let before = function[..at].trim_end();
            before.ends_with("match") || before.strip_suffix('=').map(str::trim_end).is_some_and(|binding| {
                binding.strip_suffix(')').and_then(|b| b.rsplit_once("Err(")).is_some_and(|(_, name)| !name.is_empty() && name.chars().all(|ch| ch.is_alphanumeric() || ch == '_'))
            })
        })
    }

    fn handovers_in(root: &std::path::Path) -> Vec<(String, bool)> {
        let mut found = Vec::new();
        let mut dirs = vec![root.to_path_buf()];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    found.extend(handovers(&path.strip_prefix(root).unwrap().display().to_string(), &std::fs::read_to_string(&path).unwrap()));
                }
            }
        }
        found.sort();
        found
    }

    fn handovers(file: &str, source: &str) -> Vec<(String, bool)> {
        let code = code_without_comments_or_literals(source);
        let mut found = Vec::new();
        for (at, _) in code.match_indices("take_session(") {
            let args: String = call_text(&code[at..]).split_whitespace().collect();
            let args = &args["take_session(".len()..args.len() - 1];
            if !args.split(',').any(|arg| CAN_WRITE.iter().any(|work| arg == *work || arg.ends_with(&format!("::{work}")))) {
                continue;
            }
            let start = code[..at].match_indices("fn ").filter(|(i, _)| *i == 0 || code[..*i].ends_with(char::is_whitespace)).last().expect("inside a function").0;
            let function = &code[start..at];
            let name: String = function["fn ".len()..].chars().take_while(|ch| ch.is_alphanumeric() || *ch == '_').collect();
            found.push((format!("{file} {name}"), uses_check_result(function)));
        }
        found
    }

    #[test]
    fn every_session_handover_for_work_that_can_write_follows_a_used_check() {
        let found = handovers_in(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
        let expected: Vec<(String, bool)> = ["lib.rs console_run", "lib.rs console_txn_command", "lib.rs table_apply"].iter().map(|h| (h.to_string(), true)).collect();
        assert_eq!(found, expected, "a handover of a tab's session for work that can write needs a used writes::check before it");
    }

    #[test]
    fn the_handover_scan_sees_through_comments_strings_and_line_breaks() {
        let run = |body: &str| handovers("x.rs", &format!("    async fn console_run() {{\n{body}\n        let Some(s) = console.take_session(&mut inner, id, Work::Run) else {{ return }};\n    }}\n"));
        assert_eq!(run("        if let Err(confirmation) = writes::check(&t, &sent, None) { return; }"), [("x.rs console_run".into(), true)]);
        assert_eq!(run("        match writes::check(&t, &sent, None) { Ok(_) => {} Err(_) => return }"), [("x.rs console_run".into(), true)]);
        assert_eq!(run("        // if let Err(confirmation) = writes::check(&t, &sent, None) { return; }"), [("x.rs console_run".into(), false)]);
        assert_eq!(run("        /* if let Err(c) = writes::check(&t, &sent, None) { return; } */"), [("x.rs console_run".into(), false)]);
        assert_eq!(run("        let s = \"if let Err(c) = writes::check(\"; let q = '\"';"), [("x.rs console_run".into(), false)]);
        assert_eq!(run("        let _ = writes::check(&t, &sent, None);"), [("x.rs console_run".into(), false)]);
        assert_eq!(run("        writes::check(&t, &sent, None).ok();"), [("x.rs console_run".into(), false)]);

        let split = "    fn run_elsewhere() {\n        console.take_session(\n            &mut inner,\n            run_id,\n            Work::Run,\n        );\n    }\n";
        assert_eq!(handovers("console.rs", split), [("console.rs run_elsewhere".into(), false)]);
        let qualified = "    fn apply() {\n        console.take_session(&mut inner, run_id, console::Work::Edit);\n    }\n";
        assert_eq!(handovers("console.rs", qualified), [("console.rs apply".into(), false)]);
        let reads = "    fn count() {\n        console.take_session(&mut inner, run_id, Work::Count);\n    }\n";
        assert_eq!(handovers("console.rs", reads), []);
    }
}
