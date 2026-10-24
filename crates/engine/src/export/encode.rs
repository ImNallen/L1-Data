//! Column metadata and canonical cell text go in, bytes come out. Writing into a `Vec<u8>` cannot
//! fail, so nothing here returns an error; a disk error belongs to the caller that moves the bytes.
//!
//! A cell becomes a bare JSON number or SQL numeric literal only when its canonical text already
//! matches the JSON number grammar, and then it is copied byte for byte. Nothing is parsed into a
//! float.

use std::collections::HashSet;

use crate::model::{ColumnMeta, DatabaseKind, TypeCategory};
use crate::sql::mssql_value;
use crate::text::{quote_ident_in, quote_literal_in};

/// `ExportFormat` with what each format needs resolved. Only an INSERT needs more than the column
/// metadata, so an INSERT encoder cannot be built without a target.
pub(crate) enum Format {
    Csv,
    Json,
    Markdown,
    Insert(InsertTarget),
}

pub(crate) struct InsertTarget {
    /// Quoted and schema-qualified, or `target_table` when no single source table is known.
    pub table: String,
    /// Quoted, one per result column, in result order.
    pub columns: Vec<String>,
}

pub(crate) const PLACEHOLDER_TABLE: &str = "target_table";

/// SQL Server runs a script batch by batch. A `GO` every this many INSERTs keeps a large file
/// runnable in L1 Data and SSMS without one giant batch.
const INSERTS_PER_BATCH: u64 = 1_000;

impl InsertTarget {
    pub fn placeholder(dialect: DatabaseKind, columns: &[ColumnMeta]) -> Self {
        Self { table: PLACEHOLDER_TABLE.into(), columns: columns.iter().map(|c| quote_ident_in(dialect, &c.name)).collect() }
    }
}

struct Col {
    category: TypeCategory,
    /// The native type name, which SQL Server literals convert through.
    native: String,
}

enum Shape {
    Csv { header: Vec<String> },
    /// Each key already a JSON string token followed by `:`.
    Json { keys: Vec<String> },
    Markdown { header: Vec<String>, right: Vec<bool> },
    /// `INSERT INTO <table> (<columns>) VALUES (`
    Insert { prefix: String },
}

pub(crate) struct Encoder {
    dialect: DatabaseKind,
    cols: Vec<Col>,
    shape: Shape,
    rows: u64,
}

