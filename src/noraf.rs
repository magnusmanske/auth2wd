use crate::external_importer::*;
use crate::meta_item::*;
use crate::properties::*;
use crate::url_override::maybe_rewrite;
use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use serde_json::Value;
use sophia::inmem::graph::FastGraph;
use wikimisc::wikibase::{EntityTrait, LocaleString, SnakDataType};

// Was: Bibsys

#[derive(Debug)]
pub struct NORAF {
    id: String,
    j: Value,
}

#[async_trait]
impl ExternalImporter for NORAF {
    fn my_property(&self) -> usize {
        P_NORAF
    }

    fn my_id(&self) -> String {
        self.id.clone()
    }

    fn my_stated_in(&self) -> &str {
        "Q16889143"
    }

    fn graph(&self) -> &FastGraph {
        lazy_static! {
            static ref DUMMY_GRAPH: FastGraph = FastGraph::new();
        }
        &DUMMY_GRAPH
    }

    fn primary_language(&self) -> String {
        String::from("no")
    }

    fn get_key_url(&self, _key: &str) -> String {
        format!(
            "https://authority.bibsys.no/authority/rest/authorities/v2/{}?format=xml",
            self.id
        )
    }

    fn transform_label(&self, s: &str) -> String {
        self.transform_label_last_first_name(s)
    }

    async fn run(&self) -> Result<MetaItem> {
        let mut ret = MetaItem::new();
        self.add_own_id(&mut ret)?;
        self.add_marcdata(&mut ret);
        self.parse_identifiers(&mut ret);
        self.try_rescue_prop_text(&mut ret).await?;
        ret.cleanup();
        Ok(ret)
    }
}

impl NORAF {
    pub async fn new(id: &str) -> Result<Self> {
        let rdf_url = maybe_rewrite(&format!(
            "https://authority.bibsys.no/authority/rest/authorities/v2/{id}"
        ));
        let resp = reqwest::get(&rdf_url).await?.text().await?;
        let j: Value = serde_json::from_str(&resp)?;
        Ok(Self {
            id: id.to_string(),
            j,
        })
    }

    fn add_marcdata(&self, ret: &mut MetaItem) {
        if let Some(o) = self.j["marcdata"].as_array() {
            o.iter()
                .filter_map(|field| field.as_object())
                .for_each(|field| {
                    let _ = self.add_field(field, ret);
                });
        };
    }

    fn add_field(&self, field: &serde_json::Map<String, Value>, ret: &mut MetaItem) -> Option<()> {
        let tag = field.get("tag")?.as_str()?;
        let subfields = field.get("subfields")?.as_array()?;
        match tag {
            "100" => {
                subfields.iter().for_each(|sf| {
                    match (sf["subcode"].as_str(), sf["value"].as_str()) {
                        (Some("a"), Some(name)) => self.add_name(name, ret),
                        (Some("d"), Some(date)) => self.add_dates(date, ret),
                        _ => {}
                    }
                });
            }
            "386" => {}
            _ => {}
        }
        Some(())
    }

    fn add_dates(&self, date: &str, ret: &mut MetaItem) {
        lazy_static! {
            static ref RE_BORN_DIED: Regex = Regex::new(r#"^(.*)-(.*)$"#).expect("Regexp error");
        }
        if let Some(caps) = RE_BORN_DIED.captures(date) {
            // caps.get(1) and caps.get(2) are always Some when the regex matches
            if let Some((time, precision)) = caps.get(1).and_then(|m| ret.parse_date(m.as_str())) {
                ret.add_claim(self.new_statement_time(P_DATE_OF_BIRTH, &time, precision));
            }
            if let Some((time, precision)) = caps.get(2).and_then(|m| ret.parse_date(m.as_str())) {
                ret.add_claim(self.new_statement_time(P_DATE_OF_DEATH, &time, precision));
            }
        }
    }

    fn add_name(&self, name: &str, ret: &mut MetaItem) {
        let name = self.transform_label(name);
        let language = self.primary_language();
        ret.item
            .labels_mut()
            .push(LocaleString::new(language, name));
    }

    fn parse_identifiers(&self, ret: &mut MetaItem) {
        if let Some(o) = self.j["identifiersMap"].as_object() {
            o.iter()
                .map(|(_key, value)| value)
                .filter_map(|field| field.as_array())
                .filter_map(|field| field.first())
                .filter_map(|field| field.as_str())
                .filter_map(|s| self.url2external_id(s))
                .for_each(|ext_id| {
                    let mut statement = self.new_statement_string(ext_id.property(), ext_id.id());
                    statement.set_datatype(SnakDataType::ExternalId);
                    ret.add_claim(statement);
                });
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_dates() {
        let noraf = NORAF::new("123").await.unwrap();
        let mut ret = MetaItem::new();
        noraf.add_dates("1900-2000", &mut ret);
        assert_eq!(ret.item.claims().len(), 2);
    }
}
