use sophia::serializer::*;
use sophia::serializer::nt::NtSerializer;
use sophia::term::Term;
use sophia::graph::{*, inmem::FastGraph};
use sophia::triple::Triple;
use sophia::triple::stream::TripleSource;
use sophia::term::SimpleIri;
use async_recursion::async_recursion;
use regex::Regex;
use std::vec::Vec;
use wikibase::*;

lazy_static! {
    static ref EXTERNAL_ID_REGEXPS : Vec<(Regex,String,usize)> = {
        let mut vec : Vec<(Regex,String,usize)> = vec![] ;
        // NOTE: The pattern always needs to cover the whole string, so use ^$
        vec.push((Regex::new(r"^http://viaf.org/viaf/(\d+)$").unwrap(),"${1}".to_string(),214));
        vec.push((Regex::new(r"^http://isni.org/isni/(\d{4})(\d{4})(\d{4})(\d{4})$").unwrap(),"${1} ${2} ${3} ${4}".to_string(),213));
        vec.push((Regex::new(r"^http://data.bnf.fr/ark:/12148/cb(\d{8,9}[0-9bcdfghjkmnpqrstvwxz]).*$").unwrap(),"${1}".to_string(),268));
        vec.push((Regex::new(r"^http://sws.geonames.org/([1-9][0-9]{0,8}).*$").unwrap(),"${1}".to_string(),1566));
        vec
    };
}


#[derive(Debug)]
pub struct ExternalId {
    pub property: usize,
    pub id: String
}


pub trait ExternalImporter {
    // NEEDS TO OVERLOAD
    fn get_key_url(&self, key: &str) -> String;
    fn graph(&self) -> &FastGraph;
    fn graph_mut(&mut self) -> &mut FastGraph;

    fn get_id_url(&self) -> String {
        self.get_key_url("id")
    }

    fn dump_graph(&mut self) {
        let mut nt_stringifier = NtSerializer::new_stringifier();
        let graph: &mut FastGraph = self.graph_mut();
        let example2 = nt_stringifier.serialize_graph(graph).unwrap().as_str();
        println!("The resulting graph\n{}", example2);
    }

    fn url2external_id(&self, url: &str) -> Option<ExternalId> {
        EXTERNAL_ID_REGEXPS
        .iter()
        .filter_map(|e|{
            let replaced = e.0.replace_all(&url,&e.1);
            if url==replaced {
                None
            } else {
                Some(ExternalId{
                    property: e.2,
                    id: replaced.to_string()
                })
            }
        })
        .next()
    }

    fn triples_subject_iris(&self, id_url: &str, p: &str) ->  Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut ret = vec![];
        let iri_id = SimpleIri::new(&id_url,None)?;
        let iri_p = SimpleIri::new(p, None)?;
        self.graph().triples_with_sp(&iri_id,&iri_p).for_each_triple(|t|
            if let Term::Iri(iri) = t.o() {
                ret.push(iri.ns().to_string());
            }
        )?;
        Ok(ret)
    }

    fn triples_iris(&self, p: &str) ->  Result<Vec<String>, Box<dyn std::error::Error>> {
        self.triples_subject_iris(&self.get_id_url(), p)
    }

    fn triples_subject_literals(&self, id_url: &str, p: &str) ->  Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut ret = vec![];
        let iri_id = SimpleIri::new(&id_url,None)?;
        let iri_p = SimpleIri::new(p, None)?;
        self.graph().triples_with_sp(&iri_id,&iri_p).for_each_triple(|t|
            if let Term::Literal(iri) = t.o() {
                ret.push(iri.txt().to_string());
            }
        )?;
        Ok(ret)
    }

    fn triples_literals(&self, p: &str) ->  Result<Vec<String>, Box<dyn std::error::Error>> {
        self.triples_subject_literals(&self.get_id_url(), p)
    }

    #[async_recursion]
    async fn get_item_for_external_id_value(property: usize, value: &str) -> Option<String> {
        let url = format!("https://www.wikidata.org/w/api.php?action=query&list=search&srnamespace=0&format=json&srsearch=haswbstatement:\"P{}={}\"",property,value);
        let text = reqwest::get(url).await.ok()?.text().await.ok()?;
        let j: serde_json::Value = serde_json::from_str(&text).ok()?;
        if j["query"]["searchinfo"]["totalhits"].as_i64()? == 1 {
            return Some(j["query"]["search"][0]["title"].as_str()?.to_string());
        }
        None
    }

    // Overload this to insert references to the source
    fn get_ref(&self) -> Vec<Reference> { vec![] }

    fn new_statement_string(&self, property: usize, s: &str) -> Statement {
        Statement::new(
            "statement",
            StatementRank::Normal,
            Snak::new(
                SnakDataType::ExternalId,
                format!("P{}",property),
                SnakType::Value,
                Some(DataValue::new(
                    DataValueType::StringType, 
                    Value::StringValue(s.to_owned())
                ))
                ),
            vec![],
            self.get_ref()
        )
    }

    fn new_statement_item(&self, property: usize, q: &str) -> Statement {
        Statement::new(
            "statement",
            StatementRank::Normal,
            Snak::new(
                SnakDataType::WikibaseItem,
                format!("P{}",property),
                SnakType::Value,
                Some(DataValue::new(
                    DataValueType::EntityId, 
                    Value::Entity(EntityValue::new(EntityType::Item, q))
                ))
                ),
            vec![],
            self.get_ref()
        )
    }

    fn new_statement_time(&self, property: usize, time: &str, precision: u64) -> Statement {
        // TOSO
        Statement::new(
            "statement",
            StatementRank::Normal,
            Snak::new(
                SnakDataType::ExternalId,
                format!("P{}",property),
                SnakType::Value,
                Some(DataValue::new(
                    DataValueType::Time, 
                    Value::Time(TimeValue::new(0, 0, "http://www.wikidata.org/entity/Q1985727", precision, time, 0))
                ))
                ),
            vec![],
            self.get_ref()
        )
    }

}
