use sophia::graph::inmem::FastGraph;
use sophia::triple::stream::TripleSource;
use crate::external_importer::*;
use crate::meta_item::*;

pub struct GND {
    id: String,
    graph: FastGraph,
}

unsafe impl Send for GND {}
unsafe impl Sync for GND {}


impl ExternalImporter for GND {
    fn my_property(&self) -> usize {
        227
    }

    fn my_id(&self) -> String {
        self.id.to_owned()
    }

    fn my_stated_in(&self) -> &str {
        "Q36578"
    }

    fn graph(&self) -> &FastGraph {
        &self.graph
    }

    fn graph_mut(&mut self) -> &mut FastGraph {
        &mut self.graph
    }

    fn primary_language(&self) -> String {
        "de".to_string()
    }

    fn get_key_url(&self, _key: &str) -> String {
        format!("https://d-nb.info/gnd/{}",self.id)
    }

    fn transform_label(&self, s: &str) -> String {
        self.transform_label_last_first_name(s)
    }


    fn run(&self) -> Result<MetaItem, Box<dyn std::error::Error>> {
        let mut ret = MetaItem::new();

        ret.add_claim(self.new_statement_string(self.my_property(), &self.id));

        self.add_same_as(&mut ret)?;
        self.add_gender(&mut ret)?;
        self.add_label_aliases(&mut ret)?;
        self.add_description(&mut ret)?;
        self.add_language(&mut ret)?;
/*
        // Nationality
        for text in self.triples_literals("http://www.rdaregistry.info/Elements/a/P50102")? {
            ret.prop_text.push((27,text))
        }
 */

        // Born/died
        let birth_death = [
            ("https://d-nb.info/standards/elementset/gnd#dateOfBirth",569),
            ("https://d-nb.info/standards/elementset/gnd#dateOfDeath",570),
        ];
        for bd in birth_death {
            for s in self.triples_subject_literals(&self.get_id_url(), bd.0)? {
                match ret.parse_date(&s) {
                    Some((time,precision)) => ret.add_claim(self.new_statement_time(bd.1,&time,precision)),
                    None => ret.prop_text.push((bd.1,s))
                }
            }
        }

        // Places
        let key_prop = [
            ("https://d-nb.info/standards/elementset/gnd#placeOfBirth",19),
            ("https://d-nb.info/standards/elementset/gnd#placeOfDeath",20),
            ("https://d-nb.info/standards/elementset/gnd#professionOrOccupation",106),
            ("https://d-nb.info/standards/elementset/agrelon#hasChild",40),
            // TODO parent
        ];
        for kp in key_prop {
            for url in self.triples_subject_iris(&self.get_id_url(), kp.0)? {
                if let Some(gnd_id) = url.split("/").last() {
                    if let Some(item) = ExternalId::new(227,&gnd_id).get_item_for_external_id_value() {
                        ret.add_claim(self.new_statement_item(kp.1,&item));
                    } else {
                        ret.prop_text.push((kp.1,url))
                    }
                } else {
                    ret.prop_text.push((kp.1,url))
                }
            }
        }
        
        self.try_rescue_prop_text(&mut ret)?;
        ret.cleanup();
        Ok(ret)
    }
}

impl GND {
    pub fn new(id: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let rdf_url = format!("https://d-nb.info/gnd/{}/about/lds.rdf",id);
        let resp = ureq::get(&rdf_url).call()?.into_string()?;
        let mut graph: FastGraph = FastGraph::new();
        let _ = sophia::parser::xml::parse_str(&resp).add_to_graph(&mut graph)?;
        Ok(Self { id:id.to_string(), graph })
    }
}
