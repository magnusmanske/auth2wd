use crate::external_id::*;
use crate::external_importer::*;
use crate::meta_item::*;
use crate::properties::*;
use crate::url_override::maybe_rewrite;
use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use sophia::inmem::graph::FastGraph;

lazy_static! {
    static ref RE_VIAF: Regex = Regex::new(r"\bhttps?://viaf.org/viaf/(\d+)").unwrap();
    static ref RE_GND: Regex = Regex::new(r"\bhttps?://d-nb.info/gnd/(1[012]?\d{7}[0-9X]|[47]\d{6}-\d|[1-9]\d{0,7}-[0-9X]|3\d{7}[0-9X])\b").unwrap();
    static ref RE_LOC: Regex = Regex::new(r#"\bhttps?://id.loc.gov/authorities/names/(.+?)""#).unwrap();
    static ref RE_BORN_DIED: Regex = Regex::new(r"<span>Dates:.*?</span>.*?<span>(.+?)</span>").unwrap();
    static ref RE_YEAR: Regex = Regex::new(r"(\d{3,4})").unwrap();
    static ref RE_NAME: Regex = Regex::new(r"<span>Name:.*?</span>.*?<span>(.+?)</span>").unwrap();
}

#[derive(Debug)]
pub struct ISNI {
    id: String,
    graph: FastGraph,
    html: String,
}

#[async_trait]
impl ExternalImporter for ISNI {
    fn my_property(&self) -> usize {
        P_ISNI
    }

    fn my_id(&self) -> String {
        self.id.clone()
    }

    fn my_stated_in(&self) -> &str {
        "Q423048"
    }

    fn graph(&self) -> &FastGraph {
        &self.graph
    }

    fn primary_language(&self) -> String {
        String::from("en")
    }

    fn get_key_url(&self, _key: &str) -> String {
        format!(" https://isni.org/isni/{}", self.id)
    }

    async fn add_the_usual(&self, ret: &mut MetaItem) -> Result<()> {
        self.try_viaf(ret).await?;
        self.add_own_id(ret)?;
        // ret.add_claim(self.new_statement_item(31, "Q5")); // Human TODO only for some
        Ok(())
    }

    fn transform_label(&self, s: &str) -> String {
        self.transform_label_last_first_name(s)
    }

    async fn run(&self) -> Result<MetaItem> {
        let mut ret = MetaItem::new();
        self.add_the_usual(&mut ret).await?;
        self.parse_external_ids(&mut ret);
        self.parse_dates(&mut ret);
        // self.parse_name(&mut ret);

        self.try_rescue_prop_text(&mut ret).await?;
        ret.cleanup();
        Ok(ret)
    }
}

impl ISNI {
    pub async fn new(id: &str) -> Result<Self> {
        let id = Self::remove_whitespace(id);
        let mut ret = Self {
            id: id.to_string(),
            graph: FastGraph::new(),
            html: String::new(),
        };
        let url = maybe_rewrite(&format!("https://isni.org/isni/{id}"));
        // Collapse all whitespace including newlines so regexes can match across lines
        ret.html = reqwest::get(&url)
            .await?
            .text()
            .await?
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        Ok(ret)
    }

    fn add_year_from_string(&self, ret: &mut MetaItem, prop: usize, s: &str) {
        if let Some(captures) = RE_YEAR.captures(s) {
            if let Some(year) = captures.get(1) {
                let time = format!("+{}-01-01T00:00:00Z", year.as_str());
                ret.add_claim(self.new_statement_time(prop, &time, 9));
            }
        }
    }

    fn parse_dates(&self, ret: &mut MetaItem) -> Option<()> {
        let dates = RE_BORN_DIED.captures(&self.html)?.get(1)?;
        let (born, died) = dates.as_str().split_once('-')?;
        self.add_year_from_string(ret, P_DATE_OF_BIRTH, born);
        self.add_year_from_string(ret, P_DATE_OF_DEATH, died);
        None
    }

    // this needs work
    // fn parse_name(&self, ret: &mut MetaItem) -> Option<()> {
    //     let name = RE_NAME.captures(&self.html)?.get(1)?.as_str().trim();
    //     let name = self.transform_label(name);
    //     ret.item.labels_mut().push(LocaleString::new("en", &name));
    //     None
    // }

    fn parse_external_ids(&self, ret: &mut MetaItem) {
        if let Some(captures) = RE_VIAF.captures(&self.html) {
            if let Some(id) = captures.get(1) {
                let extid = ExternalId::new(P_VIAF, id.as_str());
                ret.add_claim(self.new_statement_string(extid.property(), extid.id()));
            }
        }

        if let Some(captures) = RE_GND.captures(&self.html) {
            if let Some(id) = captures.get(1) {
                let extid = ExternalId::new(P_GND, id.as_str());
                ret.add_claim(self.new_statement_string(extid.property(), extid.id()));
            }
        }

        if let Some(captures) = RE_LOC.captures(&self.html) {
            if let Some(id) = captures.get(1) {
                let extid = ExternalId::new(P_LOC, id.as_str());
                ret.add_claim(self.new_statement_string(extid.property(), extid.id()));
            }
        }
    }

    fn remove_whitespace(s: &str) -> String {
        s.chars().filter(|c| !c.is_whitespace()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::url_override;
    use wikimisc::wikibase::EntityTrait;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TEST_ID: &str = "0000000121251077";

    async fn mock_isni() -> (MockServer, ISNI) {
        let server = MockServer::start().await;
        let fixture = include_str!("../test_data/fixtures/isni_0000000121251077.html");

        Mock::given(method("GET"))
            .and(path("/isni/0000000121251077"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
            .mount(&server)
            .await;

        // try_viaf stub (ISNI P213 is mapped in KEY2PROP)
        let viaf_fixture =
            include_str!("../test_data/fixtures/viaf_lookup_isni_0000000121251077.json");
        Mock::given(method("POST"))
            .and(path("/api/cluster-record"))
            .respond_with(ResponseTemplate::new(200).set_body_string(viaf_fixture))
            .mount(&server)
            .await;

        url_override::register("https://isni.org", server.uri());
        url_override::register("https://viaf.org", server.uri());

        let isni = ISNI::new(TEST_ID).await.unwrap();
        (server, isni)
    }

    fn cleanup() {
        url_override::unregister("https://isni.org");
        url_override::unregister("https://viaf.org");
    }

    #[tokio::test]
    async fn test_new() {
        let (_server, _isni) = mock_isni().await;
        cleanup();
    }

    #[tokio::test]
    async fn test_my_property() {
        let (_server, isni) = mock_isni().await;
        assert_eq!(isni.my_property(), P_ISNI);
        cleanup();
    }

    #[tokio::test]
    async fn test_my_stated_in() {
        let (_server, isni) = mock_isni().await;
        assert_eq!(isni.my_stated_in(), "Q423048");
        cleanup();
    }

    #[tokio::test]
    async fn test_my_id() {
        let (_server, isni) = mock_isni().await;
        assert_eq!(isni.my_id(), TEST_ID);
        cleanup();
    }

    #[tokio::test]
    async fn test_run() {
        let (_server, isni) = mock_isni().await;
        let meta_item = isni.run().await.unwrap();
        let props: Vec<String> = meta_item
            .item
            .claims()
            .iter()
            .map(|c| c.main_snak().property().to_string())
            .collect();
        assert!(props.contains(&"P213".to_string()));
        assert!(props.contains(&"P214".to_string()));
        cleanup();
    }

    #[tokio::test]
    async fn test_try_viaf() {
        let (_server, isni) = mock_isni().await;
        let mut mi = MetaItem::new();
        isni.try_viaf(&mut mi).await.unwrap();
        let ext_ids = mi.get_external_ids();
        assert_eq!(ext_ids.len(), 1); // VIAF
        assert_eq!(ext_ids[0], ExternalId::new(P_VIAF, "27063124"));
        cleanup();
    }
}
