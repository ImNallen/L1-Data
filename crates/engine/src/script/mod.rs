use crate::model::ObjectScript;

pub mod postgres;
pub mod tsql;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Ddl {
    pub text: String,
    pub warnings: Vec<String>,
}

impl Ddl {
    pub fn complete(text: String) -> Ddl {
        Ddl { text, warnings: vec![] }
    }
}

pub fn object_script(name: &str, ddl: Ddl) -> ObjectScript {
    let warnings: Vec<String> = ddl.warnings.iter().map(|w| w.replace(['\r', '\n'], " ")).collect();
    let mut text: String = warnings.iter().map(|w| format!("-- L1 Data could not script: {w}\n")).collect();
    if !text.is_empty() {
        text.push('\n');
    }
    text.push_str(&ddl.text);
    ObjectScript { title: format!("{name}.sql"), text, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warnings_head_the_text_one_comment_line_each() {
        let s = object_script("t", Ddl { text: "CREATE TABLE t ();\n".into(), warnings: vec!["privileges on public.t".into(), "rules on public.\"a\nb\"".into()] });
        assert_eq!(s.title, "t.sql");
        assert_eq!(s.text, "-- L1 Data could not script: privileges on public.t\n-- L1 Data could not script: rules on public.\"a b\"\n\nCREATE TABLE t ();\n");
        assert_eq!(s.warnings, ["privileges on public.t", "rules on public.\"a b\""]);
        assert_eq!(object_script("t", Ddl::complete("SELECT 1;\n".into())).text, "SELECT 1;\n");
    }
}
