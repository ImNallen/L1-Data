use super::{ByteTarget, DmlVerb, StmtKind, StmtSpan, relations::NameRef};
use crate::model::{ColumnFilter, FilterOp, TableQuery};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Word,
    Ident,
    String,
    Number,
    Variable,
    Symbol,
    LineComment,
    BlockComment,
}

#[derive(Debug, Clone, Copy)]
struct Token<'a> {
    kind: Kind,
    text: &'a str,
    start: usize,
    end: usize,
}

impl Token<'_> {
    fn word(&self, word: &str) -> bool {
        self.kind == Kind::Word && self.text.eq_ignore_ascii_case(word)
    }

    fn comment(&self) -> bool {
        matches!(self.kind, Kind::LineComment | Kind::BlockComment)
    }

    fn code(&self) -> bool {
        !self.comment() && self.text != ";"
    }

    fn name(&self) -> Option<String> {
        match self.kind {
            Kind::Word => Some(self.text.to_owned()),
            Kind::Ident if self.text.starts_with('[') => Some(self.text[1..self.text.len() - 1].replace("]]", "]")),
            Kind::Ident => Some(self.text[1..self.text.len() - 1].replace("\"\"", "\"")),
            _ => None,
        }
    }
}

fn word_start(ch: char) -> bool {
    ch.is_alphabetic() || matches!(ch, '_' | '@' | '#')
}

fn word_cont(ch: char) -> bool {
    word_start(ch) || ch.is_numeric() || ch == '$'
}

fn quoted_end(bytes: &[u8], start: usize, close: u8) -> Result<usize, String> {
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == close {
            if bytes.get(i + 1) == Some(&close) {
                i += 2;
            } else {
                return Ok(i + 1);
            }
        } else {
            i += 1;
        }
    }
    Err(format!("Unclosed T-SQL quote or bracket at byte {start}"))
}

fn lex(text: &str) -> Result<Vec<Token<'_>>, String> {
    if text.contains('\0') {
        return Err("NUL is not supported in T-SQL".into());
    }
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let ch = text[i..].chars().next().unwrap();
        if ch.is_whitespace() {
            i += ch.len_utf8();
            continue;
        }
        let start = i;
        let kind = match bytes[i] {
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                i += 2;
                while i < bytes.len() && !matches!(bytes[i], b'\r' | b'\n') {
                    i += 1;
                }
                Kind::LineComment
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                let mut depth = 1usize;
                while i < bytes.len() && depth > 0 {
                    if bytes[i..].starts_with(b"/*") {
                        depth += 1;
                        i += 2;
                    } else if bytes[i..].starts_with(b"*/") {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                if depth != 0 {
                    return Err(format!("Unclosed T-SQL block comment at byte {start}"));
                }
                Kind::BlockComment
            }
            b'*' if bytes.get(i + 1) == Some(&b'/') => {
                return Err(format!("Unexpected comment terminator at byte {i}"));
            }
            b'\'' => {
                i = quoted_end(bytes, i, b'\'')?;
                Kind::String
            }
            b'N' | b'n' if bytes.get(i + 1) == Some(&b'\'') => {
                i = quoted_end(bytes, i + 1, b'\'')?;
                Kind::String
            }
            b'[' | b'"' => {
                i = quoted_end(bytes, i, if bytes[i] == b'[' { b']' } else { b'"' })?;
                Kind::Ident
            }
            b'0' if matches!(bytes.get(i + 1), Some(b'x' | b'X')) => {
                i += 2;
                let digits = i;
                while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                    i += 1;
                }
                if i == digits {
                    return Err(format!("Incomplete T-SQL binary literal at byte {start}"));
                }
                Kind::Number
            }
            byte if byte.is_ascii_digit() || (byte == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)) => {
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if bytes.get(i) == Some(&b'.') {
                    i += 1;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                if matches!(bytes.get(i), Some(b'e' | b'E')) {
                    i += 1;
                    if matches!(bytes.get(i), Some(b'+' | b'-')) {
                        i += 1;
                    }
                    let digits = i;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                    if i == digits {
                        return Err(format!("Incomplete T-SQL exponent at byte {start}"));
                    }
                }
                Kind::Number
            }
            byte if word_start(ch) => {
                i += ch.len_utf8();
                while let Some(next) = text[i..].chars().next().filter(|ch| word_cont(*ch)) {
                    i += next.len_utf8();
                }
                if byte == b'@' { Kind::Variable } else { Kind::Word }
            }
            _ => {
                i += ch.len_utf8();
                if i < bytes.len()
                    && matches!(
                        &bytes[start..=i],
                        b"<="
                            | b">="
                            | b"<>"
                            | b"!="
                            | b"!<"
                            | b"!>"
                            | b"+="
                            | b"-="
                            | b"*="
                            | b"/="
                            | b"%="
                            | b"&="
                            | b"^="
                            | b"|="
                            | b"::"
                            | b"!!"
                    )
                {
                    i += 1;
                }
                Kind::Symbol
            }
        };
        out.push(Token { kind, text: &text[start..i], start, end: i });
    }
    Ok(out)
}

fn line_bounds(text: &str, at: usize) -> (usize, usize, usize) {
    let start = text[..at].rfind(['\r', '\n']).map_or(0, |i| i + 1);
    let end = text[at..].find(['\r', '\n']).map_or(text.len(), |i| at + i);
    let mut after = end;
    if text.as_bytes().get(after) == Some(&b'\r') {
        after += 1;
    }
    if text.as_bytes().get(after) == Some(&b'\n') {
        after += 1;
    }
    (start, end, after)
}

