use crate::external_importer::*;
use crate::meta_item::*;
use crate::ExternalId;
use anyhow::{anyhow, Result};
use axum::async_trait;
use regex::Regex;
use serde_json::Value;
use wikimisc::wikibase::EntityTrait;
use wikimisc::wikibase::LocaleString;
use wikimisc::wikibase::Snak;

lazy_static! {
    static ref RE_SERVER_PAYLOAD: Regex =
        Regex::new(r#" *taxon: (\{.+)\.results\[0\]"#).expect("Regexp error");
    static ref RE_IUCN_REDLIST_URL: Regex =
        Regex::new(r#"https://www.iucnredlist.org/species/(\d+)/\d+"#).expect("Regexp error");
}

#[derive(Clone)]
pub struct INaturalist {
    id: String,
    json: Value,
}

unsafe impl Send for INaturalist {}
unsafe impl Sync for INaturalist {}

#[async_trait]
impl ExternalImporter for INaturalist {
    fn my_property(&self) -> usize {
        3151 // iNaturalist taxon ID
    }
    fn my_stated_in(&self) -> &str {
        "Q16958215"
    }
    fn primary_language(&self) -> String {
        "en".to_string()
    }
    fn get_key_url(&self, _key: &str) -> String {
        format!("https://www.inaturalist.org/taxa/{}", self.id)
    }
    fn my_id(&self) -> String {
        self.id.to_owned()
    }

    async fn run(&self) -> Result<MetaItem> {
        let mut ret = MetaItem::new();
        self.add_own_id(&mut ret)?;
        let _ = self.add_parent_taxon(&mut ret).await;
        let _ = self.add_commons_compatible_image(&mut ret);
        let _ = self.add_p31(&mut ret);
        let _ = self.add_taxon_name_and_labels(&mut ret);
        let _ = self.add_taxon_rank(&mut ret);
        let _ = self.add_common_name(&mut ret);
        let _ = self.add_conservation_statuses(&mut ret);
        ret.cleanup();
        Ok(ret)
    }
}

impl INaturalist {
    pub async fn new(id: &str) -> Result<Self> {
        let url = format!("https://www.inaturalist.org/taxa/{id}");
        let resp = reqwest::get(&url).await?.text().await?;
        let j = Self::parse_html(&resp).ok_or(anyhow!("No JSON found"))?;
        Ok(Self {
            id: id.to_string(),
            json: j,
        })
    }

    fn parse_html(s: &str) -> Option<Value> {
        let js_object = RE_SERVER_PAYLOAD.captures(s)?.get(1)?.as_str().to_string();
        let j: Value = serde_json::from_str(&js_object).ok()?;
        let j = j.get("results")?.get(0)?.to_owned();
        if !j.is_object() {
            return None;
        }
        Some(j)
    }

    async fn add_parent_taxon(&self, ret: &mut MetaItem) -> Option<()> {
        let parent_id = self.json.get("parent_id")?.as_u64()?;
        let query = format!("haswbstatement:P3151={parent_id} haswbstatement:P31=Q16521");
        let item = ExternalId::search_wikidata_single_item(&query).await?;
        ret.add_claim(self.new_statement_item(171, &item));
        Some(())
    }

    fn add_commons_compatible_image(&self, ret: &mut MetaItem) -> Option<()> {
        let default_photo = self.json.get("default_photo")?.as_object()?;
        let _ = self.add_commons_compatible_image_from_photo(ret, default_photo);
        let taxon_photos = self.json.get("taxon_photos")?.as_array()?;
        let _found = taxon_photos
            .iter()
            .filter_map(|photo| photo.as_object())
            .filter_map(|photo| photo.get("photo"))
            .filter_map(|photo| photo.as_object())
            .filter_map(|photo| self.add_commons_compatible_image_from_photo(ret, photo))
            .count();
        Some(())
    }

    fn add_commons_compatible_image_from_photo(
        &self,
        ret: &mut MetaItem,
        photo: &serde_json::Map<String, Value>,
    ) -> Option<bool> {
        let license_code = photo.get("license_code")?.as_str()?.to_lowercase();
        let license_item = VALID_IMAGE_LICENSES.get(license_code.as_str())?;
        let image_url = photo
            .get("original_url")?
            .as_str()
            .or_else(|| photo.get("large_url")?.as_str())
            .or_else(|| photo.get("medium_url")?.as_str())?;
        let attribution = photo.get("attribution")?.as_str()?;
        let mut statement = self.new_statement_string(4765, image_url);
        statement.add_qualifier_snak(Snak::new_item("P275", license_item));
        statement.add_qualifier_snak(Snak::new_string("P2093", attribution));
        statement.add_qualifier_snak(Snak::new_url("P2699", image_url));
        if image_url.ends_with("jpg") || image_url.ends_with("jpeg") {
            statement.add_qualifier_snak(Snak::new_item("P2701", "Q2195"));
        }
        ret.add_claim(statement);
        Some(true)
    }

    fn add_p31(&self, ret: &mut MetaItem) -> Option<()> {
        let is_extinct = self.json.get("extinct")?.as_bool()?;
        if is_extinct {
            // Extinct taxon
            ret.add_claim(self.new_statement_item(31, "Q98961713"));
        } else {
            // Taxon
            ret.add_claim(self.new_statement_item(31, "Q16521"));
        }
        Some(())
    }

    fn add_taxon_name_and_labels(&self, ret: &mut MetaItem) -> Option<()> {
        let name = self.json.get("name")?.as_str()?;
        ret.add_claim(self.new_statement_string(225, name));
        for lang in TAXON_LABEL_LANGUAGES {
            let label = LocaleString::new(lang.to_string(), name.to_string());
            ret.item.labels_mut().push(label);
        }
        Some(())
    }

    fn add_taxon_rank(&self, ret: &mut MetaItem) -> Option<()> {
        let rank = self.json.get("rank")?.as_str()?.to_lowercase();
        let item = TAXON_MAP.get(rank.as_str())?;
        ret.add_claim(self.new_statement_item(105, item));
        Some(())
    }

    fn add_common_name(&self, ret: &mut MetaItem) -> Option<()> {
        let common_name = None
            .or_else(|| self.json.get("preferred_common_name")?.as_str())
            .or_else(|| self.json.get("english_common_name")?.as_str())?;
        ret.add_claim(self.new_statement_monolingual_text(
            1843,
            &self.primary_language(),
            common_name,
        ));
        Some(())
    }

    fn add_conservation_statuses(&self, ret: &mut MetaItem) -> Option<()> {
        let conservation_statuses = self.json.get("conservation_statuses")?.as_array()?;
        for cs in conservation_statuses {
            let _ = self.add_conservation_status(ret, cs);
        }
        Some(())
    }

    fn add_conservation_status(&self, ret: &mut MetaItem, cs: &Value) -> Option<()> {
        let cs = cs.as_object()?;
        let status = cs.get("status")?.as_str()?.to_lowercase();
        let authority = cs.get("authority")?.as_str()?;
        match authority {
            "IUCN Red List" => {
                // Try to parse IUCN Red List specis ID from URL
                if let Some(url) = cs.get("url") {
                    let url = url.as_str().unwrap_or_default();
                    if let Some(captures) = RE_IUCN_REDLIST_URL.captures(url) {
                        if let Some(s) = captures.get(1) {
                            let iucn_species_id = s.as_str();
                            ret.add_claim(self.new_statement_string(627, iucn_species_id));
                        }
                    }
                }
                // Get IUCN Red List status
                let item = IUCN_REDLIST.get(status.as_str())?;
                ret.add_claim(self.new_statement_item(141, item));
            }
            // TODO NatureServe https://www.wikidata.org/wiki/Property:P3648
            _other => {} // Ignore
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ID: &str = "627975";

    #[tokio::test]
    async fn test_new() {
        assert!(INaturalist::new(TEST_ID).await.is_ok());
    }

    #[tokio::test]
    async fn test_my_property() {
        let inaturalist = INaturalist::new(TEST_ID).await.unwrap();
        assert_eq!(inaturalist.my_property(), 3151);
    }

    #[tokio::test]
    async fn test_my_stated_in() {
        let inaturalist = INaturalist::new(TEST_ID).await.unwrap();
        assert_eq!(inaturalist.my_stated_in(), "Q16958215");
    }

    #[tokio::test]
    async fn test_primary_language() {
        let inaturalist = INaturalist::new(TEST_ID).await.unwrap();
        assert_eq!(inaturalist.primary_language(), "en");
    }

    #[tokio::test]
    async fn test_get_key_url() {
        let inaturalist = INaturalist::new(TEST_ID).await.unwrap();
        assert_eq!(
            inaturalist.get_key_url(TEST_ID),
            "https://www.inaturalist.org/taxa/627975"
        );
    }

    #[tokio::test]
    async fn test_my_id() {
        let inaturalist = INaturalist::new(TEST_ID).await.unwrap();
        assert_eq!(inaturalist.my_id(), TEST_ID);
    }

    #[tokio::test]
    async fn test_run_inaturalist() {
        let inaturalist = INaturalist::new(TEST_ID).await.unwrap();
        let meta_item = inaturalist.run().await.unwrap();
        assert_eq!(
            meta_item.item.labels()[0],
            LocaleString::new("en", "Licea bryophila")
        );
        assert_eq!(meta_item.item.claims().len(), 8);
    }
}
