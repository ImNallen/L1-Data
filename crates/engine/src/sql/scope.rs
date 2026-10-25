//! What completion may offer at a cursor: the clause the cursor sits in and the relations in
//! scope, read from the statement under the cursor with the dialect's own lexer.

use std::ops::Range;

use super::lex::{LexKind, lex};
use super::relations::{VETTED, VETTED_SIZES};
use super::split::split_statements;
use super::tsql;
use crate::model::{DatabaseKind, Expect, Qualifier, RelationSource, ScopeRelation, TableName};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Class {
    Word,
    Name,
    Str,
    LineComment,
    BlockComment,
    Number,
    Param,
    Open,
    Close,
    Comma,
    Dot,
    Semi,
    Star,
    Other,
}

#[derive(Debug, Clone)]
pub(super) struct Lexeme {
    pub class: Class,
    pub span: Range<usize>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Scope {
    /// A byte offset into the text.
    pub from: usize,
    pub expect: Expect,
    pub qualifier: Option<Qualifier>,
    pub relations: Vec<ScopeRelation>,
    pub ctes: Vec<String>,
    pub keywords: Vec<&'static str>,
    pub functions: Vec<&'static str>,
}

/// `None` inside a string, a comment, a quoted name, a number, or a parameter.
pub(crate) fn scope_at(text: &str, at: usize, kind: DatabaseKind) -> Option<Scope> {
    let lexemes = match kind {
        DatabaseKind::Postgres => postgres_statement(text, at),
        DatabaseKind::SqlServer => tsql_statement(text, at),
    };
    Walk::new(text, &lexemes, at, kind)?.run()
}

/// The statement `split_statements` puts the cursor in. A cursor past a statement's end belongs
/// to it until a `;`, so `FROM orders o JOIN customers c ON ` still reads as one statement.
fn postgres_statement(text: &str, at: usize) -> Vec<Lexeme> {
    let (start, end) = match split_statements(text).iter().rev().find(|s| s.start <= at) {
        Some(s) if at <= s.end => (s.start, s.end),
        Some(s) => {
            let gap = &text[s.end..at];
            let ws = gap.len() - gap.trim_start().len();
            if gap[ws..].starts_with(';') { (s.end + ws + 1, at) } else { (s.start, at) }
        }
        None => (0, at),
    };
    lex(&text[start..end])
        .into_iter()
        .filter_map(|l| {
            let class = match l.kind {
                LexKind::Space => return None,
                LexKind::LineComment => Class::LineComment,
                LexKind::BlockComment => Class::BlockComment,
                LexKind::Word => Class::Word,
                LexKind::QuotedIdent => Class::Name,
                LexKind::String | LexKind::DollarString => Class::Str,
                LexKind::Number => Class::Number,
                LexKind::Param => Class::Param,
                LexKind::Operator if &text[start + l.span.start..start + l.span.end] == "*" => Class::Star,
                LexKind::Dot => Class::Dot,
                LexKind::Comma => Class::Comma,
                LexKind::Open => Class::Open,
                LexKind::Close => Class::Close,
                LexKind::Semicolon => Class::Semi,
                LexKind::Operator | LexKind::OpenBracket | LexKind::CloseBracket | LexKind::Other => Class::Other,
            };
            Some(Lexeme { class, span: start + l.span.start..start + l.span.end })
        })
        .collect()
}

/// The statement around the cursor inside its `GO` batch. T-SQL needs no `;` between statements,
/// so one starts at a `;` or at a statement verb that cannot continue the statement before it.
fn tsql_statement(text: &str, at: usize) -> Vec<Lexeme> {
    let (lexemes, go) = tsql::completion_lexemes(text);
    let lo = go.iter().filter(|g| g.end <= at).map(|g| g.end).max().unwrap_or(0);
    let hi = go.iter().filter(|g| g.start >= at).map(|g| g.start).min().unwrap_or(text.len());
    let batch: Vec<Lexeme> = lexemes.into_iter().filter(|l| l.span.start >= lo && l.span.start < hi).collect();
    let starts = tsql_statement_starts(text, &batch);
    let start = starts.iter().copied().filter(|&s| s <= at).max().unwrap_or(lo);
    let end = starts.iter().copied().filter(|&s| s > at).min().unwrap_or(hi);
    batch.into_iter().filter(|l| l.span.start >= start && l.span.start < end).collect()
}

fn tsql_statement_starts(text: &str, batch: &[Lexeme]) -> Vec<usize> {
    let word = |l: &Lexeme| (l.class == Class::Word).then(|| text[l.span.clone()].to_ascii_uppercase());
    let mut starts = Vec::new();
    let mut depth = 0usize;
    let mut first: Option<String> = None;
    let mut main_verb = false;
    let mut values = false;
    let mut update = false;
    let mut prev: Option<&Lexeme> = None;
    for l in batch.iter().filter(|l| !matches!(l.class, Class::LineComment | Class::BlockComment)) {
        match l.class {
            Class::Open => depth += 1,
            Class::Close => depth = depth.saturating_sub(1),
            Class::Semi if depth == 0 => {
                starts.push(l.span.end);
                (first, main_verb, values, update, prev) = (None, false, false, false, None);
                continue;
            }
            Class::Word if depth == 0 => {
                let w = word(l).unwrap_or_default();
                let prev_word = prev.and_then(word);
                let after_comma = prev.is_some_and(|p| p.class == Class::Comma);
                let continues = after_comma
                    || prev_word.as_deref().is_some_and(|p| {
                        ["UNION", "ALL", "EXCEPT", "INTERSECT", "ON", "FOR", "AFTER", "OF", "THEN", "INSTEAD", "OR", "NOT", "GRANT", "DENY", "REVOKE"].contains(&p)
                    });
                let cte_body = first.as_deref() == Some("WITH") && !main_verb;
                let insert_source = first.as_deref() == Some("INSERT") && !values;
                let starts_new = first.is_some()
                    && !continues
                    && match w.as_str() {
                        "SELECT" => !cte_body && !insert_source,
                        "INSERT" | "UPDATE" | "DELETE" | "MERGE" => !cte_body,
                        "SET" => !update,
                        "EXEC" | "EXECUTE" => !insert_source,
                        "IF" => !matches!(first.as_deref(), Some("DROP" | "ALTER" | "CREATE")),
                        "DECLARE" | "PRINT" | "WHILE" | "RETURN" | "BEGIN" | "CREATE" | "ALTER" | "DROP" | "TRUNCATE" | "THROW" | "RAISERROR" | "USE" | "GRANT" | "DENY" | "REVOKE" => true,
                        _ => false,
                    };
                if starts_new {
                    starts.push(l.span.start);
                    (first, main_verb, values, update) = (None, false, false, false);
                }
                if first.is_none() {
                    first = Some(w.clone());
                } else if first.as_deref() == Some("WITH") && ["SELECT", "INSERT", "UPDATE", "DELETE", "MERGE"].contains(&w.as_str()) {
                    main_verb = true;
                }
                values |= w == "VALUES";
                update |= w == "UPDATE";
            }
            _ => {}
        }
        prev = Some(l);
    }
    starts
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Role {
    From,
    Join,
    Using,
    Into,
    Update,
    Table,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Slot {
    /// A relation comes next.
    Expect,
    /// Right after a relation, where an alias or the next clause may follow.
    Named,
    /// After `AS`.
    As,
    Aliased,
    /// After `LEFT`, `CROSS`, and the like, before `JOIN`.
    JoinWord,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Clause {
    Start,
    Cte,
    Select,
    Relation { role: Role, slot: Slot },
    On { target: usize, fresh: bool },
    Where,
    GroupBy,
    Having,
    OrderBy,
    Limit,
    Set,
    Values,
    Returning,
    /// Only these keywords can come next: `BY` after `GROUP`, `INTO` after `INSERT`.
    Next(&'static [&'static str]),
    Other,
}

/// Where a clause sits in a query, for the clauses that can still follow it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Stage {
    From,
    Where,
    GroupBy,
    Having,
    OrderBy,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Origin {
    Statement,
    /// A subquery in a FROM list.
    Derived,
    CteBody,
    CteColumns,
    Other,
}

#[derive(Debug, Clone)]
enum Source {
    Table { schema: Option<String>, name: String },
    Derived { columns: Vec<String>, star: Vec<TableName> },
}

#[derive(Debug, Clone)]
struct Rel {
    /// The alias or name, as written.
    written: Option<String>,
    /// The alias or name, folded, that a qualifier matches.
    key: Option<String>,
    source: Source,
}

#[derive(Debug, Clone)]
struct Cte {
    key: String,
    written: String,
    columns: Vec<String>,
    star: Vec<TableName>,
}

enum Item {
    Named(String),
    Star(Option<String>),
    Unnamed,
}

struct Frame {
    parent: Option<usize>,
    query: bool,
    origin: Origin,
    clause: Clause,
    rels: Vec<Rel>,
    ctes: Vec<Cte>,
    items: Vec<Item>,
    /// This frame's own lexemes in the select item being read, by index into `Walk::sig`.
    item: Vec<usize>,
    /// The Word and Name lexemes directly inside a group, for a CTE's column list.
    names: Vec<String>,
    pending_cte: Option<(String, String, Option<Vec<String>>)>,
    /// A set operator after the cursor ends the part of the query the cursor is in.
    sealed: bool,
    seen: bool,
}

impl Frame {
    fn new(parent: Option<usize>, query: bool, origin: Origin) -> Self {
        Frame {
            parent,
            query,
            origin,
            clause: if query { Clause::Start } else { Clause::Other },
            rels: Vec::new(),
            ctes: Vec::new(),
            items: Vec::new(),
            item: Vec::new(),
            names: Vec::new(),
            pending_cte: None,
            sealed: false,
            seen: false,
        }
    }
}

struct Snapshot {
    frame: usize,
    clause: Clause,
    seen: bool,
}

struct Walk<'a> {
    text: &'a str,
    kind: DatabaseKind,
    sig: Vec<&'a Lexeme>,
    from: usize,
    /// The first lexeme of the qualifier, or of the word being typed: where the cursor's context is read.
    context: usize,
    /// The qualifier and the word being typed, which the walk skips.
    skip: Range<usize>,
    parts: Vec<String>,
    frames: Vec<Frame>,
    stack: Vec<usize>,
    snapshot: Option<Snapshot>,
}

impl<'a> Walk<'a> {
    fn new(text: &'a str, lexemes: &'a [Lexeme], at: usize, kind: DatabaseKind) -> Option<Self> {
        for l in lexemes.iter().filter(|l| l.span.start < at && at <= l.span.end) {
            let inside = at < l.span.end;
            let open = || !closed(kind, &text[l.span.clone()]);
            let blocked = match l.class {
                Class::LineComment | Class::Number | Class::Param => true,
                Class::Str | Class::Name | Class::BlockComment => inside || open(),
                _ => false,
            };
            if blocked {
                return None;
            }
        }
        let sig: Vec<&Lexeme> = lexemes.iter().filter(|l| !matches!(l.class, Class::LineComment | Class::BlockComment)).collect();
        let typed = sig.iter().position(|l| l.class == Class::Word && l.span.start < at && at <= l.span.end);
        let (from, word_at) = match typed {
            Some(p) => (sig[p].span.start, p),
            None => (at, sig.partition_point(|l| l.span.end <= at)),
        };
        let mut context = word_at;
        let mut parts = Vec::new();
        while context >= 1 && sig[context - 1].class == Class::Dot {
            if context < 2 || !matches!(sig[context - 2].class, Class::Word | Class::Name) {
                return None;
            }
            parts.insert(0, fold(kind, text, sig[context - 2]));
            context -= 2;
        }
        let skip = context..word_at + usize::from(typed.is_some());
        Some(Walk { text, kind, sig, from, context, skip, parts, frames: vec![Frame::new(None, true, Origin::Statement)], stack: vec![0], snapshot: None })
    }

    fn word(&self, i: usize) -> Option<String> {
        let l = self.sig.get(i)?;
        (l.class == Class::Word && !self.skip.contains(&i)).then(|| self.text[l.span.clone()].to_ascii_uppercase())
    }

    fn is_word(&self, i: usize, w: &str) -> bool {
        self.word(i).as_deref() == Some(w)
    }

    fn top(&mut self) -> &mut Frame {
        let id = *self.stack.last().expect("the statement frame is never popped");
        &mut self.frames[id]
    }

    fn run(mut self) -> Option<Scope> {
        let mut i = 0;
        while i < self.sig.len() {
            if i == self.context {
                self.take_snapshot();
            }
            if self.skip.contains(&i) {
                i += 1;
                continue;
            }
            if i < self.context {
                self.top().seen = true;
            }
            i = self.step(i);
        }
        if self.snapshot.is_none() {
            self.take_snapshot();
        }
        Some(self.finish())
    }

    fn take_snapshot(&mut self) {
        let frame = *self.stack.last().unwrap();
        let f = &self.frames[frame];
        self.snapshot = Some(Snapshot { frame, clause: f.clause, seen: f.seen });
    }

    fn step(&mut self, i: usize) -> usize {
        let class = self.sig[i].class;
        match class {
            Class::Open => {
                self.open(i);
                return i + 1;
            }
            Class::Close => {
                self.close(i);
                return i + 1;
            }
            _ => {}
        }
        let id = *self.stack.last().unwrap();
        if !self.frames[id].query {
            if matches!(class, Class::Word | Class::Name) {
                let name = fold(self.kind, self.text, self.sig[i]);
                self.frames[id].names.push(name);
            }
            return i + 1;
        }
        if let Clause::On { target, fresh: true } = self.frames[id].clause {
            self.frames[id].clause = Clause::On { target, fresh: false };
        }
        if let Some(w) = self.word(i) {
            if let Some(next) = self.keyword(i, &w) {
                return next;
            }
        }
        let clause = self.frames[id].clause;
        match (clause, class) {
            (Clause::Select, Class::Comma) => self.end_item(),
            (Clause::Select, _) => self.top().item.push(i),
            (Clause::Relation { role, .. }, Class::Comma) if matches!(role, Role::From | Role::Join | Role::Using) => {
                self.top().clause = Clause::Relation { role, slot: Slot::Expect };
            }
            (Clause::Relation { role, slot: Slot::Expect }, Class::Word | Class::Name) => return self.relation(i, role),
            (Clause::Relation { role, slot: Slot::Named | Slot::As }, Class::Word | Class::Name) => {
                let written = self.text[self.sig[i].span.clone()].to_owned();
                let key = fold(self.kind, self.text, self.sig[i]);
                let f = self.top();
                if let Some(rel) = f.rels.last_mut() {
                    rel.written = Some(written);
                    rel.key = Some(key);
                }
                f.clause = Clause::Relation { role, slot: Slot::Aliased };
            }
            (Clause::Cte, Class::Word | Class::Name) if !self.word(i).is_some_and(|w| ["AS", "RECURSIVE", "NOT", "MATERIALIZED"].contains(&w.as_str())) => {
                let written = self.text[self.sig[i].span.clone()].to_owned();
                let key = fold(self.kind, self.text, self.sig[i]);
                self.top().pending_cte = Some((key, written, None));
            }
            _ => {}
        }
        i + 1
    }

    /// Handles a keyword that moves the frame to another clause. `None` when `w` is not one here.
    fn keyword(&mut self, i: usize, w: &str) -> Option<usize> {
        let clause = self.top().clause;
        let prev = i.checked_sub(1).and_then(|p| self.word(p));
        let next_opens = self.sig.get(i + 1).is_some_and(|l| l.class == Class::Open) && !self.skip.contains(&(i + 1));
        let named = matches!(clause, Clause::Relation { slot: Slot::Named | Slot::Aliased, .. });
        let in_from = matches!(clause, Clause::Relation { role: Role::From | Role::Join | Role::Using, .. } | Clause::On { .. });
        let to = match w {
            "SELECT" => Clause::Select,
            "FROM" if prev.as_deref() == Some("DISTINCT") => return Some(i + 1),
            "FROM" => Clause::Relation { role: Role::From, slot: Slot::Expect },
            "JOIN" | "APPLY" => Clause::Relation { role: Role::Join, slot: Slot::Expect },
            "LEFT" | "RIGHT" | "FULL" | "INNER" | "CROSS" | "NATURAL" | "OUTER" if in_from && !next_opens => {
                Clause::Relation { role: Role::Join, slot: Slot::JoinWord }
            }
            "ON" => match clause {
                Clause::Relation { role: Role::From | Role::Join | Role::Using, slot: Slot::Named | Slot::Aliased } if !self.top().rels.is_empty() => {
                    Clause::On { target: self.top().rels.len() - 1, fresh: true }
                }
                _ => Clause::Other,
            },
            "USING" if matches!(clause, Clause::Relation { role: Role::Join, .. }) && named => Clause::Other,
            "USING" => Clause::Relation { role: Role::Using, slot: Slot::Expect },
            "WHERE" => Clause::Where,
            "GROUP" | "ORDER" => Clause::Next(&["BY"]),
            "BY" if prev.as_deref() == Some("GROUP") => Clause::GroupBy,
            "BY" if prev.as_deref() == Some("ORDER") => Clause::OrderBy,
            "HAVING" => Clause::Having,
            "LIMIT" | "OFFSET" | "FETCH" => Clause::Limit,
            "WINDOW" => Clause::Other,
            "UNION" | "INTERSECT" | "EXCEPT" => {
                let snapshot_taken = self.snapshot.is_some();
                let f = self.top();
                if snapshot_taken {
                    f.sealed = true;
                } else {
                    f.rels.clear();
                }
                Clause::Start
            }
            "RETURNING" | "OUTPUT" => Clause::Returning,
            "SET" => Clause::Set,
            "VALUES" => Clause::Values,
            "INTO" => Clause::Relation { role: Role::Into, slot: Slot::Expect },
            "UPDATE" if matches!(prev.as_deref(), Some("FOR" | "ON")) => Clause::Other,
            "UPDATE" => Clause::Relation { role: Role::Update, slot: Slot::Expect },
            "TABLE" => Clause::Relation { role: Role::Table, slot: Slot::Expect },
            "INSERT" | "MERGE" => Clause::Next(&["INTO"]),
            "DELETE" => Clause::Next(&["FROM"]),
            "WITH" if clause == Clause::Start => Clause::Cte,
            "WITH" if named && next_opens => {
                let skip_to = self.matching_close(i + 1);
                return Some(skip_to + 1);
            }
            "AS" => {
                if let Clause::Relation { role, slot: Slot::Named } = clause {
                    self.top().clause = Clause::Relation { role, slot: Slot::As };
                    return Some(i + 1);
                }
                return None;
            }
            "LATERAL" | "ONLY" if matches!(clause, Clause::Relation { slot: Slot::Expect, .. }) => return Some(i + 1),
            _ => return None,
        };
        if clause == Clause::Select {
            self.end_item();
        }
        self.top().clause = to;
        Some(i + 1)
    }

    fn matching_close(&self, open: usize) -> usize {
        let mut depth = 0usize;
        for (j, l) in self.sig.iter().enumerate().skip(open) {
            match l.class {
                Class::Open => depth += 1,
                Class::Close => {
                    depth -= 1;
                    if depth == 0 {
                        return j;
                    }
                }
                _ => {}
            }
        }
        self.sig.len()
    }

    /// Reads `name`, `schema.name`, or `db.schema.name`, or a table function, at `i`.
    fn relation(&mut self, i: usize, role: Role) -> usize {
        let mut parts = vec![i];
        let mut j = i + 1;
        while j + 1 < self.sig.len()
            && self.sig[j].class == Class::Dot
            && matches!(self.sig[j + 1].class, Class::Word | Class::Name)
            && !self.skip.contains(&j)
            && !self.skip.contains(&(j + 1))
        {
            parts.push(j + 1);
            j += 2;
        }
        let last = *parts.last().unwrap();
        let written = self.text[self.sig[last].span.clone()].to_owned();
        let name = fold(self.kind, self.text, self.sig[last]);
        let schema = (parts.len() >= 2).then(|| fold(self.kind, self.text, self.sig[parts[parts.len() - 2]]));
        let function = matches!(role, Role::From | Role::Join | Role::Using) && self.sig.get(j).is_some_and(|l| l.class == Class::Open) && !self.skip.contains(&j);
        let source = if function {
            Source::Derived { columns: Vec::new(), star: Vec::new() }
        } else if let (None, Some(cte)) = (&schema, self.cte(&name)) {
            Source::Derived { columns: cte.columns, star: cte.star }
        } else {
            Source::Table { schema, name: name.clone() }
        };
        let f = self.top();
        if !f.sealed {
            f.rels.push(Rel { written: Some(written), key: Some(name), source });
        }
        f.clause = Clause::Relation { role, slot: Slot::Named };
        j
    }

    fn cte(&self, name: &str) -> Option<Cte> {
        let mut id = Some(*self.stack.last().unwrap());
        while let Some(f) = id {
            if let Some(c) = self.frames[f].ctes.iter().find(|c| same(self.kind, &c.key, name)) {
                return Some(c.clone());
            }
            id = self.frames[f].parent;
        }
        None
    }

    fn open(&mut self, i: usize) {
        let parent = *self.stack.last().unwrap();
        let first = (i + 1..self.sig.len()).find(|j| !self.skip.contains(j));
        let query = first.is_some_and(|j| matches!(self.word(j).as_deref(), Some("SELECT" | "WITH" | "VALUES")));
        let clause = self.frames[parent].clause;
        let origin = match clause {
            Clause::Relation { slot: Slot::Expect, .. } if query => Origin::Derived,
            Clause::Cte if self.frames[parent].pending_cte.is_some() => {
                let as_before = i.checked_sub(1).is_some_and(|p| self.is_word(p, "AS") || self.is_word(p, "MATERIALIZED"));
                if as_before && query { Origin::CteBody } else { Origin::CteColumns }
            }
            _ => Origin::Other,
        };
        if let Clause::Relation { role, slot: Slot::Expect } = clause {
            if query {
                self.frames[parent].clause = Clause::Relation { role, slot: Slot::Named };
            }
        }
        if self.frames[parent].clause == Clause::Select {
            self.frames[parent].item.push(i);
        }
        self.frames.push(Frame::new(Some(parent), query, origin));
        self.stack.push(self.frames.len() - 1);
    }

    fn close(&mut self, i: usize) {
        if self.stack.len() == 1 {
            return;
        }
        let id = self.stack.pop().unwrap();
        let parent = *self.stack.last().unwrap();
        if self.frames[id].query && self.frames[id].clause == Clause::Select {
            let saved = self.stack.clone();
            self.stack.push(id);
            self.end_item();
            self.stack = saved;
        }
        match self.frames[id].origin {
            Origin::Derived => {
                let (columns, star) = self.output(id);
                if !self.frames[parent].sealed {
                    self.frames[parent].rels.push(Rel { written: None, key: None, source: Source::Derived { columns, star } });
                }
            }
            Origin::CteColumns => {
                let names = std::mem::take(&mut self.frames[id].names);
                if let Some(p) = self.frames[parent].pending_cte.as_mut() {
                    p.2 = Some(names);
                }
            }
            Origin::CteBody => {
                let (mut columns, mut star) = self.output(id);
                if let Some((key, written, explicit)) = self.frames[parent].pending_cte.take() {
                    if let Some(explicit) = explicit {
                        (columns, star) = (explicit, Vec::new());
                    }
                    self.frames[parent].ctes.push(Cte { key, written, columns, star });
                }
            }
            Origin::Statement | Origin::Other => {}
        }
        if self.frames[parent].clause == Clause::Select {
            self.frames[parent].item.push(i);
        }
    }

    fn end_item(&mut self) {
        let f = self.top();
        let item = std::mem::take(&mut f.item);
        let class = |k: usize| self.sig[item[k]].class;
        let n = item.len();
        let named = |k: usize| matches!(class(k), Class::Word | Class::Name);
        let parsed = match n {
            0 => return,
            _ if class(n - 1) == Class::Star && n == 1 => Item::Star(None),
            _ if class(n - 1) == Class::Star && n >= 3 && class(n - 2) == Class::Dot && named(n - 3) => Item::Star(Some(fold(self.kind, self.text, self.sig[item[n - 3]]))),
            _ if named(n - 1) && !self.is_value_word(item[n - 1]) => Item::Named(fold(self.kind, self.text, self.sig[item[n - 1]])),
            _ => Item::Unnamed,
        };
        self.top().items.push(parsed);
    }

    fn is_value_word(&self, i: usize) -> bool {
        self.word(i).is_some_and(|w| ["END", "NULL", "TRUE", "FALSE", "DISTINCT", "ALL"].contains(&w.as_str()))
    }

    /// The columns a query frame's select list names, and the tables a `*` stands for.
    fn output(&self, id: usize) -> (Vec<String>, Vec<TableName>) {
        let f = &self.frames[id];
        let mut columns = Vec::new();
        let mut star = Vec::new();
        let expand = |rel: &Rel, columns: &mut Vec<String>, star: &mut Vec<TableName>| match &rel.source {
            Source::Table { schema, name } => star.push(TableName { schema: schema.clone(), name: name.clone() }),
            Source::Derived { columns: c, star: s } => {
                columns.extend(c.iter().cloned());
                star.extend(s.iter().cloned());
            }
        };
        for item in &f.items {
            match item {
                Item::Named(n) => columns.push(n.clone()),
                Item::Star(None) => f.rels.iter().for_each(|r| expand(r, &mut columns, &mut star)),
                Item::Star(Some(q)) => {
                    if let Some(r) = f.rels.iter().find(|r| r.key.as_deref().is_some_and(|k| same(self.kind, k, q))) {
                        expand(r, &mut columns, &mut star);
                    }
                }
                Item::Unnamed => {}
            }
        }
        (columns, star)
    }

    fn finish(self) -> Scope {
        let snap = self.snapshot.as_ref().expect("run takes a snapshot");
        let mut chain = Vec::new();
        let mut id = Some(snap.frame);
        while let Some(f) = id {
            chain.push(f);
            id = self.frames[f].parent;
        }
        let rels: Vec<&Rel> = chain.iter().flat_map(|&f| self.frames[f].rels.iter()).collect();
        let relations = rels
            .iter()
            .map(|r| ScopeRelation {
                reference: r.written.clone(),
                source: match &r.source {
                    Source::Table { schema, name } => RelationSource::Table { schema: schema.clone(), name: name.clone() },
                    Source::Derived { columns, star } => RelationSource::Derived { columns: columns.clone(), star: star.clone() },
                },
            })
            .collect();
        let ctes = chain.iter().flat_map(|&f| self.frames[f].ctes.iter().map(|c| c.written.clone())).collect();
        let qualifier = (!self.parts.is_empty()).then(|| self.qualifier(&rels));
        let frame = &self.frames[snap.frame];
        let (expect, keywords) = if frame.query { self.expect(snap, frame) } else { self.expect_in_group(snap, frame) };
        let functions = match expect {
            Expect::Expression | Expect::JoinCondition { .. } => match self.kind {
                DatabaseKind::Postgres => VETTED.iter().chain(VETTED_SIZES).copied().collect(),
                DatabaseKind::SqlServer => TSQL_FUNCTIONS.to_vec(),
            },
            Expect::Keyword | Expect::Relation => Vec::new(),
        };
        let mut keywords = keywords;
        let mut seen = std::collections::HashSet::new();
        keywords.retain(|k| seen.insert(*k));
        Scope { from: self.from, expect, qualifier, relations, ctes, keywords, functions }
    }

    fn qualifier(&self, rels: &[&Rel]) -> Qualifier {
        let index = match self.parts.as_slice() {
            [one] => rels.iter().position(|r| r.key.as_deref().is_some_and(|k| same(self.kind, k, one))),
            [schema, name] => rels.iter().position(|r| {
                r.written.is_some()
                    && matches!(&r.source, Source::Table { schema: Some(s), name: n } if same(self.kind, s, schema) && same(self.kind, n, name))
                    && r.key.as_deref().is_some_and(|k| same(self.kind, k, name))
            }),
            _ => None,
        };
        match index.and_then(|i| u16::try_from(i).ok()) {
            Some(index) => Qualifier::Relation { index },
            None => Qualifier::Name { parts: self.parts.clone() },
        }
    }

    fn expect(&self, snap: &Snapshot, frame: &Frame) -> (Expect, Vec<&'static str>) {
        let pg = self.kind == DatabaseKind::Postgres;
        let expression = |mut kw: Vec<&'static str>| {
            kw.extend(EXPRESSION_WORDS);
            kw.extend(if pg { PG_EXPRESSION_WORDS } else { TSQL_EXPRESSION_WORDS });
            (Expect::Expression, kw)
        };
        match snap.clause {
            Clause::Start if frame.origin == Origin::Statement && !snap.seen => {
                (Expect::Keyword, if pg { PG_STATEMENTS.to_vec() } else { TSQL_STATEMENTS.to_vec() })
            }
            Clause::Start => (Expect::Keyword, vec!["SELECT"]),
            Clause::Cte => {
                let after_body = frame.pending_cte.is_none() && !frame.ctes.is_empty();
                (Expect::Keyword, if after_body { MAIN_VERBS.to_vec() } else { vec!["AS"] })
            }
            Clause::Select => {
                let mut kw = vec!["FROM", "AS", "DISTINCT"];
                if !pg {
                    kw.extend(["TOP", "INTO"]);
                }
                expression(kw)
            }
            Clause::Relation { slot: Slot::Expect, .. } => (Expect::Relation, Vec::new()),
            Clause::Relation { slot: Slot::As, .. } => (Expect::Keyword, Vec::new()),
            Clause::Relation { slot: Slot::JoinWord, .. } => (Expect::Keyword, vec!["JOIN", "OUTER JOIN"]),
            Clause::Relation { role, slot: Slot::Named | Slot::Aliased } => {
                let kw = match role {
                    Role::Join => [if pg { &["ON", "USING"][..] } else { &["ON"][..] }, &self.after_from()].concat(),
                    Role::From | Role::Using => self.after_from(),
                    Role::Into => vec!["VALUES", "SELECT", "DEFAULT VALUES"],
                    Role::Update => vec!["SET"],
                    Role::Table => Vec::new(),
                };
                (Expect::Keyword, kw)
            }
            Clause::On { target, fresh } => {
                let (_, kw) = expression([&["AND", "OR"][..], &self.after_from()].concat());
                match u16::try_from(target) {
                    Ok(target) if fresh => (Expect::JoinCondition { target }, kw),
                    _ => (Expect::Expression, kw),
                }
            }
            Clause::Where => {
                let mut kw = vec!["AND", "OR", "NOT", "IN", "IS NULL", "IS NOT NULL", "LIKE", "BETWEEN", "EXISTS"];
                if pg {
                    kw.push("ILIKE");
                }
                kw.extend(self.clauses_after(Stage::Where));
                expression(kw)
            }
            Clause::GroupBy => expression(self.clauses_after(Stage::GroupBy)),
            Clause::Having => expression([&["AND", "OR"][..], &self.clauses_after(Stage::Having)].concat()),
            Clause::OrderBy => {
                let mut kw = vec!["ASC", "DESC"];
                if pg {
                    kw.extend(["NULLS FIRST", "NULLS LAST"]);
                }
                kw.extend(self.clauses_after(Stage::OrderBy));
                expression(kw)
            }
            Clause::Set => expression(if pg { vec!["WHERE", "FROM", "RETURNING"] } else { vec!["WHERE", "FROM", "OUTPUT"] }),
            Clause::Values | Clause::Returning => expression(Vec::new()),
            Clause::Next(words) => (Expect::Keyword, words.to_vec()),
            Clause::Limit | Clause::Other => (Expect::Keyword, Vec::new()),
        }
    }

    fn expect_in_group(&self, snap: &Snapshot, frame: &Frame) -> (Expect, Vec<&'static str>) {
        let outer = frame.parent.map(|p| &self.frames[p]);
        let lead: Vec<&'static str> = if snap.seen { Vec::new() } else { vec!["SELECT"] };
        match (frame.origin, outer.map(|o| o.clause)) {
            (Origin::CteColumns, _) => (Expect::Keyword, Vec::new()),
            (_, Some(Clause::Relation { slot: Slot::Expect, .. })) => (Expect::Relation, lead),
            (_, Some(Clause::Cte)) => (Expect::Keyword, lead),
            _ => {
                let mut kw = lead;
                kw.extend(EXPRESSION_WORDS);
                kw.extend(if self.kind == DatabaseKind::Postgres { PG_EXPRESSION_WORDS } else { TSQL_EXPRESSION_WORDS });
                (Expect::Expression, kw)
            }
        }
    }

    fn after_from(&self) -> Vec<&'static str> {
        let mut kw = vec!["JOIN", "LEFT JOIN", "INNER JOIN", "RIGHT JOIN", "FULL JOIN", "CROSS JOIN"];
        if self.kind == DatabaseKind::SqlServer {
            kw.extend(["CROSS APPLY", "OUTER APPLY"]);
        }
        kw.extend(self.clauses_after(Stage::From));
        kw
    }

