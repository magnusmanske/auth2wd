use crate::external_importer::*;
use crate::meta_item::*;
use sophia::graph::inmem::FastGraph;
use sophia::triple::stream::TripleSource;

pub struct VIAF {
    id: String,
    graph: FastGraph,
}

unsafe impl Send for VIAF {}
unsafe impl Sync for VIAF {}

impl ExternalImporter for VIAF {
    fn my_property(&self) -> usize {
        214
    }
    fn my_stated_in(&self) -> &str {
        "Q54919"
    }
    fn primary_language(&self) -> String {
        "en".to_string()
    }
    fn get_key_url(&self, _key: &str) -> String {
        format!("http://viaf.org/viaf/{}", self.id)
    }

    fn my_id(&self) -> String {
        self.id.to_owned()
    }
    fn graph(&self) -> &FastGraph {
        &self.graph
    }
    fn graph_mut(&mut self) -> &mut FastGraph {
        &mut self.graph
    }
    fn transform_label(&self, s: &str) -> String {
        self.transform_label_last_first_name(s)
    }

    fn run(&self) -> Result<MetaItem, Box<dyn std::error::Error>> {
        let mut ret = MetaItem::new();
        self.add_the_usual(&mut ret)?;
        self.try_rescue_prop_text(&mut ret)?;
        ret.cleanup();
        Ok(ret)
    }
}

impl VIAF {
    pub fn new(id: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let rdf_url = format!("https://viaf.org/viaf/{}/rdf.xml", id);
        let resp = ureq::get(&rdf_url).call()?.into_string()?;
        let mut graph: FastGraph = FastGraph::new();
        let _ = sophia::parser::xml::parse_str(&resp).add_to_graph(&mut graph)?;
        Ok(Self {
            id: id.to_string(),
            graph,
        })
    }
}

#[cfg(test)]
mod tests {
    use wikibase::{EntityTrait, LocaleString};

    use super::*;

    const TEST_ID: &str = "30701597";

    #[test]
    fn test_new() {
        assert!(VIAF::new(TEST_ID).is_ok());
    }

    #[test]
    fn test_my_property() {
        let viaf = VIAF::new(TEST_ID).unwrap();
        assert_eq!(viaf.my_property(), 214);
    }

    #[test]
    fn test_my_stated_in() {
        let viaf = VIAF::new(TEST_ID).unwrap();
        assert_eq!(viaf.my_stated_in(), "Q54919");
    }

    #[test]
    fn test_primary_language() {
        let viaf = VIAF::new(TEST_ID).unwrap();
        assert_eq!(viaf.primary_language(), "en");
    }

    #[test]
    fn test_get_key_url() {
        let viaf = VIAF::new(TEST_ID).unwrap();
        assert_eq!(viaf.get_key_url(TEST_ID), "http://viaf.org/viaf/30701597");
    }

    #[test]
    fn test_my_id() {
        let viaf = VIAF::new(TEST_ID).unwrap();
        assert_eq!(viaf.my_id(), TEST_ID);
    }

    #[test]
    fn test_transform_label() {
        let viaf = VIAF::new(TEST_ID).unwrap();
        assert_eq!(viaf.transform_label("Manske, Magnus"), "Magnus Manske");
        assert_eq!(viaf.transform_label("Manske,Magnus"), "Manske,Magnus");
        assert_eq!(viaf.transform_label("Magnus Manske"), "Magnus Manske");
    }

    #[test]
    fn test_run() {
        let viaf = VIAF::new(TEST_ID).unwrap();
        let meta_item = viaf.run().unwrap();
        assert_eq!(
            *meta_item.item.labels(),
            vec![LocaleString::new("en", "Magnus Manske")]
        );
    }

    #[test]
    fn test_graph() {
        let mut viaf = VIAF::new(TEST_ID).unwrap();
        let _graph = viaf.graph();
        let _graph = viaf.graph_mut();
    }
}