fn separators(text: &str, tokens: &[Token<'_>]) -> Result<Vec<StmtSpan>, String> {
    let mut out = Vec::new();
    for (i, token) in tokens.iter().enumerate().filter(|(_, t)| !t.comment()) {
        if token.text.ends_with('$') && tokens.get(i + 1).is_some_and(|t| t.text == "(" && t.start == token.end) {
            return Err(format!("sqlcmd substitutions are unsupported at byte {}", token.start));
        }
        if !token.word("GO") && !matches!(token.text, ":" | "!!") {
            continue;
        }
        let (start, end, after) = line_bounds(text, token.start);
        let first_on_line = text[start..token.start].trim().is_empty();
        if first_on_line && matches!(token.text, ":" | "!!") {
            return Err(format!("sqlcmd directives are unsupported at byte {}", token.start));
        }
        if !first_on_line || !token.word("GO") {
            continue;
        }
        let mut separator_end = after;
        for next in tokens[i + 1..].iter().take_while(|t| t.start < end) {
            if !next.comment() {
                return Err("GO must be alone on its line. Repetition counts are unsupported".into());
            }
            separator_end = separator_end.max(next.end);
        }
        out.push(StmtSpan { start, end: separator_end });
    }
    Ok(out)
}

pub(crate) fn split_batches(text: &str) -> Result<Vec<StmtSpan>, String> {
    let tokens = lex(text)?;
    let separators = separators(text, &tokens)?;
    let mut spans = Vec::new();
    let mut start = 0;
    for separator in separators.into_iter().chain(std::iter::once(StmtSpan { start: text.len(), end: text.len() })) {
        let end = separator.start;
        let first = tokens.partition_point(|t| t.end <= start);
        if tokens[first..].iter().take_while(|t| t.start < end).any(Token::code) {
            let part = &text[start..end];
            let trimmed = part.trim();
            let leading = part.len() - part.trim_start().len();
            spans.push(StmtSpan { start: start + leading, end: start + leading + trimmed.len() });
        }
        start = separator.end;
    }
    Ok(spans)
}

pub(crate) fn validate_batch(text: &str) -> Result<(), String> {
    let tokens = lex(text)?;
    if !separators(text, &tokens)?.is_empty() {
        return Err("Expected one T-SQL batch without a GO separator".into());
    }
    if tokens.iter().any(|t| t.word("USE")) {
        return Err("USE is unsupported. Select the database through the console instead".into());
    }
    Ok(())
}

pub(crate) fn pick(text: &str, target: ByteTarget) -> Result<Vec<StmtSpan>, String> {
    let valid = |offset| text.is_char_boundary(offset);
    let picked = match target {
        ByteTarget::Selection { from, to } if from != to => {
            if from > to || !valid(from) || !valid(to) {
                return Err("Invalid T-SQL selection byte range".into());
            }
            Ok(split_batches(&text[from..to])?.into_iter().map(|s| StmtSpan { start: s.start + from, end: s.end + from }).collect())
        }
        ByteTarget::Selection { from, .. } | ByteTarget::Cursor(from) => {
            if !valid(from) {
                return Err("Invalid T-SQL cursor byte offset".into());
            }
            let spans = split_batches(text)?;
            let tokens = lex(text)?;
            let (line_start, line_end, _) = line_bounds(text, from);
            Ok(spans
                .into_iter()
                .filter(|span| {
                    let first = tokens.partition_point(|t| t.end <= span.start);
                    let mut code = tokens[first..].iter().take_while(|t| t.start < span.end).filter(|t| t.code());
                    let Some(first) = code.next() else {
                        return false;
                    };
                    let last = code.last().unwrap_or(first);
                    first.start <= line_end && last.end >= line_start
                })
                .take(1)
                .collect())
        }
        ByteTarget::All => split_batches(text),
    }?;
    for span in &picked {
        validate_batch(span.text(text))?;
    }
    Ok(picked)
}

fn significant(text: &str) -> Result<Vec<Token<'_>>, String> {
    Ok(lex(text)?.into_iter().filter(|t| !t.comment()).collect())
}