    fn clauses_after(&self, stage: Stage) -> Vec<&'static str> {
        let pg = self.kind == DatabaseKind::Postgres;
        let mut kw: Vec<&'static str> = [(Stage::Where, "WHERE"), (Stage::GroupBy, "GROUP BY"), (Stage::Having, "HAVING"), (Stage::OrderBy, "ORDER BY")]
            .into_iter()
            .filter(|(s, _)| *s > stage)
            .map(|(_, k)| k)
            .collect();
        if pg {
            kw.extend(["LIMIT", "OFFSET"]);
        } else if stage == Stage::OrderBy {
            kw.push("OFFSET");
        }
        if stage < Stage::OrderBy {
            kw.extend(["UNION", "UNION ALL", "INTERSECT", "EXCEPT"]);
        }
        kw
    }
}

fn closed(kind: DatabaseKind, lexeme: &str) -> bool {
    match kind {
        DatabaseKind::Postgres => lex(&format!("{lexeme} x")).first().is_some_and(|l| l.span.end == lexeme.len()),
        DatabaseKind::SqlServer => tsql::lexeme_closed(lexeme),
    }
}

/// A name as the server resolves it: PostgreSQL folds an unquoted name to lower case, and a quoted
/// name keeps its case with its doubled quotes undone.
fn fold(kind: DatabaseKind, text: &str, l: &Lexeme) -> String {
    let raw = &text[l.span.clone()];
    if l.class != Class::Name {
        return if kind == DatabaseKind::Postgres { raw.to_lowercase() } else { raw.to_owned() };
    }
    let raw = raw.strip_prefix("U&").or_else(|| raw.strip_prefix("u&")).unwrap_or(raw);
    let (close, doubled) = if raw.starts_with('[') { (']', "]]") } else { ('"', "\"\"") };
    let inner = &raw[1..];
    let inner = inner.strip_suffix(close).unwrap_or(inner);
    inner.replace(doubled, &doubled[..1])
}

