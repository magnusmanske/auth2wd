use crate::external_id::*;
use crate::external_importer::*;
use crate::meta_item::*;
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

unsafe impl Send for ISNI {}
unsafe impl Sync for ISNI {}

#[async_trait]
impl ExternalImporter for ISNI {
    fn my_property(&self) -> usize {
        213
    }

    fn my_id(&self) -> String {
        self.id.to_owned()
    }

    fn my_stated_in(&self) -> &str {
        "Q423048"
    }

    fn graph(&self) -> &FastGraph {
        &self.graph
    }

    fn primary_language(&self) -> String {
        "en".to_string()
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
        let url = ret.get_key_url(&id);
        ret.html = reqwest::get(&url).await?.text().await?.replace("\n", " "); // Remove newlines
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
        self.add_year_from_string(ret, 569, born);
        self.add_year_from_string(ret, 570, died);
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
                let extid = ExternalId::new(214, id.as_str());
                ret.add_claim(self.new_statement_string(extid.property(), extid.id()));
            }
        }

        if let Some(captures) = RE_GND.captures(&self.html) {
            if let Some(id) = captures.get(1) {
                let extid = ExternalId::new(227, id.as_str());
                ret.add_claim(self.new_statement_string(extid.property(), extid.id()));
            }
        }

        if let Some(captures) = RE_LOC.captures(&self.html) {
            if let Some(id) = captures.get(1) {
                let extid = ExternalId::new(244, id.as_str());
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
    use wikimisc::wikibase::EntityTrait;

    const TEST_ID: &str = "0000000121251077";

    #[tokio::test]
    async fn test_new() {
        assert!(ISNI::new(TEST_ID).await.is_ok());
    }

    #[tokio::test]
    async fn test_my_property() {
        let isni = ISNI::new(TEST_ID).await.unwrap();
        assert_eq!(isni.my_property(), 213);
    }

    #[tokio::test]
    async fn test_my_stated_in() {
        let isni = ISNI::new(TEST_ID).await.unwrap();
        assert_eq!(isni.my_stated_in(), "Q423048");
    }

    #[tokio::test]
    async fn test_my_id() {
        let isni = ISNI::new(TEST_ID).await.unwrap();
        assert_eq!(isni.my_id(), TEST_ID);
    }

    #[tokio::test]
    async fn test_run() {
        let isni = ISNI::new(TEST_ID).await.unwrap();
        let meta_item = isni.run().await.unwrap();
        let props: Vec<String> = meta_item
            .item
            .claims()
            .iter()
            .map(|c| c.main_snak().property().to_string())
            .collect();
        assert!(props.contains(&"P213".to_string()));
        // assert!(props.contains(&"P31".to_string()));
        assert!(props.contains(&"P214".to_string()));
        // assert!(props.contains(&"P227".to_string()));
        // assert!(props.contains(&"P244".to_string()));
        // assert!(props.contains(&"P569".to_string()));
        // assert!(props.contains(&"P570".to_string()));
    }

    #[tokio::test]
    async fn test_try_viaf() {
        let isni = ISNI::new(TEST_ID).await.unwrap();
        let mut mi = MetaItem::new();
        isni.try_viaf(&mut mi).await.unwrap();
        let ext_ids = mi.get_external_ids();
        assert_eq!(ext_ids.len(), 1); // VIAF
        assert_eq!(ext_ids[0], ExternalId::new(214, "27063124"));
    }
}
