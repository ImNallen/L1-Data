use std::ops::Range;

use super::{word_cont, word_start};

/// A word at depth 0 that the statement-start rules name, or `""` for any other word. It matches on a stack copy, so the
/// scan allocates nothing per word.
fn keyword(word: &[u8]) -> &'static str {
    let mut buf = [0u8; 9];
    if !(2..=buf.len()).contains(&word.len()) {
        return "";
    }
    for (b, w) in buf.iter_mut().zip(word) {
        *b = w.to_ascii_uppercase();
    }
    match &buf[..word.len()] {
        b"AFTER" => "AFTER",
        b"ALL" => "ALL",
        b"ALTER" => "ALTER",
        b"AS" => "AS",
        b"BEGIN" => "BEGIN",
        b"CATCH" => "CATCH",
        b"CREATE" => "CREATE",
        b"DECLARE" => "DECLARE",
        b"DELETE" => "DELETE",
        b"DENY" => "DENY",
        b"DROP" => "DROP",
        b"ELSE" => "ELSE",
        b"EXCEPT" => "EXCEPT",
        b"EXEC" => "EXEC",
        b"EXECUTE" => "EXECUTE",
        b"FOR" => "FOR",
        b"GRANT" => "GRANT",
        b"IF" => "IF",
        b"INSERT" => "INSERT",
        b"INSTEAD" => "INSTEAD",
        b"INTERSECT" => "INTERSECT",
        b"MERGE" => "MERGE",
        b"NOT" => "NOT",
        b"OF" => "OF",
        b"ON" => "ON",
        b"OR" => "OR",
        b"PRINT" => "PRINT",
        b"RAISERROR" => "RAISERROR",
        b"RETURN" => "RETURN",
        b"REVOKE" => "REVOKE",
        b"SELECT" => "SELECT",
        b"SET" => "SET",
        b"THEN" => "THEN",
        b"THROW" => "THROW",
        b"TRUNCATE" => "TRUNCATE",
        b"TRY" => "TRY",
        b"UNION" => "UNION",
        b"UPDATE" => "UPDATE",
        b"USE" => "USE",
        b"VALUES" => "VALUES",
        b"WHILE" => "WHILE",
        b"WITH" => "WITH",
        _ => "",
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Prev {
    None,
    Word(&'static str),
    Comma,
    Other,
}

/// What decides whether the next word starts a statement. T-SQL needs no `;`, so a statement verb that cannot continue the
/// statement before it starts the next one.
#[derive(Default)]
struct Statement {
    first: Option<&'static str>,
    /// The verb after a `WITH`'s CTEs, or the first word.
    verb: Option<&'static str>,
    source_begun: bool,
    update: bool,
    update_set: bool,
}

impl Statement {
    fn starts_new(&self, w: &'static str, prev: Prev) -> bool {
        if w.is_empty() || self.first.is_none() {
            return false;
        }
        let prev_word = match prev {
            Prev::Word(p) => p,
            _ => "",
        };
        let set_option = self.first == Some("SET");
        let continues = prev == Prev::Comma
            || ["UNION", "ALL", "EXCEPT", "INTERSECT", "FOR", "AFTER", "OF", "THEN", "INSTEAD", "OR", "NOT", "GRANT", "DENY", "REVOKE"].contains(&prev_word)
            || prev_word == "ON" && !set_option;
        let cte_body = self.first == Some("WITH") && self.verb.is_none();
        let insert_source = self.verb == Some("INSERT") && !self.source_begun;
        self.first.is_some()
            && !continues
            && match w {
                "SELECT" => !cte_body && !insert_source,
                "INSERT" | "UPDATE" | "DELETE" | "MERGE" => !cte_body,
                "SET" => !self.update || self.update_set,
                "EXEC" | "EXECUTE" => !insert_source,
                "IF" => !matches!(self.first, Some("DROP" | "ALTER" | "CREATE")),
                "WITH" => matches!(prev_word, "BEGIN" | "TRY" | "CATCH" | "ELSE") || prev_word == "AS" && matches!(self.first, Some("CREATE" | "ALTER")),
                "DECLARE" | "PRINT" | "WHILE" | "RETURN" | "BEGIN" | "CREATE" | "ALTER" | "DROP" | "TRUNCATE" | "THROW" | "RAISERROR" | "USE" | "GRANT" | "DENY"
                | "REVOKE" => true,
                _ => false,
            }
    }

    fn word(&mut self, w: &'static str) {
        if w.is_empty() && self.first.is_some() {
            return;
        }
        if self.first.is_none() {
            self.first = Some(w);
            if w != "WITH" {
                self.verb = Some(w);
            }
        } else if self.first == Some("WITH") && self.verb.is_none() && matches!(w, "SELECT" | "INSERT" | "UPDATE" | "DELETE" | "MERGE") {
            self.verb = Some(w);
        }
        self.source_begun |= matches!(w, "VALUES" | "SELECT" | "EXEC" | "EXECUTE");
        self.update_set = self.update && (self.update_set || w == "SET") && w != "UPDATE";
        self.update |= w == "UPDATE";
    }
}

/// The end of a `GO` line that starts at `line`, when the line holds only `GO` and comments.
fn go_line(b: &[u8], line: usize) -> Option<usize> {
    let mut i = line;
    while i < b.len() && matches!(b[i], b' ' | b'\t') {
        i += 1;
    }
    if !(b.get(i..i + 2).is_some_and(|w| w.eq_ignore_ascii_case(b"GO")) && b.get(i + 2).is_none_or(|c| !c.is_ascii_alphanumeric() && !matches!(c, b'_' | b'@' | b'#' | b'$')))
    {
        return None;
    }
    i += 2;
    loop {
        while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r') {
            i += 1;
        }
        match b.get(i) {
            None | Some(b'\n') => return Some(i),
            Some(b'-') if b.get(i + 1) == Some(&b'-') => return Some(skip_line(b, i)),
            Some(b'/') if b.get(i + 1) == Some(&b'*') => i = skip_block(b, i),
            _ => return None,
        }
    }
}

fn skip_line(b: &[u8], i: usize) -> usize {
    b[i..].iter().position(|&c| c == b'\n').map_or(b.len(), |p| i + p)
}

fn skip_block(b: &[u8], mut i: usize) -> usize {
    let mut depth = 0usize;
    while i < b.len() {
        if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            depth += 1;
            i += 2;
        } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i;
            }
        } else {
            i += 1;
        }
    }
    b.len()
}

