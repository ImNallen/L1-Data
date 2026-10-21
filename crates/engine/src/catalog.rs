use crate::model::{Catalog, Relation};

impl Catalog {
    pub fn relation(&self, schema: &str, table: &str) -> Option<&Relation> {
        self.schemas.iter().find(|s| s.name == schema)?.relations.iter().find(|r| r.name == table)
    }
}

impl Relation {
    pub fn pk_columns(&self) -> Vec<String> {
        self.columns.iter().filter(|c| c.pk).map(|c| c.name.clone()).collect()
    }
}

