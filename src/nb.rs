use crate::external_id::*;
use crate::external_importer::*;
use crate::meta_item::*;
use crate::properties::*;
use crate::url_override::maybe_rewrite;
use crate::utility::Utility;
use anyhow::Result;
use async_trait::async_trait;
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use wikimisc::wikibase::EntityTrait;
use wikimisc::wikibase::LocaleString;

#[derive(Serialize, Deserialize, Debug, Clone)]
enum TermType {
    BlankNode,
    NamedNode,
    Literal,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TripleEntry {
    #[serde(rename = "termType")]
    term_type: TermType,
    value: String,
    datatype: Option<String>,
    language: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Triple {
    #[serde(rename = "0")]
    s: TripleEntry,
    #[serde(rename = "1")]
    p: TripleEntry,
    #[serde(rename = "2")]
    o: TripleEntry,
}

impl Triple {
    fn is_named_node(&self, key: &str) -> bool {
        match self.p.term_type {
            TermType::NamedNode => self.p.value == key,
            _ => false,
        }
    }
}

#[derive(Debug)]
pub struct NB {
    id: String,
    data: Vec<Triple>,
}

#[async_trait]
impl ExternalImporter for NB {
    fn my_property(&self) -> usize {
        P_NB
    }

    fn my_id(&self) -> String {
        self.id.clone()
    }

    fn my_stated_in(&self) -> &str {
        "Q105488572"
    }

    fn primary_language(&self) -> String {
        String::from("nl")
    }

    fn get_key_url(&self, _key: &str) -> String {
        format!("http://data.bibliotheken.nl/id/thes/p{}", self.id)
    }

    fn transform_label(&self, s: &str) -> String {
        self.transform_label_last_first_name(s)
    }

    async fn run(&self) -> Result<MetaItem> {
        let own_url = format!("http://data.bibliotheken.nl/id/thes/p{}", self.id);
        let mut ret = MetaItem::new();
        self.add_the_usual(&mut ret).await?;

        for triple in &self.data {
            if triple.s.value != own_url {
                // Only gather data about the subject itself
                continue;
            }
            let language = triple.o.language.to_owned().unwrap_or("nl".to_string());
            if triple.is_named_node("http://www.w3.org/1999/02/22-rdf-syntax-ns#type")
                && triple.o.value == "http://schema.org/Person"
            {
                ret.add_claim(self.new_statement_item(P_INSTANCE_OF, "Q5"));
            }
            if triple.is_named_node("http://schema.org/alternateName") {
                ret.item
                    .aliases_mut()
                    .push(LocaleString::new(&language, &triple.o.value));
            }
            if triple.is_named_node("http://schema.org/name") {
                ret.item
                    .labels_mut()
                    .push(LocaleString::new(&language, &triple.o.value));
            }
            if triple.is_named_node("http://schema.org/description") {
                ret.item
                    .descriptions_mut()
                    .push(LocaleString::new(&language, &triple.o.value));
            }
            if triple.is_named_node("http://schema.org/nationality") {
                ret.add_prop_text(ExternalId::new(P_COUNTRY_OF_CITIZENSHIP, &triple.o.value));
            }
            if triple.is_named_node("http://schema.org/birthDate") {
                match ret.parse_date(&triple.o.value) {
                    Some(date) => ret.add_claim(self.new_statement_time(P_DATE_OF_BIRTH, &date)),
                    None => ret.add_prop_text(ExternalId::new(P_DATE_OF_BIRTH, &triple.o.value)),
                };
            }
            if triple.is_named_node("http://schema.org/deathDate") {
                match ret.parse_date(&triple.o.value) {
                    Some(date) => ret.add_claim(self.new_statement_time(P_DATE_OF_DEATH, &date)),
                    None => ret.add_prop_text(ExternalId::new(P_DATE_OF_DEATH, &triple.o.value)),
                };
            }
            if (triple.is_named_node("http://schema.org/sameAs")
                || triple.is_named_node("http://www.w3.org/2002/07/owl#sameAs"))
                && let Some(ext_id) = self.url2external_id(&triple.o.value)
            {
                ret.add_claim(self.new_statement_string(ext_id.property(), ext_id.id()));
            }
        }

        self.try_rescue_prop_text(&mut ret).await?;
        ret.cleanup();
        Ok(ret)
    }
}

impl NB {
    pub async fn new(id: &str) -> Result<Self> {
        let url = maybe_rewrite(&format!("http://data.bibliotheken.nl/id/thes/p{id}"));
        let client = Utility::get_reqwest_client()?;
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        let data: Vec<Triple> = client
            .get(url)
            .headers(headers)
            .send()
            .await?
            .json()
            .await?;

        Ok(Self {
            id: id.to_string(),
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::standard_importer_tests;
    use crate::url_override;
    use serial_test::serial;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TEST_ID: &str = "068364229";

    async fn mock() -> (MockServer, NB) {
        let server = MockServer::start().await;
        let fixture = include_str!("../test_data/fixtures/nb_068364229.json");

        Mock::given(method("GET"))
            .and(path("/id/thes/p068364229"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(fixture)
                    .insert_header("Content-Type", "application/json"),
            )
            .mount(&server)
            .await;

        // try_viaf stub (NB P1006 is not mapped in KEY2PROP, so try_viaf
        // returns early, but stub it anyway for safety)
        Mock::given(method("POST"))
            .and(path("/api/cluster-record"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&server)
            .await;

        url_override::register("http://data.bibliotheken.nl", server.uri());
        url_override::register("https://viaf.org", server.uri());

        let nb = NB::new(TEST_ID).await.unwrap();
        (server, nb)
    }

    fn cleanup() {
        url_override::unregister("http://data.bibliotheken.nl");
        url_override::unregister("https://viaf.org");
    }

    standard_importer_tests!(
        P_NB,
        "Q105488572",
        "nl",
        "http://data.bibliotheken.nl/id/thes/p068364229",
        vec![LocaleString::new("nl", "Charles Robert Darwin")]
    );
}