fn skip_quoted(b: &[u8], start: usize, close: u8) -> usize {
    let mut i = start + 1;
    while i < b.len() {
        if b[i] == close {
            if b.get(i + 1) == Some(&close) {
                i += 2;
            } else {
                return i + 1;
            }
        } else {
            i += 1;
        }
    }
    b.len()
}

/// The statement around `at` inside its `GO` batch. One pass over the bytes, with no tokens, so a long script stays cheap.
pub(in crate::sql) fn statement_around(text: &str, at: usize) -> Range<usize> {
    let b = text.as_bytes();
    let mut start = 0;
    let mut statement = Statement::default();
    let mut prev = Prev::None;
    let mut depth = 0usize;
    let mut line_start = true;
    let mut i = 0;
    while i < b.len() {
        if line_start {
            line_start = false;
            if let Some(end) = go_line(b, i) {
                if i >= at {
                    return start..i;
                }
                (start, statement, prev, depth) = (end, Statement::default(), Prev::None, 0);
                i = end;
                continue;
            }
        }
        let c = b[i];
        match c {
            b'\n' => {
                line_start = true;
                i += 1;
            }
            _ if c.is_ascii_whitespace() => i += 1,
            b'-' if b.get(i + 1) == Some(&b'-') => i = skip_line(b, i),
            b'/' if b.get(i + 1) == Some(&b'*') => i = skip_block(b, i),
            b'\'' => {
                i = skip_quoted(b, i, b'\'');
                prev = Prev::Other;
            }
            b'N' | b'n' if b.get(i + 1) == Some(&b'\'') => {
                i = skip_quoted(b, i + 1, b'\'');
                prev = Prev::Other;
            }
            b'[' | b'"' => {
                i = skip_quoted(b, i, if c == b'[' { b']' } else { b'"' });
                prev = Prev::Other;
            }
            b'(' => {
                depth += 1;
                prev = Prev::Other;
                i += 1;
            }
            b')' => {
                depth = depth.saturating_sub(1);
                prev = Prev::Other;
                i += 1;
            }
            b',' => {
                prev = Prev::Comma;
                i += 1;
            }
            b';' if depth == 0 => {
                i += 1;
                if i > at {
                    return start..i;
                }
                (start, statement, prev) = (i, Statement::default(), Prev::None);
            }
            _ => {
                let ascii_word = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'@' | b'#' | b'$');
                let word = if c.is_ascii() { c.is_ascii_alphabetic() || matches!(c, b'_' | b'@' | b'#') } else { text[i..].chars().next().is_some_and(word_start) };
                if !word {
                    prev = Prev::Other;
                    i += text[i..].chars().next().map_or(1, char::len_utf8);
                    continue;
                }
                let mut word_end = i + 1;
                while word_end < b.len() {
                    let c = b[word_end];
                    if ascii_word(c) || c >= 0x80 && !text.is_char_boundary(word_end) {
                        word_end += 1;
                    } else if c >= 0x80 && text[word_end..].chars().next().is_some_and(word_cont) {
                        word_end += 1;
                    } else {
                        break;
                    }
                }
                let w = keyword(&b[i..word_end]);
                if depth == 0 {
                    if statement.starts_new(w, prev) {
                        if i > at {
                            return start..i;
                        }
                        (start, statement) = (i, Statement::default());
                    }
                    statement.word(w);
                }
                prev = Prev::Word(w);
                i = word_end;
            }
        }
    }
    start..b.len()
}
