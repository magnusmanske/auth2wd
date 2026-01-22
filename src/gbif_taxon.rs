use crate::external_importer::*;
use crate::meta_item::*;
use crate::ExternalId;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use wikimisc::wikibase::EntityTrait;
use wikimisc::wikibase::LocaleString;
use wikimisc::wikibase::Snak;

#[derive(Clone, Debug)]
pub struct GBIFtaxon {
    id: String,
    json: Value,
}

unsafe impl Send for GBIFtaxon {}
unsafe impl Sync for GBIFtaxon {}

#[async_trait]
impl ExternalImporter for GBIFtaxon {
    fn my_property(&self) -> usize {
        846
    }
    fn my_stated_in(&self) -> &str {
        "Q1531570"
    }
    fn primary_language(&self) -> String {
        "en".to_string()
    }
    fn get_key_url(&self, _key: &str) -> String {
        format!("https://www.gbif.org/species/{}", self.id)
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
        let _ = self.add_common_name(&mut ret);
        let _ = self.add_taxon_rank(&mut ret);
        let _ = self.add_commons_compatible_image(&mut ret).await;
        ret.cleanup();
        Ok(ret)
    }
}

impl GBIFtaxon {
    pub async fn new(id: &str) -> Result<Self> {
        let url = format!("https://api.gbif.org/v1/species/{id}");
        let resp = reqwest::get(&url).await?.text().await?;
        let json = serde_json::from_str(&resp)?;
        Ok(Self {
            id: id.to_string(),
            json,
        })
    }

    async fn add_parent_taxon(&self, ret: &mut MetaItem) -> Option<()> {
        let parent_id = self.json.get("parentKey")?.as_i64()?;
        let query = format!(
            "haswbstatement:P{}={parent_id} haswbstatement:P31=Q16521",
            self.my_property()
        );
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
        let name = self.json.get("Battus philenor")?.as_str()?;
        ret.add_claim(self.new_statement_string(225, name));
        for lang in TAXON_LABEL_LANGUAGES {
            let label = LocaleString::new(lang.to_string(), name.to_string());
            ret.item.labels_mut().push(label);
        }
        Some(())
    }

    fn add_common_name(&self, ret: &mut MetaItem) -> Option<()> {
        let common_name = self.json.get("vernacularName")?.as_str()?;
        ret.add_claim(self.new_statement_monolingual_text(
            1843,
            &self.primary_language(),
            common_name,
        ));
        Some(())
    }

    fn add_taxon_rank(&self, ret: &mut MetaItem) -> Option<()> {
        let rank = self.json.get("rank")?.as_str()?.to_lowercase();
        let item = TAXON_MAP.get(rank.as_str())?;
        ret.add_claim(self.new_statement_item(105, item));
        Some(())
    }

    async fn add_commons_compatible_image(&self, ret: &mut MetaItem) -> Option<()> {
        let url = format!(
            "https://api.gbif.org/v1/occurrence/search?limit=20&media_type=stillImage&taxon_key={}",
            self.id
        );
        let resp = reqwest::get(&url).await.ok()?.text().await.ok()?;
        let json: Value = serde_json::from_str(&resp).ok()?;
        let results = json.get("results")?.as_array()?;
        for result in results {
            let _ = self.add_commons_compatible_image_from_photo(ret, result);
        }
        Some(())
    }

    fn add_commons_compatible_image_from_photo(
        &self,
        ret: &mut MetaItem,
        json: &Value,
    ) -> Option<()> {
        if json.get("taxonKey")?.as_i64()?.to_string() != self.id {
            return None;
        }
        for medium in json.get("media")?.as_array()? {
            let image_url = medium.get("identifier")?.as_str()?;
            let license_url = medium.get("license")?.as_str()?.to_lowercase();
            let license_item = match VALID_IMAGE_LICENSES.get(license_url.as_str()) {
                Some(item) => item,
                None => continue,
            };
            let attribution = None
                .or_else(|| medium.get("rightsHolder")?.as_str())
                .or_else(|| medium.get("creator")?.as_str())?;
            let mut statement = self.new_statement_string(4765, image_url);
            statement.add_qualifier_snak(Snak::new_item("P275", license_item));
            statement.add_qualifier_snak(Snak::new_string("P2093", attribution));
            statement.add_qualifier_snak(Snak::new_url("P2699", image_url));
            let format = medium.get("format")?.as_str()?;
            if format == "image/jpeg" {
                statement.add_qualifier_snak(Snak::new_item("P2701", "Q2195"));
            }

            ret.add_claim(statement);
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ID: &str = "5141342";

    #[tokio::test]
    async fn test_all() {
        let gbif = GBIFtaxon::new(TEST_ID).await.unwrap();
        assert_eq!(gbif.my_property(), 846);
        assert_eq!(gbif.my_stated_in(), "Q1531570");
        assert_eq!(gbif.primary_language(), "en");
        assert_eq!(gbif.my_id(), TEST_ID);
        assert_eq!(
            gbif.get_key_url(TEST_ID),
            format!("https://www.gbif.org/species/{TEST_ID}")
        );
        let new_item = gbif.run().await.unwrap();
        assert_eq!(new_item.item.claims().len(), 6);
    }
}