fn same(kind: DatabaseKind, a: &str, b: &str) -> bool {
    match kind {
        DatabaseKind::Postgres => a == b,
        DatabaseKind::SqlServer => a.to_lowercase() == b.to_lowercase(),
    }
}

const PG_STATEMENTS: &[&str] = &[
    "SELECT", "WITH", "INSERT INTO", "UPDATE", "DELETE FROM", "VALUES", "TABLE", "EXPLAIN", "CREATE", "ALTER", "DROP", "TRUNCATE", "BEGIN", "COMMIT", "ROLLBACK", "SET", "SHOW",
    "GRANT", "REVOKE", "COPY", "VACUUM", "ANALYZE",
];

const TSQL_STATEMENTS: &[&str] = &[
    "SELECT", "WITH", "INSERT INTO", "UPDATE", "DELETE FROM", "MERGE INTO", "DECLARE", "SET", "EXEC", "IF", "BEGIN", "BEGIN TRANSACTION", "COMMIT", "ROLLBACK", "CREATE", "ALTER", "DROP",
    "TRUNCATE TABLE", "PRINT", "GRANT", "REVOKE",
];

const MAIN_VERBS: &[&str] = &["SELECT", "INSERT INTO", "UPDATE", "DELETE FROM"];

const EXPRESSION_WORDS: &[&str] = &["CASE", "WHEN", "THEN", "ELSE", "END", "AND", "OR", "NOT", "NULL", "IS", "IN", "LIKE", "BETWEEN", "EXISTS", "CAST", "COALESCE", "NULLIF", "DISTINCT", "AS"];

