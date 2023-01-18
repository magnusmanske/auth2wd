use sophia::graph::inmem::FastGraph;
use sophia::triple::stream::TripleSource;
use crate::external_importer::*;
use crate::external_id::*;
use crate::meta_item::*;

pub struct IdRef {
    id: String,
    graph: FastGraph,
}

unsafe impl Send for IdRef {}
unsafe impl Sync for IdRef {}

impl ExternalImporter for IdRef {
    fn my_property(&self) -> usize {
        269
    }

    fn my_id(&self) -> String {
        self.id.to_owned()
    }

    fn my_stated_in(&self) -> &str {
        "Q47757534"
    }

    fn graph(&self) -> &FastGraph {
        &self.graph
    }

    fn graph_mut(&mut self) -> &mut FastGraph {
        &mut self.graph
    }

    fn primary_language(&self) -> String {
        "fr".to_string()
    }

    fn get_key_url(&self, key: &str) -> String {
        format!("http://www.idref.fr/{}/{}",self.id,key)
    }


    fn run(&self) -> Result<MetaItem, Box<dyn std::error::Error>> {
        let mut ret = MetaItem::new();
        self.add_the_usual(&mut ret)?;

        for url in self.triples_iris("http://dbpedia.org/ontology/citizenship")? {
            match self.url2external_id(&url) {
                Some(extid) => {
                    match extid.get_item_for_external_id_value() {
                        Some(item) => ret.add_claim(self.new_statement_item(27,&item)),
                        None => ret.prop_text.push(ExternalId::new(27,&url))
                    }
                }
                None => ret.prop_text.push(ExternalId::new(27,&url))
            }
        }

        let birth_death = [
            ("birth",569),
            ("death",570),
        ];
        for bd in birth_death {
            for s in self.triples_subject_literals(&format!("http://www.idref.fr/{}/{}",self.id,bd.0),"http://purl.org/vocab/bio/0.1/date")? {
                match ret.parse_date(&s) {
                    Some((time,precision)) => ret.add_claim(self.new_statement_time(bd.1,&time,precision)),
                    None => ret.prop_text.push(ExternalId::new(bd.1,&s))
                }
            }
        }

        self.try_rescue_prop_text(&mut ret)?;
        ret.cleanup();
        Ok(ret)
    }

}

impl IdRef {
    pub fn new(id: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let rdf_url = format!("https://www.idref.fr/{}.rdf",id);
        let resp = ureq::get(&rdf_url).call()?.into_string()?;
        let mut graph: FastGraph = FastGraph::new();
        let _ = sophia::parser::xml::parse_str(&resp).add_to_graph(&mut graph)?;
        Ok(Self { id:id.to_string(), graph })
    }

}