impl Encoder {
    pub fn new(format: Format, dialect: DatabaseKind, columns: &[ColumnMeta]) -> Self {
        let cols = columns.iter().map(|c| Col { category: c.r#type.category, native: c.r#type.display.clone() }).collect();
        let names = || columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
        let shape = match format {
            Format::Csv => Shape::Csv { header: names() },
            Format::Json => Shape::Json {
                keys: unique_keys(&names()).iter().map(|k| format!("{}:", serde_json::to_string(k).expect("a string serializes"))).collect(),
            },
            Format::Markdown => Shape::Markdown {
                header: names(),
                right: columns.iter().map(|c| matches!(c.r#type.category, TypeCategory::Integer | TypeCategory::Numeric)).collect(),
            },
            Format::Insert(target) => Shape::Insert { prefix: format!("INSERT INTO {} ({}) VALUES (", target.table, target.columns.join(", ")) },
        };
        Self { dialect, cols, shape, rows: 0 }
    }

    pub fn begin(&mut self, out: &mut Vec<u8>) {
        match &self.shape {
            Shape::Csv { header } => csv_record(out, header.iter().map(|h| Some(h.as_str()))),
            Shape::Json { .. } => out.push(b'['),
            Shape::Markdown { header, right } => {
                out.push(b'|');
                for h in header {
                    out.push(b' ');
                    markdown_text(out, h);
                    out.extend_from_slice(b" |");
                }
                out.extend_from_slice(b"\n|");
                for r in right {
                    out.extend_from_slice(if *r { b" ---: |" } else { b" --- |" });
                }
                out.push(b'\n');
            }
            Shape::Insert { .. } => {}
        }
    }

    /// `cells` has one entry per column, in column order.
    pub fn row<'a>(&mut self, cells: impl Iterator<Item = Option<&'a str>>, out: &mut Vec<u8>) {
        match &self.shape {
            Shape::Csv { .. } => csv_record(out, cells),
            Shape::Json { keys } => {
                out.extend_from_slice(if self.rows == 0 { b"\n  {" } else { b",\n  {" });
                for (i, (cell, col)) in cells.zip(&self.cols).enumerate() {
                    if i > 0 {
                        out.push(b',');
                    }
                    out.extend_from_slice(keys[i].as_bytes());
                    json_value(out, self.dialect, col.category, cell);
                }
                out.push(b'}');
            }
            Shape::Markdown { .. } => {
                out.push(b'|');
                for cell in cells {
                    out.push(b' ');
                    match cell {
                        None => out.extend_from_slice(b"NULL"),
                        Some(s) => markdown_text(out, s),
                    }
                    out.extend_from_slice(b" |");
                }
                out.push(b'\n');
            }
            Shape::Insert { prefix } => {
                out.extend_from_slice(prefix.as_bytes());
                for (i, (cell, col)) in cells.zip(&self.cols).enumerate() {
                    if i > 0 {
                        out.extend_from_slice(b", ");
                    }
                    sql_literal(out, self.dialect, col, cell);
                }
                out.extend_from_slice(b");\n");
                if self.dialect == DatabaseKind::SqlServer && (self.rows + 1) % INSERTS_PER_BATCH == 0 {
                    out.extend_from_slice(b"GO\n");
                }
            }
        }
        self.rows += 1;
    }

    /// Consumes the encoder, so nothing can be written after the footer.
    pub fn finish(self, out: &mut Vec<u8>) {
        if let Shape::Json { .. } = self.shape {
            out.extend_from_slice(if self.rows == 0 { b"]\n" } else { b"\n]\n" });
        }
    }
}

/// RFC 4180 with CRLF, NULL as an empty field and the empty string as `""`, as `COPY … CSV` writes.
fn csv_record<'a>(out: &mut Vec<u8>, cells: impl Iterator<Item = Option<&'a str>>) {
    for (i, cell) in cells.enumerate() {
        if i > 0 {
            out.push(b',');
        }
        match cell {
            None => {}
            Some(s) if s.is_empty() || s.contains(['"', ',', '\r', '\n']) => {
                out.push(b'"');
                out.extend_from_slice(s.replace('"', "\"\"").as_bytes());
                out.push(b'"');
            }
            Some(s) => out.extend_from_slice(s.as_bytes()),
        }
    }
    out.extend_from_slice(b"\r\n");
}

/// RFC 8259's number grammar: `-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`. `NaN`, `Infinity`,
/// PostgreSQL money (`$1.00`), and anything else fail and stay strings.
pub(crate) fn json_number(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = |mut i: usize| {
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        (i > start).then_some(i)
    };
    let mut i = usize::from(b.first() == Some(&b'-'));
    i = match b.get(i) {
        Some(b'0') => i + 1,
        Some(b'1'..=b'9') => match digits(i) {
            Some(end) => end,
            None => return false,
        },
        _ => return false,
    };
    if b.get(i) == Some(&b'.') {
        let Some(end) = digits(i + 1) else { return false };
        i = end;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let Some(end) = digits(i) else { return false };
        i = end;
    }
    i == b.len()
}

fn json_value(out: &mut Vec<u8>, dialect: DatabaseKind, category: TypeCategory, cell: Option<&str>) {
    use DatabaseKind::{Postgres, SqlServer};
    use TypeCategory::*;
    let Some(text) = cell else { return out.extend_from_slice(b"null") };
    match (category, dialect, text) {
        (Integer | Numeric, _, t) if json_number(t) => out.extend_from_slice(t.as_bytes()),
        (Bool, Postgres, "t") | (Bool, SqlServer, "1") => out.extend_from_slice(b"true"),
        (Bool, Postgres, "f") | (Bool, SqlServer, "0") => out.extend_from_slice(b"false"),
        // PostgreSQL validates json on input, so this fails only for text the server did not
        // produce; it keeps the file valid JSON whatever the cell holds.
        (Json, _, t) if serde_json::from_str::<serde::de::IgnoredAny>(t).is_ok() => out.extend_from_slice(t.as_bytes()),
        _ => serde_json::to_writer(&mut *out, text).expect("a Vec accepts every write"),
    }
}

/// First occurrences keep their names. A later duplicate becomes `name_2`, `name_3`, …, skipping
/// any key already taken, including a real column named `name_2`, so no value is lost the way a
/// parser's last-key-wins would lose it.
pub(crate) fn unique_keys(names: &[String]) -> Vec<String> {
    let mut taken: HashSet<&str> = HashSet::new();
    let firsts: Vec<bool> = names.iter().map(|n| taken.insert(n)).collect();
    let mut taken: HashSet<String> = taken.into_iter().map(str::to_owned).collect();
    names
        .iter()
        .zip(firsts)
        .map(|(name, first)| {
            if first {
                return name.clone();
            }
            let mut n = 2;
            while !taken.insert(format!("{name}_{n}")) {
                n += 1;
            }
            format!("{name}_{n}")
        })
        .collect()
}

/// `\` and `|` are escaped, and each line break (`\r\n`, `\r`, or `\n`) becomes `<br>`.
fn markdown_text(out: &mut Vec<u8>, s: &str) {
    let mut bytes = s.bytes().peekable();
    while let Some(b) = bytes.next() {
        match b {
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'|' => out.extend_from_slice(b"\\|"),
            b'\r' => {
                if bytes.peek() == Some(&b'\n') {
                    bytes.next();
                }
                out.extend_from_slice(b"<br>");
            }
            b'\n' => out.extend_from_slice(b"<br>"),
            _ => out.push(b),
        }
    }
}

fn mssql_hex(s: &str) -> bool {
    s.strip_prefix("0x").is_some_and(|hex| hex.len() % 2 == 0 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn sql_literal(out: &mut Vec<u8>, dialect: DatabaseKind, col: &Col, cell: Option<&str>) {
    use DatabaseKind::{Postgres, SqlServer};
    use TypeCategory::*;
    let Some(t) = cell else { return out.extend_from_slice(b"NULL") };
    let literal = match (dialect, col.category, t) {
        (_, Integer | Numeric, t) if json_number(t) => t.to_owned(),
        (Postgres, Bool, "t") => "true".into(),
        (Postgres, Bool, "f") => "false".into(),
        (SqlServer, Bool, "1" | "0") => t.to_owned(),
        // The SQL Server driver's canonical binary text is already a T-SQL binary literal.
        (SqlServer, Binary, t) if mssql_hex(t) => t.to_owned(),
        // Converts with an explicit style, so DATEFORMAT and LANGUAGE cannot change what a
        // datetime means. A type it cannot spell falls back to a national string literal.
        (SqlServer, _, t) => mssql_value::input_sql(&col.native, t).unwrap_or_else(|_| quote_literal_in(SqlServer, t)),
        // PostgreSQL's text output is accepted by the type's input function, including bytea's
        // `\x..` hex form under standard_conforming_strings; an untyped literal coerces on insert.
        (Postgres, _, t) => quote_literal_in(Postgres, t),
    };
    out.extend_from_slice(literal.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SqlTypeRef;

    fn col(name: &str, display: &str, category: TypeCategory) -> ColumnMeta {
        ColumnMeta { name: name.into(), r#type: SqlTypeRef { display: display.into(), category }, source: None }
    }

    fn encode(format: Format, dialect: DatabaseKind, columns: &[ColumnMeta], rows: &[&[Option<&str>]]) -> String {
        let mut enc = Encoder::new(format, dialect, columns);
        let mut out = Vec::new();
        enc.begin(&mut out);
        for row in rows {
            enc.row(row.iter().copied(), &mut out);
        }
        enc.finish(&mut out);
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn csv_quotes_like_copy_csv() {
        let cols = [col("c0", "text", TypeCategory::Text), col("c1", "text", TypeCategory::Text), col("c2", "text", TypeCategory::Text)];
        let csv = encode(Format::Csv, DatabaseKind::Postgres, &cols, &[&[Some("plain"), None, Some("")], &[Some("a,b"), Some("say \"hi\""), Some("line\nbreak")]]);
        assert_eq!(csv, "c0,c1,c2\r\nplain,,\"\"\r\n\"a,b\",\"say \"\"hi\"\"\",\"line\nbreak\"\r\n");
    }

    #[test]
    fn json_types_values_by_category_without_floating_point() {
        let cols = [
            col("id", "bigint", TypeCategory::Integer),
            col("n", "numeric", TypeCategory::Numeric),
            col("ok", "boolean", TypeCategory::Bool),
            col("doc", "jsonb", TypeCategory::Json),
            col("id", "text", TypeCategory::Text),
            col("cash", "money", TypeCategory::Numeric),
        ];
        let json = encode(
            Format::Json,
            DatabaseKind::Postgres,
            &cols,
            &[&[Some("9007199254740993"), Some("NaN"), Some("t"), Some(r#"{"a": [1, 2]}"#), None, Some("$1.00")], &[Some("-0.5"), Some("1e+20"), Some("f"), Some("not json"), Some("tab\there"), None]],
        );
        assert_eq!(
            json,
            "[\n  {\"id\":9007199254740993,\"n\":\"NaN\",\"ok\":true,\"doc\":{\"a\": [1, 2]},\"id_2\":null,\"cash\":\"$1.00\"},\n  {\"id\":-0.5,\"n\":1e+20,\"ok\":false,\"doc\":\"not json\",\"id_2\":\"tab\\there\",\"cash\":null}\n]\n"
        );
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 2);
        assert_eq!(encode(Format::Json, DatabaseKind::Postgres, &cols, &[]), "[]\n");
    }

    #[test]
    fn json_reads_sql_server_bits_as_booleans() {
        let cols = [col("flag", "bit", TypeCategory::Bool), col("n", "int", TypeCategory::Integer)];
        assert_eq!(encode(Format::Json, DatabaseKind::SqlServer, &cols, &[&[Some("1"), Some("7")], &[Some("0"), None]]), "[\n  {\"flag\":true,\"n\":7},\n  {\"flag\":false,\"n\":null}\n]\n");
    }

    #[test]
    fn duplicate_names_get_suffixes_that_skip_real_columns() {
        let names = ["id", "id_2", "id", "id", "x"].map(String::from);
        assert_eq!(unique_keys(&names), ["id", "id_2", "id_3", "id_4", "x"]);
    }

    #[test]
    fn markdown_escapes_pipes_backslashes_and_breaks() {
        let cols = [col("a|b", "text", TypeCategory::Text), col("n", "integer", TypeCategory::Integer)];
        let md = encode(Format::Markdown, DatabaseKind::Postgres, &cols, &[&[Some("x\\|y\r\nz\rw"), None]]);
        assert_eq!(md, "| a\\|b | n |\n| --- | ---: |\n| x\\\\\\|y<br>z<br>w | NULL |\n");
    }

    #[test]
    fn postgres_inserts_quote_by_category() {
        let cols = [
            col("id", "integer", TypeCategory::Integer),
            col("n", "numeric", TypeCategory::Numeric),
            col("ok", "boolean", TypeCategory::Bool),
            col("raw", "bytea", TypeCategory::Binary),
            col("note", "text", TypeCategory::Text),
        ];
        let target = InsertTarget { table: "\"public\".\"t\"".into(), columns: ["id", "n", "ok", "raw", "note"].iter().map(|c| quote_ident_in(DatabaseKind::Postgres, c)).collect() };
        let sql = encode(Format::Insert(target), DatabaseKind::Postgres, &cols, &[&[Some("1"), Some("NaN"), Some("f"), Some("\\x00ff"), Some("O'Brien")], &[None, Some("2.50"), Some("t"), None, None]]);
        assert_eq!(
            sql,
            "INSERT INTO \"public\".\"t\" (\"id\", \"n\", \"ok\", \"raw\", \"note\") VALUES (1, 'NaN', false, '\\x00ff', 'O''Brien');\n\
             INSERT INTO \"public\".\"t\" (\"id\", \"n\", \"ok\", \"raw\", \"note\") VALUES (NULL, 2.50, true, NULL, NULL);\n"
        );
    }

    #[test]
    fn sql_server_inserts_keep_binary_bare_bits_numeric_and_convert_temporal_with_a_style() {
        let cols = [
            col("b", "varbinary", TypeCategory::Binary),
            col("f", "bit", TypeCategory::Bool),
            col("s", "nvarchar", TypeCategory::Text),
            col("at", "datetime", TypeCategory::Temporal),
            col("d", "decimal", TypeCategory::Numeric),
            col("odd", "sql_variant", TypeCategory::Other),
        ];
        let target = InsertTarget { table: "[dbo].[t]".into(), columns: ["[b]", "[f]", "[s]", "[at]", "[d]", "[odd]"].map(String::from).to_vec() };
        let sql = encode(Format::Insert(target), DatabaseKind::SqlServer, &cols, &[&[Some("0x00FF"), Some("1"), Some("O'Brien"), Some("2024-01-02 03:04:05.123"), Some("-12.50"), Some("x")]]);
        assert_eq!(sql, "INSERT INTO [dbo].[t] ([b], [f], [s], [at], [d], [odd]) VALUES (0x00FF, 1, N'O''Brien', CONVERT(datetime, N'2024-01-02 03:04:05.123', 121), -12.50, N'x');\n");
    }

    #[test]
    fn sql_server_inserts_break_into_batches_and_postgres_never_does() {
        let cols = [col("n", "int", TypeCategory::Integer)];
        let rows: Vec<[Option<&str>; 1]> = (0..2_001).map(|_| [Some("1")]).collect();
        let rows: Vec<&[Option<&str>]> = rows.iter().map(|r| r.as_slice()).collect();
        let script = encode(Format::Insert(InsertTarget::placeholder(DatabaseKind::SqlServer, &cols)), DatabaseKind::SqlServer, &cols, &rows);
        let lines: Vec<&str> = script.lines().collect();
        assert_eq!(lines.iter().filter(|l| **l == "GO").count(), 2);
        assert_eq!((lines[1_000], lines[2_001]), ("GO", "GO"));
        assert_eq!(lines[0], "INSERT INTO target_table ([n]) VALUES (1);");
        assert_eq!(lines.len(), 2_003);
        let pg = encode(Format::Insert(InsertTarget::placeholder(DatabaseKind::Postgres, &cols)), DatabaseKind::Postgres, &cols, &rows);
        assert!(!pg.contains("GO"));
        assert!(pg.starts_with("INSERT INTO target_table (\"n\") VALUES (1);\n"));
    }

    #[test]
    fn json_number_grammar() {
        for ok in ["0", "-0", "12", "1.50", "1e+20", "-0.00000000000000000000000000000000000001", "6.02E23"] {
            assert!(json_number(ok), "{ok}");
        }
        for bad in ["", "-", "01", "1.", ".5", "+1", "1e", "NaN", "Infinity", "-Infinity", "$1.00", "1,000", "0x1F", "inf"] {
            assert!(!json_number(bad), "{bad}");
        }
    }
}