const PG_EXPRESSION_WORDS: &[&str] = &["TRUE", "FALSE", "ILIKE", "GREATEST", "LEAST", "EXTRACT", "INTERVAL", "CURRENT_DATE", "CURRENT_TIMESTAMP"];

const TSQL_EXPRESSION_WORDS: &[&str] = &["CURRENT_TIMESTAMP"];

/// SQL Server's built-in scalar, aggregate, and window functions that a query commonly calls.
const TSQL_FUNCTIONS: &[&str] = &[
    "ABS", "ACOS", "APPROX_COUNT_DISTINCT", "ASCII", "ASIN", "ATAN", "ATN2", "AVG", "CEILING", "CHAR", "CHARINDEX", "CHECKSUM", "CHOOSE", "CONCAT", "CONCAT_WS", "CONVERT", "COS",
    "COT", "COUNT", "COUNT_BIG", "CUME_DIST", "DATALENGTH", "DATEADD", "DATEDIFF", "DATEDIFF_BIG", "DATEFROMPARTS", "DATENAME", "DATEPART", "DATETIME2FROMPARTS", "DATETRUNC",
    "DAY", "DB_NAME", "DEGREES", "DENSE_RANK", "DIFFERENCE", "EOMONTH", "ERROR_MESSAGE", "ERROR_NUMBER", "EXP", "FIRST_VALUE", "FLOOR", "FORMAT", "GETDATE", "GETUTCDATE",
    "GREATEST", "GROUPING", "HASHBYTES", "IIF", "ISDATE", "ISJSON", "ISNULL", "ISNUMERIC", "JSON_ARRAY", "JSON_MODIFY", "JSON_OBJECT", "JSON_QUERY", "JSON_VALUE", "LAG",
    "LAST_VALUE", "LEAD", "LEAST", "LEFT", "LEN", "LOG", "LOG10", "LOWER", "LTRIM", "MAX", "MIN", "MONTH", "NCHAR", "NEWID", "NTILE", "OBJECT_ID", "OBJECT_NAME", "PARSE",
    "PATINDEX", "PERCENT_RANK", "PERCENTILE_CONT", "PERCENTILE_DISC", "PI", "POWER", "QUOTENAME", "RADIANS", "RAND", "RANK", "REPLACE", "REPLICATE", "REVERSE", "RIGHT",
    "ROUND", "ROW_NUMBER", "RTRIM", "SCHEMA_NAME", "SCOPE_IDENTITY", "SIGN", "SIN", "SOUNDEX", "SPACE", "SQRT", "SQUARE", "STDEV", "STDEVP", "STR", "STRING_AGG",
    "STRING_SPLIT", "STUFF", "SUBSTRING", "SUM", "SWITCHOFFSET", "SYSDATETIME", "SYSDATETIMEOFFSET", "SYSUTCDATETIME", "TAN", "TODATETIMEOFFSET", "TRANSLATE", "TRIM",
    "TRY_CAST", "TRY_CONVERT", "TRY_PARSE", "UNICODE", "UPPER", "USER_NAME", "VAR", "VARP", "YEAR",
];

