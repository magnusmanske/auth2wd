use crate::external_importer::*;
use anyhow::Result;
use std::future::Future;
use std::pin::Pin;

/// Type alias for an async factory function that creates a boxed `ExternalImporter` from an ID.
type ImporterFactory =
    fn(&str) -> Pin<Box<dyn Future<Output = Result<Box<dyn ExternalImporter>>> + Send + '_>>;

/// Macro to create an `ImporterFactory` for a given importer type.
/// The type must have `pub async fn new(id: &str) -> Result<Self>` and implement `ExternalImporter`.
macro_rules! importer_factory {
    ($type:ty) => {
        (|id: &str| -> Pin<Box<dyn Future<Output = Result<Box<dyn ExternalImporter>>> + Send + '_>> {
            Box::pin(async move { Ok(Box::new(<$type>::new(id).await?) as Box<dyn ExternalImporter>) })
        }) as ImporterFactory
    };
}

lazy_static! {
    /// Examples of all supported properties
    pub static ref SUPPORTED_PROPERTIES: Vec<SupportedProperty> = {
        vec![
            SupportedProperty::new(
                213,
                "ISNI",
                "International Standard Name Identifier",
                "0000000121251077",
                None,
                importer_factory!(crate::isni::ISNI),
            ),
            SupportedProperty::new(
                214,
                "VIAF",
                "Virtual International Authority File",
                "27063124",
                None,
                importer_factory!(crate::viaf::VIAF),
            ),
            SupportedProperty::new(227, "GND", "Deutsche Nationalbibliothek", "118523813", None, importer_factory!(crate::gnd::GND)),
            SupportedProperty::new(244, "LoC", "Library of Congress", "n78095637", None, importer_factory!(crate::loc::LOC)),
            SupportedProperty::new(
                349,
                "NDL",
                "National Diet Library",
                "00054222",
                Some("Natsume Soseki".into()),
                importer_factory!(crate::ndl::NDL),
            ),
            SupportedProperty::new(245, "ULAN", "Union List of Artist Names", "500228559", None, importer_factory!(crate::ulan::ULAN)),
            SupportedProperty::new(
                268,
                "BnF",
                "Bibliothèque nationale de France",
                "11898689q",
                None,
                importer_factory!(crate::bnf::BNF),
            ),
            SupportedProperty::new(269, "IdRef", "IdRef/SUDOC", "026812304", None, importer_factory!(crate::id_ref::IdRef)),
            SupportedProperty::new(662, "PubChem CID", "PubChem Compound ID", "22027196", Some("4-[1-(4-Hydroxyphenyl)heptyl]phenol".to_string()), importer_factory!(crate::pubchem_cid::PubChemCid)),
            SupportedProperty::new(906, "SELIBR", "National Library of Sweden", "231727", None, importer_factory!(crate::selibr::SELIBR)),
            SupportedProperty::new(
                950,
                "BNE",
                "Biblioteca Nacional de España",
                "XX990809",
                None,
                importer_factory!(crate::bne::BNE),
            ),
            SupportedProperty::new(
                1015,
                "NORAF",
                "Norwegian Authority File",
                "90053126",
                Some("Rainer Maria Rilke".into()),
                importer_factory!(crate::noraf::NORAF),
            ),
            SupportedProperty::new(
                1207,
                "NUKAT",
                "NUKAT Center of Warsaw University Library",
                "n96637319",
                Some("Al Gore".into()),
                importer_factory!(crate::nukat::NUKAT),
            ),
            SupportedProperty::new(
                1006,
                "NB",
                "Nationale Thesaurus voor Auteurs ID",
                "068364229",
                None,
                importer_factory!(crate::nb::NB),
            ),
            SupportedProperty::new(
                10832,
                "WorldCat",
                "WorldCat Identities",
                "E39PBJd87VvgDDTV6RxBYm6qcP",
                None,
                importer_factory!(crate::worldcat::WorldCat),
            ),
            SupportedProperty::new(
                3151,
                "INaturalist",
                "INaturalist taxon ID",
                "890",
                Some("Ruffed Grouse".to_string()),
                importer_factory!(crate::inaturalist::INaturalist),
            ),
            SupportedProperty::new(
                685,
                "NCBI taxonomy",
                "NCBI taxon ID",
                "1747344",
                Some("Priocnessus nuperus".to_string()),
                importer_factory!(crate::ncbi_taxonomy::NCBItaxonomy),
            ),
            SupportedProperty::new(
                846,
                "GBIF taxon",
                "GBIF taxon ID",
                "5141342",
                Some("Battus philenor".to_string()),
                importer_factory!(crate::gbif_taxon::GBIFtaxon),
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
    factory: ImporterFactory,
}

impl std::fmt::Debug for SupportedProperty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SupportedProperty")
            .field("property", &self.property)
            .field("name", &self.name)
            .field("source", &self.source)
            .field("demo_id", &self.demo_id)
            .field("demo_name", &self.demo_name)
            .finish()
    }
}

impl SupportedProperty {
    fn new(
        property: usize,
        name: &str,
        source: &str,
        demo_id: &str,
        demo_name: Option<String>,
        factory: ImporterFactory,
    ) -> Self {
        Self {
            property,
            name: name.into(),
            source: source.into(),
            demo_id: demo_id.into(),
            demo_name: demo_name.unwrap_or("Charles Darwin".into()),
            factory,
        }
    }

    pub async fn generator(&self, id: &str) -> Result<Box<dyn ExternalImporter>> {
        (self.factory)(id).await
    }

    pub fn as_li(&self) -> String {
        format!(
            r#"<li><a href="/item/P{}/{}">{}</a> ("{}" from {}) <small>[[<a href="https://www.wikidata.org/wiki/Property:P{}">P{}</a>]]</small></li>"#,
            self.property,
            &self.demo_id,
            &self.name,
            &self.demo_name,
            &self.source,
            &self.property,
            &self.property
        )
    }

    pub const fn property(&self) -> usize {
        self.property
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_supported_properties_have_unique_property_ids() {
        let mut seen = std::collections::HashSet::new();
        for sp in SUPPORTED_PROPERTIES.iter() {
            assert!(
                seen.insert(sp.property),
                "Duplicate property ID: {}",
                sp.property
            );
        }
    }

    #[tokio::test]
    async fn test_generator_creates_correct_importer() {
        // Use VIAF as a quick test: the factory should produce an importer
        // whose my_property() matches the SupportedProperty's property.
        let sp = SUPPORTED_PROPERTIES
            .iter()
            .find(|sp| sp.property == 214)
            .expect("VIAF (P214) should be in SUPPORTED_PROPERTIES");
        let importer = sp.generator("30701597").await.unwrap();
        assert_eq!(importer.my_property(), sp.property);
    }

    #[tokio::test]
    async fn test_generator_for_each_supported_property() {
        // Verify every entry's factory produces an importer with a matching property number,
        // using each entry's demo_id.
        for sp in SUPPORTED_PROPERTIES.iter() {
            let importer = sp
                .generator(&sp.demo_id)
                .await
                .unwrap_or_else(|e| panic!("generator failed for P{}: {}", sp.property, e));
            assert_eq!(
                importer.my_property(),
                sp.property,
                "my_property() mismatch for {}",
                sp.name
            );
        }
    }
}
