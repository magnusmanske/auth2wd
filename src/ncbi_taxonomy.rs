use crate::external_importer::*;
use crate::meta_item::*;
use crate::ExternalId;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use quickxml_to_serde::xml_string_to_json;
use serde_json::Value;
use wikimisc::wikibase::EntityTrait;
use wikimisc::wikibase::LocaleString;

#[derive(Clone, Debug)]
pub struct NCBItaxonomy {
    id: String,
    json: Value,
}

unsafe impl Send for NCBItaxonomy {}
unsafe impl Sync for NCBItaxonomy {}

#[async_trait]
impl ExternalImporter for NCBItaxonomy {
    fn my_property(&self) -> usize {
        685
    }
    fn my_stated_in(&self) -> &str {
        "Q13711410"
    }
    fn primary_language(&self) -> String {
        "en".to_string()
    }
    fn get_key_url(&self, _key: &str) -> String {
        format!(
            "https://www.ncbi.nlm.nih.gov/Taxonomy/Browser/wwwtax.cgi?mode=Info&id={}",
            self.id
        )
    }
    fn my_id(&self) -> String {
        self.id.to_owned()
    }

    async fn run(&self) -> Result<MetaItem> {
        let mut ret = MetaItem::new();
        self.add_own_id(&mut ret)?;
        let _ = self.add_parent_taxon(&mut ret).await;
        let _ = self.add_p31(&mut ret);
        let _ = self.add_taxon_name_and_labels(&mut ret);
        let _ = self.add_taxon_rank(&mut ret);
        ret.cleanup();
        Ok(ret)
    }
}

impl NCBItaxonomy {
    pub async fn new(id: &str) -> Result<Self> {
        let url = format!("https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi?db=taxonomy&id={id}&format=xml");
        let resp = reqwest::get(&url).await?.text().await?;
        let conf = quickxml_to_serde::Config::new_with_defaults();
        let json = xml_string_to_json(resp.to_owned(), &conf)?;
        let json = json
            .get("TaxaSet")
            .ok_or_else(|| anyhow!("Invalid JSON"))?
            .get("Taxon")
            .ok_or_else(|| anyhow!("Invalid JSON"))?
            .to_owned();
        Ok(Self {
            id: id.to_string(),
            json,
        })
    }

    async fn add_parent_taxon(&self, ret: &mut MetaItem) -> Option<()> {
        let parent_id = self.json.get("ParentTaxId")?.as_i64()?;
        let query = format!("haswbstatement:P685={parent_id} haswbstatement:P31=Q16521");
        let item = ExternalId::search_wikidata_single_item(&query).await?;
        ret.add_claim(self.new_statement_item(171, &item));
        Some(())
    }

    fn add_p31(&self, ret: &mut MetaItem) -> Option<()> {
        // Taxon
        ret.add_claim(self.new_statement_item(31, "Q16521"));
        Some(())
    }

    fn add_taxon_name_and_labels(&self, ret: &mut MetaItem) -> Option<()> {
        let name = self.json.get("ScientificName")?.as_str()?;
        ret.add_claim(self.new_statement_string(225, name));
        for lang in TAXON_LABEL_LANGUAGES {
            let label = LocaleString::new(lang.to_string(), name.to_string());
            ret.item.labels_mut().push(label);
        }
        Some(())
    }

    fn add_taxon_rank(&self, ret: &mut MetaItem) -> Option<()> {
        let rank = self.json.get("Rank")?.as_str()?.to_lowercase();
        let item = TAXON_MAP.get(rank.as_str())?;
        ret.add_claim(self.new_statement_item(105, item));
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ID: &str = "1747344";

    // Using all tests together to get around NCBI rate limiting
    #[tokio::test]
    async fn test_all() {
        let ncbi_taxonomy = NCBItaxonomy::new(TEST_ID).await.unwrap();
        assert_eq!(ncbi_taxonomy.my_property(), 685);
        assert_eq!(ncbi_taxonomy.my_stated_in(), "Q13711410");
        assert_eq!(ncbi_taxonomy.primary_language(), "en");
        assert_eq!(ncbi_taxonomy.my_id(), TEST_ID);
        assert_eq!(
            ncbi_taxonomy.get_key_url(TEST_ID),
            format!(
                "https://www.ncbi.nlm.nih.gov/Taxonomy/Browser/wwwtax.cgi?mode=Info&id={}",
                TEST_ID
            )
        );
        let new_item = ncbi_taxonomy.run().await.unwrap();
        assert_eq!(new_item.item.claims().len(), 5);
    }
}