#[cfg(test)]
mod tests {
    use super::*;
    use DatabaseKind::{Postgres, SqlServer};

    fn at(kind: DatabaseKind, marked: &str) -> Option<Scope> {
        let cursor = marked.find('|').expect("a | marks the cursor");
        let text = marked.replacen('|', "", 1);
        scope_at(&text, cursor, kind)
    }

    fn pg(marked: &str) -> Scope {
        at(Postgres, marked).unwrap_or_else(|| panic!("{marked:?} offers nothing"))
    }

    fn ms(marked: &str) -> Scope {
        at(SqlServer, marked).unwrap_or_else(|| panic!("{marked:?} offers nothing"))
    }

    fn table(schema: Option<&str>, name: &str) -> RelationSource {
        RelationSource::Table { schema: schema.map(str::to_owned), name: name.to_owned() }
    }

    fn refs(scope: &Scope) -> Vec<(Option<&str>, &RelationSource)> {
        scope.relations.iter().map(|r| (r.reference.as_deref(), &r.source)).collect()
    }

    #[test]
    fn each_clause_expects_what_can_stand_there() {
        let cases: &[(&str, Expect)] = &[
            ("|", Expect::Keyword),
            ("sel|", Expect::Keyword),
            ("select |", Expect::Expression),
            ("select a, |", Expect::Expression),
            ("select * from |", Expect::Relation),
            ("select * from orders, |", Expect::Relation),
            ("select * from orders o join |", Expect::Relation),
            ("select * from orders o |", Expect::Keyword),
            ("select * from orders o left |", Expect::Keyword),
            ("select * from orders o join customers c on |", Expect::JoinCondition { target: 1 }),
            ("select * from orders o join customers c on c.id = |", Expect::Expression),
            ("select * from orders where |", Expect::Expression),
            ("select * from orders where a = 1 and |", Expect::Expression),
            ("select * from orders group |", Expect::Keyword),
            ("select * from orders group by |", Expect::Expression),
            ("select status from orders group by status having |", Expect::Expression),
            ("select * from orders order by |", Expect::Expression),
            ("insert into |", Expect::Relation),
            ("insert |", Expect::Keyword),
            ("update |", Expect::Relation),
            ("update orders set |", Expect::Expression),
            ("delete from orders where |", Expect::Expression),
            ("select * from orders as |", Expect::Keyword),
        ];
        for (marked, expect) in cases {
            assert_eq!(pg(marked).expect, *expect, "{marked:?}");
        }
    }

