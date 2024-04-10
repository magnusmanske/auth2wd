use crate::external_id::*;
use crate::external_importer::*;
use crate::meta_item::*;
use anyhow::{anyhow, Result};
use std::collections::HashMap;

lazy_static! {
    pub static ref SUPPORTED_PROPERTIES: Vec<SupportedProperty> = {
        vec![
            SupportedProperty::new(
                214,
                "VIAF",
                "Virtual International Authority File",
                "27063124",
                None,
            ),
            SupportedProperty::new(227, "GND", "Deutsche Nationalbibliothek", "118523813", None),
            SupportedProperty::new(244, "LoC", "Library of Congress", "n78095637", None),
            SupportedProperty::new(
                268,
                "BnF",
                "Bibliothèque nationale de France",
                "11898689q",
                None,
            ),
            SupportedProperty::new(269, "IdRef", "IdRef/SUDOC", "026812304", None),
            SupportedProperty::new(906, "SELIBR", "National Library of Sweden", "231727", None),
            SupportedProperty::new(
                950,
                "BNE",
                "Biblioteca Nacional de España",
                "XX990809",
                None,
            ),
            SupportedProperty::new(
                1015,
                "NORAF",
                "Norwegian Authority File",
                "90053126",
                Some("Rainer Maria Rilke".into()),
            ),
            SupportedProperty::new(
                1006,
                "NB",
                "Nationale Thesaurus voor Auteurs ID",
                "068364229",
                None,
            ),
        ]
    };
}

pub struct SupportedProperty {
    pub property: usize,
    pub name: String,
    pub source: String,
    pub demo_id: String,
    pub demo_name: String,
}

unsafe impl Send for SupportedProperty {}
unsafe impl Sync for SupportedProperty {}

impl SupportedProperty {
    fn new(
        property: usize,
        name: &str,
        source: &str,
        demo_id: &str,
        demo_name: Option<String>,
    ) -> Self {
        Self {
            property,
            name: name.into(),
            source: source.into(),
            demo_id: demo_id.into(),
            demo_name: demo_name.unwrap_or("Charles Darwin".into()),
        }
    }

    pub async fn generator(&self, id: &str) -> Result<Box<dyn ExternalImporter + Send + Sync>> {
        let ret: Box<dyn ExternalImporter + Send + Sync> = match self.property {
            214 => Box::new(crate::viaf::VIAF::new(id).await?),
            227 => Box::new(crate::gnd::GND::new(id).await?),
            244 => Box::new(crate::loc::LOC::new(id).await?),
            268 => Box::new(crate::bnf::BNF::new(id).await?),
            269 => Box::new(crate::id_ref::IdRef::new(id).await?),
            906 => Box::new(crate::selibr::SELIBR::new(id).await?),
            950 => Box::new(crate::bne::BNE::new(id).await?),
            1006 => Box::new(crate::nb::NB::new(id).await?),
            1015 => Box::new(crate::noraf::NORAF::new(id).await?),
            _ => panic!("no generator for property: 'P{}'", self.property),
        };
        Ok(ret)
    }

    pub fn as_li(&self) -> String {
        format!(
            r#"<li><a href="/item/P{}/{}">{}</a> ("{}" from {})</li>"#,
            self.property, &self.demo_id, &self.name, &self.demo_name, &self.source,
        )
    }
}

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
        SUPPORTED_PROPERTIES.iter().map(|sp| sp.property).collect()
    }

    pub async fn get_parser_for_ext_id(
        id: &ExternalId,
    ) -> Result<Box<dyn ExternalImporter + Send + Sync>> {
        match SUPPORTED_PROPERTIES
            .iter()
            .find(|sp| sp.property == id.property())
        {
            Some(sp) => sp.generator(id.id()).await,
            None => Err(anyhow!("unsupported property: '{}'", id.property())),
        }
    }

    pub fn has_parser_for_ext_id(id: &ExternalId) -> bool {
        SUPPORTED_PROPERTIES
            .iter()
            .any(|sp| sp.property == id.property())
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
