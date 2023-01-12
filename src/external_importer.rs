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
use crate::meta_item::*;

lazy_static! {
    static ref EXTERNAL_ID_REGEXPS : Vec<(Regex,String,usize)> = {
        let mut vec : Vec<(Regex,String,usize)> = vec![] ;
        // NOTE: The pattern always needs to cover the whole string, so use ^$
        vec.push((Regex::new(r"^https{0,1}://viaf.org/viaf/(\d+)$").unwrap(),"${1}".to_string(),214));
        vec.push((Regex::new(r"^https{0,1}://isni.org/isni/(\d{4})(\d{4})(\d{4})(\d{4})$").unwrap(),"${1} ${2} ${3} ${4}".to_string(),213));
        vec.push((Regex::new(r"^https{0,1}://isni-url.oclc.nl/isni/(\d{4})(\d{4})(\d{4})(\d{4})$").unwrap(),"${1} ${2} ${3} ${4}".to_string(),213));
        vec.push((Regex::new(r"^https{0,1}://d-nb.info/gnd/(1[012]?\d{7}[0-9X]|[47]\d{6}-\d|[1-9]\d{0,7}-[0-9X]|3\d{7}[0-9X])$").unwrap(),"${1}".to_string(),227));
        vec.push((Regex::new(r"^https{0,1}://id.loc.gov/authorities/names/(gf|n|nb|nr|no|ns|sh|sj)([4-9][0-9]|00|20[0-2][0-9])([0-9]{6})$").unwrap(),"${1}${2}${3}".to_string(),244));
        vec.push((Regex::new(r"^https{0,1}://id.loc.gov/rwo/agents/(gf|n|nb|nr|no|ns|sh|sj)([4-9][0-9]|00|20[0-2][0-9])([0-9]{6})(\.html){0,1}$").unwrap(),"${1}${2}${3}".to_string(),244));
        vec.push((Regex::new(r"^https{0,1}://data.bnf.fr/(\d{8,9}).*$").unwrap(),"${1}".to_string(),268));
        vec.push((Regex::new(r"^https{0,1}://data.bnf.fr/ark:/12148/cb(\d{8,9}[0-9bcdfghjkmnpqrstvwxz]).*$").unwrap(),"${1}".to_string(),268));
        vec.push((Regex::new(r"^https{0,1}://www.idref.fr/(\d{8}[\dX])$").unwrap(),"${1}".to_string(),269));
        vec.push((Regex::new(r"^https{0,1}://libris.kb.se/resource/auth/([1-9]\d{4,5})$").unwrap(),"${1}".to_string(),906));
        vec.push((Regex::new(r"^https{0,1}://sws.geonames.org/([1-9][0-9]{0,8}).*$").unwrap(),"${1}".to_string(),1566));
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
    fn primary_language(&self) -> String;



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

    fn triples_property_object_iris(&self, p: &str, o: &str) ->  Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut ret = vec![];
        let iri_p = SimpleIri::new(p, None)?;
        let iri_o = SimpleIri::new(o, None)?;
        self.graph().triples_with_po(&iri_p,&iri_o).for_each_triple(|t|
            if let Term::Iri(iri) = t.s() {
                ret.push(iri.ns().to_string());
            }
        )?;
        Ok(ret)
    }

    #[async_recursion]
    async fn search_wikidata_single_item(query: &str) -> Option<String> {
        // TODO urlencode?
        let url = format!("https://www.wikidata.org/w/api.php?action=query&list=search&srnamespace=0&format=json&srsearch={}",&query);
        let text = reqwest::get(url).await.ok()?.text().await.ok()?;
        let j: serde_json::Value = serde_json::from_str(&text).ok()?;
        if j["query"]["searchinfo"]["totalhits"].as_i64()? == 1 {
            return Some(j["query"]["search"][0]["title"].as_str()?.to_string());
        }
        None
    }

    #[async_recursion]
    async fn get_item_for_external_id_value(property: usize, value: &str) -> Option<String> {
        let query = format!("haswbstatement:\"P{}={}\"",property,value);
        Self::search_wikidata_single_item(&query).await
    }

    #[async_recursion]
    async fn get_item_for_string_external_id_value(s: &str, property: usize, value: &str) -> Option<String> {
        let query = format!("{s} haswbstatement:\"P{property}={value}\"");
        Self::search_wikidata_single_item(&query).await
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


    fn add_same_as(&self, ret: &mut MetaItem) -> Result<(), Box<dyn std::error::Error>> {
        let iris = [
            "http://www.w3.org/2002/07/owl#sameAs",
            "http://www.w3.org/2002/07/owl#sameAs",
        ];
        for iri in iris {
            for url in self.triples_iris(iri)? {
                match self.url2external_id(&url) {
                    Some(extid) => ret.item.add_claim(self.new_statement_string(extid.property, &extid.id)),
                    None => ret.same_as_iri.push(url)
                }
            }
        }
        Ok(())
    }

    fn add_gender(&self, ret: &mut MetaItem) -> Result<(), Box<dyn std::error::Error>> {
        for s in self.triples_literals("http://xmlns.com/foaf/0.1/gender")? {
            match s.as_str() {
                "male" => ret.item.add_claim(self.new_statement_item(21,"Q6581097")),
                "female" => ret.item.add_claim(self.new_statement_item(21,"Q6581072")),
                _ => ret.prop_text.push((21,s))
            }
        }

        for s in self.triples_literals("http://www.rdaregistry.info/Elements/a/P50116")? {
            match s.as_str() {
                "Masculino" => ret.item.add_claim(self.new_statement_item(21,"Q6581097")),
                "Femenino" => ret.item.add_claim(self.new_statement_item(21,"Q6581072")),
                _ => ret.prop_text.push((21,s))
            }
        }

        for url in self.triples_iris("https://d-nb.info/standards/elementset/gnd#gender")? {
            match url.as_str() {
                "https://d-nb.info/standards/vocab/gnd/gender#male" => ret.item.add_claim(self.new_statement_item(21,"Q6581097")),
                "https://d-nb.info/standards/vocab/gnd/gender#female" => ret.item.add_claim(self.new_statement_item(21,"Q6581072")),
                _ => ret.prop_text.push((21,url))
            }
        }

        Ok(())
    }

    fn limit_string_length(&self, s: &str) -> String {
        match s.get(..250) {
            Some(s) => s.to_string(),
            None => s.to_string()
        }
    }

    fn add_label_aliases(&self, ret: &mut MetaItem) -> Result<(), Box<dyn std::error::Error>> {
        let language = self.primary_language();

        // TODO "last, first" => "first last"
        let urls = [
            "http://xmlns.com/foaf/0.1/name",
            "http://www.w3.org/2000/01/rdf-schema#label",
            "https://datos.bne.es/def/P5012",
            "https://d-nb.info/standards/elementset/gnd#preferredNameForThePerson",
            "https://d-nb.info/standards/elementset/gnd#variantNameForThePerson",
        ];
        for url in urls {
            for s in self.triples_literals(&url)? {
                let s = self.limit_string_length(&s);
                if ret.item.label_in_locale(&language).is_none() {
                    ret.item.labels_mut().push(LocaleString::new(&language, &s));
                } else {
                    ret.item.aliases_mut().push(LocaleString::new(&language, &s));
                }
            }
        }

        Ok(())
    }

    fn add_description(&self, ret: &mut MetaItem) -> Result<(), Box<dyn std::error::Error>> {
        let language = self.primary_language();
        let iris = [
            "http://www.w3.org/2004/02/skos/core#prefLabel",
            "https://datos.bne.es/def/P3067",
        ];
        for iri in iris {
            for s in self.triples_literals(iri)? {
                if ret.item.description_in_locale(&language).is_none() {
                    let s = self.limit_string_length(&s);
                    ret.item.descriptions_mut().push(LocaleString::new(&language, &s));
                }
            }
        }
        Ok(())
    }

    fn add_language(&self, ret: &mut MetaItem) -> Result<(), Box<dyn std::error::Error>> {
        for s in self.triples_literals("http://www.rdaregistry.info/Elements/a/P50102")? {
            ret.prop_text.push((1412,s))
        }
        Ok(())
    }

    #[async_recursion]
    async fn try_rescue_prop_text(&self, mi : &mut MetaItem) -> Result<Vec<(usize,String)>, Box<dyn std::error::Error>> {
        let mut new_prop_text = vec![];
        let mut new_statements = vec![];
        for (prop,s) in &mi.prop_text {
            let p31 = match prop {
                1412 => "Q34770", // Language spoken or written => laguage
                131 => "Q515", // Located in => city
                27 => "Q6256", // Nationality
                _ => continue
            };
            match Self::get_item_for_string_external_id_value(&s,31,p31).await {
                Some(item) => new_statements.push((*prop,item.to_string())),
                None => new_prop_text.push((*prop,s.to_owned()))
            }
        }
        mi.prop_text = new_prop_text;
        Ok(new_statements)
    }

}
