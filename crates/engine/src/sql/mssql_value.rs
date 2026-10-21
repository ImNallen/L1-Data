use super::tsql::quote_literal;
use crate::model::ColumnInfo;

pub(crate) enum Input<'a> {
    Parameter { index: usize, value: Option<&'a str> },
    Literal(&'a str),
}

pub(crate) fn input_sql(column: &ColumnInfo, input: Input<'_>) -> Result<String, String> {
    let (expression, value) = match input {
        Input::Parameter { index, value } => (format!("@P{index}"), value),
        Input::Literal(value) => (quote_literal(value), Some(value)),
    };
    let native = column.type_display.as_str();
    let base = native.split('(').next().unwrap_or(native);
    let bound = native.strip_suffix(')').and_then(|s| s.split_once('(')).and_then(|(_, n)| n.parse::<usize>().ok());
    match base {
        "char" | "varchar" | "nchar" | "nvarchar" | "text" | "ntext" => {
            if matches!(base, "nchar" | "nvarchar") && value.is_some_and(|v| bound.is_some_and(|n| v.encode_utf16().count() > n)) {
                return Err(format!("Text value exceeds the capacity of {} ({native})", column.name));
            }
            Ok(expression)
        }
        "binary" | "varbinary" | "image" | "timestamp" | "rowversion" => {
            if let Some(value) = value {
                let hex = value.strip_prefix("0x").ok_or("Binary values must use 0x-prefixed hexadecimal text")?;
                if hex.len() % 2 != 0 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err("Binary values must contain complete hexadecimal bytes".into());
                }
                let bound = if matches!(base, "timestamp" | "rowversion") { Some(8) } else { bound };
                if bound.is_some_and(|n| hex.len() / 2 > n) {
                    return Err(format!("Binary value exceeds the capacity of {} ({native})", column.name));
                }
            }
            Ok(format!("CONVERT(varbinary(max), {expression}, 1)"))
        }
        "date" => Ok(format!("CONVERT({native}, {expression}, 23)")),
        "time" | "datetime" | "smalldatetime" | "datetime2" | "datetimeoffset" => Ok(format!("CONVERT({native}, {expression}, 121)")),
        "tinyint" | "smallint" | "int" | "bigint" | "decimal" | "numeric" | "money" | "smallmoney" | "float" | "real" | "bit" | "uniqueidentifier" | "xml" => {
            Ok(format!("CONVERT({native}, {expression})"))
        }
        _ => Err(format!("Canonical input for SQL Server type {native} is unsupported")),
    }
}

#[cfg(all(test, feature = "mssql-tests"))]
mod tests {
    use crate::connection::ConnectParams;
    use crate::model::*;
    use crate::session::Session;
    use crate::text;
    use futures_util::FutureExt;
    use std::panic::AssertUnwindSafe;

    #[tokio::test]
    async fn generated_counts_use_canonical_values_under_each_session_dateformat() {
        let params = ConnectParams {
            database_kind: DatabaseKind::SqlServer,
            host: std::env::var("L1_DATA_TEST_MSSQL_HOST").unwrap_or_else(|_| "localhost".into()),
            port: std::env::var("L1_DATA_TEST_MSSQL_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(1434),
            database: "shop".into(),
            user: "sa".into(),
            password: Some(std::env::var("L1_DATA_TEST_MSSQL_PASSWORD").unwrap_or_else(|_| "L1Data_dev_Str0ng".into())),
            ssl_mode: SslMode::Require,
            ca_cert_path: None,
        };
        let mut session = Session::open(&params, None, "UTC").await.unwrap();
        let table = format!("l1_typed_count_{}", uuid::Uuid::new_v4().simple());
        session.batch(&format!("CREATE TABLE dbo.[{table}] (old datetime, small smalldatetime, day date, modern datetime2(7), moment datetimeoffset(7), tick time(7), bytes varbinary(4)); INSERT INTO dbo.[{table}] VALUES (N'2024-01-02T03:04:05.123',N'2024-01-02T03:04:00',N'20240102',N'2024-01-02T03:04:05.1234567',N'2024-01-02T03:04:05.1234567+02:00',N'03:04:05.1234567',0xDEADBEEF)")).await.unwrap();
        let outcome = AssertUnwindSafe(async {
            let db = DatabaseRef { connection_id: ConnectionId(uuid::Uuid::new_v4()), database: "shop".into() };
            let catalog = session.catalog(db, 1, &[]).await.unwrap();
            let relation = catalog.relation("dbo", &table).unwrap();
            for dateformat in ["dmy", "mdy"] {
                session.batch(&format!("SET DATEFORMAT {dateformat}")).await.unwrap();
                for (column, value) in [
                    ("old", "2024-01-02 03:04:05.123"),
                    ("small", "2024-01-02 03:04:00"),
                    ("day", "2024-01-02"),
                    ("modern", "2024-01-02 03:04:05.1234567"),
                    ("moment", "2024-01-02 03:04:05.1234567+02:00"),
                    ("tick", "03:04:05.1234567"),
                    ("bytes", "0xDEADBEEF"),
                ] {
                    for (op, expected) in [(FilterOp::Eq, 1), (FilterOp::Ne, 0)] {
                        let query = TableQuery { filters: vec![ColumnFilter { column: column.into(), op, value: value.into() }], ..Default::default() };
                        let sql = text::table_count(DatabaseKind::SqlServer, "dbo", &table, Some(relation), &query).unwrap();
                        assert_eq!(session.count(&sql).await.unwrap(), expected, "{column} {op:?} under {dateformat}");
                    }
                }
            }
        })
        .catch_unwind()
        .await;
        session.batch(&format!("DROP TABLE dbo.[{table}]")).await.unwrap();
        assert_eq!(session.count(&format!("SELECT COUNT_BIG(*) FROM sys.tables WHERE name=N'{table}'")).await.unwrap(), 0);
        if let Err(panic) = outcome {
            std::panic::resume_unwind(panic);
        }
    }
}
