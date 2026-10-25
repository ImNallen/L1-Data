use super::{Ddl, Grant, privileges};
use crate::text::{quote_ident_if_needed as ident, quote_literal};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Table {
    pub schema: String,
    pub name: String,
    pub unlogged: bool,
    pub foreign: Option<Foreign>,
    pub access_method: Option<String>,
    pub options: Vec<String>,
    pub tablespace: Option<String>,
    pub comment: Option<String>,
    pub columns: Vec<Column>,
    pub owned_sequences: Vec<Sequence>,
    pub constraints: Vec<Constraint>,
    pub parent: Parent,
    pub partition_key: Option<String>,
    pub partitions: Vec<Table>,
    pub indexes: Vec<Index>,
    pub replica_identity: ReplicaIdentity,
    pub triggers: Vec<Trigger>,
    pub rules: Vec<Rule>,
    pub row_security: bool,
    pub force_row_security: bool,
    pub policies: Vec<Policy>,
    pub statistics: Vec<Statistics>,
    pub unscripted: Unscripted,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Unscripted {
    pub privileges: Vec<Grant>,
    /// The privileges on each sequence the script creates for the table, by schema and name.
    pub sequence_privileges: Vec<((String, String), Vec<Grant>)>,
    pub security_labels: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Foreign {
    pub server: String,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Parent {
    #[default]
    None,
    Partition {
        schema: String,
        name: String,
        bound: String,
    },
    Inherits(Vec<(String, String)>),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReplicaIdentity {
    #[default]
    Default,
    Nothing,
    Full,
    Index,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub type_name: String,
    pub collation: Option<(String, String)>,
    pub not_null: bool,
    pub default: Option<String>,
    pub generated: Option<Generated>,
    pub identity: Option<Identity>,
    pub comment: Option<String>,
    pub declared: bool,
    pub inherited: Option<Inherited>,
    pub statistics: Option<i32>,
    pub storage: String,
    pub type_storage: String,
    pub compression: String,
    pub options: Vec<String>,
    pub fdw_options: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inherited {
    pub replay_order: i64,
    pub default: Option<String>,
    pub not_null: bool,
    pub storage: String,
    pub compression: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Generated {
    Stored,
    Virtual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub always: bool,
    pub sequence: Sequence,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sequence {
    pub schema: String,
    pub name: String,
    pub column: String,
    pub type_name: String,
    pub start: i64,
    pub increment: i64,
    pub min: i64,
    pub max: i64,
    pub cache: i64,
    pub cycle: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Constraint {
    pub name: String,
    pub definition: String,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Index {
    pub name: String,
    pub definition: Option<String>,
    pub comment: Option<String>,
    pub clustered: bool,
    pub replica_identity: bool,
    pub tablespace: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trigger {
    pub name: String,
    pub definition: String,
    pub enabled: char,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rule {
    pub name: String,
    pub definition: String,
    pub enabled: char,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Policy {
    pub name: String,
    pub permissive: bool,
    pub command: char,
    pub roles: Vec<String>,
    pub using: Option<String>,
    pub check: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Statistics {
    pub schema: String,
    pub name: String,
    pub definition: String,
    pub target: Option<i32>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct View {
    pub schema: String,
    pub name: String,
    pub materialized: bool,
    pub populated: bool,
    pub access_method: Option<String>,
    pub options: Vec<String>,
    pub tablespace: Option<String>,
    /// `pg_get_viewdef`, which ends in `;`.
    pub query: String,
    pub comment: Option<String>,
    pub columns: Vec<Column>,
    pub indexes: Vec<Index>,
    pub triggers: Vec<Trigger>,
    pub rules: Vec<Rule>,
    pub unscripted: Unscripted,
}

pub fn qualified(schema: &str, name: &str) -> String {
    format!("{}.{}", ident(schema), ident(name))
}

pub fn table(t: &Table) -> Ddl {
    let mut statements = vec![];
    let mut warnings = vec![];
    table_statements(t, &mut statements, &mut warnings);
    Ddl { text: script(statements), warnings }
}

fn table_statements(t: &Table, out: &mut Vec<String>, warnings: &mut Vec<String>) {
    let name = qualified(&t.schema, &t.name);
    let keyword = if t.foreign.is_some() { "FOREIGN TABLE" } else { "TABLE" };
    let (deferred, inline): (Vec<_>, Vec<_>) = t.constraints.iter().partition(|k| k.definition.ends_with(" NOT VALID"));
    let constraints = inline.iter().map(|k| format!("CONSTRAINT {} {}", ident(&k.name), k.definition));
    out.extend(t.owned_sequences.iter().map(create_sequence));

    let mut head = format!("CREATE {}{keyword} {name}", if t.unlogged { "UNLOGGED " } else { "" });
    match &t.parent {
        Parent::Partition { schema, name: parent, bound } => {
            head.push_str(&format!(" PARTITION OF {}", qualified(schema, parent)));
            let body: Vec<_> = constraints.collect();
            if !body.is_empty() {
                head.push_str(&format!(" (\n    {}\n)", body.join(",\n    ")));
            }
            head.push_str(&format!("\n    {bound}"));
        }
        parent => {
            let columns = t.columns.iter().filter(|c| c.declared).map(|c| if c.inherited.is_some() { column_head(c) } else { column(c, &t.schema, &t.name) });
            let body: Vec<_> = columns.chain(constraints).collect();
            head.push_str(&format!(" (\n    {}\n)", body.join(",\n    ")));
            if let Parent::Inherits(parents) = parent {
                head.push_str(&format!(" INHERITS ({})", parents.iter().map(|(s, n)| qualified(s, n)).collect::<Vec<_>>().join(", ")));
            }
        }
    }
    if let Some(key) = &t.partition_key {
        head.push_str(&format!(" PARTITION BY {key}"));
    }
    if let Some(method) = &t.access_method {
        head.push_str(&format!(" USING {}", ident(method)));
    }
    if let Some(foreign) = &t.foreign {
        head.push_str(&format!(" SERVER {}{}", ident(&foreign.server), fdw_options("", &foreign.options)));
    }
    if !t.options.is_empty() {
        head.push_str(&format!(" WITH ({})", t.options.join(", ")));
    }
    if let Some(space) = &t.tablespace {
        head.push_str(&format!(" TABLESPACE {}", ident(space)));
    }
    out.push(head);

    out.extend(t.owned_sequences.iter().map(|s| format!("ALTER SEQUENCE {} OWNED BY {name}.{}", qualified(&s.schema, &s.name), ident(&s.column))));
    out.extend(deferred.iter().map(|k| format!("ALTER TABLE {name} ADD CONSTRAINT {} {}", ident(&k.name), k.definition)));
    for c in &t.columns {
        out.extend(column_changes(c, keyword, &name, !matches!(t.parent, Parent::Partition { .. }) && c.declared && c.inherited.is_none()));
    }
    indexes(&t.indexes, keyword, &t.schema, &name, out);
    match t.replica_identity {
        _ if t.foreign.is_some() => {}
        ReplicaIdentity::Nothing => out.push(format!("ALTER TABLE {name} REPLICA IDENTITY NOTHING")),
        ReplicaIdentity::Full => out.push(format!("ALTER TABLE {name} REPLICA IDENTITY FULL")),
        ReplicaIdentity::Default | ReplicaIdentity::Index => {}
    }
    for tr in &t.triggers {
        out.extend(trigger(tr, keyword, &name));
    }
    for r in &t.rules {
        out.extend(rule(r, keyword, &name));
    }
    if t.row_security {
        out.push(format!("ALTER TABLE {name} ENABLE ROW LEVEL SECURITY"));
    }
    if t.force_row_security {
        out.push(format!("ALTER TABLE {name} FORCE ROW LEVEL SECURITY"));
    }
    for p in &t.policies {
        out.push(policy(p, &name));
        out.extend(comment(&format!("POLICY {} ON {name}", ident(&p.name)), &p.comment));
    }
    for s in &t.statistics {
        let stat = qualified(&s.schema, &s.name);
        out.push(s.definition.clone());
        if let Some(target) = s.target {
            out.push(format!("ALTER STATISTICS {stat} SET STATISTICS {target}"));
        }
        out.extend(comment(&format!("STATISTICS {stat}"), &s.comment));
    }
    out.extend(comment(&format!("{keyword} {name}"), &t.comment));
    for k in &t.constraints {
        out.extend(comment(&format!("CONSTRAINT {} ON {name}", ident(&k.name)), &k.comment));
    }
    for c in &t.columns {
        out.extend(comment(&format!("COLUMN {name}.{}", ident(&c.name)), &c.comment));
    }
    warnings.extend(unscripted(&t.unscripted, &name));
    if !in_replay_order(&t.columns) {
        warnings.push(format!("the column order of {name}, because a replay puts inherited columns first"));
    }
    for p in &t.partitions {
        table_statements(p, out, warnings);
    }
}

fn in_replay_order(columns: &[Column]) -> bool {
    let positions: Vec<i64> = columns.iter().map(|c| c.inherited.as_ref().map_or(i64::MAX, |p| p.replay_order)).collect();
    positions.windows(2).all(|w| w[0] <= w[1])
}

fn unscripted(u: &Unscripted, name: &str) -> Vec<String> {
    let mut out: Vec<String> = privileges(name, &u.privileges).into_iter().collect();
    out.extend(u.sequence_privileges.iter().filter_map(|((schema, sequence), grants)| privileges(&qualified(schema, sequence), grants)));
    if u.security_labels {
        out.push(format!("security labels on {name}"));
    }
    out
}

fn column(c: &Column, schema: &str, table: &str) -> String {
    let mut out = column_head(c);
    if let Some(identity) = &c.identity {
        out.push_str(&format!(" {}", identity_clause(identity, schema, table)));
    }
    match (c.generated, &c.default) {
        (Some(kind), Some(expr)) => out.push_str(&format!(" GENERATED ALWAYS AS ({expr}) {}", if kind == Generated::Stored { "STORED" } else { "VIRTUAL" })),
        (None, Some(expr)) => out.push_str(&format!(" DEFAULT {expr}")),
        _ => {}
    }
    if c.not_null {
        out.push_str(" NOT NULL");
    }
    out
}

fn column_head(c: &Column) -> String {
    let mut out = format!("{} {}", ident(&c.name), c.type_name);
    if let Some((schema, name)) = &c.collation {
        out.push_str(&format!(" COLLATE {}", if schema == "pg_catalog" { ident(name) } else { qualified(schema, name) }));
    }
    out
}

fn column_changes(c: &Column, keyword: &str, relation: &str, declared: bool) -> Vec<String> {
    let (default, not_null, storage, compression) = match &c.inherited {
        _ if declared => (c.default.as_ref(), c.not_null, &c.type_storage, ""),
        Some(p) => (p.default.as_ref(), p.not_null, &p.storage, p.compression.as_str()),
        None => (None, false, &c.type_storage, ""),
    };
    let mut changes = vec![];
    if c.generated.is_none() && c.identity.is_none() && c.default.as_ref() != default {
        changes.push(c.default.as_ref().map_or("DROP DEFAULT".into(), |d| format!("SET DEFAULT {d}")));
    }
    if c.not_null != not_null {
        changes.push(if c.not_null { "SET NOT NULL" } else { "DROP NOT NULL" }.into());
    }
    if let Some(target) = c.statistics {
        changes.push(format!("SET STATISTICS {target}"));
    }
    if &c.storage != storage {
        let kind = match c.storage.as_str() {
            "p" => "PLAIN",
            "e" => "EXTERNAL",
            "m" => "MAIN",
            _ => "EXTENDED",
        };
        changes.push(format!("SET STORAGE {kind}"));
    }
    if c.compression != compression {
        changes.push(format!("SET COMPRESSION {}", match c.compression.as_str() {
            "p" => "pglz",
            "l" => "lz4",
            _ => "default",
        }));
    }
    if !c.options.is_empty() {
        changes.push(format!("SET ({})", c.options.join(", ")));
    }
    if !c.fdw_options.is_empty() {
        changes.push(fdw_options("ADD ", &c.fdw_options).trim_start().to_owned());
    }
    changes.into_iter().map(|change| format!("ALTER {keyword} {relation} ALTER COLUMN {} {change}", ident(&c.name))).collect()
}

fn fdw_options(lead: &str, options: &[String]) -> String {
    if options.is_empty() {
        return String::new();
    }
    let pairs: Vec<String> = options
        .iter()
        .map(|o| {
            let (k, v) = o.split_once('=').unwrap_or((o, ""));
            format!("{lead}{} {}", ident(k), quote_literal(v))
        })
        .collect();
    format!(" OPTIONS ({})", pairs.join(", "))
}

fn sequence_options(s: &Sequence) -> Vec<String> {
    let (type_min, type_max) = match s.type_name.as_str() {
        "smallint" => (i64::from(i16::MIN), i64::from(i16::MAX)),
        "integer" => (i64::from(i32::MIN), i64::from(i32::MAX)),
        _ => (i64::MIN, i64::MAX),
    };
    let (min, max) = if s.increment > 0 { (1, type_max) } else { (type_min, -1) };
    let start = if s.increment > 0 { s.min } else { s.max };
    let mut options = vec![];
    if s.start != start {
        options.push(format!("START WITH {}", s.start));
    }
    if s.increment != 1 {
        options.push(format!("INCREMENT BY {}", s.increment));
    }
    if s.min != min {
        options.push(format!("MINVALUE {}", s.min));
    }
    if s.max != max {
        options.push(format!("MAXVALUE {}", s.max));
    }
    if s.cache != 1 {
        options.push(format!("CACHE {}", s.cache));
    }
    if s.cycle {
        options.push("CYCLE".into());
    }
    options
}

fn create_sequence(s: &Sequence) -> String {
    let options = sequence_options(s);
    let mut out = format!("CREATE SEQUENCE {} AS {}", qualified(&s.schema, &s.name), s.type_name);
    if !options.is_empty() {
        out.push_str(&format!(" {}", options.join(" ")));
    }
    out
}

fn identity_clause(i: &Identity, schema: &str, table: &str) -> String {
    let s = &i.sequence;
    let mut options = sequence_options(s);
    if s.schema != schema || s.name != format!("{table}_{}_seq", s.column) {
        options.insert(0, format!("SEQUENCE NAME {}", qualified(&s.schema, &s.name)));
    }
    let head = format!("GENERATED {} AS IDENTITY", if i.always { "ALWAYS" } else { "BY DEFAULT" });
    if options.is_empty() { head } else { format!("{head} ({})", options.join(" ")) }
}

fn indexes(list: &[Index], keyword: &str, schema: &str, relation: &str, out: &mut Vec<String>) {
    for i in list {
        let name = ident(&i.name);
        out.extend(i.definition.as_deref().map(index));
        if let Some(space) = &i.tablespace {
            out.push(format!("ALTER INDEX {} SET TABLESPACE {}", qualified(schema, &i.name), ident(space)));
        }
        if i.clustered {
            out.push(format!("ALTER {keyword} {relation} CLUSTER ON {name}"));
        }
        if i.replica_identity {
            out.push(format!("ALTER TABLE {relation} REPLICA IDENTITY USING INDEX {name}"));
        }
        out.extend(comment(&format!("INDEX {}", qualified(schema, &i.name)), &i.comment));
    }
}

pub fn view(v: &View) -> Ddl {
    let name = qualified(&v.schema, &v.name);
    let keyword = if v.materialized { "MATERIALIZED VIEW" } else { "VIEW" };
    let check = v.options.iter().find_map(|o| o.strip_prefix("check_option=")).map(|c| c.to_uppercase());
    let options: Vec<_> = v.options.iter().filter(|o| !o.starts_with("check_option=")).cloned().collect();
    let mut head = format!("CREATE {keyword} {name}");
    if let Some(method) = &v.access_method {
        head.push_str(&format!(" USING {}", ident(method)));
    }
    if !options.is_empty() {
        head.push_str(&format!(" WITH ({})", options.join(", ")));
    }
    if let Some(space) = &v.tablespace {
        head.push_str(&format!(" TABLESPACE {}", ident(space)));
    }
    head.push_str(&format!(" AS\n{}", v.query.trim_end().trim_end_matches(';')));
    if let Some(check) = check {
        head.push_str(&format!("\n  WITH {check} CHECK OPTION"));
    }
    if v.materialized {
        head.push_str(if v.populated { "\nWITH DATA" } else { "\nWITH NO DATA" });
    }
    let mut statements = vec![head];
    for c in &v.columns {
        statements.extend(column_changes(c, keyword, &name, false));
    }
    indexes(&v.indexes, keyword, &v.schema, &name, &mut statements);
    for tr in &v.triggers {
        statements.extend(trigger(tr, keyword, &name));
    }
    for r in &v.rules {
        statements.extend(rule(r, keyword, &name));
    }
    statements.extend(comment(&format!("{keyword} {name}"), &v.comment));
    for c in &v.columns {
        statements.extend(comment(&format!("COLUMN {name}.{}", ident(&c.name)), &c.comment));
    }
    Ddl { text: script(statements), warnings: unscripted(&v.unscripted, &name) }
}

pub fn routine(definition: &str, procedure: bool, signature: &str, description: &Option<String>, grants: &[Grant]) -> Ddl {
    let mut statements = vec![definition.trim_end().to_owned()];
    statements.extend(comment(&format!("{} {signature}", if procedure { "PROCEDURE" } else { "FUNCTION" }), description));
    Ddl { text: script(statements), warnings: privileges(signature, grants).into_iter().collect() }
}

pub fn index_script(i: &Index, constraint: Option<&Constraint>, keyword: &str, schema: &str, relation: &str) -> Ddl {
    let mut statements: Vec<String> = constraint.map(|k| format!("ALTER {keyword} {relation} ADD CONSTRAINT {} {}", ident(&k.name), k.definition)).into_iter().collect();
    indexes(std::slice::from_ref(i), keyword, schema, relation, &mut statements);
    Ddl::complete(script(statements))
}

/// `pg_get_indexdef` prints `ON ONLY` for a partitioned table's index, which would create the
/// index without its partitions' copies.
pub fn index(definition: &str) -> String {
    match top_level(definition, " ON ONLY ") {
        Some(at) => format!("{} ON {}", &definition[..at], &definition[at + " ON ONLY ".len()..]),
        None => definition.to_owned(),
    }
}

pub fn trigger(t: &Trigger, keyword: &str, table: &str) -> Vec<String> {
    let state = match t.enabled {
        'D' => Some("DISABLE"),
        'R' => Some("ENABLE REPLICA"),
        'A' => Some("ENABLE ALWAYS"),
        _ => None,
    };
    std::iter::once(t.definition.clone())
        .chain(state.map(|s| format!("ALTER {} {table} {s} TRIGGER {}", if keyword == "FOREIGN TABLE" { keyword } else { "TABLE" }, ident(&t.name))))
        .chain(comment(&format!("TRIGGER {} ON {table}", ident(&t.name)), &t.comment))
        .collect()
}

fn rule(r: &Rule, keyword: &str, relation: &str) -> Vec<String> {
    let state = match r.enabled {
        'D' => Some("DISABLE"),
        'R' => Some("ENABLE REPLICA"),
        'A' => Some("ENABLE ALWAYS"),
        _ => None,
    };
    let keyword = if keyword == "VIEW" { "VIEW" } else { "TABLE" };
    std::iter::once(r.definition.trim_end().trim_end_matches(';').to_owned())
        .chain(state.map(|s| format!("ALTER {keyword} {relation} {s} RULE {}", ident(&r.name))))
        .chain(comment(&format!("RULE {} ON {relation}", ident(&r.name)), &r.comment))
        .collect()
}

fn policy(p: &Policy, table: &str) -> String {
    let mut out = format!("CREATE POLICY {} ON {table}", ident(&p.name));
    if !p.permissive {
        out.push_str(" AS RESTRICTIVE");
    }
    let command = match p.command {
        'r' => Some("SELECT"),
        'a' => Some("INSERT"),
        'w' => Some("UPDATE"),
        'd' => Some("DELETE"),
        _ => None,
    };
    if let Some(command) = command {
        out.push_str(&format!(" FOR {command}"));
    }
    if !p.roles.is_empty() {
        out.push_str(&format!(" TO {}", p.roles.iter().map(|r| ident(r)).collect::<Vec<_>>().join(", ")));
    }
    if let Some(using) = &p.using {
        out.push_str(&format!(" USING ({using})"));
    }
    if let Some(check) = &p.check {
        out.push_str(&format!(" WITH CHECK ({check})"));
    }
    out
}

fn comment(target: &str, text: &Option<String>) -> Option<String> {
    text.as_ref().map(|t| format!("COMMENT ON {target} IS {}", quote_literal(t)))
}

pub fn script(statements: Vec<String>) -> String {
    statements.iter().map(|s| format!("{s};\n")).collect::<Vec<_>>().join("\n")
}

/// Byte offset of the first `needle` outside quotes and parentheses.
pub fn top_level(s: &str, needle: &str) -> Option<usize> {
    let mut walk = Walk::default();
    s.char_indices().find_map(|(i, c)| (walk.step(c) == 0 && !walk.quoted() && s[i..].starts_with(needle)).then_some(i))
}

/// Byte offset just past the `)` that closes the `(` at `open`.
pub fn group_end(s: &str, open: usize) -> Option<usize> {
    let mut walk = Walk::default();
    for (i, c) in s[open..].char_indices() {
        let closes = c == ')' && !walk.quoted();
        if walk.step(c) == 0 && closes {
            return Some(open + i + 1);
        }
    }
    None
}

/// Parenthesis depth outside single- and double-quoted text. A doubled quote inside a quoted
/// run closes and reopens it, which leaves the state right.
#[derive(Default)]
struct Walk {
    depth: u32,
    quote: Option<char>,
}

impl Walk {
    fn quoted(&self) -> bool {
        self.quote.is_some()
    }

    fn step(&mut self, c: char) -> u32 {
        match (self.quote, c) {
            (Some(q), c) if c == q => self.quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => self.quote = Some(c),
            (None, '(') => self.depth += 1,
            (None, ')') => self.depth = self.depth.saturating_sub(1),
            _ => {}
        }
        self.depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::Change;

    fn grant(privilege: &str, column: Option<&str>, grantee: &str, change: Change) -> Grant {
        Grant { privilege: privilege.into(), column: column.map(Into::into), grantee: grantee.into(), change }
    }

    fn sequence(start: i64, increment: i64, min: i64, max: i64) -> Sequence {
        Sequence { schema: "public".into(), name: "t_id_seq".into(), column: "id".into(), type_name: "bigint".into(), start, increment, min, max, cache: 1, cycle: false }
    }

    fn col(name: &str, type_name: &str) -> Column {
        Column { name: name.into(), type_name: type_name.into(), declared: true, storage: "x".into(), type_storage: "x".into(), ..Default::default() }
    }

    #[test]
    fn a_table_carries_its_columns_constraints_indexes_triggers_rules_policies_and_comments() {
        let t = Table {
            schema: "public".into(),
            name: "t".into(),
            comment: Some("it's".into()),
            columns: vec![
                Column { not_null: true, identity: Some(Identity { always: false, sequence: sequence(1000, 10, 1, i64::MAX) }), ..col("id", "bigint") },
                Column { collation: Some(("pg_catalog".into(), "C".into())), not_null: true, storage: "e".into(), statistics: Some(500), ..col("code", "text") },
                Column { default: Some("(price * qty)".into()), generated: Some(Generated::Stored), ..col("Total", "numeric(12,2)") },
                Column { default: Some("'new'::text".into()), comment: Some("lifecycle".into()), compression: "l".into(), options: vec!["n_distinct=5".into()], ..col("status", "text") },
                Column { not_null: true, ..col("seq", "bigserial") },
            ],
            owned_sequences: vec![Sequence { name: "old_seq".into(), start: 777, ..sequence(1, 1, 1, i64::MAX) }],
            constraints: vec![
                Constraint { name: "t_pkey".into(), definition: "PRIMARY KEY (id)".into(), comment: None },
                Constraint { name: "positive".into(), definition: "CHECK ((id > 0)) NOT VALID".into(), comment: Some("checked later".into()) },
            ],
            indexes: vec![
                Index { name: "t_pkey".into(), comment: Some("pk".into()), replica_identity: true, ..Default::default() },
                Index { name: "t_code".into(), definition: Some("CREATE INDEX t_code ON public.t USING btree (code)".into()), clustered: true, ..Default::default() },
            ],
            replica_identity: ReplicaIdentity::Index,
            triggers: vec![Trigger { name: "audit".into(), definition: "CREATE TRIGGER audit AFTER INSERT ON public.t FOR EACH ROW EXECUTE FUNCTION public.audit()".into(), enabled: 'D', comment: Some("tr".into()) }],
            rules: vec![Rule { name: "r".into(), definition: "CREATE RULE r AS\n    ON UPDATE TO public.t DO INSTEAD NOTHING;".into(), enabled: 'D', comment: None }],
            row_security: true,
            force_row_security: true,
            policies: vec![
                Policy { name: "own".into(), permissive: false, command: 'w', roles: vec!["Admins".into(), "app".into()], using: Some("(id > 0)".into()), check: Some("true".into()), comment: Some("p".into()) },
                Policy { name: "all".into(), permissive: true, command: '*', ..Default::default() },
            ],
            statistics: vec![Statistics { schema: "public".into(), name: "s".into(), definition: "CREATE STATISTICS public.s ON id, code FROM public.t".into(), target: Some(50), comment: None }],
            ..Default::default()
        };
        assert_eq!(
            table(&t),
            Ddl::complete(
                "CREATE SEQUENCE public.old_seq AS bigint START WITH 777;

CREATE TABLE public.t (
    id bigint GENERATED BY DEFAULT AS IDENTITY (START WITH 1000 INCREMENT BY 10) NOT NULL,
    code text COLLATE \"C\" NOT NULL,
    \"Total\" numeric(12,2) GENERATED ALWAYS AS ((price * qty)) STORED,
    status text DEFAULT 'new'::text,
    seq bigserial NOT NULL,
    CONSTRAINT t_pkey PRIMARY KEY (id)
);

ALTER SEQUENCE public.old_seq OWNED BY public.t.id;

ALTER TABLE public.t ADD CONSTRAINT positive CHECK ((id > 0)) NOT VALID;

ALTER TABLE public.t ALTER COLUMN code SET STATISTICS 500;

ALTER TABLE public.t ALTER COLUMN code SET STORAGE EXTERNAL;

ALTER TABLE public.t ALTER COLUMN status SET COMPRESSION lz4;

ALTER TABLE public.t ALTER COLUMN status SET (n_distinct=5);

ALTER TABLE public.t REPLICA IDENTITY USING INDEX t_pkey;

COMMENT ON INDEX public.t_pkey IS 'pk';

CREATE INDEX t_code ON public.t USING btree (code);

ALTER TABLE public.t CLUSTER ON t_code;

CREATE TRIGGER audit AFTER INSERT ON public.t FOR EACH ROW EXECUTE FUNCTION public.audit();

ALTER TABLE public.t DISABLE TRIGGER audit;

COMMENT ON TRIGGER audit ON public.t IS 'tr';

CREATE RULE r AS
    ON UPDATE TO public.t DO INSTEAD NOTHING;

ALTER TABLE public.t DISABLE RULE r;

ALTER TABLE public.t ENABLE ROW LEVEL SECURITY;

ALTER TABLE public.t FORCE ROW LEVEL SECURITY;

CREATE POLICY own ON public.t AS RESTRICTIVE FOR UPDATE TO \"Admins\", app USING ((id > 0)) WITH CHECK (true);

COMMENT ON POLICY own ON public.t IS 'p';

CREATE POLICY \"all\" ON public.t;

CREATE STATISTICS public.s ON id, code FROM public.t;

ALTER STATISTICS public.s SET STATISTICS 50;

COMMENT ON TABLE public.t IS 'it''s';

COMMENT ON CONSTRAINT positive ON public.t IS 'checked later';

COMMENT ON COLUMN public.t.status IS 'lifecycle';
"
                .into()
            )
        );
    }

    #[test]
    fn a_partition_tree_scripts_each_partition_after_its_parent_with_what_differs_from_it() {
        let inherited = |default: Option<&str>| Some(Inherited { replay_order: 1, default: default.map(Into::into), not_null: false, storage: "p".into(), compression: String::new() });
        let partition = |name: &str, columns: Vec<Column>| Table {
            schema: "public".into(),
            name: name.into(),
            columns,
            parent: Parent::Partition { schema: "public".into(), name: "e".into(), bound: "FOR VALUES IN (1)".into() },
            ..Default::default()
        };
        let v = Column { declared: false, storage: "p".into(), type_storage: "p".into(), ..col("v", "integer") };
        let leaf = Table {
            parent: Parent::Partition { schema: "public".into(), name: "e1".into(), bound: "DEFAULT".into() },
            ..partition("e1_rest", vec![Column { default: Some("7".into()), inherited: inherited(Some("7")), ..v.clone() }])
        };
        let t = Table {
            schema: "public".into(),
            name: "e".into(),
            columns: vec![Column { default: Some("7".into()), storage: "p".into(), type_storage: "p".into(), ..col("v", "integer") }],
            partition_key: Some("LIST (v)".into()),
            partitions: vec![
                Table {
                    unlogged: true,
                    options: vec!["fillfactor=70".into()],
                    constraints: vec![Constraint { name: "e1_note".into(), definition: "CHECK ((v < 10))".into(), comment: None }],
                    partition_key: Some("RANGE (v)".into()),
                    partitions: vec![leaf],
                    ..partition("e1", vec![Column { default: Some("9".into()), not_null: true, inherited: inherited(Some("7")), ..v.clone() }])
                },
                Table {
                    foreign: Some(Foreign { server: "remote".into(), options: vec!["table_name=a, b".into(), "Odd=it's".into()] }),
                    parent: Parent::Partition { schema: "public".into(), name: "e".into(), bound: "FOR VALUES IN (2)".into() },
                    ..partition("e2", vec![Column { inherited: inherited(Some("7")), fdw_options: vec!["column_name=vv".into()], ..v.clone() }])
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            table(&t).text,
            "CREATE TABLE public.e (
    v integer DEFAULT 7
) PARTITION BY LIST (v);

CREATE UNLOGGED TABLE public.e1 PARTITION OF public.e (
    CONSTRAINT e1_note CHECK ((v < 10))
)
    FOR VALUES IN (1) PARTITION BY RANGE (v) WITH (fillfactor=70);

ALTER TABLE public.e1 ALTER COLUMN v SET DEFAULT 9;

ALTER TABLE public.e1 ALTER COLUMN v SET NOT NULL;

CREATE TABLE public.e1_rest PARTITION OF public.e1
    DEFAULT;

CREATE FOREIGN TABLE public.e2 PARTITION OF public.e
    FOR VALUES IN (2) SERVER remote OPTIONS (table_name 'a, b', \"Odd\" 'it''s');

ALTER FOREIGN TABLE public.e2 ALTER COLUMN v DROP DEFAULT;

ALTER FOREIGN TABLE public.e2 ALTER COLUMN v OPTIONS (ADD column_name 'vv');
"
        );
    }

    #[test]
    fn an_inheritance_child_declares_its_own_columns_and_warns_when_a_replay_reorders_them() {
        let from_base = |position: i64| Some(Inherited { replay_order: position, default: None, not_null: false, storage: "x".into(), compression: String::new() });
        let t = Table {
            schema: "app".into(),
            name: "child".into(),
            columns: vec![
                Column { declared: false, inherited: from_base(1), default: Some("2".into()), ..col("id", "integer") },
                Column { inherited: from_base(2), ..col("note", "text") },
                col("extra", "text"),
                Column { declared: false, inherited: from_base(3), ..col("late", "integer") },
            ],
            parent: Parent::Inherits(vec![("app".into(), "base".into()), ("app".into(), "Other".into())]),
            unscripted: Unscripted {
                privileges: vec![grant("SELECT", None, "app", Change::Granted), grant("UPDATE", Some("\"Mixed\""), "PUBLIC", Change::GrantedWithGrantOption)],
                sequence_privileges: vec![(("app".into(), "child_id_seq".into()), vec![grant("USAGE", None, "app", Change::Granted)])],
                security_labels: true,
            },
            ..Default::default()
        };
        let ddl = table(&t);
        assert_eq!(ddl.text, "CREATE TABLE app.child (\n    note text,\n    extra text\n) INHERITS (app.base, app.\"Other\");\n\nALTER TABLE app.child ALTER COLUMN id SET DEFAULT 2;\n");
        assert_eq!(
            ddl.warnings,
            [
                "privileges on app.child (SELECT granted to app, UPDATE (\"Mixed\") granted with grant option to PUBLIC)",
                "privileges on app.child_id_seq (USAGE granted to app)",
                "security labels on app.child",
                "the column order of app.child, because a replay puts inherited columns first",
            ]
        );
    }

    #[test]
    fn identity_prints_only_the_options_that_differ_from_the_defaults() {
        let clause = |s: Sequence| identity_clause(&Identity { always: false, sequence: s }, "public", "t");
        assert_eq!(clause(sequence(1, 1, 1, i64::MAX)), "GENERATED BY DEFAULT AS IDENTITY");
        assert_eq!(
            identity_clause(&Identity { always: true, sequence: Sequence { type_name: "integer".into(), ..sequence(1, 1, 1, i64::from(i32::MAX)) } }, "public", "t"),
            "GENERATED ALWAYS AS IDENTITY"
        );
        assert_eq!(clause(Sequence { type_name: "smallint".into(), ..sequence(-1, -1, i64::from(i16::MIN), -1) }), "GENERATED BY DEFAULT AS IDENTITY (INCREMENT BY -1)");
        assert_eq!(clause(Sequence { cache: 20, cycle: true, ..sequence(5, 1, 5, 100) }), "GENERATED BY DEFAULT AS IDENTITY (MINVALUE 5 MAXVALUE 100 CACHE 20 CYCLE)");
        assert_eq!(clause(sequence(50, 1, 1, i64::MAX)), "GENERATED BY DEFAULT AS IDENTITY (START WITH 50)");
        assert_eq!(clause(Sequence { name: "custom".into(), ..sequence(1, 1, 1, i64::MAX) }), "GENERATED BY DEFAULT AS IDENTITY (SEQUENCE NAME public.custom)");
    }

    #[test]
    fn views_keep_their_options_check_option_population_column_defaults_and_rules() {
        let v = View {
            schema: "public".into(),
            name: "paid".into(),
            options: vec!["security_barrier=true".into(), "check_option=cascaded".into()],
            query: " SELECT orders.id\n   FROM public.orders;".into(),
            comment: Some("paid only".into()),
            columns: vec![Column { default: Some("42".into()), comment: Some("the id".into()), ..col("id", "integer") }],
            rules: vec![Rule { name: "ins".into(), definition: "CREATE RULE ins AS\n    ON INSERT TO public.paid DO INSTEAD NOTHING;".into(), enabled: 'O', comment: None }],
            unscripted: Unscripted { privileges: vec![grant("SELECT", None, "PUBLIC", Change::Revoked)], ..Default::default() },
            ..Default::default()
        };
        assert_eq!(
            view(&v),
            Ddl {
                text: "CREATE VIEW public.paid WITH (security_barrier=true) AS\n SELECT orders.id\n   FROM public.orders\n  WITH CASCADED CHECK OPTION;\n\nALTER VIEW public.paid ALTER COLUMN id SET DEFAULT 42;\n\nCREATE RULE ins AS\n    ON INSERT TO public.paid DO INSTEAD NOTHING;\n\nCOMMENT ON VIEW public.paid IS 'paid only';\n\nCOMMENT ON COLUMN public.paid.id IS 'the id';\n".into(),
                warnings: vec!["privileges on public.paid (SELECT revoked from PUBLIC)".into()],
            }
        );
        let m = View {
            schema: "public".into(),
            name: "m".into(),
            materialized: true,
            query: " SELECT 1 AS day;".into(),
            columns: vec![Column { statistics: Some(200), ..col("day", "integer") }],
            indexes: vec![Index { name: "m_day".into(), definition: Some("CREATE UNIQUE INDEX m_day ON public.m USING btree (day)".into()), clustered: true, ..Default::default() }],
            ..Default::default()
        };
        assert_eq!(
            view(&m).text,
            "CREATE MATERIALIZED VIEW public.m AS\n SELECT 1 AS day\nWITH NO DATA;\n\nALTER MATERIALIZED VIEW public.m ALTER COLUMN day SET STATISTICS 200;\n\nCREATE UNIQUE INDEX m_day ON public.m USING btree (day);\n\nALTER MATERIALIZED VIEW public.m CLUSTER ON m_day;\n"
        );
    }

    #[test]
    fn a_routine_ends_its_definition_and_comments_on_its_signature() {
        let def = "CREATE OR REPLACE PROCEDURE public.p(a integer)\n LANGUAGE sql\nAS $procedure$ SELECT 1 $procedure$\n";
        assert_eq!(
            routine(def, true, "public.p(a integer)", &Some("x".into()), &[]),
            Ddl::complete("CREATE OR REPLACE PROCEDURE public.p(a integer)\n LANGUAGE sql\nAS $procedure$ SELECT 1 $procedure$;\n\nCOMMENT ON PROCEDURE public.p(a integer) IS 'x';\n".into())
        );
        assert_eq!(routine(def, true, "public.p(integer)", &None, &[grant("EXECUTE", None, "PUBLIC", Change::Revoked)]).warnings, ["privileges on public.p(integer) (EXECUTE revoked from PUBLIC)"]);
    }

    #[test]
    fn an_index_a_constraint_creates_scripts_as_the_constraint() {
        let pkey = Index { name: "orders_pkey".into(), comment: Some("pk".into()), ..Default::default() };
        let constraint = Constraint { name: "orders_pkey".into(), definition: "PRIMARY KEY (id)".into(), comment: None };
        assert_eq!(
            index_script(&pkey, Some(&constraint), "TABLE", "public", "public.orders").text,
            "ALTER TABLE public.orders ADD CONSTRAINT orders_pkey PRIMARY KEY (id);\n\nCOMMENT ON INDEX public.orders_pkey IS 'pk';\n"
        );
        let plain = Index { name: "i".into(), definition: Some("CREATE INDEX i ON ONLY public.t USING btree (a)".into()), tablespace: Some("fast".into()), ..Default::default() };
        assert_eq!(index_script(&plain, None, "TABLE", "public", "public.t").text, "CREATE INDEX i ON public.t USING btree (a);\n\nALTER INDEX public.i SET TABLESPACE fast;\n");
    }

    #[test]
    fn only_a_top_level_on_only_is_dropped() {
        assert_eq!(index(r#"CREATE INDEX "x ON ONLY y" ON ONLY public.t USING btree (a)"#), r#"CREATE INDEX "x ON ONLY y" ON public.t USING btree (a)"#);
        assert_eq!(index("CREATE INDEX i ON public.t USING btree (a)"), "CREATE INDEX i ON public.t USING btree (a)");
    }
}
