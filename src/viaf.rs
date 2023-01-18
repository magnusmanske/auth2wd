use sophia::graph::inmem::FastGraph;
use sophia::triple::stream::TripleSource;
use crate::external_importer::*;
use crate::meta_item::*;

pub struct VIAF {
    id: String,
    graph: FastGraph,
}


unsafe impl Send for VIAF {}
unsafe impl Sync for VIAF {}

impl ExternalImporter for VIAF {
    fn my_property(&self) -> usize { 214 }
    fn my_stated_in(&self) -> &str { "Q54919" }
    fn primary_language(&self) -> String { "en".to_string() }
    fn get_key_url(&self, _key: &str) -> String { format!("http://viaf.org/viaf/{}",self.id) }

    fn my_id(&self) -> String { self.id.to_owned() }
    fn graph(&self) -> &FastGraph { &self.graph }
    fn graph_mut(&mut self) -> &mut FastGraph { &mut self.graph }
    fn transform_label(&self, s: &str) -> String { self.transform_label_last_first_name(s) }


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
        let rdf_url = format!("https://viaf.org/viaf/{}/rdf.xml",id);
        let resp = ureq::get(&rdf_url).call()?.into_string()?;
        let mut graph: FastGraph = FastGraph::new();
        let _ = sophia::parser::xml::parse_str(&resp).add_to_graph(&mut graph)?;
        Ok(Self { id:id.to_string(), graph })
    }

}
