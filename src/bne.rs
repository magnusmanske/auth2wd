use sophia::graph::inmem::FastGraph;
use sophia::triple::stream::TripleSource;
use wikibase::*;
use crate::external_importer::*;
use crate::meta_item::*;

pub struct BNE {
    id: String,
    graph: FastGraph,
}

impl ExternalImporter for BNE {
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
        format!("https://datos.bne.es/resource/{}",self.id)
    }
}

impl BNE {
    pub async fn new(id: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let rdf_url = format!("https://datos.bne.es/resource/{}.rdf",id);
        let resp = reqwest::get(rdf_url).await?.text().await?;
        let mut graph: FastGraph = FastGraph::new();
        let _ = sophia::parser::xml::parse_str(&resp).add_to_graph(&mut graph)?;
        Ok(Self { id:id.to_string(), graph })
    }


    pub async fn run(&self) -> Result<MetaItem, Box<dyn std::error::Error>> {
        let mut ret = MetaItem::new();

        ret.item.add_claim(self.new_statement_string(950, &self.id));

        self.add_same_as(&mut ret)?;
        self.add_gender(&mut ret)?;
        self.add_label_aliases(&mut ret)?;
        self.add_description(&mut ret)?;
        self.add_language(&mut ret)?;

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
                    Some((time,precision)) => ret.item.add_claim(self.new_statement_time(bd.1,&time,precision)),
                    None => ret.prop_text.push((bd.1,s))
                }
            }
        }

/*


        for s in self.triples_subject_literals(&format!("http://www.BNE.fr/{}/birth",self.id),"http://purl.org/vocab/bio/0.1/date")? {
            match ret.parse_date(&s) {
                Some((time,precision)) => ret.item.add_claim(self.new_statement_time(569,&time,precision)),
                None => ret.prop_text.push((569,s))
            }
        }
 */
        //self.bibliography(&mut ret)?;

        // TODO find better way
        let new_statements = self.try_rescue_prop_text(&mut ret).await?;
        for (prop,item) in new_statements {
            let statement = self.new_statement_item(prop,&item);
            ret.item.add_claim(statement);
        }
        Ok(ret)
    }

    /*
    fn bibliography(&self, ret: &mut MetaItem) -> Result<(), Box<dyn std::error::Error>> {
        let id_url = self.get_id_url();
        let authored = self.triples_property_object_iris("http://id.loc.gov/vocabulary/relators/aut",&id_url)?;
        println!("{:?}",&authored);
        Ok(())
    } */
}

// https://datos.bne.es/resource/XX1553066.rdf
