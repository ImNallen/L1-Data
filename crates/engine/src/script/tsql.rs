use crate::sql::tsql::quote_ident as ident;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Table {
    pub schema: String,
    pub name: String,
    pub ansi_nulls: bool,
    pub columns: Vec<Column>,
    pub keys: Vec<Key>,
    pub foreign_keys: Vec<ForeignKey>,
    /// `sys.check_constraints.definition` carries its own parentheses.
    pub checks: Vec<(String, String)>,
    pub indexes: Vec<Index>,
    pub triggers: Vec<Module>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub type_name: String,
    pub collation: Option<String>,
    pub nullable: bool,
    pub identity: Option<(String, String)>,
    pub computed: Option<Computed>,
    pub default: Option<(String, String)>,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Index {
    pub name: String,
    pub unique: bool,
    pub clustered: bool,
    pub keys: Vec<(String, bool)>,
    pub include: Vec<String>,
    pub filter: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub definition: String,
    pub ansi_nulls: bool,
    pub quoted_identifier: bool,
    pub disabled: Option<(String, String)>,
}

pub fn qualified(schema: &str, name: &str) -> String {
    format!("{}.{}", ident(schema), ident(name))
}

pub fn table(t: &Table) -> String {
    let name = qualified(&t.schema, &t.name);
    let body: Vec<String> = t
        .columns
        .iter()
        .map(column)
        .chain(t.keys.iter().map(key))
        .chain(t.foreign_keys.iter().map(foreign_key))
        .chain(t.checks.iter().map(|(n, def)| format!("CONSTRAINT {} CHECK {def}", ident(n))))
        .collect();
    let create = format!("{}CREATE TABLE {name} (\n    {}\n);\nGO\n", set_options(t.ansi_nulls, true), body.join(",\n    "));
    std::iter::once(create).chain(t.indexes.iter().map(|i| index(i, &name))).chain(t.triggers.iter().map(module)).collect::<Vec<_>>().join("\n")
}

fn column(c: &Column) -> String {
    let name = ident(&c.name);
    if let Some(computed) = &c.computed {
        let persisted = if computed.persisted { if c.nullable { " PERSISTED" } else { " PERSISTED NOT NULL" } } else { "" };
        return format!("{name} AS {}{persisted}", computed.definition);
    }
    let mut out = format!("{name} {}", c.type_name);
    if let Some(collation) = &c.collation {
        out.push_str(&format!(" COLLATE {collation}"));
    }
    if let Some((seed, increment)) = &c.identity {
        out.push_str(&format!(" IDENTITY({seed},{increment})"));
    }
    out.push_str(if c.nullable { " NULL" } else { " NOT NULL" });
    if let Some((constraint, definition)) = &c.default {
        out.push_str(&format!(" CONSTRAINT {} DEFAULT {definition}", ident(constraint)));
    }
    out
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

fn key(k: &Key) -> String {
    format!("CONSTRAINT {} {} {} ({})", ident(&k.name), if k.primary { "PRIMARY KEY" } else { "UNIQUE" }, clustering(k.clustered), sorted(&k.columns))
}

fn foreign_key(f: &ForeignKey) -> String {
    let mut out = format!("CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({})", ident(&f.name), names(&f.columns), qualified(&f.ref_schema, &f.ref_table), names(&f.ref_columns));
    for (event, action) in [("DELETE", &f.on_delete), ("UPDATE", &f.on_update)] {
        if action != "NO_ACTION" {
            out.push_str(&format!(" ON {event} {}", action.replace('_', " ")));
        }
    }
    out
}

pub fn index(i: &Index, table: &str) -> String {
    let mut out = format!("CREATE {}{} INDEX {} ON {table} ({})", if i.unique { "UNIQUE " } else { "" }, clustering(i.clustered), ident(&i.name), sorted(&i.keys));
    if !i.include.is_empty() {
        out.push_str(&format!(" INCLUDE ({})", names(&i.include)));
    }
    if let Some(filter) = &i.filter {
        out.push_str(&format!(" WHERE {filter}"));
    }
    format!("{out};\nGO\n")
}

pub fn module(m: &Module) -> String {
    let mut out = format!("{}{}\nGO\n", set_options(m.ansi_nulls, m.quoted_identifier), m.definition.trim());
    if let Some((trigger, table)) = &m.disabled {
        out.push_str(&format!("DISABLE TRIGGER {trigger} ON {table};\nGO\n"));
    }
    out
}

fn set_options(ansi_nulls: bool, quoted_identifier: bool) -> String {
    let on = |b: bool| if b { "ON" } else { "OFF" };
    format!("SET ANSI_NULLS {}\nGO\nSET QUOTED_IDENTIFIER {}\nGO\n", on(ansi_nulls), on(quoted_identifier))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_carries_identity_computed_collation_defaults_keys_indexes_and_triggers() {
        let t = Table {
            schema: "dbo".into(),
            name: "tickets".into(),
            ansi_nulls: true,
            columns: vec![
                Column { name: "id".into(), type_name: "int".into(), identity: Some(("100".into(), "5".into())), ..Default::default() },
                Column { name: "code".into(), type_name: "nvarchar(20)".into(), collation: Some("Latin1_General_BIN2".into()), ..Default::default() },
                Column { name: "qty".into(), type_name: "int".into(), default: Some(("DF_tickets_qty".into(), "((1))".into())), ..Default::default() },
                Column { name: "total".into(), type_name: "decimal(21,2)".into(), nullable: true, computed: Some(Computed { definition: "([price]*[qty])".into(), persisted: true }), ..Default::default() },
                Column { name: "a]b".into(), type_name: "[lab].[phone]".into(), nullable: true, ..Default::default() },
            ],
            keys: vec![
                Key { name: "PK_tickets".into(), primary: true, clustered: true, columns: vec![("id".into(), false)] },
                Key { name: "UQ_code".into(), primary: false, clustered: false, columns: vec![("code".into(), true)] },
            ],
            foreign_keys: vec![ForeignKey {
                name: "FK_owner".into(),
                columns: vec!["id".into(), "code".into()],
                ref_schema: "dbo".into(),
                ref_table: "owners".into(),
                ref_columns: vec!["id".into(), "code".into()],
                on_delete: "CASCADE".into(),
                on_update: "SET_NULL".into(),
            }],
            checks: vec![("CK_qty".into(), "([qty]>(0))".into())],
            indexes: vec![Index { name: "IX_code".into(), unique: false, clustered: false, keys: vec![("code".into(), false)], include: vec!["qty".into()], filter: Some("([qty]>(1))".into()) }],
            triggers: vec![Module { definition: "CREATE TRIGGER dbo.t ON dbo.tickets AFTER UPDATE AS SELECT 1\n".into(), ansi_nulls: true, quoted_identifier: false, disabled: Some(("[dbo].[t]".into(), "[dbo].[tickets]".into())) }],
        };
        assert_eq!(
            table(&t),
            "SET ANSI_NULLS ON
GO
SET QUOTED_IDENTIFIER ON
GO
CREATE TABLE [dbo].[tickets] (
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

CREATE NONCLUSTERED INDEX [IX_code] ON [dbo].[tickets] ([code] ASC) INCLUDE ([qty]) WHERE ([qty]>(1));
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
        );
    }

    #[test]
    fn a_non_null_persisted_column_says_so() {
        let c = Column { name: "n".into(), computed: Some(Computed { definition: "([a]+(1))".into(), persisted: true }), ..Default::default() };
        assert_eq!(column(&c), "[n] AS ([a]+(1)) PERSISTED NOT NULL");
        let virtual_column = Column { computed: Some(Computed { definition: "([a])".into(), persisted: false }), ..c };
        assert_eq!(column(&virtual_column), "[n] AS ([a])");
    }

    #[test]
    fn a_module_keeps_its_text_under_its_set_options() {
        let m = Module { definition: "CREATE PROCEDURE dbo.p AS SELECT 1;".into(), ansi_nulls: false, quoted_identifier: true, disabled: None };
        assert_eq!(module(&m), "SET ANSI_NULLS OFF\nGO\nSET QUOTED_IDENTIFIER ON\nGO\nCREATE PROCEDURE dbo.p AS SELECT 1;\nGO\n");
    }
}
