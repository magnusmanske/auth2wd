use crate::external_importer::*;
use crate::meta_item::*;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use wikibase_rest_api::prelude::StatementValueContent;

#[derive(Clone, Debug)]
pub struct PubChemCid {
    id: String,
    json: Value,
}

unsafe impl Send for PubChemCid {}
unsafe impl Sync for PubChemCid {}

#[async_trait]
impl ExternalImporter for PubChemCid {
    fn my_property(&self) -> usize {
        662
    }
    fn my_stated_in(&self) -> &str {
        "Q278487"
    }
    fn primary_language(&self) -> String {
        "en".to_string()
    }
    fn get_key_url(&self, _key: &str) -> String {
        format!("https://pubchem.ncbi.nlm.nih.gov/compound/{}", self.id)
    }
    fn my_id(&self) -> String {
        self.id.to_owned()
    }

    async fn run(&self) -> Result<MetaItem> {
        let mut ret = MetaItem::new();
        self.add_own_id(&mut ret)?;
        let _ = self.add_p31(&mut ret);
        let _ = self.add_p279(&mut ret);
        let _ = self.add_label(&mut ret);
        let _ = self.add_other_ids(&mut ret);
        ret.cleanup();
        Ok(ret)
    }
}

impl PubChemCid {
    pub async fn new(id: &str) -> Result<Self> {
        let url =
            format!("https://pubchem.ncbi.nlm.nih.gov/rest/pug_view/data/compound/{id}/JSON/");
        let resp = reqwest::get(&url).await?.text().await?;
        let json = serde_json::from_str(&resp)?;
        Ok(Self {
            id: id.to_string(),
            json,
        })
    }

    fn add_p31(&self, ret: &mut MetaItem) -> Option<()> {
        ret.add_claim(self.new_statement_item(31, "Q113145171"));
        Some(())
    }

    fn add_p279(&self, ret: &mut MetaItem) -> Option<()> {
        ret.add_claim(self.new_statement_item(279, "Q11173"));
        Some(())
    }

    fn add_label(&self, ret: &mut MetaItem) -> Option<()> {
        let name = self.json["Record"]["RecordTitle"].as_str()?;
        let label = StatementValueContent::new_monolingual_text("en".to_string(), name.to_string());
        ret.item.labels_mut().push(label);
        Some(())
    }

    fn add_other_ids(&self, ret: &mut MetaItem) -> Option<()> {
        let main_sections = self.json["Record"]["Section"].as_array()?;
        let identifier_section = main_sections
            .iter()
            .filter(|s| s["TOCHeading"].as_str() == Some("Names and Identifiers"))
            .filter_map(|s| s["Section"].as_array())
            .next()?;

        let computed_descriptors = identifier_section
            .iter()
            .filter(|s| s["TOCHeading"].as_str() == Some("Computed Descriptors"))
            .filter_map(|s| s["Section"].as_array())
            .next()?;
        for o in computed_descriptors.iter() {
            match o["TOCHeading"].as_str() {
                Some("SMILES") => self.extract_information_as_string_values(ret, o, 233),
                Some("InChI") => self.extract_information_as_string_values(ret, o, 234),
                Some("InChIKey") => self.extract_information_as_string_values(ret, o, 235),
                _ => {} // Ignore
            }
        }

        let other_identifiers = identifier_section
            .iter()
            .filter(|s| s["TOCHeading"].as_str() == Some("Other Identifiers"))
            .filter_map(|s| s["Section"].as_array())
            .next()?;
        for o in other_identifiers.iter() {
            if let Some("Nikkaji Number") = o["TOCHeading"].as_str() {
                self.extract_information_as_string_values(ret, o, 2085);
            }
        }

        Some(())
    }

    fn extract_information_as_string_values(&self, ret: &mut MetaItem, o: &Value, property: usize) {
        if let Some(information_arr) = o["Information"].as_array() {
            for information in information_arr {
                if let Some(string_arr) = information["Value"]["StringWithMarkup"].as_array() {
                    for s in string_arr {
                        if let Some(target_id) = s["String"].as_str() {
                            ret.add_claim(self.new_statement_string(property, target_id));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ID: &str = "22027196";

    #[tokio::test]
    async fn test_all() {
        let gbif = PubChemCid::new(TEST_ID).await.unwrap();
        assert_eq!(gbif.my_property(), 662);
        assert_eq!(gbif.my_stated_in(), "Q278487");
        assert_eq!(gbif.primary_language(), "en");
        assert_eq!(gbif.my_id(), TEST_ID);
        assert_eq!(
            gbif.get_key_url(TEST_ID),
            format!("https://pubchem.ncbi.nlm.nih.gov/compound/{TEST_ID}")
        );
        let new_item = gbif.run().await.unwrap();
        assert_eq!(new_item.item.statements().len(), 7);
    }
}
