use crate::external_id::*;
use crate::external_importer::*;
use crate::meta_item::*;
use crate::supported_property::SUPPORTED_PROPERTIES;
use anyhow::{anyhow, Result};
use futures::future::join_all;
use std::collections::HashMap;
use std::collections::HashSet;
use wikimisc::merge_diff::MergeDiff;

#[derive(Debug, Clone, Default)]
pub struct Combinator {
    pub items: HashMap<String, MetaItem>,
}

impl Combinator {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get_parser_for_property(
        property: &str,
        id: &str,
    ) -> Result<Box<dyn ExternalImporter>> {
        let property = match ExternalId::prop_numeric(property) {
            Some(property) => property,
            None => return Err(anyhow!("malformed property: '{property}'")),
        };
        let ext_id = ExternalId::new(property, id);
        Self::get_parser_for_ext_id(&ext_id).await
    }

    pub fn get_supported_properties() -> Vec<usize> {
        SUPPORTED_PROPERTIES
            .iter()
            .map(|sp| sp.property())
            .collect()
    }

    pub async fn get_parser_for_ext_id(id: &ExternalId) -> Result<Box<dyn ExternalImporter>> {
        match SUPPORTED_PROPERTIES
            .iter()
            .find(|sp| sp.property() == id.property())
        {
            Some(sp) => sp.generator(id.id()).await,
            None => Err(anyhow!("unsupported property: '{}'", id.property())),
        }
    }

    pub fn has_parser_for_ext_id(id: &ExternalId) -> bool {
        SUPPORTED_PROPERTIES
            .iter()
            .any(|sp| sp.property() == id.property())
    }

    async fn import_get_parsers(
        &self,
        ids: &Vec<ExternalId>,
        ids_used: &mut HashSet<ExternalId>,
    ) -> Vec<Box<dyn ExternalImporter>> {
        let mut futures = vec![];
        for ext_id in ids {
            ids_used.insert(ext_id.to_owned());
            let parser = Self::get_parser_for_ext_id(ext_id);
            futures.push(parser);
        }
        let parsers = join_all(futures).await;
        let parsers: Vec<Box<dyn ExternalImporter>> = parsers
            .into_iter()
            .filter_map(|parser| parser.ok())
            .collect();
        parsers
    }

    pub async fn import(&mut self, ids: Vec<ExternalId>) -> Result<()> {
        let mut ids_used: HashSet<ExternalId> = HashSet::new();
        let mut ids = ids.to_owned();
        while !ids.is_empty() {
            ids.sort();
            ids.dedup();
            let parsers = self.import_get_parsers(&ids, &mut ids_used).await;
            ids.clear();
            let mut futures = vec![];
            let mut running_parsers = vec![];
            for parser in &parsers {
                let key = ExternalId::new(parser.my_property(), &parser.my_id()).to_string();
                if self.items.contains_key(&key) {
                    continue;
                }
                running_parsers.push(parser);
                futures.push(parser.run());
            }
            let items = join_all(futures).await;
            for (parser, item) in std::iter::zip(running_parsers, items) {
                let item = match item {
                    Ok(item) => item,
                    Err(_) => continue,
                };
                let key = ExternalId::new(parser.my_property(), &parser.my_id()).to_string();
                if self.items.contains_key(&key) {
                    continue;
                }
                let external_ids = item.get_external_ids();
                self.items.insert(key, item);
                for external_id in external_ids {
                    if !ids_used.contains(&external_id) && !ids.contains(&external_id) {
                        ids.push(external_id.to_owned());
                    }
                }
            }
        }
        Ok(())
    }

    pub fn combine(&mut self) -> Option<(MetaItem, MergeDiff)> {
        let mut merge_diff = MergeDiff::default();
        while self.items.len() > 1 {
            let keys: Vec<String> = self.items.keys().cloned().collect();
            let k1 = &keys[0];
            let k2 = &keys[1];
            let other = self.items.remove(k2)?;
            let diff = self.items.get_mut(k1)?.merge(&other);
            merge_diff.extend(&diff);
        }
        // self.items
        //     .iter_mut()
        //     .for_each(|(_id, mi)| mi.clear_fake_statement_ids());
        let meta_item = self.items.iter().next().map(|(_, v)| v.to_owned())?;
        Some((meta_item, merge_diff))
    }

    pub fn combine_on_base_item(&mut self, base_item: &mut MetaItem) -> Option<MergeDiff> {
        let mut merge_diff = MergeDiff::default();
        if self.items.is_empty() {
            return None;
        }
        for (_id, item) in self.items.iter() {
            let diff = base_item.merge(item);
            diff.apply(&mut base_item.item);
            merge_diff.extend(&diff);
        }
        Some(merge_diff)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use wikimisc::wikibase::{EntityTrait, ItemEntity};

    use super::*;

    #[test]
    fn test_combine() {
        // this test does not work correctly ... yet!
        let mut combinator = Combinator::new();

        let s1 = include_str!("../test_data/item1.json");
        let j1: Value = serde_json::from_str(s1).unwrap();
        let i1 = ItemEntity::new_from_json(&j1).unwrap();
        let mi1 = MetaItem::new_from_item(i1);

        let s2 = include_str!("../test_data/item2.json");
        let j2: Value = serde_json::from_str(s2).unwrap();
        let i2 = ItemEntity::new_from_json(&j2).unwrap();
        let mi2 = MetaItem::new_from_item(i2);

        combinator.items.insert("Q1".to_string(), mi1.to_owned());
        combinator.items.insert("Q2".to_string(), mi2.to_owned());
        combinator.items.insert("Q3".to_string(), mi2.to_owned());
        let (res_item1, _res_diff1) = combinator.combine().unwrap();

        combinator.items.insert("Q2".to_string(), mi2.to_owned());
        combinator.items.insert("Q1".to_string(), mi1.to_owned());
        combinator.items.insert("Q3".to_string(), mi1.to_owned());
        let (res_item2, _res_diff2) = combinator.combine().unwrap();

        assert_eq!(res_item1.item.claims().len(), res_item2.item.claims().len());
    }
}
