use crate::text::{quote_ident_if_needed as ident, quote_literal};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Table {
    pub schema: String,
    pub name: String,
    pub unlogged: bool,
    pub options: Vec<String>,
    pub comment: Option<String>,
    pub columns: Vec<Column>,
    pub constraints: Vec<(String, String)>,
    pub partition_key: Option<String>,
    pub partition_of: Option<PartitionOf>,
    pub partitions: Vec<Partition>,
    pub indexes: Vec<String>,
    pub triggers: Vec<Trigger>,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Generated {
    Stored,
    Virtual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub always: bool,
    pub type_name: String,
    pub start: i64,
    pub increment: i64,
    pub min: i64,
    pub max: i64,
    pub cache: i64,
    pub cycle: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionOf {
    pub schema: String,
    pub name: String,
    pub bound: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Partition {
    pub schema: String,
    pub name: String,
    pub bound: String,
    pub partition_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trigger {
    pub name: String,
    pub definition: String,
    pub enabled: char,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct View {
    pub schema: String,
    pub name: String,
    pub materialized: bool,
    pub populated: bool,
    pub options: Vec<String>,
    /// `pg_get_viewdef`, which ends in `;`.
    pub query: String,
    pub comment: Option<String>,
    pub indexes: Vec<String>,
    pub triggers: Vec<Trigger>,
}

pub fn qualified(schema: &str, name: &str) -> String {
    format!("{}.{}", ident(schema), ident(name))
}

pub fn table(t: &Table) -> String {
    let name = qualified(&t.schema, &t.name);
    let (deferred, inline): (Vec<_>, Vec<_>) = t.constraints.iter().partition(|(_, def)| def.ends_with(" NOT VALID"));
    let constraints = inline.iter().map(|(n, def)| format!("CONSTRAINT {} {def}", ident(n)));
    let mut head = format!("CREATE {}TABLE {name}", if t.unlogged { "UNLOGGED " } else { "" });
    match &t.partition_of {
        Some(parent) => {
            head.push_str(&format!(" PARTITION OF {}", qualified(&parent.schema, &parent.name)));
            let body: Vec<_> = constraints.collect();
            if !body.is_empty() {
                head.push_str(&format!(" (\n    {}\n)", body.join(",\n    ")));
            }
            head.push_str(&format!("\n    {}", parent.bound));
        }
        None => {
            let body: Vec<_> = t.columns.iter().map(column).chain(constraints).collect();
            head.push_str(&format!(" (\n    {}\n)", body.join(",\n    ")));
        }
    }
    if let Some(key) = &t.partition_key {
        head.push_str(&format!(" PARTITION BY {key}"));
    }
    if !t.options.is_empty() {
        head.push_str(&format!(" WITH ({})", t.options.join(", ")));
    }
    let mut statements = vec![head];
    statements.extend(deferred.iter().map(|(n, def)| format!("ALTER TABLE {name} ADD CONSTRAINT {} {def}", ident(n))));
    statements.extend(t.partitions.iter().map(|p| {
        let key = p.partition_key.as_ref().map(|k| format!(" PARTITION BY {k}")).unwrap_or_default();
        format!("CREATE TABLE {} PARTITION OF {name}\n    {}{key}", qualified(&p.schema, &p.name), p.bound)
    }));
    statements.extend(t.indexes.iter().map(|def| index(def)));
    statements.extend(t.triggers.iter().flat_map(|tr| trigger(tr, &name)));
    statements.extend(comment("TABLE", &name, &t.comment));
    for c in &t.columns {
        statements.extend(comment("COLUMN", &format!("{name}.{}", ident(&c.name)), &c.comment));
    }
    script(statements)
}

fn column(c: &Column) -> String {
    let mut out = format!("{} {}", ident(&c.name), c.type_name);
    if let Some((schema, name)) = &c.collation {
        out.push_str(&format!(" COLLATE {}", if schema == "pg_catalog" { ident(name) } else { qualified(schema, name) }));
    }
    if let Some(identity) = &c.identity {
        out.push_str(&format!(" {}", identity_clause(identity)));
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

fn identity_clause(i: &Identity) -> String {
    let (type_min, type_max) = match i.type_name.as_str() {
        "smallint" => (i64::from(i16::MIN), i64::from(i16::MAX)),
        "integer" => (i64::from(i32::MIN), i64::from(i32::MAX)),
        _ => (i64::MIN, i64::MAX),
    };
    let (min, max) = if i.increment > 0 { (1, type_max) } else { (type_min, -1) };
    let start = if i.increment > 0 { i.min } else { i.max };
    let mut options = vec![];
    if i.start != start {
        options.push(format!("START WITH {}", i.start));
    }
    if i.increment != 1 {
        options.push(format!("INCREMENT BY {}", i.increment));
    }
    if i.min != min {
        options.push(format!("MINVALUE {}", i.min));
    }
    if i.max != max {
        options.push(format!("MAXVALUE {}", i.max));
    }
    if i.cache != 1 {
        options.push(format!("CACHE {}", i.cache));
    }
    if i.cycle {
        options.push("CYCLE".into());
    }
    let head = format!("GENERATED {} AS IDENTITY", if i.always { "ALWAYS" } else { "BY DEFAULT" });
    if options.is_empty() { head } else { format!("{head} ({})", options.join(" ")) }
}

pub fn view(v: &View) -> String {
    let name = qualified(&v.schema, &v.name);
    let check = v.options.iter().find_map(|o| o.strip_prefix("check_option=")).map(|c| c.to_uppercase());
    let options: Vec<_> = v.options.iter().filter(|o| !o.starts_with("check_option=")).cloned().collect();
    let with = if options.is_empty() { String::new() } else { format!(" WITH ({})", options.join(", ")) };
    let query = v.query.trim_end().trim_end_matches(';');
    let mut head = format!("CREATE {}VIEW {name}{with} AS\n{query}", if v.materialized { "MATERIALIZED " } else { "" });
    if let Some(check) = check {
        head.push_str(&format!("\n  WITH {check} CHECK OPTION"));
    }
    if v.materialized {
        head.push_str(if v.populated { "\nWITH DATA" } else { "\nWITH NO DATA" });
    }
    let mut statements = vec![head];
    statements.extend(v.indexes.iter().map(|def| index(def)));
    statements.extend(v.triggers.iter().flat_map(|tr| trigger(tr, &name)));
    statements.extend(comment(if v.materialized { "MATERIALIZED VIEW" } else { "VIEW" }, &name, &v.comment));
    script(statements)
}

pub fn routine(definition: &str, procedure: bool, signature: &str, description: &Option<String>) -> String {
    let mut statements = vec![definition.trim_end().to_owned()];
    statements.extend(comment(if procedure { "PROCEDURE" } else { "FUNCTION" }, signature, description));
    script(statements)
}

/// `pg_get_indexdef` prints `ON ONLY` for a partitioned table's index, which would create the
/// index without its partitions' copies.
pub fn index(definition: &str) -> String {
    match top_level(definition, " ON ONLY ") {
        Some(at) => format!("{} ON {}", &definition[..at], &definition[at + " ON ONLY ".len()..]),
        None => definition.to_owned(),
    }
}

pub fn trigger(t: &Trigger, table: &str) -> Vec<String> {
    let state = match t.enabled {
        'D' => Some("DISABLE"),
        'R' => Some("ENABLE REPLICA"),
        'A' => Some("ENABLE ALWAYS"),
        _ => None,
    };
    std::iter::once(t.definition.clone()).chain(state.map(|s| format!("ALTER TABLE {table} {s} TRIGGER {}", ident(&t.name)))).collect()
}

fn comment(kind: &str, name: &str, text: &Option<String>) -> Option<String> {
    text.as_ref().map(|t| format!("COMMENT ON {kind} {name} IS {}", quote_literal(t)))
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

    fn identity(start: i64, increment: i64, min: i64, max: i64) -> Identity {
        Identity { always: false, type_name: "bigint".into(), start, increment, min, max, cache: 1, cycle: false }
    }

    #[test]
    fn a_table_carries_its_columns_constraints_partitions_indexes_triggers_and_comments() {
        let t = Table {
            schema: "public".into(),
            name: "order_events".into(),
            comment: Some("it's partitioned".into()),
            columns: vec![
                Column { name: "id".into(), type_name: "bigint".into(), not_null: true, identity: Some(identity(1000, 10, 1, i64::MAX)), ..Default::default() },
                Column { name: "code".into(), type_name: "text".into(), collation: Some(("pg_catalog".into(), "C".into())), not_null: true, ..Default::default() },
                Column { name: "Total".into(), type_name: "numeric(12,2)".into(), default: Some("(price * qty)".into()), generated: Some(Generated::Stored), ..Default::default() },
                Column { name: "status".into(), type_name: "text".into(), default: Some("'new'::text".into()), comment: Some("lifecycle".into()), ..Default::default() },
                Column { name: "seq".into(), type_name: "bigserial".into(), not_null: true, ..Default::default() },
            ],
            constraints: vec![
                ("order_events_pkey".into(), "PRIMARY KEY (id, happened_on)".into()),
                ("positive".into(), "CHECK ((id > 0)) NOT VALID".into()),
            ],
            partition_key: Some("RANGE (happened_on)".into()),
            partitions: vec![
                Partition { schema: "public".into(), name: "order_events_h1".into(), bound: "FOR VALUES FROM ('2026-01-01') TO ('2026-07-01')".into(), partition_key: None },
                Partition { schema: "public".into(), name: "order_events_rest".into(), bound: "DEFAULT".into(), partition_key: Some("LIST (code)".into()) },
            ],
            indexes: vec!["CREATE INDEX order_events_kind_idx ON ONLY public.order_events USING btree (kind)".into()],
            triggers: vec![Trigger { name: "audit".into(), definition: "CREATE TRIGGER audit AFTER INSERT ON public.order_events FOR EACH ROW EXECUTE FUNCTION public.audit()".into(), enabled: 'D' }],
            ..Default::default()
        };
        assert_eq!(
            table(&t),
            "CREATE TABLE public.order_events (
    id bigint GENERATED BY DEFAULT AS IDENTITY (START WITH 1000 INCREMENT BY 10) NOT NULL,
    code text COLLATE \"C\" NOT NULL,
    \"Total\" numeric(12,2) GENERATED ALWAYS AS ((price * qty)) STORED,
    status text DEFAULT 'new'::text,
    seq bigserial NOT NULL,
    CONSTRAINT order_events_pkey PRIMARY KEY (id, happened_on)
) PARTITION BY RANGE (happened_on);

ALTER TABLE public.order_events ADD CONSTRAINT positive CHECK ((id > 0)) NOT VALID;

CREATE TABLE public.order_events_h1 PARTITION OF public.order_events
    FOR VALUES FROM ('2026-01-01') TO ('2026-07-01');

CREATE TABLE public.order_events_rest PARTITION OF public.order_events
    DEFAULT PARTITION BY LIST (code);

CREATE INDEX order_events_kind_idx ON public.order_events USING btree (kind);

CREATE TRIGGER audit AFTER INSERT ON public.order_events FOR EACH ROW EXECUTE FUNCTION public.audit();

ALTER TABLE public.order_events DISABLE TRIGGER audit;

COMMENT ON TABLE public.order_events IS 'it''s partitioned';

COMMENT ON COLUMN public.order_events.status IS 'lifecycle';
"
        );
    }

    #[test]
    fn a_partition_names_its_parent_bound_and_own_constraints_but_no_columns() {
        let t = Table {
            schema: "public".into(),
            name: "e1".into(),
            unlogged: true,
            columns: vec![Column { name: "id".into(), type_name: "integer".into(), ..Default::default() }],
            constraints: vec![("e1_note".into(), "CHECK ((id < 10))".into())],
            partition_of: Some(PartitionOf { schema: "public".into(), name: "e".into(), bound: "FOR VALUES IN (1)".into() }),
            options: vec!["fillfactor=70".into()],
            ..Default::default()
        };
        assert_eq!(table(&t), "CREATE UNLOGGED TABLE public.e1 PARTITION OF public.e (\n    CONSTRAINT e1_note CHECK ((id < 10))\n)\n    FOR VALUES IN (1) WITH (fillfactor=70);\n");
    }

    #[test]
    fn identity_prints_only_the_options_that_differ_from_the_defaults() {
        let clause = |i: Identity| identity_clause(&i);
        assert_eq!(clause(identity(1, 1, 1, i64::MAX)), "GENERATED BY DEFAULT AS IDENTITY");
        assert_eq!(clause(Identity { always: true, type_name: "integer".into(), ..identity(1, 1, 1, i64::from(i32::MAX)) }), "GENERATED ALWAYS AS IDENTITY");
        assert_eq!(clause(Identity { type_name: "smallint".into(), ..identity(-1, -1, i64::from(i16::MIN), -1) }), "GENERATED BY DEFAULT AS IDENTITY (INCREMENT BY -1)");
        assert_eq!(clause(Identity { cache: 20, cycle: true, ..identity(5, 1, 5, 100) }), "GENERATED BY DEFAULT AS IDENTITY (MINVALUE 5 MAXVALUE 100 CACHE 20 CYCLE)");
        assert_eq!(clause(identity(50, 1, 1, i64::MAX)), "GENERATED BY DEFAULT AS IDENTITY (START WITH 50)");
    }

    #[test]
    fn views_keep_their_options_check_option_and_population() {
        let v = View {
            schema: "public".into(),
            name: "paid".into(),
            options: vec!["security_barrier=true".into(), "check_option=cascaded".into()],
            query: " SELECT orders.id\n   FROM public.orders;".into(),
            comment: Some("paid only".into()),
            ..Default::default()
        };
        assert_eq!(
            view(&v),
            "CREATE VIEW public.paid WITH (security_barrier=true) AS\n SELECT orders.id\n   FROM public.orders\n  WITH CASCADED CHECK OPTION;\n\nCOMMENT ON VIEW public.paid IS 'paid only';\n"
        );
        let m = View {
            schema: "public".into(),
            name: "m".into(),
            materialized: true,
            query: " SELECT 1 AS day;".into(),
            indexes: vec!["CREATE UNIQUE INDEX m_day ON public.m USING btree (day)".into()],
            ..Default::default()
        };
        assert_eq!(view(&m), "CREATE MATERIALIZED VIEW public.m AS\n SELECT 1 AS day\nWITH NO DATA;\n\nCREATE UNIQUE INDEX m_day ON public.m USING btree (day);\n");
    }

    #[test]
    fn a_routine_ends_its_definition_and_comments_on_its_signature() {
        let def = "CREATE OR REPLACE PROCEDURE public.p(a integer)\n LANGUAGE sql\nAS $procedure$ SELECT 1 $procedure$\n";
        assert_eq!(
            routine(def, true, "public.p(a integer)", &Some("x".into())),
            "CREATE OR REPLACE PROCEDURE public.p(a integer)\n LANGUAGE sql\nAS $procedure$ SELECT 1 $procedure$;\n\nCOMMENT ON PROCEDURE public.p(a integer) IS 'x';\n"
        );
    }

    #[test]
    fn only_a_top_level_on_only_is_dropped() {
        assert_eq!(index(r#"CREATE INDEX "x ON ONLY y" ON ONLY public.t USING btree (a)"#), r#"CREATE INDEX "x ON ONLY y" ON public.t USING btree (a)"#);
        assert_eq!(index("CREATE INDEX i ON public.t USING btree (a)"), "CREATE INDEX i ON public.t USING btree (a)");
    }
}
