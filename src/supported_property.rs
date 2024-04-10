use crate::external_importer::*;
use anyhow::{anyhow, Result};

lazy_static! {
    /// Examples of all supported properties
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
    property: usize,
    name: String,
    source: String,
    demo_id: String,
    demo_name: String,
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
            _ => return Err(anyhow!("no generator for property: 'P{}'", self.property)),
        };
        Ok(ret)
    }

    pub fn as_li(&self) -> String {
        format!(
            r#"<li><a href="/item/P{}/{}">{}</a> ("{}" from {})</li>"#,
            self.property, &self.demo_id, &self.name, &self.demo_name, &self.source,
        )
    }

    pub fn property(&self) -> usize {
        self.property
    }
}
