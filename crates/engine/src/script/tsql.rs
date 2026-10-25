use super::Ddl;
use crate::sql::tsql::{module_name, quote_ident as ident, quote_literal};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Table {
    pub schema: String,
    pub name: String,
    pub ansi_nulls: bool,
    pub columns: Vec<Column>,
    pub keys: Vec<Key>,
    pub foreign_keys: Vec<ForeignKey>,
    pub checks: Vec<Check>,
    pub indexes: Vec<Index>,
    pub triggers: Vec<Module>,
    pub heap: Option<Storage>,
    pub large_values: Option<Place>,
    pub lock_escalation: String,
    pub period: Option<(String, String)>,
    pub versioning: Option<Versioning>,
    pub history_of: Option<(String, String)>,
    pub properties: Vec<Property>,
    pub classifications: Vec<Classification>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Classification {
    pub column: String,
    pub label: Option<String>,
    pub label_id: Option<String>,
    pub information_type: Option<String>,
    pub information_type_id: Option<String>,
    pub rank: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub type_name: String,
    pub collation: Option<String>,
    pub nullable: bool,
    pub identity: Option<(String, String)>,
    pub not_for_replication: bool,
    pub computed: Option<Computed>,
    pub default: Option<(String, String)>,
    pub sparse: bool,
    pub column_set: bool,
    pub rowguidcol: bool,
    pub masked: Option<String>,
    pub generated: u8,
    pub hidden: bool,
    pub ansi_padding_off: bool,
    pub filestream: bool,
    pub encrypted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Computed {
    pub definition: String,
    pub persisted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub name: String,
    pub primary: bool,
    pub clustered: bool,
    pub columns: Vec<(String, bool)>,
    pub storage: Storage,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub disabled: bool,
    pub untrusted: bool,
    pub not_for_replication: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignKey {
    pub name: String,
    pub columns: Vec<String>,
    pub ref_schema: String,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
    pub on_delete: String,
    pub on_update: String,
    pub state: State,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub name: String,
    /// `sys.check_constraints.definition` carries its own parentheses.
    pub definition: String,
    pub state: State,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Index {
    pub name: String,
    pub kind: u8,
    pub unique: bool,
    pub keys: Vec<(String, bool)>,
    /// SQL Server marks a nonclustered columnstore index's columns as included.
    pub include: Vec<String>,
    pub order: Vec<String>,
    pub filter: Option<String>,
    pub on_view: bool,
    pub storage: Storage,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Storage {
    pub fill_factor: u8,
    pub pad_index: bool,
    pub ignore_dup_key: bool,
    pub no_recompute: bool,
    pub deny_row_locks: bool,
    pub deny_page_locks: bool,
    pub sequential_key: bool,
    pub compression: Option<String>,
    pub xml_compression: bool,
    pub compression_delay: Option<i32>,
    pub disabled: bool,
    pub place: Option<Place>,
    pub incremental: bool,
    pub suppress_dup_key_messages: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    Filegroup(String),
    PartitionScheme(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versioning {
    pub schema: String,
    pub name: String,
    pub retention: Option<(i32, String)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Module {
    pub schema: String,
    pub name: String,
    pub definition: String,
    pub ansi_nulls: bool,
    pub quoted_identifier: bool,
    pub disabled_on: Option<String>,
    pub properties: Vec<Property>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct View {
    pub module: Module,
    pub indexes: Vec<Index>,
    pub triggers: Vec<Module>,
    pub properties: Vec<Property>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    pub name: String,
    pub value: Option<String>,
    pub base_type: Option<String>,
    pub levels: Vec<(String, String)>,
}

/// Indexed views and filtered indexes can be created only under these options.
const INDEX_OPTIONS: &str = "SET ANSI_NULLS, ANSI_PADDING, ANSI_WARNINGS, ARITHABORT, CONCAT_NULL_YIELDS_NULL, QUOTED_IDENTIFIER ON;\nSET NUMERIC_ROUNDABORT OFF;\nGO\n";

pub fn qualified(schema: &str, name: &str) -> String {
    format!("{}.{}", ident(schema), ident(name))
}

pub fn table(t: &Table) -> Ddl {
    let mut warnings = vec![];
    let name = qualified(&t.schema, &t.name);
    if let Some((schema, history_of)) = &t.history_of {
        warnings.push(format!("history table role for {} on {name}", qualified(schema, history_of)));
    }
    if let Some(place) = &t.large_values {
        warnings.push(format!("{} for large values on {name}", place.describe()));
    }
    let mut body: Vec<String> = t.columns.iter().map(|c| column(c, &name, &mut warnings)).collect();
    body.extend(t.keys.iter().map(|k| key(k, &name, &mut warnings)));
    body.extend(t.foreign_keys.iter().map(foreign_key));
    body.extend(t.checks.iter().map(|c| format!("CONSTRAINT {} CHECK {}{}", ident(&c.name), replication(&c.state), c.definition)));
    if let Some((start, end)) = &t.period {
        body.push(format!("PERIOD FOR SYSTEM_TIME ({}, {})", ident(start), ident(end)));
    }
    let mut with = vec![];
    if let Some(heap) = &t.heap {
        storage_warnings(heap, "", &name, &mut warnings);
        with.extend(options(heap));
    }
    if let Some(v) = &t.versioning {
        let retention = v.retention.as_ref().map_or(String::new(), |(n, unit)| format!(", HISTORY_RETENTION_PERIOD = {n} {unit}S"));
        with.push(format!("SYSTEM_VERSIONING = ON (HISTORY_TABLE = {}{retention})", qualified(&v.schema, &v.name)));
    }
    let with = if with.is_empty() { String::new() } else { format!(" WITH ({})", with.join(", ")) };
    let mut parts = vec![format!("{}CREATE TABLE {name} (\n    {}\n){with};\nGO\n", set_options(t.ansi_nulls, true), body.join(",\n    "))];
    let states: Vec<String> = t
        .foreign_keys
        .iter()
        .map(|f| (&f.name, &f.state))
        .chain(t.checks.iter().map(|c| (&c.name, &c.state)))
        .filter_map(|(constraint, state)| constraint_state(&name, constraint, state))
        .collect();
    if !states.is_empty() {
        parts.push(format!("{}\nGO\n", states.join("\n")));
    }
    if t.lock_escalation != "TABLE" {
        parts.push(format!("ALTER TABLE {name} SET (LOCK_ESCALATION = {});\nGO\n", t.lock_escalation));
    }
    if !t.classifications.is_empty() {
        parts.push(format!("{}GO\n", t.classifications.iter().map(|c| classification(&name, c)).collect::<String>()));
    }
    let disabled_keys = t.keys.iter().filter(|k| k.storage.disabled).map(|k| k.name.as_str());
    parts.extend(dependents(&name, disabled_keys, &t.indexes, &t.triggers, &t.properties, &mut warnings));
    Ddl { text: parts.join("\n"), warnings }
}

pub fn view(v: &View) -> Ddl {
    let mut warnings = vec![];
    let name = qualified(&v.module.schema, &v.module.name);
    let mut parts = vec![module_text(&v.module, &mut warnings)];
    parts.extend(dependents(&name, std::iter::empty(), &v.indexes, &v.triggers, &v.properties, &mut warnings));
    Ddl { text: parts.join("\n"), warnings }
}

pub fn index(i: &Index, table: &str, props: &[Property]) -> Option<Ddl> {
    let mut warnings = vec![];
    let text = index_text(i, table, &mut warnings)?;
    Some(standalone(text, &i.name, i.storage.disabled, table, props, warnings))
}

/// A key's index opens as the constraint, because a unique index would replay without it.
pub fn key_script(k: &Key, table: &str, props: &[Property]) -> Ddl {
    let mut warnings = vec![];
    let text = format!("ALTER TABLE {table} ADD {};\nGO\n", key(k, table, &mut warnings));
    standalone(text, &k.name, k.storage.disabled, table, props, warnings)
}

fn standalone(mut text: String, name: &str, disabled: bool, table: &str, props: &[Property], mut warnings: Vec<String>) -> Ddl {
    if let Some(properties) = properties(props, &mut warnings) {
        text.push('\n');
        text.push_str(&properties);
    }
    if disabled {
        text.push_str(&disable(table, &[name]));
    }
    Ddl { text, warnings }
}

pub fn module(m: &Module) -> Ddl {
    let mut warnings = vec![];
    let mut text = module_text(m, &mut warnings);
    if let Some(properties) = properties(&m.properties, &mut warnings) {
        text.push('\n');
        text.push_str(&properties);
    }
    Ddl { text, warnings }
}

/// Indexes disable last, because a disabled clustered index makes its table unreadable to every later step.
fn dependents<'a>(name: &str, disabled_keys: impl Iterator<Item = &'a str>, indexes: &'a [Index], triggers: &[Module], props: &[Property], warnings: &mut Vec<String>) -> Vec<String> {
    let mut parts: Vec<String> = indexes.iter().filter_map(|i| index_text(i, name, warnings)).collect();
    parts.extend(triggers.iter().map(|m| module_text(m, warnings)));
    parts.extend(properties(props, warnings));
    let disabled: Vec<&str> = disabled_keys.chain(indexes.iter().filter(|i| i.storage.disabled && matches!(i.kind, 1 | 2 | 5 | 6)).map(|i| i.name.as_str())).collect();
    if !disabled.is_empty() {
        parts.push(disable(name, &disabled));
    }
    parts
}

fn disable(table: &str, names: &[&str]) -> String {
    names.iter().map(|n| format!("ALTER INDEX {} ON {table} DISABLE;\n", ident(n))).chain(std::iter::once("GO\n".to_owned())).collect()
}

fn column(c: &Column, table: &str, warnings: &mut Vec<String>) -> String {
    let name = ident(&c.name);
    let path = format!("{table}.{name}");
    for (on, property) in [(c.ansi_padding_off, "ANSI_PADDING OFF"), (c.filestream, "FILESTREAM"), (c.encrypted, "Always Encrypted")] {
        if on {
            warnings.push(format!("{property} on {path}"));
        }
    }
    if let Some(computed) = &c.computed {
        let persisted = if computed.persisted { if c.nullable { " PERSISTED" } else { " PERSISTED NOT NULL" } } else { "" };
        return format!("{name} AS {}{persisted}", computed.definition);
    }
    let mut out = format!("{name} {}", c.type_name);
    if let Some(collation) = &c.collation {
        out.push_str(&format!(" COLLATE {collation}"));
    }
    if c.sparse {
        out.push_str(" SPARSE");
    }
    if c.column_set {
        out.push_str(" COLUMN_SET FOR ALL_SPARSE_COLUMNS");
    }
    if let Some(function) = &c.masked {
        out.push_str(&format!(" MASKED WITH (FUNCTION = '{}')", function.replace('\'', "''")));
    }
    if let Some((seed, increment)) = &c.identity {
        out.push_str(&format!(" IDENTITY({seed},{increment})"));
        if c.not_for_replication {
            out.push_str(" NOT FOR REPLICATION");
        }
    }
    match c.generated {
        0 => {}
        1 | 2 => {
            out.push_str(if c.generated == 1 { " GENERATED ALWAYS AS ROW START" } else { " GENERATED ALWAYS AS ROW END" });
            if c.hidden {
                out.push_str(" HIDDEN");
            }
        }
        kind => warnings.push(format!("GENERATED ALWAYS type {kind} on {path}")),
    }
    if c.rowguidcol {
        out.push_str(" ROWGUIDCOL");
    }
    out.push_str(if c.nullable { " NULL" } else { " NOT NULL" });
    if let Some((constraint, definition)) = &c.default {
        out.push_str(&format!(" CONSTRAINT {} DEFAULT {definition}", ident(constraint)));
    }
    out
}

fn classification(table: &str, c: &Classification) -> String {
    let mut options: Vec<String> = [("LABEL", &c.label), ("LABEL_ID", &c.label_id), ("INFORMATION_TYPE", &c.information_type), ("INFORMATION_TYPE_ID", &c.information_type_id)]
        .into_iter()
        .filter_map(|(option, value)| value.as_ref().map(|v| format!("{option} = {}", quote_literal(v))))
        .collect();
    options.extend(c.rank.as_ref().map(|rank| format!("RANK = {rank}")));
    format!("ADD SENSITIVITY CLASSIFICATION TO {table}.{} WITH ({});\n", ident(&c.column), options.join(", "))
}

fn sorted(columns: &[(String, bool)]) -> String {
    columns.iter().map(|(c, desc)| format!("{} {}", ident(c), if *desc { "DESC" } else { "ASC" })).collect::<Vec<_>>().join(", ")
}

fn names(columns: &[String]) -> String {
    columns.iter().map(|c| ident(c)).collect::<Vec<_>>().join(", ")
}

fn clustering(clustered: bool) -> &'static str {
    if clustered { "CLUSTERED" } else { "NONCLUSTERED" }
}

fn with_options(storage: &Storage) -> String {
    let options = options(storage);
    if options.is_empty() { String::new() } else { format!(" WITH ({})", options.join(", ")) }
}

fn options(s: &Storage) -> Vec<String> {
    let mut out = vec![];
    for (on, option) in [(s.pad_index, "PAD_INDEX = ON"), (s.ignore_dup_key, "IGNORE_DUP_KEY = ON"), (s.no_recompute, "STATISTICS_NORECOMPUTE = ON")] {
        if on {
            out.push(option.to_owned());
        }
    }
    if s.fill_factor != 0 {
        out.push(format!("FILLFACTOR = {}", s.fill_factor));
    }
    for (on, option) in [(s.deny_row_locks, "ALLOW_ROW_LOCKS = OFF"), (s.deny_page_locks, "ALLOW_PAGE_LOCKS = OFF"), (s.sequential_key, "OPTIMIZE_FOR_SEQUENTIAL_KEY = ON")] {
        if on {
            out.push(option.to_owned());
        }
    }
    if let Some(compression) = &s.compression {
        out.push(format!("DATA_COMPRESSION = {compression}"));
    }
    if s.xml_compression {
        out.push("XML_COMPRESSION = ON".to_owned());
    }
    if let Some(delay) = s.compression_delay.filter(|d| *d > 0) {
        out.push(format!("COMPRESSION_DELAY = {delay}"));
    }
    out
}

impl Place {
    fn describe(&self) -> String {
        match self {
            Place::Filegroup(name) => format!("filegroup {}", ident(name)),
            Place::PartitionScheme(name) => format!("partition scheme {}", ident(name)),
        }
    }
}

/// `STATISTICS_INCREMENTAL` fails on an index that is not partitioned, as every replayed index
/// is, and SQL Server has no statement that sets `SUPPRESS_DUP_KEY_MESSAGES`.
fn storage_warnings(s: &Storage, index: &str, table: &str, warnings: &mut Vec<String>) {
    let of = if index.is_empty() { String::new() } else { format!(" for index {}", ident(index)) };
    if let Some(place) = &s.place {
        warnings.push(format!("{}{of} on {table}", place.describe()));
    }
    for (on, option) in [(s.incremental, "STATISTICS_INCREMENTAL"), (s.suppress_dup_key_messages, "SUPPRESS_DUP_KEY_MESSAGES")] {
        if on {
            warnings.push(format!("{option}{of} on {table}"));
        }
    }
}

fn key(k: &Key, table: &str, warnings: &mut Vec<String>) -> String {
    storage_warnings(&k.storage, &k.name, table, warnings);
    format!(
        "CONSTRAINT {} {} {} ({}){}",
        ident(&k.name),
        if k.primary { "PRIMARY KEY" } else { "UNIQUE" },
        clustering(k.clustered),
        sorted(&k.columns),
        with_options(&k.storage)
    )
}

fn replication(state: &State) -> &'static str {
    if state.not_for_replication { "NOT FOR REPLICATION " } else { "" }
}

fn foreign_key(f: &ForeignKey) -> String {
    let mut out = format!("CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({})", ident(&f.name), names(&f.columns), qualified(&f.ref_schema, &f.ref_table), names(&f.ref_columns));
    for (event, action) in [("DELETE", &f.on_delete), ("UPDATE", &f.on_update)] {
        if action != "NO_ACTION" {
            out.push_str(&format!(" ON {event} {}", action.replace('_', " ")));
        }
    }
    if f.state.not_for_replication {
        out.push_str(" NOT FOR REPLICATION");
    }
    out
}

/// A constraint created inside `CREATE TABLE` is enabled and trusted. NOT FOR REPLICATION already makes it untrusted.
fn constraint_state(table: &str, constraint: &str, state: &State) -> Option<String> {
    let name = ident(constraint);
    if state.disabled {
        Some(format!("ALTER TABLE {table} NOCHECK CONSTRAINT {name};"))
    } else if state.untrusted && !state.not_for_replication {
        Some(format!("ALTER TABLE {table} NOCHECK CONSTRAINT {name};\nALTER TABLE {table} WITH NOCHECK CHECK CONSTRAINT {name};"))
    } else {
        None
    }
}

fn index_text(i: &Index, table: &str, warnings: &mut Vec<String>) -> Option<String> {
    let name = ident(&i.name);
    let mut out = match i.kind {
        1 | 2 => format!("CREATE {}{} INDEX {name} ON {table} ({})", if i.unique { "UNIQUE " } else { "" }, clustering(i.kind == 1), sorted(&i.keys)),
        5 => format!("CREATE CLUSTERED COLUMNSTORE INDEX {name} ON {table}"),
        6 => format!("CREATE NONCLUSTERED COLUMNSTORE INDEX {name} ON {table} ({})", names(&i.include)),
        kind => {
            let what = match kind {
                3 => "XML index".to_owned(),
                4 => "spatial index".to_owned(),
                7 => "hash index".to_owned(),
                _ => format!("index of type {kind}"),
            };
            warnings.push(format!("{what} {name} on {table}"));
            return None;
        }
    };
    if i.kind <= 2 && !i.include.is_empty() {
        out.push_str(&format!(" INCLUDE ({})", names(&i.include)));
    }
    if !i.order.is_empty() {
        out.push_str(&format!(" ORDER ({})", names(&i.order)));
    }
    if let Some(filter) = &i.filter {
        out.push_str(&format!(" WHERE {filter}"));
    }
    let storage = match i.kind {
        // A columnstore index reports row and page locks as off, though neither option applies to it.
        5 | 6 => Storage { deny_row_locks: false, deny_page_locks: false, ..i.storage.clone() },
        _ => i.storage.clone(),
    };
    out.push_str(&with_options(&storage));
    storage_warnings(&i.storage, &i.name, table, warnings);
    let set = if i.filter.is_some() || i.on_view { INDEX_OPTIONS } else { "" };
    Some(format!("{set}{out};\nGO\n"))
}

/// `OBJECT_DEFINITION` keeps the name a module was created with, so a module renamed by
/// `sp_rename` or moved by `ALTER SCHEMA ... TRANSFER` would replay under its old name. A
/// one-part name would replay in the default schema of whoever runs the script.
fn module_text(m: &Module, warnings: &mut Vec<String>) -> String {
    let name = qualified(&m.schema, &m.name);
    let definition = m.definition.trim();
    let current = match module_name(definition) {
        None => {
            warnings.push(format!("current name in the CREATE header on {name}"));
            None
        }
        Some(header) => match header.parts.as_slice() {
            [_] => Some((header.span, name.clone())),
            [s, n] if *s != m.schema || *n != m.name => Some((header.span, name.clone())),
            _ => None,
        },
    };
    let definition = match current {
        Some((span, current)) => format!("{}{current}{}", &definition[..span.start], &definition[span.end..]),
        None => definition.to_owned(),
    };
    let mut out = format!("{}{definition}\nGO\n", set_options(m.ansi_nulls, m.quoted_identifier));
    if let Some(table) = &m.disabled_on {
        out.push_str(&format!("DISABLE TRIGGER {name} ON {table};\nGO\n"));
    }
    out
}

fn properties(props: &[Property], warnings: &mut Vec<String>) -> Option<String> {
    let lines: Vec<String> = props
        .iter()
        .filter_map(|p| {
            let value = match (&p.value, p.base_type.as_deref()) {
                (None, _) => "NULL".to_owned(),
                (Some(v), Some("nvarchar")) => quote_literal(v),
                (Some(v), Some("varchar")) => format!("'{}'", v.replace('\'', "''")),
                (Some(_), base_type) => {
                    let object = p.levels.iter().map(|(_, n)| ident(n)).collect::<Vec<_>>().join(".");
                    warnings.push(format!("extended property {} of type {} on {object}", ident(&p.name), base_type.unwrap_or("unknown")));
                    return None;
                }
            };
            let levels: String = p.levels.iter().enumerate().map(|(i, (kind, name))| format!(", @level{i}type = N'{kind}', @level{i}name = {}", quote_literal(name))).collect();
            Some(format!("EXEC sys.sp_addextendedproperty @name = {}, @value = {value}{levels};\n", quote_literal(&p.name)))
        })
        .collect();
    (!lines.is_empty()).then(|| format!("{}GO\n", lines.concat()))
}

fn set_options(ansi_nulls: bool, quoted_identifier: bool) -> String {
    let on = |b: bool| if b { "ON" } else { "OFF" };
    format!("SET ANSI_NULLS {}\nGO\nSET QUOTED_IDENTIFIER {}\nGO\n", on(ansi_nulls), on(quoted_identifier))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str, type_name: &str) -> Column {
        Column { name: name.into(), type_name: type_name.into(), ..Default::default() }
    }

    fn table_named(name: &str) -> Table {
        Table { schema: "dbo".into(), name: name.into(), ansi_nulls: true, lock_escalation: "TABLE".into(), columns: vec![col("id", "int")], ..Default::default() }
    }

    fn index_on(name: &str, kind: u8, keys: &[&str]) -> Index {
        Index { name: name.into(), kind, unique: false, keys: keys.iter().map(|k| (k.to_string(), false)).collect(), include: vec![], order: vec![], filter: None, on_view: false, storage: Storage::default() }
    }

    fn property(name: &str, value: &str, path: &[(&str, &str)]) -> Property {
        Property { name: name.into(), value: Some(value.into()), base_type: Some("nvarchar".into()), levels: path.iter().map(|(k, n)| (k.to_string(), n.to_string())).collect() }
    }

    const HEAD: &str = "SET ANSI_NULLS ON\nGO\nSET QUOTED_IDENTIFIER ON\nGO\n";

    #[test]
    fn a_table_carries_identity_computed_collation_defaults_keys_indexes_and_triggers() {
        let t = Table {
            columns: vec![
                Column { identity: Some(("100".into(), "5".into())), ..col("id", "int") },
                Column { collation: Some("Latin1_General_BIN2".into()), ..col("code", "nvarchar(20)") },
                Column { default: Some(("DF_tickets_qty".into(), "((1))".into())), ..col("qty", "int") },
                Column { nullable: true, computed: Some(Computed { definition: "([price]*[qty])".into(), persisted: true }), ..col("total", "decimal(21,2)") },
                Column { nullable: true, ..col("a]b", "[lab].[phone]") },
            ],
            keys: vec![
                Key { name: "PK_tickets".into(), primary: true, clustered: true, columns: vec![("id".into(), false)], storage: Storage::default() },
                Key { name: "UQ_code".into(), primary: false, clustered: false, columns: vec![("code".into(), true)], storage: Storage::default() },
            ],
            foreign_keys: vec![ForeignKey {
                name: "FK_owner".into(),
                columns: vec!["id".into(), "code".into()],
                ref_schema: "dbo".into(),
                ref_table: "owners".into(),
                ref_columns: vec!["id".into(), "code".into()],
                on_delete: "CASCADE".into(),
                on_update: "SET_NULL".into(),
                state: State::default(),
            }],
            checks: vec![Check { name: "CK_qty".into(), definition: "([qty]>(0))".into(), state: State::default() }],
            indexes: vec![Index { include: vec!["qty".into()], ..index_on("IX_code", 2, &["code"]) }],
            triggers: vec![Module {
                schema: "dbo".into(),
                name: "t".into(),
                definition: "CREATE TRIGGER dbo.t ON dbo.tickets AFTER UPDATE AS SELECT 1\n".into(),
                ansi_nulls: true,
                quoted_identifier: false,
                disabled_on: Some("[dbo].[tickets]".into()),
                properties: vec![],
            }],
            ..table_named("tickets")
        };
        assert_eq!(
            table(&t),
            Ddl::complete(format!(
                "{HEAD}CREATE TABLE [dbo].[tickets] (
    [id] int IDENTITY(100,5) NOT NULL,
    [code] nvarchar(20) COLLATE Latin1_General_BIN2 NOT NULL,
    [qty] int NOT NULL CONSTRAINT [DF_tickets_qty] DEFAULT ((1)),
    [total] AS ([price]*[qty]) PERSISTED,
    [a]]b] [lab].[phone] NULL,
    CONSTRAINT [PK_tickets] PRIMARY KEY CLUSTERED ([id] ASC),
    CONSTRAINT [UQ_code] UNIQUE NONCLUSTERED ([code] DESC),
    CONSTRAINT [FK_owner] FOREIGN KEY ([id], [code]) REFERENCES [dbo].[owners] ([id], [code]) ON DELETE CASCADE ON UPDATE SET NULL,
    CONSTRAINT [CK_qty] CHECK ([qty]>(0))
);
GO

CREATE NONCLUSTERED INDEX [IX_code] ON [dbo].[tickets] ([code] ASC) INCLUDE ([qty]);
GO

SET ANSI_NULLS ON
GO
SET QUOTED_IDENTIFIER OFF
GO
CREATE TRIGGER dbo.t ON dbo.tickets AFTER UPDATE AS SELECT 1
GO
DISABLE TRIGGER [dbo].[t] ON [dbo].[tickets];
GO
"
            ))
        );
    }

    #[test]
    fn a_non_null_persisted_column_says_so() {
        let c = Column { name: "n".into(), computed: Some(Computed { definition: "([a]+(1))".into(), persisted: true }), ..Default::default() };
        assert_eq!(column(&c, "[dbo].[t]", &mut vec![]), "[n] AS ([a]+(1)) PERSISTED NOT NULL");
        let virtual_column = Column { computed: Some(Computed { definition: "([a])".into(), persisted: false }), ..c };
        assert_eq!(column(&virtual_column, "[dbo].[t]", &mut vec![]), "[n] AS ([a])");
    }

    #[test]
    fn columns_keep_sparse_column_sets_masks_replication_identity_period_columns_and_rowguidcol() {
        let line = |c: Column| {
            let mut warnings = vec![];
            let text = column(&c, "[dbo].[t]", &mut warnings);
            assert!(warnings.is_empty(), "{warnings:?}");
            text
        };
        assert_eq!(line(Column { nullable: true, sparse: true, ..col("s", "int") }), "[s] int SPARSE NULL");
        assert_eq!(line(Column { nullable: true, column_set: true, ..col("cs", "xml") }), "[cs] xml COLUMN_SET FOR ALL_SPARSE_COLUMNS NULL");
        assert_eq!(
            line(Column { nullable: true, collation: Some("Latin1_General_BIN2".into()), masked: Some("partial(1,\"x'x\",1)".into()), ..col("m", "nvarchar(100)") }),
            "[m] nvarchar(100) COLLATE Latin1_General_BIN2 MASKED WITH (FUNCTION = 'partial(1,\"x''x\",1)') NULL"
        );
        assert_eq!(line(Column { identity: Some(("1".into(), "1".into())), not_for_replication: true, ..col("id", "int") }), "[id] int IDENTITY(1,1) NOT FOR REPLICATION NOT NULL");
        assert_eq!(line(Column { generated: 1, ..col("vf", "datetime2(7)") }), "[vf] datetime2(7) GENERATED ALWAYS AS ROW START NOT NULL");
        assert_eq!(line(Column { generated: 2, hidden: true, ..col("vt", "datetime2(7)") }), "[vt] datetime2(7) GENERATED ALWAYS AS ROW END HIDDEN NOT NULL");
        assert_eq!(
            line(Column { rowguidcol: true, default: Some(("DF_g".into(), "(newid())".into())), ..col("g", "uniqueidentifier") }),
            "[g] uniqueidentifier ROWGUIDCOL NOT NULL CONSTRAINT [DF_g] DEFAULT (newid())"
        );
    }

    #[test]
    fn columns_warn_for_ansi_padding_off_filestream_always_encrypted_and_other_generated_kinds() {
        let mut warnings = vec![];
        column(&Column { ansi_padding_off: true, filestream: true, encrypted: true, generated: 7, ..col("c]x", "varbinary(max)") }, "[dbo].[t]", &mut warnings);
        assert_eq!(
            warnings,
            ["ANSI_PADDING OFF on [dbo].[t].[c]]x]", "FILESTREAM on [dbo].[t].[c]]x]", "Always Encrypted on [dbo].[t].[c]]x]", "GENERATED ALWAYS type 7 on [dbo].[t].[c]]x]"]
        );
    }

    #[test]
    fn constraints_replay_disabled_untrusted_and_not_for_replication() {
        let check = |name: &str, state: State| Check { name: name.into(), definition: "([id]>(0))".into(), state };
        let fk = |name: &str, state: State| ForeignKey {
            name: name.into(),
            columns: vec!["id".into()],
            ref_schema: "dbo".into(),
            ref_table: "p".into(),
            ref_columns: vec!["id".into()],
            on_delete: "NO_ACTION".into(),
            on_update: "NO_ACTION".into(),
            state,
        };
        let t = Table {
            checks: vec![
                check("CK_off", State { disabled: true, untrusted: true, not_for_replication: false }),
                check("CK_untrusted", State { untrusted: true, ..State::default() }),
                check("CK_nfr", State { untrusted: true, not_for_replication: true, ..State::default() }),
            ],
            foreign_keys: vec![fk("FK_off", State { disabled: true, untrusted: true, not_for_replication: true }), fk("FK_untrusted", State { untrusted: true, ..State::default() })],
            ..table_named("c")
        };
        assert_eq!(
            table(&t),
            Ddl::complete(format!(
                "{HEAD}CREATE TABLE [dbo].[c] (
    [id] int NOT NULL,
    CONSTRAINT [FK_off] FOREIGN KEY ([id]) REFERENCES [dbo].[p] ([id]) NOT FOR REPLICATION,
    CONSTRAINT [FK_untrusted] FOREIGN KEY ([id]) REFERENCES [dbo].[p] ([id]),
    CONSTRAINT [CK_off] CHECK ([id]>(0)),
    CONSTRAINT [CK_untrusted] CHECK ([id]>(0)),
    CONSTRAINT [CK_nfr] CHECK NOT FOR REPLICATION ([id]>(0))
);
GO

ALTER TABLE [dbo].[c] NOCHECK CONSTRAINT [FK_off];
ALTER TABLE [dbo].[c] NOCHECK CONSTRAINT [FK_untrusted];
ALTER TABLE [dbo].[c] WITH NOCHECK CHECK CONSTRAINT [FK_untrusted];
ALTER TABLE [dbo].[c] NOCHECK CONSTRAINT [CK_off];
ALTER TABLE [dbo].[c] NOCHECK CONSTRAINT [CK_untrusted];
ALTER TABLE [dbo].[c] WITH NOCHECK CHECK CONSTRAINT [CK_untrusted];
GO
"
            ))
        );
    }

    #[test]
    fn keys_and_heaps_keep_their_options_compression_and_lock_escalation_and_disabled_keys_disable_last() {
        let options = Storage { fill_factor: 80, pad_index: true, ignore_dup_key: true, no_recompute: true, deny_row_locks: true, deny_page_locks: true, sequential_key: true, compression: Some("PAGE".into()), xml_compression: true, ..Storage::default() };
        let t = Table {
            keys: vec![
                Key { name: "PK_h".into(), primary: true, clustered: false, columns: vec![("id".into(), false)], storage: Storage { fill_factor: 80, ..Storage::default() } },
                Key { name: "UQ_h".into(), primary: false, clustered: false, columns: vec![("id".into(), false)], storage: Storage { disabled: true, ..options } },
            ],
            heap: Some(Storage { compression: Some("PAGE".into()), ..Storage::default() }),
            lock_escalation: "DISABLE".into(),
            ..table_named("h")
        };
        assert_eq!(
            table(&t),
            Ddl::complete(format!(
                "{HEAD}CREATE TABLE [dbo].[h] (
    [id] int NOT NULL,
    CONSTRAINT [PK_h] PRIMARY KEY NONCLUSTERED ([id] ASC) WITH (FILLFACTOR = 80),
    CONSTRAINT [UQ_h] UNIQUE NONCLUSTERED ([id] ASC) WITH (PAD_INDEX = ON, IGNORE_DUP_KEY = ON, STATISTICS_NORECOMPUTE = ON, FILLFACTOR = 80, ALLOW_ROW_LOCKS = OFF, ALLOW_PAGE_LOCKS = OFF, OPTIMIZE_FOR_SEQUENTIAL_KEY = ON, DATA_COMPRESSION = PAGE, XML_COMPRESSION = ON)
) WITH (DATA_COMPRESSION = PAGE);
GO

ALTER TABLE [dbo].[h] SET (LOCK_ESCALATION = DISABLE);
GO

ALTER INDEX [UQ_h] ON [dbo].[h] DISABLE;
GO
"
            ))
        );
    }

    #[test]
    fn filegroups_and_partition_schemes_warn_for_the_heap_keys_indexes_and_large_values() {
        let on = |place: Place| Storage { place: Some(place), ..Storage::default() };
        let t = Table {
            keys: vec![Key { name: "PK_p".into(), primary: true, clustered: false, columns: vec![("id".into(), false)], storage: on(Place::Filegroup("FG keys".into())) }],
            indexes: vec![Index { storage: on(Place::PartitionScheme("ps]x".into())), ..index_on("IX_p", 2, &["id"]) }],
            heap: Some(on(Place::PartitionScheme("ps".into()))),
            large_values: Some(Place::Filegroup("LOB".into())),
            ..table_named("p")
        };
        assert_eq!(
            table(&t).warnings,
            ["filegroup [LOB] for large values on [dbo].[p]", "filegroup [FG keys] for index [PK_p] on [dbo].[p]", "partition scheme [ps] on [dbo].[p]", "partition scheme [ps]]x] for index [IX_p] on [dbo].[p]"]
        );
    }

    #[test]
    fn sensitivity_classifications_replay_with_each_set_option() {
        let t = Table {
            classifications: vec![
                Classification {
                    column: "id".into(),
                    label: Some("Highly 'Confidential".into()),
                    label_id: Some("b8b3c1b5-1d4c-4ab4-9c1c-0f4d6e2a3b11".into()),
                    information_type: Some("Financial".into()),
                    information_type_id: Some("c44193e1-0e58-4b2a-9001-f7d6e7bc1373".into()),
                    rank: Some("HIGH".into()),
                },
                Classification { column: "b]x".into(), label: Some("Public".into()), ..Classification::default() },
            ],
            ..table_named("sc")
        };
        assert!(
            table(&t).text.ends_with(
                "GO

ADD SENSITIVITY CLASSIFICATION TO [dbo].[sc].[id] WITH (LABEL = N'Highly ''Confidential', LABEL_ID = N'b8b3c1b5-1d4c-4ab4-9c1c-0f4d6e2a3b11', INFORMATION_TYPE = N'Financial', INFORMATION_TYPE_ID = N'c44193e1-0e58-4b2a-9001-f7d6e7bc1373', RANK = HIGH);
ADD SENSITIVITY CLASSIFICATION TO [dbo].[sc].[b]]x] WITH (LABEL = N'Public');
GO
"
            ),
            "{}",
            table(&t).text
        );
    }

    #[test]
    fn incremental_statistics_and_suppressed_duplicate_key_messages_warn() {
        let t = Table {
            keys: vec![Key { name: "PK_i".into(), primary: true, clustered: true, columns: vec![("id".into(), false)], storage: Storage { incremental: true, ..Storage::default() } }],
            indexes: vec![Index { unique: true, storage: Storage { ignore_dup_key: true, suppress_dup_key_messages: true, ..Storage::default() }, ..index_on("UX_i", 2, &["id"]) }],
            ..table_named("i")
        };
        let ddl = table(&t);
        assert_eq!(ddl.warnings, ["STATISTICS_INCREMENTAL for index [PK_i] on [dbo].[i]", "SUPPRESS_DUP_KEY_MESSAGES for index [UX_i] on [dbo].[i]"]);
        assert!(!ddl.text.contains("STATISTICS_INCREMENTAL") && !ddl.text.contains("SUPPRESS_DUP_KEY_MESSAGES"), "{}", ddl.text);
        assert!(ddl.text.contains("CREATE UNIQUE NONCLUSTERED INDEX [UX_i] ON [dbo].[i] ([id] ASC) WITH (IGNORE_DUP_KEY = ON);"), "{}", ddl.text);
    }

    #[test]
    fn a_system_versioned_table_keeps_its_period_history_table_and_retention() {
        let t = Table {
            columns: vec![col("id", "int"), Column { generated: 1, ..col("vf", "datetime2(7)") }, Column { generated: 2, hidden: true, ..col("vt", "datetime2(7)") }],
            keys: vec![Key { name: "PK_t".into(), primary: true, clustered: true, columns: vec![("id".into(), false)], storage: Storage::default() }],
            period: Some(("vf".into(), "vt".into())),
            versioning: Some(Versioning { schema: "dbo".into(), name: "t_history".into(), retention: Some((6, "MONTH".into())) }),
            ..table_named("t")
        };
        assert_eq!(
            table(&t),
            Ddl::complete(format!(
                "{HEAD}CREATE TABLE [dbo].[t] (
    [id] int NOT NULL,
    [vf] datetime2(7) GENERATED ALWAYS AS ROW START NOT NULL,
    [vt] datetime2(7) GENERATED ALWAYS AS ROW END HIDDEN NOT NULL,
    CONSTRAINT [PK_t] PRIMARY KEY CLUSTERED ([id] ASC),
    PERIOD FOR SYSTEM_TIME ([vf], [vt])
) WITH (SYSTEM_VERSIONING = ON (HISTORY_TABLE = [dbo].[t_history], HISTORY_RETENTION_PERIOD = 6 MONTHS));
GO
"
            ))
        );
        let forever = Table { versioning: Some(Versioning { schema: "h".into(), name: "x".into(), retention: None }), ..t };
        assert!(table(&forever).text.contains(") WITH (SYSTEM_VERSIONING = ON (HISTORY_TABLE = [h].[x]));\n"));
        let history = Table { history_of: Some(("dbo".into(), "t".into())), ..table_named("t_history") };
        assert_eq!(table(&history).warnings, ["history table role for [dbo].[t] on [dbo].[t_history]"]);
    }

    #[test]
    fn columnstore_indexes_keep_order_filter_and_options_under_the_index_set_options() {
        let cci = Index { order: vec!["id".into()], storage: Storage { compression: Some("COLUMNSTORE_ARCHIVE".into()), compression_delay: Some(10), ..Storage::default() }, ..index_on("CCI", 5, &[]) };
        assert_eq!(index(&cci, "[dbo].[c]", &[]), Some(Ddl::complete("CREATE CLUSTERED COLUMNSTORE INDEX [CCI] ON [dbo].[c] ORDER ([id]) WITH (DATA_COMPRESSION = COLUMNSTORE_ARCHIVE, COMPRESSION_DELAY = 10);\nGO\n".into())));
        let ncci = Index {
            include: vec!["id".into(), "a]b".into()],
            filter: Some("([id]>(0))".into()),
            storage: Storage { compression_delay: Some(0), deny_row_locks: true, deny_page_locks: true, ..Storage::default() },
            ..index_on("NCCI", 6, &[])
        };
        assert_eq!(
            index(&ncci, "[dbo].[c]", &[]),
            Some(Ddl::complete(format!("{INDEX_OPTIONS}CREATE NONCLUSTERED COLUMNSTORE INDEX [NCCI] ON [dbo].[c] ([id], [a]]b]) WHERE ([id]>(0));\nGO\n")))
        );
    }

    #[test]
    fn rowstore_indexes_keep_their_options_and_a_disabled_index_replays_disabled() {
        let i = Index {
            unique: true,
            include: vec!["t".into()],
            storage: Storage { fill_factor: 70, pad_index: true, ignore_dup_key: true, deny_row_locks: true, deny_page_locks: true, compression: Some("ROW".into()), disabled: true, ..Storage::default() },
            ..index_on("IX opts", 2, &["g"])
        };
        assert_eq!(
            index(&i, "[dbo].[t]", &[]),
            Some(Ddl::complete(
                "CREATE UNIQUE NONCLUSTERED INDEX [IX opts] ON [dbo].[t] ([g] ASC) INCLUDE ([t]) WITH (PAD_INDEX = ON, IGNORE_DUP_KEY = ON, FILLFACTOR = 70, ALLOW_ROW_LOCKS = OFF, ALLOW_PAGE_LOCKS = OFF, DATA_COMPRESSION = ROW);\nGO\nALTER INDEX [IX opts] ON [dbo].[t] DISABLE;\nGO\n".into()
            ))
        );
    }

    #[test]
    fn a_key_index_opens_as_its_constraint_with_its_options_properties_and_disabled_state() {
        let k = Key { name: "UQ_refd".into(), primary: false, clustered: false, columns: vec![("a".into(), false), ("b".into(), true)], storage: Storage { ignore_dup_key: true, deny_page_locks: true, disabled: true, ..Storage::default() } };
        let props = [property("MS_Description", "the key", &[("SCHEMA", "dbo"), ("TABLE", "refd"), ("CONSTRAINT", "UQ_refd")])];
        assert_eq!(
            key_script(&k, "[dbo].[refd]", &props),
            Ddl::complete(
                "ALTER TABLE [dbo].[refd] ADD CONSTRAINT [UQ_refd] UNIQUE NONCLUSTERED ([a] ASC, [b] DESC) WITH (IGNORE_DUP_KEY = ON, ALLOW_PAGE_LOCKS = OFF);\nGO\n\nEXEC sys.sp_addextendedproperty @name = N'MS_Description', @value = N'the key', @level0type = N'SCHEMA', @level0name = N'dbo', @level1type = N'TABLE', @level1name = N'refd', @level2type = N'CONSTRAINT', @level2name = N'UQ_refd';\nGO\nALTER INDEX [UQ_refd] ON [dbo].[refd] DISABLE;\nGO\n".into()
            )
        );
        let pk = Key { name: "PK_tickets".into(), primary: true, clustered: true, columns: vec![("id".into(), false)], storage: Storage::default() };
        assert_eq!(key_script(&pk, "[dbo].[tickets]", &[]).text, "ALTER TABLE [dbo].[tickets] ADD CONSTRAINT [PK_tickets] PRIMARY KEY CLUSTERED ([id] ASC);\nGO\n");
    }

    #[test]
    fn xml_spatial_hash_and_unknown_indexes_warn_in_a_table_and_refuse_alone() {
        let t = Table { indexes: vec![index_on("IX x", 3, &["x"]), index_on("SIX", 4, &["g"]), index_on("H", 7, &["id"]), index_on("Q", 9, &["id"]), index_on("ok", 2, &["id"])], ..table_named("t") };
        let ddl = table(&t);
        assert_eq!(ddl.warnings, ["XML index [IX x] on [dbo].[t]", "spatial index [SIX] on [dbo].[t]", "hash index [H] on [dbo].[t]", "index of type 9 [Q] on [dbo].[t]"]);
        assert!(ddl.text.ends_with("GO\n\nCREATE NONCLUSTERED INDEX [ok] ON [dbo].[t] ([id] ASC);\nGO\n"), "{}", ddl.text);
        assert_eq!(index(&index_on("IX x", 3, &["x"]), "[dbo].[t]", &[]), None);
    }

    #[test]
    fn extended_properties_replay_for_the_table_its_parts_and_warn_for_other_types() {
        let base = [("SCHEMA", "we]ird s"), ("TABLE", "Par ent")];
        let t = Table {
            properties: vec![
                property("MS_Description", "table comment", &base),
                property("MS_Description", "it's masked", &[base[0], base[1], ("COLUMN", "masked")]),
                Property { base_type: Some("varchar".into()), ..property("note", "plain", &[base[0], base[1], ("INDEX", "UX ü")]) },
                Property { value: None, base_type: None, ..property("empty", "", &[base[0], base[1], ("TRIGGER", "trg")]) },
                Property { base_type: Some("int".into()), ..property("Version", "3", &base) },
            ],
            ..Table { schema: "we]ird s".into(), ..table_named("Par ent") }
        };
        let ddl = table(&t);
        assert_eq!(ddl.warnings, ["extended property [Version] of type int on [we]]ird s].[Par ent]"]);
        assert!(
            ddl.text.ends_with(
                "GO

EXEC sys.sp_addextendedproperty @name = N'MS_Description', @value = N'table comment', @level0type = N'SCHEMA', @level0name = N'we]ird s', @level1type = N'TABLE', @level1name = N'Par ent';
EXEC sys.sp_addextendedproperty @name = N'MS_Description', @value = N'it''s masked', @level0type = N'SCHEMA', @level0name = N'we]ird s', @level1type = N'TABLE', @level1name = N'Par ent', @level2type = N'COLUMN', @level2name = N'masked';
EXEC sys.sp_addextendedproperty @name = N'note', @value = 'plain', @level0type = N'SCHEMA', @level0name = N'we]ird s', @level1type = N'TABLE', @level1name = N'Par ent', @level2type = N'INDEX', @level2name = N'UX ü';
EXEC sys.sp_addextendedproperty @name = N'empty', @value = NULL, @level0type = N'SCHEMA', @level0name = N'we]ird s', @level1type = N'TABLE', @level1name = N'Par ent', @level2type = N'TRIGGER', @level2name = N'trg';
GO
"
            ),
            "{}",
            ddl.text
        );
    }

    fn procedure(definition: &str) -> Module {
        Module { schema: "dbo".into(), name: "newname".into(), definition: definition.into(), ansi_nulls: true, quoted_identifier: true, ..Default::default() }
    }

    #[test]
    fn a_module_keeps_its_text_under_its_set_options_and_its_properties() {
        let m = Module { ansi_nulls: false, properties: vec![property("MS_Description", "p", &[("SCHEMA", "dbo"), ("PROCEDURE", "newname")])], ..procedure("CREATE PROCEDURE dbo.newname AS SELECT 1;") };
        assert_eq!(
            module(&m),
            Ddl::complete(
                "SET ANSI_NULLS OFF\nGO\nSET QUOTED_IDENTIFIER ON\nGO\nCREATE PROCEDURE dbo.newname AS SELECT 1;\nGO\n\nEXEC sys.sp_addextendedproperty @name = N'MS_Description', @value = N'p', @level0type = N'SCHEMA', @level0name = N'dbo', @level1type = N'PROCEDURE', @level1name = N'newname';\nGO\n".into()
            )
        );
    }

    #[test]
    fn a_renamed_or_moved_module_replays_under_its_current_name() {
        let text = |definition: &str, schema: &str| module(&Module { schema: schema.into(), ..procedure(definition) });
        assert_eq!(text("CREATE PROCEDURE oldname AS SELECT 1", "dbo"), Ddl::complete(format!("{HEAD}CREATE PROCEDURE [dbo].[newname] AS SELECT 1\nGO\n")));
        assert_eq!(text("CREATE PROCEDURE [oldname] AS SELECT 1", "lab"), Ddl::complete(format!("{HEAD}CREATE PROCEDURE [lab].[newname] AS SELECT 1\nGO\n")));
        assert_eq!(text("/* x */ CREATE   PROC dbo.oldname AS SELECT 'oldname'", "dbo"), Ddl::complete(format!("{HEAD}/* x */ CREATE   PROC [dbo].[newname] AS SELECT 'oldname'\nGO\n")));
        assert_eq!(text("CREATE PROCEDURE dbo.newname AS SELECT 1", "we]ird s"), Ddl::complete(format!("{HEAD}CREATE PROCEDURE [we]]ird s].[newname] AS SELECT 1\nGO\n")));
        assert_eq!(text("CREATE PROCEDURE newname AS SELECT 1", "lab"), Ddl::complete(format!("{HEAD}CREATE PROCEDURE [lab].[newname] AS SELECT 1\nGO\n")));
        assert_eq!(text("create procedure [dbo].[newname] as select 1", "dbo"), Ddl::complete(format!("{HEAD}create procedure [dbo].[newname] as select 1\nGO\n")));
        assert_eq!(
            text("CREATE PROCEDURE p AS SELECT 'unclosed", "dbo"),
            Ddl { text: format!("{HEAD}CREATE PROCEDURE p AS SELECT 'unclosed\nGO\n"), warnings: vec!["current name in the CREATE header on [dbo].[newname]".into()] }
        );
    }

    #[test]
    fn a_view_carries_its_indexes_under_the_index_set_options_its_triggers_and_properties() {
        let module = |name: &str, definition: &str| Module { schema: "dbo".into(), name: name.into(), definition: definition.into(), ansi_nulls: true, quoted_identifier: true, ..Default::default() };
        let v = View {
            module: module("v", "CREATE VIEW dbo.v WITH SCHEMABINDING AS SELECT id FROM dbo.t"),
            indexes: vec![Index { unique: true, on_view: true, storage: Storage { fill_factor: 90, ..Storage::default() }, ..index_on("IX_v", 1, &["id"]) }],
            triggers: vec![module("v_ins", "CREATE TRIGGER dbo.v_ins ON dbo.v INSTEAD OF INSERT AS SELECT 1")],
            properties: vec![property("MS_Description", "bound", &[("SCHEMA", "dbo"), ("VIEW", "v")])],
        };
        assert_eq!(
            view(&v),
            Ddl::complete(format!(
                "{HEAD}CREATE VIEW dbo.v WITH SCHEMABINDING AS SELECT id FROM dbo.t
GO

{INDEX_OPTIONS}CREATE UNIQUE CLUSTERED INDEX [IX_v] ON [dbo].[v] ([id] ASC) WITH (FILLFACTOR = 90);
GO

{HEAD}CREATE TRIGGER dbo.v_ins ON dbo.v INSTEAD OF INSERT AS SELECT 1
GO

EXEC sys.sp_addextendedproperty @name = N'MS_Description', @value = N'bound', @level0type = N'SCHEMA', @level0name = N'dbo', @level1type = N'VIEW', @level1name = N'v';
GO
"
            ))
        );
    }
}
