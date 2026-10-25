pub mod postgres;
pub mod tsql;

pub fn title(name: &str) -> String {
    format!("{name}.sql")
}
