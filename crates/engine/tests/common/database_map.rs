use engine::model::*;

pub fn fixture(schema: &str, kind: DatabaseKind) -> String {
    let fan_columns = (0..35).map(|i| format!("c{i} int, CONSTRAINT fan_{i} FOREIGN KEY(c{i}) REFERENCES {schema}.root(a)")).collect::<Vec<_>>().join(",");
    let mut sql = format!("CREATE TABLE {schema}.anchor(id int PRIMARY KEY);
        CREATE TABLE {schema}.root(a int NOT NULL,b int NOT NULL,other int,
            PRIMARY KEY(a,b),UNIQUE(a),CONSTRAINT outgoing FOREIGN KEY(other) REFERENCES {schema}.anchor(id),
            CONSTRAINT self_pair FOREIGN KEY(a,b) REFERENCES {schema}.root(a,b));
        CREATE TABLE {schema}.child(x int,y int,CONSTRAINT composite FOREIGN KEY(x,y) REFERENCES {schema}.root(a,b));
        CREATE TABLE {schema}.fan({fan_columns});
        CREATE TABLE {schema}.alone(id int);");
    if kind == DatabaseKind::SqlServer { sql.push_str(&format!("ALTER TABLE {schema}.fan NOCHECK CONSTRAINT fan_0;")); }
    sql
}

pub async fn verify(engine: &engine::Engine, db: DatabaseRef, schema: &str, kind: DatabaseKind) {
    engine.refresh_catalog(db.clone()).await.unwrap();
    // DDL run completion can precede the coalesced catalog refresh it scheduled.
    let (catalog, first) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let catalog = engine.catalog(db.clone()).await.unwrap();
            let root = catalog.schemas.iter().find(|s| s.name==schema).unwrap().relations.iter().find(|r| r.name=="root").unwrap().oid;
            match engine.database_map_neighborhood(db.clone(),root,catalog.version,None).await {
                Ok(first) => break (catalog, first),
                Err(MapError::Stale) => tokio::time::sleep(std::time::Duration::from_millis(20)).await,
                Err(e) => panic!("{e:?}"),
            }
        }
    }).await.unwrap();
    let relations = &catalog.schemas.iter().find(|s| s.name==schema).unwrap().relations;
    let root = relations.iter().find(|r| r.name=="root").unwrap().oid;
    let alone = relations.iter().find(|r| r.name=="alone").unwrap().oid;
    assert_eq!(first.foreign_keys.len(),32);
    assert_eq!(first.root.name,"root");
    assert!(first.relations.iter().all(|r| r.columns.len()<=37));
    let cursor = first.after.clone().expect("pagination");
    for invalid in [MapCursor { root: alone,..cursor.clone() }, MapCursor { db: DatabaseRef { database:"reporting".into(),..db.clone() },..cursor.clone() }, MapCursor { catalog_version:catalog.version+1,..cursor.clone() }] {
        assert!(matches!(engine.database_map_neighborhood(db.clone(),root,catalog.version,Some(invalid)).await,Err(MapError::Stale)));
    }
    let second = engine.database_map_neighborhood(db.clone(),root,catalog.version,Some(cursor)).await.unwrap();
    assert!(second.after.is_none());
    let keys = first.foreign_keys.into_iter().chain(second.foreign_keys).collect::<Vec<_>>();
    assert_eq!(keys.len(),38);
    assert!(keys.windows(2).all(|k| k[0].id<k[1].id));
    let composite = keys.iter().find(|k| k.name=="composite").unwrap();
    assert_eq!(composite.target,root);
    assert_eq!(composite.columns.iter().map(|p| (p.source.as_str(),p.target.as_str())).collect::<Vec<_>>(),[("x","a"),("y","b")]);
    let self_key = keys.iter().find(|k| k.name=="self_pair").unwrap();
    assert_eq!((self_key.source,self_key.target),(root,root));
    assert_eq!(self_key.columns.len(),2);
    assert_eq!(keys.iter().filter(|k| k.name.starts_with("fan_")).count(),35,"parallel constraints stay separate");
    assert!(keys.iter().any(|k| k.source==root && k.target!=root),"outgoing key");
    if kind==DatabaseKind::SqlServer {
        let disabled = keys.iter().find(|k| k.name=="fan_0").unwrap();
        assert!(!disabled.enabled && !disabled.validated);
    }
    let isolated = engine.database_map_neighborhood(db.clone(),alone,catalog.version,None).await.unwrap();
    assert_eq!(isolated.root.name,"alone");
    assert!(isolated.foreign_keys.is_empty() && isolated.after.is_none());
    assert!(matches!(engine.database_map_neighborhood(db.clone(),0,catalog.version,None).await,Err(MapError::Gone)));
    engine.refresh_catalog(db.clone()).await.unwrap();
    assert!(matches!(engine.database_map_neighborhood(db,root,catalog.version,None).await,Err(MapError::Stale)));
}
