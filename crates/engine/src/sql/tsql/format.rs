use super::{Kind, STATEMENT_WORDS, Token, lex, split_batches};

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

#[cfg(test)]
mod tests {
    use super::*;

    fn batches(text: &str) -> Vec<&str> {
        split_batches(text).unwrap().iter().map(|s| s.text(text)).collect()
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
