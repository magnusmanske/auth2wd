use serde_json::json;
use serde::ser::{Serialize, Serializer, SerializeStruct};
use std::collections::HashMap;
use std::vec::Vec;
use wikibase::*;


/// This contains the wbeditentiry payload to ADD data to a base item, generated from a merge
#[derive(Debug, Clone)]
pub struct MergeDiff {
    pub labels: Vec<LocaleString>,
    pub aliases: Vec<LocaleString>,
    pub descriptions: Vec<LocaleString>,
    pub sitelinks: Vec<SiteLink>,
    pub altered_statements: HashMap<String,Statement>,
    pub added_statements: Vec<Statement>,
}

impl MergeDiff {
    pub fn new() -> Self {
        Self {
            labels: vec!(), 
            aliases: vec!(), 
            descriptions: vec!(), 
            sitelinks: vec!(), 
            altered_statements: HashMap::new(), 
            added_statements: vec!()
        }
    }

    pub fn add_statement(&mut self, s: Statement) {
        if let Some(id) = s.id() {
            self.altered_statements.insert(id, s);
        } else {
            self.added_statements.push(s);
        }
    }

    fn serialize_labels(&self, list: &Vec<LocaleString>) -> Option<serde_json::Value> {
        match list.is_empty() {
            true => None,
            false => {
                let labels : HashMap<String,serde_json::Value> = list
                    .iter()
                    .map(|l|(l.language().to_owned(),json!({"language":l.language(),"value":l.value(), "add": ""})))
                    .collect();
                Some(json!(labels))
            }
        }
    }

    fn serialize_aliases(&self) -> Option<serde_json::Value> {
        match self.aliases.is_empty() {
            true => None,
            false => {
                let mut ret: HashMap<String,Vec<serde_json::Value>> = HashMap::new();
                for alias in &self.aliases {
                    let v = json!({"language":alias.language(),"value":alias.value(), "add": ""});
                    ret
                        .entry(alias.language().into())
                        .and_modify(|vec|vec.push(v.to_owned()))
                        .or_insert(vec![v]);
                }
                Some(json!(ret))
            }
        }
    }

    fn serialize_sitelinks(&self) -> Option<serde_json::Value> {
        match self.sitelinks.is_empty() {
            true => None,
            false => {
                let labels : HashMap<String,serde_json::Value> = self
                    .sitelinks
                    .iter()
                    .map(|l|(l.site().to_owned(),json!({"site":l.site(),"title":l.title()})))
                    .collect();
                Some(json!(labels))
            }
        }
    }

    fn serialize_claims(&self) -> Option<serde_json::Value> {
        let ret: Vec<serde_json::Value> = self.added_statements
            .iter()
            .chain(self.altered_statements.values())
            .cloned()
            .map(|c|json!(c))
            .map(|c|{
                let mut c = c;
                let _ = c["mainsnak"].as_object_mut().unwrap().remove("datatype");
                for refgroup in c["references"].as_array_mut().unwrap() {
                    for snaks in refgroup["snaks"].as_object_mut().unwrap() {
                        for value in snaks.1.as_array_mut().unwrap() {
                            let _ = value.as_object_mut().unwrap().remove("datatype");
                        }
                    }
                }
                c
            })
            .collect();
        match ret.is_empty() {
            true => None,
            false => Some(json!(ret[0])) // tEStING FIXME [0]
        }
    }
}

impl Serialize for MergeDiff {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut data: HashMap<&str,Option<serde_json::Value>> = HashMap::new();
        data.insert("label",self.serialize_labels(&self.labels));
        data.insert("descriptions",self.serialize_labels(&self.descriptions));
        data.insert("aliases",self.serialize_aliases());
        data.insert("sitelinks",self.serialize_sitelinks());
        data.insert("claims",self.serialize_claims());
        let data: HashMap<&str,serde_json::Value> = data
            .iter()
            .filter(|(_,v)|v.is_some())
            .map(|(k,v)|(k.to_owned(),v.to_owned().unwrap()))
            .collect();

        let mut state = serializer.serialize_struct("MergeDiff", data.len())?;
        for (k,v) in data {
            state.serialize_field(&k, &v)?
        }
        state.end()
    }
}
