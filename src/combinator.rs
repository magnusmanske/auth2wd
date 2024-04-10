use crate::external_id::*;
use crate::external_importer::*;
use crate::meta_item::*;
use crate::supported_property::SUPPORTED_PROPERTIES;
use anyhow::{anyhow, Result};
use std::collections::HashMap;

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
    ) -> Result<Box<dyn ExternalImporter + Send + Sync>> {
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

    pub async fn get_parser_for_ext_id(
        id: &ExternalId,
    ) -> Result<Box<dyn ExternalImporter + Send + Sync>> {
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

    pub async fn import(&mut self, ids: Vec<ExternalId>) -> Result<()> {
        let mut ids_used: Vec<ExternalId> = vec![];
        let mut ids = ids.to_owned();
        while !ids.is_empty() {
            let id = match ids.pop() {
                Some(id) => id,
                None => break,
            };
            ids_used.push(id.to_owned());
            let parser = match Self::get_parser_for_property(
                &format!("P{}", id.property()),
                id.id(),
            )
            .await
            {
                Ok(parser) => parser,
                _ => continue,
            };
            let key = ExternalId::new(id.property(), &parser.my_id()).to_string();
            if self.items.contains_key(&key) {
                continue;
            }
            let item = parser.run().await?;
            let external_ids = item.get_external_ids();
            self.items.insert(key, item);
            for external_id in external_ids {
                if !ids_used.contains(&external_id) && !ids.contains(&external_id) {
                    ids.push(external_id.to_owned());
                }
            }
        }
        Ok(())
    }

    pub fn combine(&mut self) -> Option<MetaItem> {
        while self.items.len() > 1 {
            let keys: Vec<String> = self.items.keys().cloned().collect();
            let k1 = &keys[0];
            let k2 = &keys[1];
            let other = self.items.get(k2)?.to_owned();
            let _ = self.items.get_mut(k1)?.merge(&other);
            self.items.remove(k2);
        }
        self.items.iter().next().map(|(_, v)| v.to_owned())
    }
}
