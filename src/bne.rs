use sophia::graph::inmem::FastGraph;
use sophia::triple::stream::TripleSource;
use crate::external_importer::*;
use crate::meta_item::*;

pub struct BNE {
    id: String,
    graph: FastGraph,
}


unsafe impl Send for BNE {}
unsafe impl Sync for BNE {}

impl ExternalImporter for BNE {
    fn my_property(&self) -> usize {
        950
    }

    fn my_id(&self) -> String {
        self.id.to_owned()
    }

    fn my_stated_in(&self) -> &str {
        "Q50358336"
    }

    fn graph(&self) -> &FastGraph {
        &self.graph
    }

    fn graph_mut(&mut self) -> &mut FastGraph {
        &mut self.graph
    }

    fn primary_language(&self) -> String {
        "es".to_string()
    }

    fn get_key_url(&self, _key: &str) -> String {
        format!("https://datos.bne.es/resource/{}",self.id)
    }

    fn transform_label(&self, s: &str) -> String {
        self.transform_label_last_first_name(s)
    }


    fn run(&self) -> Result<MetaItem, Box<dyn std::error::Error>> {
        let mut ret = MetaItem::new();
        self.add_the_usual(&mut ret)?;

        // Nationality
        for text in self.triples_literals("http://www.rdaregistry.info/Elements/a/P50102")? {
            ret.prop_text.push((27,text))
        }

        // Born/died
        let birth_death = [
            ("https://datos.bne.es/def/P5010",569),
            ("https://datos.bne.es/def/P5011",570),
        ];
        for bd in birth_death {
            for s in self.triples_subject_literals(&self.get_id_url(), bd.0)? {
                match ret.parse_date(&s) {
                    Some((time,precision)) => ret.add_claim(self.new_statement_time(bd.1,&time,precision)),
                    None => ret.prop_text.push((bd.1,s))
                }
            }
        }

        self.try_rescue_prop_text(&mut ret)?;
        ret.cleanup();
        Ok(ret)
    }
}

impl BNE {
    pub fn new(id: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let rdf_url = format!("https://datos.bne.es/resource/{}.rdf",id);
        let resp = ureq::get(&rdf_url).call()?.into_string()?;
        let mut graph: FastGraph = FastGraph::new();
        let _ = sophia::parser::xml::parse_str(&resp).add_to_graph(&mut graph)?;
        Ok(Self { id:id.to_string(), graph })
    }

}
