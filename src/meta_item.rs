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
    pub same_as_iri: Vec<String>,
}

impl MetaItem {
    pub fn new() -> Self {
        Self {
            item: ItemEntity::new_empty(),
            prop_text: vec![],
            same_as_iri: vec![],
        }
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
}
