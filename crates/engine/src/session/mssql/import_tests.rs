use super::*;

#[tokio::test]
async fn import_lock_blocks_ddl_on_empty_and_populated_tables_and_late_error_rolls_back() {
    for populated in [false, true] {
        let mut owner = MsSession::open(&tests::params()).await.unwrap();
        let mut other = MsSession::open(&tests::params()).await.unwrap();
        let table = format!("dbo.import_lock_{}", uuid::Uuid::new_v4().simple());
        owner.batch(&format!("CREATE TABLE {table}(id int PRIMARY KEY, value int CHECK(value > 0))")).await.unwrap();
        if populated { owner.batch(&format!("INSERT {table} VALUES (0,1)")).await.unwrap(); }
        owner.batch(&format!("BEGIN TRANSACTION; SELECT TOP (1) 1 FROM {table} WITH (TABLOCKX,HOLDLOCK)")).await.unwrap();
        assert_eq!(owner.txn().0, TxnState::Open);
        for ddl in [format!("ALTER TABLE {table} ADD extra int"), format!("DROP TABLE {table}"), format!("CREATE TRIGGER import_trigger_{} ON {table} AFTER INSERT AS SELECT 1", uuid::Uuid::new_v4().simple())] {
            other.batch("SET LOCK_TIMEOUT 100").await.unwrap();
            let error = other.batch(&ddl).await.unwrap_err();
            assert!(error.message.contains("Lock request time out"), "{error:?}");
        }
        owner.batch(&format!("INSERT {table} VALUES (1,1),(2,2)")).await.unwrap();
        assert!(owner.batch(&format!("INSERT {table} VALUES (3,-1)")).await.is_err());
        owner.batch("ROLLBACK TRANSACTION").await.unwrap();
        assert_eq!(owner.txn().0, TxnState::Idle);
        assert_eq!(other.count(&format!("SELECT COUNT_BIG(*) FROM {table}")).await.unwrap(), u64::from(populated));
        other.batch(&format!("DROP TABLE {table}")).await.unwrap();
    }
}