    #[test]
    fn keywords_name_the_clauses_that_can_follow() {
        assert_eq!(pg("|").keywords[..3], ["SELECT", "WITH", "INSERT INTO"]);
        assert_eq!(pg("select * from orders group |").keywords, ["BY"]);
        assert_eq!(pg("insert |").keywords, ["INTO"]);
        assert_eq!(pg("select * from orders o left |").keywords, ["JOIN", "OUTER JOIN"]);
        assert!(pg("select * from |").keywords.is_empty());
        let after_join = pg("select * from orders o join customers c |").keywords;
        assert_eq!(after_join[..2], ["ON", "USING"]);
        assert!(after_join.contains(&"WHERE") && after_join.contains(&"LIMIT"));
        let after_from = pg("select * from orders o |").keywords;
        assert!(!after_from.contains(&"ON"));
        assert!(after_from.contains(&"LEFT JOIN") && after_from.contains(&"GROUP BY"));
        let after_where = pg("select * from orders where a = 1 |").keywords;
        assert!(after_where.contains(&"AND") && after_where.contains(&"ORDER BY") && !after_where.contains(&"WHERE"));
        let after_order = pg("select * from orders order by id |").keywords;
        assert!(after_order.contains(&"DESC") && after_order.contains(&"LIMIT") && !after_order.contains(&"GROUP BY") && !after_order.contains(&"UNION"));
        assert_eq!(pg("update orders |").keywords, ["SET"]);
    }

