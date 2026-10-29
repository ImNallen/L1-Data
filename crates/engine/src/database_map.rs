use std::sync::Arc;

use crate::model::*;
use crate::{Engine, Shared};

pub(crate) const PAGE_SIZE: usize = 32;

pub(crate) struct Neighborhood {
    pub root: MapRelation,
    pub relations: Vec<MapRelation>,
    pub foreign_keys: Vec<MapForeignKey>,
}

impl Neighborhood {
    pub fn new(root: MapRelation) -> Self {
        Self { root, relations: vec![], foreign_keys: vec![] }
    }

    pub fn pair(&mut self, key: MapForeignKey, source: MapRelation, target: MapRelation, pair: MapColumnPair) {
        for (mut relation, column) in [(source, pair.source.clone()), (target, pair.target.clone())] {
            if let Some(held) = self.relations.iter_mut().find(|r| r.oid == relation.oid) {
                if !held.columns.contains(&column) { held.columns.push(column); }
            } else {
                relation.columns.push(column);
                self.relations.push(relation);
            }
        }
        if let Some(held) = self.foreign_keys.iter_mut().find(|k| k.id == key.id) {
            held.columns.push(pair);
        } else {
            self.foreign_keys.push(MapForeignKey { columns: vec![pair], ..key });
        }
    }

    fn page(mut self, db: DatabaseRef, catalog_version: u64) -> MapNeighborhood {
        let more = self.foreign_keys.len() > PAGE_SIZE;
        self.foreign_keys.truncate(PAGE_SIZE);
        // The lookahead key must not leak endpoints or columns into the displayed page.
        for relation in &mut self.relations {
            relation.columns.retain(|column| self.foreign_keys.iter().any(|k| k.columns.iter().any(|p| (k.source == relation.oid && p.source == *column) || (k.target == relation.oid && p.target == *column))));
        }
        self.relations.retain(|r| self.foreign_keys.iter().any(|k| k.source == r.oid || k.target == r.oid));
        if let Some(root) = self.relations.iter().find(|r| r.oid == self.root.oid) { self.root = root.clone(); }
        let after = more.then(|| MapCursor { db: db.clone(), root: self.root.oid, catalog_version, last_constraint: self.foreign_keys.last().unwrap().id });
        MapNeighborhood { db, catalog_version, root: self.root, relations: self.relations, foreign_keys: self.foreign_keys, after }
    }
}

impl Shared {
    fn map_current(&self, instance: &Arc<crate::connection::Instance>, slot: &Arc<crate::connection::DbSlot>, version: u64) -> bool {
        !instance.is_closed()
            && self.instance(slot.db.connection_id).is_some_and(|i| Arc::ptr_eq(&i, instance))
            && instance.slot(&slot.db.database).is_some_and(|s| Arc::ptr_eq(&s, slot))
            && slot.last().is_some_and(|c| c.version == version)
    }
}

impl Engine {
    pub async fn database_map_neighborhood(&self, db: DatabaseRef, oid: u32, catalog_version: u64, after: Option<MapCursor>) -> Result<MapNeighborhood, MapError> {
        let shared = &self.shared;
        let (instance, slot) = shared.slot(&db).map_err(|error| MapError::Catalog { error })?;
        shared.ensure_catalog(&instance, &slot).await.map_err(|error| MapError::Catalog { error })?;
        let mut meta = slot.meta.lock().await;
        if !shared.map_current(&instance, &slot, catalog_version) { return Err(MapError::Stale); }
        if after.as_ref().is_some_and(|c| c.db != slot.db || c.root != oid || c.catalog_version != catalog_version) { return Err(MapError::Stale); }
        let session = shared.session_in(&instance, &slot, &mut meta).await.map_err(|e| MapError::Failed { message: crate::connection::connect_error_text(&e) })?;
        let neighborhood = session.map_neighborhood(oid, after.map_or(0, |c| c.last_constraint)).await.map_err(|e| MapError::Failed { message: e.message })?;
        if !shared.map_current(&instance, &slot, catalog_version) { return Err(MapError::Stale); }
        Ok(neighborhood.ok_or(MapError::Gone)?.page(slot.db.clone(), catalog_version))
    }
}

impl Engine {
    pub async fn database_map_query(&self, db: DatabaseRef, text: String, cursor: DocOffset, catalog_version: u64) -> Result<MapQuery, MapError> {
        let shared = &self.shared;
        let (instance,slot) = shared.slot(&db).map_err(|error| MapError::Catalog { error })?;
        shared.ensure_catalog(&instance,&slot).await.map_err(|error| MapError::Catalog { error })?;
        let _meta = slot.meta.lock().await;
        if !shared.map_current(&instance,&slot,catalog_version) { return Err(MapError::Stale); }
        let catalog = slot.last().ok_or(MapError::Stale)?;
        let result = crate::sql::map::analyze(&text,cursor,&catalog);
        if !shared.map_current(&instance,&slot,catalog_version) { return Err(MapError::Stale); }
        Ok(result)
    }
}
