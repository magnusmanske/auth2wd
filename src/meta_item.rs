use regex::Regex;
use std::vec::Vec;
use wikibase::*;

lazy_static! {
    static ref DATES : Vec<(Regex,String,u64)> = {
        let mut vec : Vec<(Regex,String,u64)> = vec![] ;
        // NOTE: The pattern always needs to cover the whole string, so use ^$
        vec.push((Regex::new(r"^(\d{3,})$").unwrap(),"+${1}-01-01T00:00:00Z".to_string(),9));
        vec.push((Regex::new(r"^(\d{3,})-(\d{2})$").unwrap(),"+${1}-${2}-01T00:00:00Z".to_string(),10));
        vec.push((Regex::new(r"^(\d{3,})-(\d{2})-(\d{2})$").unwrap(),"+${1}-${2}-${3}T00:00:00Z".to_string(),11));
        vec
    };
}

#[derive(Debug)]
pub struct MetaItem {
    pub item: ItemEntity,
    pub prop_text: Vec<(usize,String)>,
    pub prop_item : Vec<(usize,String)>,
}

impl MetaItem {
    pub fn new() -> Self {
        Self {
            item: ItemEntity::new_empty(),
            prop_text: vec![],
            prop_item: vec![],
        }
    }

    pub fn to_string(&self) -> String {
        let mut ret = "{\"item\":".to_string();
        ret += &self.item.to_json().to_string().replace(",\"type\":null",",\"type\":\"item\""); // Fixing type issue with JSON generator for new items
        ret += ",\"prop_text\":";
        ret += &serde_json::to_string(&self.prop_text).unwrap();
        ret += ",\"prop_item\":";
        ret += &serde_json::to_string(&self.prop_item).unwrap();
        ret += "}";
        ret
    }

    pub fn parse_date(&self, s: &str) -> Option<(String,u64)> {
        DATES.iter().filter_map(|e|{
            let replaced = e.0.replace_all(&s,&e.1);
            if replaced==s {
                None
            } else {
                Some((replaced.to_string(),e.2))
            }
        }).next()
    }

    pub fn add_claim(&mut self, s: Statement) {
        for s2 in self.item.claims_mut() {
            if s.main_snak()==s2.main_snak() && s.qualifiers()==s2.qualifiers() {
                let mut new_references = s.references().clone();
                for r in s.references() {
                    if !s2.references().contains(r) {
                        new_references.push(r.to_owned());
                    }
                }
                s2.set_references(new_references);
                // TODO merge references
                return ;
            }
        }
        self.item.add_claim(s);
    }

    pub fn cleanup(&mut self) {
        self.prop_text.sort();
        self.prop_text.dedup();
        self.prop_item.sort();
        self.prop_item.dedup();
    }
}