    #[test]
    fn functions_come_only_where_an_expression_can_stand() {
        assert!(pg("select date_|").functions.contains(&"date_trunc"));
        assert!(pg("select * from orders where |").functions.contains(&"date_part"));
        assert!(pg("select * from |").functions.is_empty());
        assert!(pg("|").functions.is_empty());
        assert!(ms("SELECT DATE|").functions.contains(&"DATEADD"));
    }

    #[test]
    fn the_word_being_typed_starts_the_replaced_range() {
        assert_eq!(pg("select * from ord|").from, 14);
        assert_eq!(pg("select * from |").from, 14);
        assert_eq!(pg("select o.cu| from orders o").from, 9);
        assert_eq!(pg("select * from ord|ers").from, 14);
    }

    #[test]
    fn relations_carry_their_alias_as_written_and_their_folded_name() {
        let s = pg("SELECT * FROM Orders O JOIN public.\"Line Items\" AS li ON |");
        assert_eq!(refs(&s), [(Some("O"), &table(None, "orders")), (Some("li"), &table(Some("public"), "Line Items"))]);
        assert_eq!(s.expect, Expect::JoinCondition { target: 1 });
        let s = pg("select * from orders, lab.type_zoo where |");
        assert_eq!(refs(&s), [(Some("orders"), &table(None, "orders")), (Some("type_zoo"), &table(Some("lab"), "type_zoo"))]);
    }

    #[test]
    fn relations_after_the_cursor_are_in_scope() {
        let s = pg("select o.| from orders o join customers c on c.id = o.customer_id");
        assert_eq!(refs(&s), [(Some("o"), &table(None, "orders")), (Some("c"), &table(None, "customers"))]);
        assert_eq!(s.qualifier, Some(Qualifier::Relation { index: 0 }));
        assert_eq!(s.expect, Expect::Expression);
    }

    #[test]
    fn a_qualifier_names_a_relation_in_scope_or_falls_back_to_its_names() {
        assert_eq!(pg("select * from orders o where o.|").qualifier, Some(Qualifier::Relation { index: 0 }));
        assert_eq!(pg("select * from orders o where O.st|").qualifier, Some(Qualifier::Relation { index: 0 }));
        assert_eq!(pg("select * from orders where orders.|").qualifier, Some(Qualifier::Relation { index: 0 }));
        assert_eq!(pg("select * from public.orders where public.orders.|").qualifier, Some(Qualifier::Relation { index: 0 }));
        assert_eq!(pg("select * from orders o where orders.|").qualifier, Some(Qualifier::Name { parts: vec!["orders".into()] }));
        let s = pg("select * from Lab.|");
        assert_eq!((s.expect, s.qualifier), (Expect::Relation, Some(Qualifier::Name { parts: vec!["lab".into()] })));
        assert_eq!(pg("select * from \"Lab\".|").qualifier, Some(Qualifier::Name { parts: vec!["Lab".into()] }));
        assert!(refs(&pg("select * from lab.|")).is_empty());
    }

