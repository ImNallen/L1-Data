use crate::model::{Catalog, Relation};

impl Catalog {
    pub fn relation(&self, schema: &str, table: &str) -> Option<&Relation> {
        self.schemas.iter().find(|s| s.name == schema)?.relations.iter().find(|r| r.name == table)
    }

    pub fn relation_by_oid(&self, oid: u32) -> Option<(&str, &Relation)> {
        self.schemas.iter().find_map(|s| s.relations.iter().find(|r| r.oid == oid).map(|r| (s.name.as_str(), r)))
    }
}

impl Relation {
    pub fn pk_columns(&self) -> Vec<String> {
        self.columns.iter().filter(|c| c.pk).map(|c| c.name.clone()).collect()
    }
}
