use sophia::graph::inmem::FastGraph;
use sophia::triple::stream::TripleSource;
use regex::Regex;
use crate::external_importer::*;
use crate::meta_item::*;

lazy_static!{
    static ref RE_NUMERIC_ID : Regex = Regex::new(r#"^(\d{8,9})[0-9bcdfghjkmnpqrstvwxz]$"#).expect("Regexp error");
    static ref RE_URL : Regex = Regex::new(r#"<meta property="og:url" content="https://data.bnf.fr/\d+/(.+?)/" />"#).expect("Regexp error");
}


pub struct BNF {
    id: String,
    graph: FastGraph,
}


unsafe impl Send for BNF {}
unsafe impl Sync for BNF {}

impl ExternalImporter for BNF {
    fn my_property(&self) -> usize {
        268
    }

    fn my_id(&self) -> String {
        self.id.to_owned()
    }

    fn my_stated_in(&self) -> &str {
        "Q19938912"
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

    fn get_key_url(&self, _key: &str) -> String {
        format!("https://data.bnf.fr/ark:/12148/cb{}#about",self.id)
    }

    fn transform_label(&self, s: &str) -> String {
        self.transform_label_last_first_name(s)
    }


    fn run(&self) -> Result<MetaItem, Box<dyn std::error::Error>> {
        let mut ret = MetaItem::new();
        self.add_the_usual(&mut ret)?;

        // Born/died
        let birth_death = [
            ("http://rdvocab.info/ElementsGr2/dateOfBirth",569),
            ("http://rdvocab.info/ElementsGr2/dateOfDeath",570),
        ];
        for bd in birth_death {
            for s in self.triples_subject_iris(&self.get_id_url(), bd.0)? {
                match ret.parse_date(&s) {
                    Some((time,precision)) => ret.add_claim(self.new_statement_time(bd.1,&time,precision)),
                    None => ret.prop_text.push((bd.1,s))
                }
            }
        }

        let birth_death = [
            ("http://vocab.org/bio/0.1/birth",569),
            ("http://vocab.org/bio/0.1/death",570),
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

impl BNF {
    pub fn new(id: &str) -> Result<Self, Box<dyn std::error::Error>> {
        if !RE_NUMERIC_ID.is_match(&id) {
            return Err(format!("ID format error for '{id}'").into())
        }
        let numeric_id = RE_NUMERIC_ID.replace_all(&id,"${1}");

        let name = match Self::get_name_for_id(&numeric_id) {
            Some(name) => name,
            None => return Err(format!("Name retrieval error for '{id}'").into())
        };

        let rdf_url = format!("https://data.bnf.fr/{numeric_id}/{name}/rdf.xml");
        let resp = ureq::get(&rdf_url).call()?.into_string()?;

        let mut graph: FastGraph = FastGraph::new();
        let _ = sophia::parser::xml::parse_str(&resp).add_to_graph(&mut graph)?;
        Ok(Self { id:id.to_string(), graph })
    }

    fn get_name_for_id(numeric_id: &str) -> Option<String> {
        let rdf_url = format!("https://data.bnf.fr/en/{numeric_id}");
        let resp = ureq::get(&rdf_url).call().ok()?.into_string().ok()?;
        Some(RE_URL.captures(&resp)?.get(1)?.as_str().to_string())
    }

}
