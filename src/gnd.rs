use regex::Regex;
use sophia::graph::inmem::FastGraph;
use sophia::triple::stream::TripleSource;
use crate::external_importer::*;
use crate::external_id::*;
use crate::meta_item::*;


lazy_static! {
    static ref RE_COUNTRY: Regex = Regex::new(r"^https?://d-nb.info/standards/vocab/gnd/geographic-area-code#XA-(.+)$").expect("Regexp error");
}

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
        self.add_the_usual(&mut ret)?;

        // P31
        let p31s = [
            ("https://d-nb.info/standards/elementset/gnd#DifferentiatedPerson","Q5"),
        ];
        for url in self.triples_subject_iris(&self.get_id_url(), "http://www.w3.org/1999/02/22-rdf-syntax-ns#type")? {
            let item = p31s.iter().filter(|(url2,_)|url==url2.to_string()).map(|(_,q)|q.to_string()).next();
            let _ = match item {
                Some(item) => ret.add_claim(self.new_statement_item(31,&item)),
                None => ret.add_prop_text(ExternalId::new(31,&url)),
            };
        }

        // Nationality
        for url in self.triples_subject_iris(&self.get_id_url(), "https://d-nb.info/standards/elementset/gnd#geographicAreaCode")? {
            let country_code = RE_COUNTRY.replace(&url,"${1}");
            if country_code!=url {
                let ext_id = ExternalId::new(297, &country_code);
                let _ = match ext_id.get_item_for_external_id_value() {
                    Some(item) => ret.add_claim(self.new_statement_item(27,&item)),
                    None => ret.add_prop_text(ext_id),
                };
            }
        }

        // Born/died
        let birth_death = [
            ("https://d-nb.info/standards/elementset/gnd#dateOfBirth",569),
            ("https://d-nb.info/standards/elementset/gnd#dateOfDeath",570),
        ];
        for bd in birth_death {
            for s in self.triples_subject_literals(&self.get_id_url(), bd.0)? {
                let _ = match ret.parse_date(&s) {
                    Some((time,precision)) => ret.add_claim(self.new_statement_time(bd.1,&time,precision)),
                    None => ret.add_prop_text(ExternalId::new(bd.1,&s))
                };
            }
        }

        // Places
        let key_prop = [
            ("https://d-nb.info/standards/elementset/gnd#placeOfBirth",19),
            ("https://d-nb.info/standards/elementset/gnd#placeOfDeath",20),
            ("https://d-nb.info/standards/elementset/agrelon#hasChild",40),
            ("https://d-nb.info/standards/elementset/gnd#fieldOfStudy",101),
            ("https://d-nb.info/standards/elementset/gnd#professionOrOccupation",106),
            ("https://d-nb.info/standards/elementset/gnd#placeOfActivity",937),
            // TODO parent
        ];
        for kp in key_prop {
            for url in self.triples_subject_iris(&self.get_id_url(), kp.0)? {
                if let Some(gnd_id) = url.split("/").last() {
                    if let Some(item) = ExternalId::new(227,&gnd_id).get_item_for_external_id_value() {
                        ret.add_claim(self.new_statement_item(kp.1,&item));
                    } else {
                        let _ = ret.add_prop_text(ExternalId::new(kp.1,&url));
                    }
                } else {
                    let _ = ret.add_prop_text(ExternalId::new(kp.1,&url));
                }
            }
        }
        
        self.try_rescue_prop_text(&mut ret)?;
        ret.cleanup();
        Ok(ret)
    }
}

impl GND {
    /// Changes internal ID in case of redirect
    fn fix_own_id(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let ids = self.triples_property_literals("https://d-nb.info/standards/elementset/gnd#gndIdentifier")?;
        if ids.len()==1 {
            self.id = ids[0].to_owned();
        }
        Ok(())
    }

    pub fn new(id: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let rdf_url = format!("https://d-nb.info/gnd/{}/about/lds.rdf",id);
        let resp = ureq::get(&rdf_url).call()?.into_string()?;
        let mut graph: FastGraph = FastGraph::new();
        let _ = sophia::parser::xml::parse_str(&resp).add_to_graph(&mut graph)?;
        let mut ret = Self { id:id.to_string(), graph };
        ret.fix_own_id()?;
        Ok(ret)
    }
}
