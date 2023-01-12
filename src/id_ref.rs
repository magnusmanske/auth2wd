use sophia::graph::inmem::FastGraph;
use sophia::triple::stream::TripleSource;
use wikibase::*;
use crate::external_importer::*;
use crate::meta_item::*;


pub struct IdRef {
    id: String,
    graph: FastGraph,
}

impl ExternalImporter for IdRef {
    fn graph(&self) -> &FastGraph {
        &self.graph
    }

    fn graph_mut(&mut self) -> &mut FastGraph {
        &mut self.graph
    }

    fn get_key_url(&self, key: &str) -> String {
        format!("http://www.idref.fr/{}/{}",self.id,key)
    }
}

impl IdRef {
    pub async fn new(id: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let rdf_url = format!("https://www.idref.fr/{}.rdf",id);
        let resp = reqwest::get(rdf_url).await?.text().await?;
        let mut graph: FastGraph = FastGraph::new();
        let _ = sophia::parser::xml::parse_str(&resp).add_to_graph(&mut graph)?;
        Ok(Self { id:id.to_string(), graph })
    }


    pub async fn run(&self) -> Result<MetaItem, Box<dyn std::error::Error>> {
        let mut ret = MetaItem::new();

        ret.item.add_claim(self.new_statement_string(269, &self.id));

        for url in self.triples_iris("http://www.w3.org/2002/07/owl#sameAs")? {
            match self.url2external_id(&url) {
                Some(extid) => ret.item.add_claim(self.new_statement_string(extid.property, &extid.id)),
                None => ret.same_as_iri.push(url)
            }
        }

        for s in self.triples_literals("http://xmlns.com/foaf/0.1/gender")? {
            match s.as_str() {
                "male" => ret.item.add_claim(self.new_statement_item(21,"Q6581097")),
                "female" => ret.item.add_claim(self.new_statement_item(21,"Q6581072")),
                _ => ret.prop_text.push((21,s))
            }
        }

        for url in self.triples_iris("http://dbpedia.org/ontology/citizenship")? {
            match self.url2external_id(&url) {
                Some(extid) => {
                    match Self::get_item_for_external_id_value(extid.property,&extid.id).await {
                        Some(item) => ret.item.add_claim(self.new_statement_item(27,&item)),
                        None => ret.prop_text.push((27,url))
                    }
                }
                None => ret.prop_text.push((27,url))
            }
        }

        for s in self.triples_literals("http://xmlns.com/foaf/0.1/name")? {
            if ret.item.label_in_locale("fr").is_none() {
                ret.item.labels_mut().push(LocaleString::new("fr", &s));
            } else {
                ret.item.aliases_mut().push(LocaleString::new("fr", &s));
            }
        }

        for s in self.triples_literals("http://www.w3.org/2004/02/skos/core#prefLabel")? {
            if ret.item.description_in_locale("fr").is_none() {
                ret.item.descriptions_mut().push(LocaleString::new("fr", &s));
            }
        }

        for s in self.triples_subject_literals(&format!("http://www.idref.fr/{}/birth",self.id),"http://purl.org/vocab/bio/0.1/date")? {
            match ret.parse_date(&s) {
                Some((time,precision)) => ret.item.add_claim(self.new_statement_time(569,&time,precision)),
                None => ret.prop_text.push((569,s))
            }
        }

        Ok(ret)
    }
}

// https://www.idref.fr/051626241.xml
