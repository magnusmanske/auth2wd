use crate::external_importer::*;
use crate::meta_item::*;
use crate::ExternalId;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use wikibase_rest_api::prelude::LanguageStrings;
use wikibase_rest_api::LanguageString;

#[derive(Debug, Clone)]
pub struct WorldCat {
    id: String,
    json: Value,
}

unsafe impl Send for WorldCat {}
unsafe impl Sync for WorldCat {}

#[async_trait]
impl ExternalImporter for WorldCat {
    fn my_property(&self) -> usize {
        10832
    }
    fn my_stated_in(&self) -> &str {
        "Q112122720"
    }
    fn primary_language(&self) -> String {
        "en".to_string()
    }
    fn get_key_url(&self, _key: &str) -> String {
        format!("https://id.oclc.org/worldcat/entity/{}", self.id)
    }
    fn my_id(&self) -> String {
        self.id.to_owned()
    }

    async fn run(&self) -> Result<MetaItem> {
        let mut ret = MetaItem::new();
        self.add_own_id(&mut ret)?;
        let _ = self.add_date(&mut ret, "dateOfBirth", 569);
        let _ = self.add_date(&mut ret, "dateOfDeath", 570);
        let _ = self.add_p31(&mut ret);
        let _ = self.add_labels(&mut ret);
        let _ = self.add_aliases(&mut ret);
        let _ = self.add_descriptions(&mut ret);
        ret.cleanup();
        Ok(ret)
    }
}

impl WorldCat {
    pub async fn new(id: &str) -> Result<Self> {
        let url = format!("https://id.oclc.org/worldcat/entity/{id}.jsonld");
        let resp = reqwest::get(&url).await?.text().await?;
        let j = serde_json::from_str(&resp)?;
        Ok(Self {
            id: id.to_string(),
            json: j,
        })
    }

    fn add_p31(&self, ret: &mut MetaItem) -> Option<()> {
        let types = self.json.get("type")?.as_array()?;
        for the_type in types {
            if let Some(the_type) = the_type.as_str() {
                match the_type {
                    "Person" => {
                        let _ = ret.add_claim(self.new_statement_item(31, "Q5"));
                    }
                    other => {
                        let ext_id = ExternalId::new(31, other);
                        let _ = ret.add_prop_text(ext_id);
                    }
                }
            }
        }
        Some(())
    }

    fn add_labels(&self, ret: &mut MetaItem) -> Option<()> {
        let labels = self.json.get("prefLabel")?.as_object()?;
        for (language, s) in labels {
            if let Some(s) = s.as_str() {
                ret.item
                    .labels_mut()
                    .insert(LanguageString::new(language, s));
            }
        }
        Some(())
    }

    fn add_aliases(&self, ret: &mut MetaItem) -> Option<()> {
        let aliases = self.json.get("altLabel")?.as_object()?;
        for (language, aliases_in_language) in aliases {
            if let Some(aliases_in_language) = aliases_in_language.as_array() {
                for alias in aliases_in_language {
                    if let Some(alias) = alias.as_str() {
                        ret.item
                            .aliases_mut()
                            .insert(LanguageString::new(language, alias));
                    }
                }
            }
        }
        Some(())
    }

    fn add_descriptions(&self, ret: &mut MetaItem) -> Option<()> {
        let descriptions = self.json.get("description")?.as_object()?;
        for (language, s) in descriptions {
            if let Some(s) = s.as_str() {
                ret.item
                    .descriptions_mut()
                    .insert(LanguageString::new(language, s));
            }
        }
        Some(())
    }

    fn add_date(&self, ret: &mut MetaItem, key: &str, prop: usize) -> Option<()> {
        let date = self.json.get(key)?.get(0)?;
        let dt = date.get("time:inDateTime")?;
        let mut time = Self::dt2part(dt, "time:year")?;
        if let Some(month) = Self::dt2part(dt, "time:month") {
            match Self::dt2part(dt, "time:day") {
                Some(day) => {
                    time.push_str(&format!(
                        "-{:02}-{:02}",
                        month.replace('-', ""),
                        day.replace('-', "")
                    ));
                }
                None => {
                    time.push_str(&format!("-{:02}", month.replace('-', "")));
                }
            }
        }

        if let Some((time, precision)) = ret.parse_date(&time) {
            let _ = ret.add_claim(self.new_statement_time(prop, &time, precision));
        };
        Some(())
    }

    fn dt2part(j: &Value, key: &str) -> Option<String> {
        Some(j.get(key)?.get("@value")?.as_str()?.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ID: &str = "E39PBJrcqvXdm3kkwGr7HVG8md";

    #[tokio::test]
    async fn test_new() {
        assert!(WorldCat::new(TEST_ID).await.is_ok());
    }

    #[tokio::test]
    async fn test_my_property() {
        let worldcat = WorldCat::new(TEST_ID).await.unwrap();
        assert_eq!(worldcat.my_property(), 10832);
    }

    #[tokio::test]
    async fn test_my_stated_in() {
        let worldcat = WorldCat::new(TEST_ID).await.unwrap();
        assert_eq!(worldcat.my_stated_in(), "Q112122720");
    }

    #[tokio::test]
    async fn test_primary_language() {
        let worldcat = WorldCat::new(TEST_ID).await.unwrap();
        assert_eq!(worldcat.primary_language(), "en");
    }

    #[tokio::test]
    async fn test_get_key_url() {
        let worldcat = WorldCat::new(TEST_ID).await.unwrap();
        assert_eq!(
            worldcat.get_key_url(TEST_ID),
            "https://id.oclc.org/worldcat/entity/E39PBJrcqvXdm3kkwGr7HVG8md"
        );
    }

    #[tokio::test]
    async fn test_my_id() {
        let worldcat = WorldCat::new(TEST_ID).await.unwrap();
        assert_eq!(worldcat.my_id(), TEST_ID);
    }

    #[tokio::test]
    async fn test_run() {
        let worldcat = WorldCat::new(TEST_ID).await.unwrap();
        let meta_item = worldcat.run().await.unwrap();
        assert_eq!(meta_item.item.labels().get_lang("en"), Some("Helen Clark"));
        assert!(meta_item
            .item
            .aliases()
            .get_lang("en")
            .contains(&"Helen Elizabeth Clark"));
        assert_eq!(meta_item.item.statements().len(), 3);
    }
}