fn word_at(tokens: &[Token<'_>], at: usize, word: &str) -> bool {
    tokens.get(at).is_some_and(|t| t.word(word))
}

fn transaction_at(tokens: &[Token<'_>], at: usize) -> Option<StmtKind> {
    let token = tokens.get(at)?;
    if at > 0 && tokens[at - 1].text == "." {
        return None;
    }
    let tran = |at| word_at(tokens, at, "TRAN") || word_at(tokens, at, "TRANSACTION");
    if token.word("BEGIN") && (tran(at + 1) || word_at(tokens, at + 1, "DISTRIBUTED") && tran(at + 2)) {
        Some(StmtKind::Begin)
    } else if token.word("COMMIT") {
        Some(StmtKind::Commit)
    } else if token.word("ROLLBACK") {
        let named = tran(at + 1) && tokens.get(at + 2).is_some_and(|t| t.name().is_some() || t.kind == Kind::Variable);
        Some(if named { StmtKind::RollbackTo } else { StmtKind::Rollback })
    } else if token.word("SAVE") && tran(at + 1) {
        Some(StmtKind::Savepoint)
    } else {
        None
    }
}

pub(crate) fn has_transaction_control(text: &str) -> bool {
    significant(text).map_or(true, |tokens| (0..tokens.len()).any(|i| transaction_at(&tokens, i).is_some()))
}

fn ddl_at(tokens: &[Token<'_>], at: usize) -> Option<StmtKind> {
    let token = &tokens[at];
    if !["CREATE", "ALTER", "DROP", "TRUNCATE", "GRANT", "DENY", "REVOKE"].iter().any(|w| token.word(w)) {
        return None;
    }
    let object = tokens[at + 1..].iter().find(|t| !["OR", "ALTER", "UNIQUE", "CLUSTERED", "NONCLUSTERED"].iter().any(|w| t.word(w)));
    let verb = token.text.to_ascii_uppercase();
    Some(StmtKind::Ddl(object.filter(|t| t.kind == Kind::Word).map_or(verb.clone(), |t| format!("{verb} {}", t.text.to_ascii_uppercase()))))
}

pub(crate) fn classify(text: &str) -> StmtKind {
    let Ok(tokens) = significant(text) else {
        return StmtKind::Other("BATCH".into());
    };
    // A batch may change schema after its first statement. Refresh conservatively.
    let ddl: Vec<_> = (0..tokens.len()).filter_map(|i| ddl_at(&tokens, i)).collect();
    if let Some(kind) = ddl.iter().find(|k| matches!(k, StmtKind::Ddl(tag) if tag.ends_with(" DATABASE"))).or(ddl.first()) {
        return kind.clone();
    }
    let first = tokens.iter().position(Token::code).unwrap_or(tokens.len());
    if let Some(kind) = transaction_at(&tokens, first) {
        return kind;
    }
    for (word, verb) in [("INSERT", DmlVerb::Insert), ("UPDATE", DmlVerb::Update), ("DELETE", DmlVerb::Delete), ("MERGE", DmlVerb::Merge)] {
        if tokens.iter().any(|t| t.word(word)) {
            return StmtKind::Dml(verb);
        }
    }
    if tokens.iter().any(|t| t.word("SELECT")) {
        return StmtKind::Query;
    }
    StmtKind::Other(tokens.get(first).filter(|t| t.kind == Kind::Word).map_or(String::new(), |t| t.text.to_ascii_uppercase()))
}

pub(crate) fn is_write(text: &str) -> bool {
    significant(text).map_or(true, |tokens| {
        tokens.iter().any(|t| {
            [
                "INSERT", "UPDATE", "DELETE", "MERGE", "INTO", "CREATE", "ALTER", "DROP", "TRUNCATE", "GRANT", "DENY", "REVOKE", "EXEC",
                "EXECUTE", "DBCC", "BULK", "BACKUP", "RESTORE",
            ]
            .iter()
            .any(|w| t.word(w))
        })
    })
}

fn name_path(tokens: &[Token<'_>], mut at: usize) -> Option<(Vec<String>, usize)> {
    let mut names = vec![tokens.get(at)?.name()?];
    at += 1;
    while tokens.get(at).is_some_and(|t| t.text == ".") {
        names.push(tokens.get(at + 1)?.name()?);
        at += 2;
    }
    Some((names, at))
}

pub(crate) fn relation_names(text: &str) -> Vec<NameRef> {
    let Ok(tokens) = significant(text) else {
        return Vec::new();
    };
    let mut ctes = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        if t.word("AS") && tokens.get(i + 1).is_some_and(|t| t.text == "(") {
            let mut name_at = i.checked_sub(1);
            if name_at.is_some_and(|j| tokens[j].text == ")") {
                let mut depth = 1;
                let mut j = i - 1;
                while j > 0 && depth != 0 {
                    j -= 1;
                    match tokens[j].text {
                        ")" => depth += 1,
                        "(" => depth -= 1,
                        _ => {}
                    }
                }
                name_at = j.checked_sub(1);
            }
            if let Some(name) = name_at.and_then(|j| tokens[j].name()) {
                ctes.push(name);
            }
        }
    }
    let mut found = Vec::new();
    let mut from_levels = vec![false];
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Expect {
        None,
        From,
        Target,
    }
    let mut expect = Expect::None;
    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i];
        match t.text {
            "(" => {
                from_levels.push(false);
                expect = Expect::None;
            }
            ")" => {
                if from_levels.len() > 1 {
                    from_levels.pop();
                }
                expect = Expect::None;
            }
            "," if *from_levels.last().unwrap() => expect = Expect::From,
            ";" => {
                *from_levels.last_mut().unwrap() = false;
                expect = Expect::None;
            }
            _ if t.word("FROM") || t.word("JOIN") || t.word("APPLY") => {
                *from_levels.last_mut().unwrap() = true;
                expect = Expect::From;
            }
            _ if ["INTO", "UPDATE", "TABLE", "MERGE"].iter().any(|w| t.word(w)) => expect = Expect::Target,
            _ if ["WHERE", "ON", "SET", "GROUP", "ORDER", "HAVING", "UNION", "EXCEPT", "INTERSECT", "VALUES", "OUTPUT", "OPTION"]
                .iter()
                .any(|w| t.word(w)) =>
            {
                *from_levels.last_mut().unwrap() = false;
                expect = Expect::None;
            }
            _ if expect != Expect::None => {
                let context = expect;
                expect = Expect::None;
                if let Some((names, after)) = name_path(&tokens, i) {
                    if names.len() <= 2 && !(context == Expect::From && tokens.get(after).is_some_and(|t| t.text == "(")) {
                        let name = NameRef {
                            schema: if names.len() == 2 { Some(names[0].clone()) } else { None },
                            name: names.last().unwrap().clone(),
                        };
                        if !(name.schema.is_none() && ctes.contains(&name.name)) && !found.contains(&name) {
                            found.push(name);
                        }
                    }
                    i = after - 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    found
}

pub(crate) fn quote_ident(name: &str) -> String {
    format!("[{}]", name.replace(']', "]]"))
}

pub(crate) fn quote_literal(value: &str) -> String {
    format!("N'{}'", value.replace('\'', "''"))
}

const STATEMENT_WORDS: &[&str] = &[
    "ALTER",
    "BACKUP",
    "BEGIN",
    "BREAK",
    "BULK",
    "CHECKPOINT",
    "CLOSE",
    "COMMIT",
    "CONTINUE",
    "CREATE",
    "DBCC",
    "DEALLOCATE",
    "DECLARE",
    "DELETE",
    "DENY",
    "DROP",
    "ELSE",
    "END",
    "EXEC",
    "EXECUTE",
    "FETCH",
    "GO",
    "GOTO",
    "GRANT",
    "IF",
    "INSERT",
    "KILL",
    "MERGE",
    "OPEN",
    "PRINT",
    "RAISERROR",
    "RECONFIGURE",
    "RESTORE",
    "RETURN",
    "REVERT",
    "REVOKE",
    "ROLLBACK",
    "SAVE",
    "SELECT",
    "SEND",
    "SET",
    "SHUTDOWN",
    "THROW",
    "TRUNCATE",
    "UPDATE",
    "USE",
    "WAITFOR",
    "WHILE",
    "WITH",
];

fn expression_name(token: &Token<'_>) -> bool {
    token.kind == Kind::Ident
        || token.kind == Kind::Word
            && !STATEMENT_WORDS
                .iter()
                .chain(
                    [
                        "ALL",
                        "AND",
                        "ANY",
                        "AS",
                        "ASC",
                        "BETWEEN",
                        "BY",
                        "CASE",
                        "COLLATE",
                        "DESC",
                        "DISTINCT",
                        "ESCAPE",
                        "EXCEPT",
                        "EXISTS",
                        "FOR",
                        "FROM",
                        "GROUP",
                        "HAVING",
                        "IN",
                        "INTERSECT",
                        "INTO",
                        "IS",
                        "JOIN",
                        "LIKE",
                        "NOT",
                        "NULL",
                        "OFFSET",
                        "OPTION",
                        "OR",
                        "ORDER",
                        "OVER",
                        "SOME",
                        "THEN",
                        "TOP",
                        "UNION",
                        "WHEN",
                        "WHERE",
                    ]
                    .iter(),
                )
                .any(|w| token.word(w))
}

const SCALAR_FUNCTIONS: &[&str] = &[
    "ABS",
    "ASCII",
    "CEILING",
    "CHAR",
    "CHARINDEX",
    "COALESCE",
    "CONCAT",
    "CONCAT_WS",
    "DATALENGTH",
    "DATEADD",
    "DATEDIFF",
    "DATEDIFF_BIG",
    "DATENAME",
    "DATEPART",
    "DAY",
    "EOMONTH",
    "FLOOR",
    "GETDATE",
    "GETUTCDATE",
    "ISNULL",
    "LEFT",
    "LEN",
    "LOWER",
    "LTRIM",
    "MONTH",
    "NCHAR",
    "NULLIF",
    "PATINDEX",
    "POWER",
    "REPLACE",
    "REPLICATE",
    "REVERSE",
    "RIGHT",
    "ROUND",
    "RTRIM",
    "SIGN",
    "SQRT",
    "STUFF",
    "SUBSTRING",
    "SYSDATETIME",
    "SYSDATETIMEOFFSET",
    "SYSUTCDATETIME",
    "TRIM",
    "UNICODE",
    "UPPER",
    "YEAR",
];

struct Expression<'a> {
    tokens: &'a [Token<'a>],
    at: usize,
}

impl Expression<'_> {
    fn take(&mut self, word: &str) -> bool {
        if self.tokens.get(self.at).is_some_and(|t| t.word(word) || t.kind == Kind::Symbol && t.text == word) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn require(&mut self, word: &str) -> Result<(), String> {
        if self.take(word) { Ok(()) } else { Err(format!("Expected {word} in T-SQL expression")) }
    }

    fn expr(&mut self, min: u8, depth: usize) -> Result<(), String> {
        if depth > 64 {
            return Err("T-SQL expression nesting exceeds 64".into());
        }
        if self.take("NOT") {
            self.expr(3, depth + 1)?;
        } else if self.take("+") || self.take("-") || self.take("~") {
            self.expr(6, depth + 1)?;
        } else if self.take("(") {
            self.expr(0, depth + 1)?;
            self.require(")")?;
        } else {
            let token = self.tokens.get(self.at).ok_or("Expected a T-SQL expression")?;
            if matches!(token.kind, Kind::String | Kind::Number | Kind::Variable) || token.word("NULL") || token.word("CURRENT_TIMESTAMP") {
                self.at += 1;
            } else if expression_name(token) {
                self.at += 1;
                if self.take("(") {
                    if token.kind != Kind::Word || !SCALAR_FUNCTIONS.iter().any(|w| token.word(w)) {
                        return Err("Unsupported T-SQL expression function".into());
                    }
                    if !self.take(")") {
                        loop {
                            self.expr(0, depth + 1)?;
                            if !self.take(",") {
                                break;
                            }
                        }
                        self.require(")")?;
                    }
                } else {
                    while self.take(".") {
                        let part = self.tokens.get(self.at).ok_or("Incomplete T-SQL column name")?;
                        if !expression_name(part) {
                            return Err("Unsupported T-SQL column name".into());
                        }
                        self.at += 1;
                    }
                }
            } else {
                return Err(format!("Unsupported T-SQL expression token {}", token.text));
            }
        }
        loop {
            let Some(token) = self.tokens.get(self.at) else {
                break;
            };
            let bp = if token.word("OR") {
                1
            } else if token.word("AND") {
                2
            } else if ["IS", "IN", "LIKE", "BETWEEN", "NOT"].iter().any(|w| token.word(w))
                || matches!(token.text, "=" | "<>" | "!=" | "<" | ">" | "<=" | ">=" | "!<" | "!>")
            {
                3
            } else if matches!(token.text, "+" | "-" | "&" | "|" | "^") {
                4
            } else if matches!(token.text, "*" | "/" | "%") {
                5
            } else {
                break;
            };
            if bp < min {
                break;
            }
            if self.take("IS") {
                self.take("NOT");
                self.require("NULL")?;
            } else if bp == 3 && ["IN", "LIKE", "BETWEEN", "NOT"].iter().any(|w| token.word(w)) {
                self.take("NOT");
                if self.take("IN") {
                    self.require("(")?;
                    loop {
                        self.expr(0, depth + 1)?;
                        if !self.take(",") {
                            break;
                        }
                    }
                    self.require(")")?;
                } else if self.take("BETWEEN") {
                    self.expr(4, depth + 1)?;
                    self.require("AND")?;
                    self.expr(4, depth + 1)?;
                } else if self.take("LIKE") {
                    self.expr(4, depth + 1)?;
                    if self.take("ESCAPE") {
                        self.expr(4, depth + 1)?;
                    }
                } else {
                    return Err("Expected IN, BETWEEN or LIKE after NOT".into());
                }
            } else {
                self.at += 1;
                self.expr(bp + 1, depth + 1)?;
            }
        }
        Ok(())
    }
}

pub(crate) fn validate_expression(text: &str, order_by: bool) -> Result<(), String> {
    if text.trim().is_empty() {
        return Ok(());
    }
    if text.len() > 65_536 {
        return Err("T-SQL expression exceeds 64 KiB".into());
    }
    let tokens = lex(text)?;
    if tokens.len() > 4096 {
        return Err("T-SQL expression has too many tokens".into());
    }
    if tokens.iter().any(|t| t.comment() || t.text == ";") {
        return Err("Comments and statement separators are unsupported in T-SQL expressions".into());
    }
    let mut parser = Expression { tokens: &tokens, at: 0 };
    loop {
        parser.expr(0, 0)?;
        if !order_by {
            break;
        }
        if !parser.take("ASC") {
            parser.take("DESC");
        }
        if !parser.take(",") {
            break;
        }
    }
    if parser.at == tokens.len() {
        Ok(())
    } else {
        Err(format!("Unsupported trailing T-SQL expression syntax at byte {}", tokens[parser.at].start))
    }
}

pub(crate) fn parse_order_by(text: &str) -> Option<Vec<(String, bool)>> {
    let tokens = lex(text).ok()?;
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if !expression_name(&tokens[i]) {
            return None;
        }
        let name = tokens[i].name()?;
        i += 1;
        let desc = word_at(&tokens, i, "DESC");
        if desc || word_at(&tokens, i, "ASC") {
            i += 1;
        }
        out.push((name, desc));
        if i == tokens.len() {
            break;
        }
        if tokens[i].text != "," || i + 1 == tokens.len() {
            return None;
        }
        i += 1;
    }
    Some(out)
}

pub(crate) fn format_order_by(keys: &[(String, bool)]) -> String {
    keys.iter().map(|(name, desc)| format!("{} {}", quote_ident(name), if *desc { "DESC" } else { "ASC" })).collect::<Vec<_>>().join(", ")
}

fn format_batch(text: &str) -> Result<String, String> {
    let tokens = lex(text)?;
    let mut first = true;
    let mut depth = 0usize;
    let mut out = String::new();
    let mut previous: Option<Token<'_>> = None;
    for token in tokens {
        if !token.comment() {
            if first && token.text != ";" {
                if !token.word("SELECT") {
                    return Err("Unsupported T-SQL formatting. Only simple SELECT batches are supported".into());
                }
                first = false;
            }
            if token.kind == Kind::Word
                && (STATEMENT_WORDS.iter().any(|w| *w != "SELECT" && token.word(w))
                    || ["APPLY", "CASE", "EXCEPT", "FOR", "INTERSECT", "INTO", "OVER", "PIVOT", "UNION", "UNPIVOT"]
                        .iter()
                        .any(|w| token.word(w)))
            {
                return Err(format!("Unsupported T-SQL formatting syntax {}", token.text));
            }
            if token.kind == Kind::Symbol
                && ![
                    "(", ")", ".", ",", ";", "+", "-", "*", "/", "%", "&", "|", "^", "~", "=", "<", ">", "<=", ">=", "<>", "!=", "!<", "!>",
                ]
                .contains(&token.text)
            {
                return Err(format!("Unsupported T-SQL formatting symbol {}", token.text));
            }
        }
        let gap = previous.map_or("", |p| &text[p.end..token.start]);
        let clause = depth == 0 && ["FROM", "WHERE", "GROUP", "HAVING", "ORDER", "OFFSET", "FETCH"].iter().any(|w| token.word(w));
        let newline = clause
            || previous.is_some_and(|p| p.kind == Kind::LineComment || p.text == ";")
            || token.comment() && gap.contains(['\r', '\n']);
        if !out.is_empty() {
            if newline {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
            } else if !matches!(token.text, "." | "," | ")" | ";")
                && !previous.is_some_and(|p| matches!(p.text, "." | "("))
                && !(token.text == "(" && gap.is_empty())
            {
                out.push(' ');
            }
        }
        out.push_str(token.text);
        match token.text {
            "(" if !token.comment() => depth += 1,
            ")" if !token.comment() => depth = depth.checked_sub(1).ok_or("Unsupported unbalanced T-SQL parentheses")?,
            ";" if !token.comment() => first = true,
            _ => {}
        }
        previous = Some(token);
    }
    if depth != 0 {
        return Err("Unsupported unbalanced T-SQL parentheses".into());
    }
    Ok(out)
}

pub(crate) fn format(text: &str) -> Result<String, String> {
    if text.len() > 65_536 {
        return Err("Unsupported T-SQL formatting above 64 KiB".into());
    }
    let batches = split_batches(text)?;
    if batches.is_empty() {
        return Err("No T-SQL batch to format".into());
    }
    let mut out = String::new();
    let mut at = 0;
    for span in batches {
        out.push_str(&text[at..span.start]);
        out.push_str(&format_batch(span.text(text))?);
        at = span.end;
    }
    out.push_str(&text[at..]);
    let signature = |text: &str| -> Result<Vec<Vec<(Kind, String)>>, String> {
        split_batches(text)?
            .into_iter()
            .map(|s| Ok(lex(s.text(text))?.into_iter().map(|t| (t.kind, t.text.to_owned())).collect()))
            .collect()
    };
    if signature(text)? != signature(&out)? {
        return Err("Unsupported T-SQL formatting would change tokens or batch scope".into());
    }
    Ok(out)
}

fn like_pattern(value: &str, prefix_only: bool) -> String {
    let mut pattern = String::new();
    if !prefix_only {
        pattern.push('%');
    }
    for ch in value.chars() {
        if matches!(ch, '~' | '%' | '_' | '[') {
            pattern.push('~');
        }
        pattern.push(ch);
    }
    pattern.push('%');
    quote_literal(&pattern)
}

fn filter_sql(filter: &ColumnFilter) -> String {
    let column = quote_ident(&filter.column);
    let op = match filter.op {
        FilterOp::IsNull => return format!("{column} IS NULL"),
        FilterOp::NotNull => return format!("{column} IS NOT NULL"),
        FilterOp::Contains | FilterOp::StartsWith => {
            return format!(
                "CONVERT(nvarchar(max), {column}) LIKE {} ESCAPE N'~'",
                like_pattern(&filter.value, filter.op == FilterOp::StartsWith)
            );
        }
        FilterOp::Eq => "=",
        FilterOp::Ne => "<>",
        FilterOp::Lt => "<",
        FilterOp::Le => "<=",
        FilterOp::Gt => ">",
        FilterOp::Ge => ">=",
    };
    format!("{column} {op} {}", quote_literal(&filter.value))
}

fn from_sql(schema: &str, table: &str, query: &TableQuery) -> String {
    let mut sql = format!("FROM {}.{}", quote_ident(schema), quote_ident(table));
    let mut filters = Vec::new();
    if !query.r#where.trim().is_empty() {
        filters.push(format!("({}\n)", query.r#where.trim()));
    }
    filters.extend(query.filters.iter().map(filter_sql));
    if !filters.is_empty() {
        sql.push_str(&format!(" WHERE {}", filters.join(" AND ")));
    }
    sql
}

fn ordered_columns(text: &str) -> Vec<String> {
    let Ok(tokens) = lex(text) else {
        return Vec::new();
    };
    let mut columns = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for i in 0..=tokens.len() {
        if i == tokens.len() || tokens[i].text == "," && depth == 0 {
            if let Some((names, after)) = name_path(&tokens[start..i], 0) {
                let part = &tokens[start..i];
                if after == part.len() || after + 1 == part.len() && (word_at(part, after, "ASC") || word_at(part, after, "DESC")) {
                    columns.push(names.last().unwrap().clone());
                }
            }
            start = i + 1;
        } else if tokens[i].text == "(" {
            depth += 1;
        } else if tokens[i].text == ")" {
            depth = depth.saturating_sub(1);
        }
    }
    columns
}

pub(crate) fn browse_sql(
    schema: &str,
    table: &str,
    projection: &str,
    pk: &[String],
    query: &TableQuery,
    limit: u32,
    offset: u64,
) -> String {
    let mut sql = format!("SELECT {projection} {}", from_sql(schema, table, query));
    let mut order = Vec::new();
    let mut ordered = ordered_columns(&query.order_by);
    if !query.order_by.trim().is_empty() {
        order.push(format!("{}\n", query.order_by.trim()));
    }
    for column in pk {
        if !ordered.contains(column) {
            order.push(quote_ident(column));
            ordered.push(column.clone());
        }
    }
    if order.is_empty() {
        order.push("(SELECT NULL)".into());
    }
    sql.push_str(&format!(" ORDER BY {} OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY", order.join(", ")));
    sql
}

pub(crate) fn count_sql(schema: &str, table: &str, query: &TableQuery) -> String {
    format!("SELECT COUNT_BIG(*) {}", from_sql(schema, table, query))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::Invalidates;

    fn batches(text: &str) -> Vec<&str> {
        split_batches(text).unwrap().iter().map(|s| s.text(text)).collect()
    }

    fn selected(text: &str, target: ByteTarget) -> Vec<&str> {
        pick(text, target).unwrap().iter().map(|s| s.text(text)).collect()
    }

    #[test]
    fn only_go_splits_batches_and_preserves_variable_and_procedure_scope() {
        let declare = "DECLARE @x int = 1;\n\nSELECT @x; SELECT 2;";
        let procedure = "CREATE OR ALTER PROCEDURE [dbo].[p] AS\nBEGIN\nDECLARE @n int; SELECT @n;\nEND;";
        assert_eq!(batches(declare), [declare]);
        assert_eq!(batches(procedure), [procedure]);
        let text = format!("  {declare}\r\ngO -- next batch\r\n{procedure}  ");
        assert_eq!(batches(&text), [declare, procedure]);
        assert_eq!(batches("SELECT 1\rGO\rSELECT 2"), ["SELECT 1", "SELECT 2"]);
        assert!(batches("-- note\nGO\n/* nested /* x */ */\nGO\n;;").is_empty());
    }

    #[test]
    fn go_inside_every_tsql_quote_and_comment_form_stays_in_its_batch() {
        for text in [
            "SELECT 'first''\nGO\nlast';",
            "SELECT N'first''\nGO\nlast';",
            "SELECT n'GO';",
            "SELECT \"first\"\"\nGO\nlast\";",
            "SELECT [first]]\nGO\nlast];",
            "SELECT 1; -- GO\nSELECT 2;",
            "SELECT 1; /*\nGO\n/* nested */\nGO\n*/ SELECT 2;",
            "SELECT @GO, #GO, [GO], N'--GO', '/*GO*/';",
            "SELECT 'backslash\\'; SELECT 2;",
        ] {
            assert_eq!(batches(text), [text], "{text}");
        }
        assert_eq!(batches("SELECT 1\nGO /* next */ -- note\nSELECT 2"), ["SELECT 1", "SELECT 2"]);
        assert_eq!(batches("SELECT 1\nGO /* next\n/* nested */ */ SELECT 2"), ["SELECT 1", "SELECT 2"]);
    }

    #[test]
    fn malformed_text_counts_and_sqlcmd_are_rejected_before_any_batch_runs() {
        for tail in [
            "GO 2",
            "go 0",
            "GO -1",
            "GO +3",
            "GO 1.5",
            "GO something",
            "GO /* comment */ 2",
            ":r file.sql",
            ":setvar name value",
            "!! echo shell",
            "SELECT $(name)",
            "SELECT prefix$(name)",
            "SELECT 'open",
            "SELECT N'open",
            "SELECT \"open",
            "SELECT [open",
            "/* open",
            "/* nested /* open */",
            "*/",
            "SELECT '\0'",
        ] {
            let text = format!("SELECT 1\nGO\n{tail}");
            assert!(split_batches(&text).is_err(), "accepted {tail:?}");
        }
        assert_eq!(batches("SELECT N':r file', [$(name)], 'GO 2' -- !!\n"), ["SELECT N':r file', [$(name)], 'GO 2' -- !!"]);
    }

    #[test]
    fn spans_are_original_utf8_bytes_and_selections_never_expand() {
        let text = "-- café\nSELECT N'😀';\nGO\nSELECT [日本語] FROM [Å].[旅];\n";
        let spans = split_batches(text).unwrap();
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, text.find("\nGO").unwrap());
        assert_eq!(spans[1].text(text), "SELECT [日本語] FROM [Å].[旅];");
        let from = text.find("N'😀'").unwrap();
        let to = from + "N'😀'".len();
        assert_eq!(pick(text, ByteTarget::Selection { from, to }).unwrap(), [StmtSpan { start: from, end: to }]);
        let from = text.find("SELECT N").unwrap();
        let to = text.find(" FROM").unwrap();
        assert_eq!(selected(text, ByteTarget::Selection { from, to }), ["SELECT N'😀';", "SELECT [日本語]"]);
        assert_eq!(selected("'unclosed\nSELECT 1", ByteTarget::Selection { from: 10, to: 18 }), ["SELECT 1"]);
        assert!(pick(text, ByteTarget::Cursor(text.find('😀').unwrap() + 1)).is_err());
        assert!(pick(text, ByteTarget::Cursor(text.len() + 1)).is_err());
        assert!(pick(text, ByteTarget::Selection { from: 20, to: 1 }).is_err());
    }

    #[test]
    fn cursor_picks_the_batch_but_not_the_trivia_between_batches() {
        let text = "-- header\nDECLARE @x int;\n\nSELECT @x;\n-- footer\n\nGO -- split\n\n/* header */\nSELECT 2;\n-- end\n";
        let spans = split_batches(text).unwrap();
        for needle in ["DECLARE", "\n\nSELECT", "SELECT @x"] {
            assert_eq!(pick(text, ByteTarget::Cursor(text.find(needle).unwrap())).unwrap(), [spans[0]], "{needle}");
        }
        for needle in ["-- header", "-- footer", "GO", "/* header */", "-- end"] {
            assert!(pick(text, ByteTarget::Cursor(text.find(needle).unwrap())).unwrap().is_empty(), "{needle}");
        }
        assert!(pick(text, ByteTarget::Cursor(text.len())).unwrap().is_empty());
        assert_eq!(selected("SELECT 1", ByteTarget::Selection { from: 2, to: 2 }), ["SELECT 1"]);
    }

    #[test]
    fn postgres_brackets_keep_their_existing_meaning() {
        assert_eq!(super::super::split_statements("SELECT ARRAY[1, 2]; SELECT 3").len(), 2);
        assert_eq!(batches("SELECT [a;b]; SELECT 3").len(), 1);
    }

    #[test]
    fn batch_validation_rejects_use_but_preserves_quoted_text() {
        for text in [
            "USE master",
            "use [db]",
            "SELECT 1; USE master",
            "IF 1=1 USE master",
            "SELECT 1\nGO\nSELECT 2",
            "SELECT 'open",
            ":r file.sql",
            "GO 2",
        ] {
            assert!(validate_batch(text).is_err(), "accepted {text:?}");
        }
        for text in
            ["SELECT N'USE master', [USE], \"USE\", @USE, #USE", "SELECT useful FROM reuse", "/* USE master */ SELECT 1 -- USE tempdb"]
        {
            assert!(validate_batch(text).is_ok(), "{text:?}");
        }
    }

    #[test]
    fn planning_validates_all_picked_batches_before_returning_any() {
        let text = "SELECT 1\nGO\nUSE master\nGO\nSELECT 2";
        assert!(pick(text, ByteTarget::All).is_err());
        assert!(pick(text, ByteTarget::Selection { from: 0, to: text.len() }).is_err());
        assert_eq!(selected(text, ByteTarget::Cursor(0)), ["SELECT 1"]);
        let from = text.find("SELECT 2").unwrap();
        assert_eq!(selected(text, ByteTarget::Selection { from, to: text.len() }), ["SELECT 2"]);
        let text = "USE master\nSELECT 'unclosed\nSELECT 1";
        let from = text.rfind("SELECT 1").unwrap();
        assert_eq!(selected(text, ByteTarget::Selection { from, to: text.len() }), ["SELECT 1"]);
    }

    #[test]
    fn transactions_are_distinct_from_procedural_blocks_and_quoted_words() {
        for text in [
            "BEGIN",
            "BEGIN SELECT 1; END",
            "BEGIN TRY SELECT 1 END TRY BEGIN CATCH SELECT 2 END CATCH",
            "SELECT [COMMIT], \"ROLLBACK\", N'BEGIN TRAN', @COMMIT; /* SAVE TRAN */",
        ] {
            assert!(!has_transaction_control(text), "{text}");
            assert!(!matches!(classify(text), StmtKind::Begin | StmtKind::Commit | StmtKind::Rollback));
        }
        for (text, kind) in [
            ("BEGIN TRAN", StmtKind::Begin),
            ("BEGIN /* x */ TRANSACTION named", StmtKind::Begin),
            ("BEGIN DISTRIBUTED TRANSACTION", StmtKind::Begin),
            ("COMMIT", StmtKind::Commit),
            ("COMMIT WORK", StmtKind::Commit),
            ("ROLLBACK TRANSACTION", StmtKind::Rollback),
            ("ROLLBACK TRAN [save]", StmtKind::RollbackTo),
            ("SAVE TRANSACTION [save]", StmtKind::Savepoint),
        ] {
            assert_eq!(classify(text), kind, "{text}");
            assert!(has_transaction_control(text));
            assert!(has_transaction_control(&format!("DECLARE @x int; {text}")));
        }
        assert!(has_transaction_control("'unclosed"));
    }

    #[test]
    fn advisory_classification_accounts_for_writes_and_ddl_later_in_a_batch() {
        assert_eq!(classify("DECLARE @x int = 1; SELECT @x"), StmtKind::Query);
        assert_eq!(classify("WITH c AS (SELECT 1 AS id) UPDATE t SET id = 1"), StmtKind::Dml(DmlVerb::Update));
        assert_eq!(classify("SELECT 1; CREATE OR ALTER PROCEDURE p AS SELECT 2"), StmtKind::Ddl("CREATE PROCEDURE".into()));
        assert_eq!(classify("SELECT 1; ALTER TABLE t ADD x int").invalidates(), Invalidates::Catalog);
        assert_eq!(classify("CREATE TABLE t (id int); DROP DATABASE d").invalidates(), Invalidates::DatabaseList);
        assert!(is_write("SELECT 1; INSERT INTO t VALUES (1)"));
        assert!(is_write("SELECT id INTO #copy FROM t"));
        assert!(is_write("EXEC p"));
        assert!(!is_write("SELECT [UPDATE], N'DROP TABLE t', @INSERT /* DELETE */"));
    }

    #[test]
    fn relation_names_preserve_case_quoting_temp_tables_and_cte_exclusions() {
        assert_eq!(
            relation_names("SELECT * FROM [Å].[旅]]人] JOIN \"Sales\".\"Order\" ON 1=1"),
            [NameRef { schema: Some("Å".into()), name: "旅]人".into() }, NameRef { schema: Some("Sales".into()), name: "Order".into() },]
        );
        assert_eq!(
            relation_names("INSERT INTO [dbo].[T] ([id]) SELECT id FROM #Temp"),
            [NameRef { schema: Some("dbo".into()), name: "T".into() }, NameRef { schema: None, name: "#Temp".into() },]
        );
        assert_eq!(
            relation_names("WITH C (id) AS (SELECT id FROM dbo.Source) SELECT * FROM C, dbo.Other JOIN dbo.f() ON 1=1"),
            [NameRef { schema: Some("dbo".into()), name: "Source".into() }, NameRef { schema: Some("dbo".into()), name: "Other".into() },]
        );
        assert!(relation_names("SELECT N'FROM bad' -- FROM worse\n").is_empty());
        assert!(relation_names("SELECT * FROM OtherDb.dbo.T").is_empty());
    }

    #[test]
    fn unicode_identifiers_and_literals_are_quoted_without_case_folding() {
        assert_eq!(quote_ident("Å].旅"), "[Å]].旅]");
        assert_eq!(quote_ident(""), "[]");
        assert_eq!(quote_literal("旅's 😀\\"), "N'旅''s 😀\\'");
        assert_eq!(quote_literal(""), "N''");
        assert!(validate_expression("[旅]=N'😀' AND 名称=N'日本語'", false).is_ok());
    }

    #[test]
    fn expression_subset_consumes_predicates_arithmetic_and_scalar_calls() {
        for text in [
            "",
            "id = 1",
            "([Price] * quantity + 2) >= .5e+2 AND active = 1",
            "name IS NOT NULL OR id IS NULL",
            "NOT (id BETWEEN 1 AND 10 OR id NOT IN (11, 12, 13))",
            "name NOT LIKE N'%x~_%' ESCAPE N'~'",
            "LOWER([Name]) = LOWER(N'A') AND LEN([Name]) > 2",
            "id = @id",
            "data = 0xDEADBEEF",
            "date >= CURRENT_TIMESTAMP",
            "dbo.[t].[id] <= -12.25",
            "amount & 1 = 0",
            "COALESCE(a, ISNULL(b, 0)) > 0",
            "[EXEC] = N'GO\nDELETE FROM x; -- /*'",
            "\"DROP\" = 'it''s safe'",
            "name = N'\\'",
        ] {
            assert!(validate_expression(text, false).is_ok(), "{text}");
        }
        for text in ["[Name] DESC, id ASC", "LOWER([Name]) DESC, id", "(price * quantity) DESC", "名 ASC"] {
            assert!(validate_expression(text, true).is_ok(), "{text}");
        }
    }

    #[test]
    fn raw_fragments_refuse_statement_injection_without_a_semicolon() {
        for text in [
            "1=1 DELETE FROM dbo.t",
            "id=1 UPDATE dbo.t SET x=2",
            "1=1 INSERT INTO t VALUES (1)",
            "1=1 DROP TABLE t",
            "1=1 ALTER TABLE t ADD a int",
            "1=1 CREATE TABLE t (id int)",
            "1=1 EXEC(N'DROP TABLE t')",
            "1=1 USE master",
            "1=1 DBCC CHECKDB",
            "1=1 WAITFOR DELAY '00:01'",
            "1=1 SELECT 2",
            "1=1 COMMIT TRAN",
            "1=1 ROLLBACK",
            "1=1 BEGIN TRAN",
            "1=1 PRINT 'x'",
            "1=1 DECLARE @x int",
            "1=1; SELECT 2",
            "1=1\nGO\nDELETE t",
            "1=1 -- hidden",
            "1=1 /* hidden */",
            "1=1 */ DELETE t /*",
            "1=1 UNION SELECT 2",
            "id IN (SELECT id FROM t)",
            "EXISTS (SELECT 1)",
            "id=1 OPTION(RECOMPILE)",
            "[id]=1) DELETE t; --",
            "EXEC('delete t')",
            "OPENROWSET('a','b','c')=1",
            "dbo.f()=1",
            "[f]()=1",
            "name = 'open",
            "name = N'open",
            "[id = 1",
            "id = 1e",
            "id = 0x",
            "id += 1",
            "id = 1 2",
            "CASE WHEN id=1 THEN 2 ELSE 3 END=2",
            "CAST(id AS int)=1",
            "name COLLATE Latin1_General_BIN = 'a'",
        ] {
            assert!(validate_expression(text, false).is_err(), "accepted {text:?}");
        }
        for text in [
            "id DESC DELETE FROM t",
            "id OFFSET 0 ROWS FETCH NEXT 1 ROWS ONLY",
            "id DESC; SELECT 1",
            "id, EXEC('x')",
            "id DESC -- tail",
            "id NULLS LAST",
            "id ASC DESC",
            "id,",
        ] {
            assert!(validate_expression(text, true).is_err(), "accepted order {text:?}");
        }
    }

    #[test]
    fn expression_resource_limits_refuse_deep_or_large_input() {
        let nested = format!("{}id{}", "(".repeat(66), ")".repeat(66));
        assert!(validate_expression(&nested, false).is_err());
        assert!(validate_expression(&format!("{}id=1", "NOT ".repeat(66)), false).is_err());
        assert!(validate_expression(&"a".repeat(65_537), false).is_err());
        assert!(validate_expression(&std::iter::repeat_n("id=1", 1500).collect::<Vec<_>>().join(" OR "), false).is_err());
    }

    #[test]
    fn unicode_boundaries_cannot_hide_statement_tokens_or_panic_the_lexer() {
        for whitespace in ['\u{a0}', '\u{2003}', '\u{2028}', '\u{feff}'] {
            for fragment in [format!("id{whitespace}DESC{whitespace}DELETE{whitespace}t"), format!("id=1{whitespace}COMMIT")] {
                assert!(validate_expression(&fragment, true).is_err(), "{fragment:?}");
            }
        }
        for text in ["旅=名", "(旅)", "id+😀", ":旅", "SELECT $action", "SELECT $$pg$$"] {
            let _ = split_batches(text);
            let _ = validate_expression(text, false);
            let _ = format(text);
        }
    }

    #[test]
    fn simple_order_inspection_and_generation_preserve_identifier_spelling() {
        let keys = vec![("Name".into(), true), ("旅]人".into(), false), ("id".into(), false)];
        assert_eq!(parse_order_by("Name DESC, [旅]]人] ASC, \"id\""), Some(keys.clone()));
        assert_eq!(format_order_by(&keys), "[Name] DESC, [旅]]人] ASC, [id] ASC");
        assert_eq!(parse_order_by(&format_order_by(&keys)), Some(keys));
        assert_eq!(parse_order_by(""), Some(vec![]));
        for advanced in ["LOWER(Name) DESC", "dbo.id", "id COLLATE x", "id -- comment", "id,", "id DESC SELECT 1", "[open", "1 DESC"] {
            assert_eq!(parse_order_by(advanced), None, "{advanced}");
        }
    }

    #[test]
    fn browse_paginates_with_pk_tiebreakers_and_preserves_trusted_projection() {
        let query = TableQuery::default();
        assert_eq!(
            browse_sql("s]", "旅", "*", &["Id".into(), "Tenant".into()], &query, 50, 200),
            "SELECT * FROM [s]]].[旅] ORDER BY [Id], [Tenant] OFFSET 200 ROWS FETCH NEXT 50 ROWS ONLY"
        );
        assert_eq!(
            browse_sql("s", "t", "CONVERT(decimal(19,4), [money]) AS [money]", &[], &query, 1, u64::MAX),
            "SELECT CONVERT(decimal(19,4), [money]) AS [money] FROM [s].[t] ORDER BY (SELECT NULL) OFFSET 18446744073709551615 ROWS FETCH NEXT 1 ROWS ONLY"
        );
        let query = TableQuery { order_by: "[Id] DESC, LOWER([Name])".into(), ..Default::default() };
        assert_eq!(
            browse_sql("s", "t", "*", &["Id".into(), "Tenant".into()], &query, 10, 0),
            "SELECT * FROM [s].[t] ORDER BY [Id] DESC, LOWER([Name])\n, [Tenant] OFFSET 0 ROWS FETCH NEXT 10 ROWS ONLY"
        );
        let query = TableQuery { order_by: "[t].[Id] DESC".into(), ..Default::default() };
        assert!(!browse_sql("s", "t", "*", &["Id".into()], &query, 10, 0).contains(", [Id]"));
    }

    #[test]
    fn count_big_and_column_filters_escape_every_like_metacharacter() {
        let query = TableQuery {
            r#where: "[active] = 1 OR [旅] = N'人'".into(),
            filters: vec![
                ColumnFilter { column: "na]me".into(), op: FilterOp::Contains, value: "~%_[x]'旅".into() },
                ColumnFilter { column: "prefix".into(), op: FilterOp::StartsWith, value: "a_b".into() },
                ColumnFilter { column: "空".into(), op: FilterOp::IsNull, value: "ignored".into() },
            ],
            order_by: "ignored".into(),
        };
        assert_eq!(
            count_sql("Sales", "Orders", &query),
            "SELECT COUNT_BIG(*) FROM [Sales].[Orders] WHERE ([active] = 1 OR [旅] = N'人'\n) AND CONVERT(nvarchar(max), [na]]me]) LIKE N'%~~~%~_~[x]''旅%' ESCAPE N'~' AND CONVERT(nvarchar(max), [prefix]) LIKE N'a~_b%' ESCAPE N'~' AND [空] IS NULL"
        );
        for (op, expected) in [
            (FilterOp::Eq, "="),
            (FilterOp::Ne, "<>"),
            (FilterOp::Lt, "<"),
            (FilterOp::Le, "<="),
            (FilterOp::Gt, ">"),
            (FilterOp::Ge, ">="),
        ] {
            assert_eq!(
                filter_sql(&ColumnFilter { column: "x".into(), op, value: "O'Brien".into() }),
                format!("[x] {expected} N'O''Brien'")
            );
        }
        assert_eq!(count_sql("s", "t", &TableQuery::default()), "SELECT COUNT_BIG(*) FROM [s].[t]");
    }

    #[test]
    fn formatter_preserves_tokens_comments_identifiers_and_batch_scope() {
        let text = "-- header\nselect  [旅]]人] ,N'go\nGO\nit''s', @value from [Sales].[T] /* keep\n GO */ where id=1 order by [旅]]人] desc; -- end\nGO -- separator\nselect  \"MixedCase\", lower(N'A') from #Temp;\n";
        let formatted = format(text).unwrap();
        assert!(formatted.contains("\nfrom [Sales].[T]"), "{formatted}");
        assert!(formatted.contains("GO -- separator\n"));
        assert_eq!(batches(&formatted).len(), 2);
        let tokens = |s: &str| lex(s).unwrap().into_iter().map(|t| (t.kind, t.text.to_owned())).collect::<Vec<_>>();
        assert_eq!(tokens(text), tokens(&formatted));
        assert_eq!(format(&formatted).unwrap(), formatted);
        assert_eq!(format("select  a,b from t where a=1; select 2").unwrap(), "select a, b\nfrom t\nwhere a = 1;\nselect 2");
    }

    #[test]
    fn formatter_refuses_unsupported_syntax_instead_of_using_the_pg_formatter() {
        for text in [
            "DECLARE @x int; SELECT @x",
            "BEGIN SELECT 1 END",
            "CREATE PROCEDURE p AS SELECT 1",
            "UPDATE t SET x=1",
            "WITH c AS (SELECT 1) SELECT * FROM c",
            "SELECT CASE WHEN 1=1 THEN 2 END",
            "SELECT ROW_NUMBER() OVER (ORDER BY id) FROM t",
            "SELECT 1 UNION SELECT 2",
            "SELECT * INTO #t FROM t",
            "SELECT $$pg$$",
            "SELECT (1",
            "SELECT 1)",
            "SELECT 'open",
            "SELECT 1\nGO 2",
            "-- comments only",
        ] {
            assert!(format(text).is_err(), "accepted {text:?}");
        }
        assert!(format(&format!("SELECT '{}'", "x".repeat(65_536))).is_err());
    }
}
