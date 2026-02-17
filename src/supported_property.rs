use crate::external_importer::*;
use crate::properties::*;
use anyhow::Result;
use std::future::Future;
use std::pin::Pin;

/// Type alias for an async factory function that creates a boxed `ExternalImporter` from an ID.
type ImporterFactory =
    fn(&str) -> Pin<Box<dyn Future<Output = Result<Box<dyn ExternalImporter>>> + Send + '_>>;

/// Macro to create a `SupportedProperty` entry from a type and its metadata.
/// The property number is derived from the type's associated `P_*` constant via `$prop`.
/// The type must have `pub async fn new(id: &str) -> Result<Self>` and implement `ExternalImporter`.
macro_rules! supported_property {
    ($prop:expr, $type:ty, $name:expr, $source:expr, $demo_id:expr, $demo_name:expr) => {
        SupportedProperty::new(
            $prop,
            $name,
            $source,
            $demo_id,
            $demo_name,
            (|id: &str| -> Pin<Box<dyn Future<Output = Result<Box<dyn ExternalImporter>>> + Send + '_>> {
                Box::pin(async move { Ok(Box::new(<$type>::new(id).await?) as Box<dyn ExternalImporter>) })
            }) as ImporterFactory,
        )
    };
}

lazy_static! {
    /// Examples of all supported properties
    pub static ref SUPPORTED_PROPERTIES: Vec<SupportedProperty> = {
        vec![
            supported_property!(P_ISNI, crate::isni::ISNI, "ISNI", "International Standard Name Identifier", "0000000121251077", None),
            supported_property!(P_VIAF, crate::viaf::VIAF, "VIAF", "Virtual International Authority File", "27063124", None),
            supported_property!(P_GND, crate::gnd::GND, "GND", "Deutsche Nationalbibliothek", "118523813", None),
            supported_property!(P_LOC, crate::loc::LOC, "LoC", "Library of Congress", "n78095637", None),
            supported_property!(P_NDL, crate::ndl::NDL, "NDL", "National Diet Library", "00054222", Some("Natsume Soseki".into())),
            supported_property!(P_ULAN, crate::ulan::ULAN, "ULAN", "Union List of Artist Names", "500228559", None),
            supported_property!(P_BNF, crate::bnf::BNF, "BnF", "Bibliothèque nationale de France", "11898689q", None),
            supported_property!(P_IDREF, crate::id_ref::IdRef, "IdRef", "IdRef/SUDOC", "026812304", None),
            supported_property!(P_PUBCHEM_CID, crate::pubchem_cid::PubChemCid, "PubChem CID", "PubChem Compound ID", "22027196", Some("4-[1-(4-Hydroxyphenyl)heptyl]phenol".to_string())),
            supported_property!(P_SELIBR, crate::selibr::SELIBR, "SELIBR", "National Library of Sweden", "231727", None),
            supported_property!(P_BNE, crate::bne::BNE, "BNE", "Biblioteca Nacional de España", "XX990809", None),
            supported_property!(P_NORAF, crate::noraf::NORAF, "NORAF", "Norwegian Authority File", "90053126", Some("Rainer Maria Rilke".into())),
            supported_property!(P_NUKAT, crate::nukat::NUKAT, "NUKAT", "NUKAT Center of Warsaw University Library", "n96637319", Some("Al Gore".into())),
            supported_property!(P_NB, crate::nb::NB, "NB", "Nationale Thesaurus voor Auteurs ID", "068364229", None),
            supported_property!(P_WORLDCAT, crate::worldcat::WorldCat, "WorldCat", "WorldCat Identities", "E39PBJd87VvgDDTV6RxBYm6qcP", None),
            supported_property!(P_INATURALIST_TAXON, crate::inaturalist::INaturalist, "INaturalist", "INaturalist taxon ID", "890", Some("Ruffed Grouse".to_string())),
            supported_property!(P_NCBI_TAXONOMY, crate::ncbi_taxonomy::NCBItaxonomy, "NCBI taxonomy", "NCBI taxon ID", "1747344", Some("Priocnessus nuperus".to_string())),
            supported_property!(P_GBIF_TAXON, crate::gbif_taxon::GBIFtaxon, "GBIF taxon", "GBIF taxon ID", "5141342", Some("Battus philenor".to_string())),
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