    #[test]
    fn a_cte_is_a_relation_with_the_columns_its_body_names() {
        let s = pg("with recent as (select * from orders) select * from rec|");
        assert_eq!((s.expect, s.ctes.clone()), (Expect::Relation, vec!["recent".to_owned()]));
        let s = pg("with recent as (select * from orders), totals(cust, n) as (select customer_id, count(*) from orders group by 1) select r.| from recent r, totals t");
        assert_eq!(
            refs(&s),
            [
                (Some("r"), &RelationSource::Derived { columns: vec![], star: vec![TableName { schema: None, name: "orders".into() }] }),
                (Some("t"), &RelationSource::Derived { columns: vec!["cust".into(), "n".into()], star: vec![] }),
            ]
        );
        assert_eq!(s.ctes, ["recent", "totals"]);
        let s = pg("with x as (select o.id, o.total as amount, count(*) n from orders o) select * from x where |");
        assert_eq!(refs(&s), [(Some("x"), &RelationSource::Derived { columns: vec!["id".into(), "amount".into(), "n".into()], star: vec![] })]);
        assert_eq!(pg("with recent as (select 1) |").keywords, MAIN_VERBS);
    }

    #[test]
    fn a_subquery_sees_its_own_relations_then_the_enclosing_ones() {
        let s = pg("select * from orders o where exists (select 1 from customers c where c.id = o.|)");
        assert_eq!(refs(&s), [(Some("c"), &table(None, "customers")), (Some("o"), &table(None, "orders"))]);
        assert_eq!(s.qualifier, Some(Qualifier::Relation { index: 1 }));
        let s = pg("select t.| from (select id, total from orders) t");
        assert_eq!(refs(&s), [(Some("t"), &RelationSource::Derived { columns: vec!["id".into(), "total".into()], star: vec![] })]);
        assert_eq!(s.qualifier, Some(Qualifier::Relation { index: 0 }));
        let s = pg("select * from orders o join (select * from customers c where c.|) x on true");
        assert_eq!(refs(&s)[0], (Some("c"), &table(None, "customers")));
        assert_eq!(pg("select * from (|").expect, Expect::Relation);
        assert_eq!(pg("select count(|) from orders").expect, Expect::Expression);
        assert_eq!(refs(&pg("insert into orders (|")), [(Some("orders"), &table(None, "orders"))]);
    }

    #[test]
    fn a_set_operation_scopes_each_part_to_its_own_relations() {
        assert_eq!(refs(&pg("select a from t1 union select b from t2 where t2.|")), [(Some("t2"), &table(None, "t2"))]);
        assert_eq!(refs(&pg("select t1.| from t1 union select b from t2")), [(Some("t1"), &table(None, "t1"))]);
    }

    #[test]
    fn nothing_is_offered_inside_strings_comments_quoted_names_or_numbers() {
        for marked in [
            "select 'ab|c' from orders",
            "select 'abc|",
            "select 'it''s|",
            "select * from orders -- note|",
            "select * from orders /* note| */",
            "select * from orders /* open|",
            "select $$ body|",
            "select \"Col|",
            "select 12|",
            "select $1|",
            "-- just a comment|",
            "select 1; -- after|",
        ] {
            assert!(at(Postgres, marked).is_none(), "{marked:?}");
        }
        assert!(at(Postgres, "select 'abc'|").is_some());
        assert!(at(Postgres, "select * from orders /* x */ |").is_some());
    }

    #[test]
    fn the_statement_under_the_cursor_is_the_one_read() {
        let s = pg("select * from customers;\nselect * from orders o where o.|");
        assert_eq!(refs(&s), [(Some("o"), &table(None, "orders"))]);
        let s = pg("select * from orders o where o.|;\nselect * from customers c");
        assert_eq!(refs(&s), [(Some("o"), &table(None, "orders"))]);
        let s = pg("select * from orders;\n|");
        assert_eq!((s.expect, s.relations.len()), (Expect::Keyword, 0));
        let s = pg("select * from orders o join customers c on\n  |");
        assert_eq!(s.expect, Expect::JoinCondition { target: 1 });
    }

    #[test]
    fn tsql_reads_brackets_and_splits_statements_without_semicolons() {
        let s = ms("SELECT * FROM dbo.[Order Lines] AS ol JOIN Sales.Customers c ON |");
        assert_eq!(refs(&s), [(Some("ol"), &table(Some("dbo"), "Order Lines")), (Some("c"), &table(Some("Sales"), "Customers"))]);
        assert_eq!(s.expect, Expect::JoinCondition { target: 1 });
        let s = ms("DECLARE @n int = 1\nSELECT * FROM customers c\nSELECT * FROM orders o WHERE o.|\nUPDATE orders SET total = 0");
        assert_eq!(refs(&s), [(Some("o"), &table(None, "orders"))]);
        assert_eq!(s.qualifier, Some(Qualifier::Relation { index: 0 }));
        let s = ms("SELECT * FROM Orders O WHERE o.|");
        assert_eq!(s.qualifier, Some(Qualifier::Relation { index: 0 }));
        let s = ms("SELECT * FROM customers\nGO\nSELECT * FROM orders WHERE |");
        assert_eq!(refs(&s), [(Some("orders"), &table(None, "orders"))]);
        let s = ms("INSERT INTO archive SELECT * FROM orders o WHERE o.|");
        assert_eq!(refs(&s).len(), 2);
        let s = ms("WITH r AS (SELECT * FROM orders) SELECT * FROM r WHERE r.|");
        assert_eq!(s.qualifier, Some(Qualifier::Relation { index: 0 }));
        let s = ms("SELECT * FROM orders o WITH (NOLOCK) JOIN customers c ON |");
        assert_eq!(s.expect, Expect::JoinCondition { target: 1 });
        assert!(ms("SELECT * FROM orders o |").keywords.contains(&"CROSS APPLY"));
        assert!(!ms("SELECT * FROM orders o |").keywords.contains(&"LIMIT"));
        assert!(ms("SELECT |").keywords.contains(&"TOP"));
    }

    #[test]
    fn tsql_offers_nothing_inside_its_own_quotes_and_comments() {
        for marked in ["SELECT N'ab|", "SELECT 'a''|", "SELECT [Col|", "SELECT 1 -- x|", "SELECT 1 /* a /* b */ c| */", "SELECT @na|"] {
            assert!(at(SqlServer, marked).is_none(), "{marked:?}");
        }
        assert!(at(SqlServer, "SELECT [Col]|").is_some());
    }
}
