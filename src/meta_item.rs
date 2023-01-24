use serde_json::json;
use serde::ser::{Serialize, Serializer, SerializeStruct};
use regex::Regex;
use std::vec::Vec;
use std::cmp::Ordering;
use wikibase::*;
use crate::external_id::*;
use crate::merge_diff::*;

lazy_static! {
    static ref DATES : Vec<(Regex,String,u64)> = {
        let mut vec : Vec<(Regex,String,u64)> = vec![] ;
        // NOTE: The pattern always needs to cover the whole string, so use ^$
        vec.push((Regex::new(r"^(\d{3,})$").unwrap(),"+${1}-01-01T00:00:00Z".to_string(),9));
        vec.push((Regex::new(r"^(\d{3,})-(\d{2})$").unwrap(),"+${1}-${2}-01T00:00:00Z".to_string(),10));
        vec.push((Regex::new(r"^(\d{3,})-(\d{2})-(\d{2})$").unwrap(),"+${1}-${2}-${3}T00:00:00Z".to_string(),11));
        vec.push((Regex::new(r"^https{0,1}://data.bnf.fr/date/(\d+)/$").unwrap(),"+${1}-01-01T00:00:00Z".to_string(),9));
        vec
    };
}


#[derive(Debug, Clone)]
pub struct MetaItem {
    pub item: ItemEntity,
    pub prop_text: Vec<ExternalId>,
}

impl Serialize for MetaItem {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("MetaItem", 2)?;
        let mut item = self.item.to_json();
        item["type"] = json!("item");
        state.serialize_field("item", &item)?;
        state.serialize_field("prop_text", &self.prop_text)?;
        state.end()
    }
}

impl MetaItem {
    pub fn new() -> Self {
        Self {
            item: ItemEntity::new_empty(),
            prop_text: vec![],
        }
    }

    pub async fn from_entity(id: &str) -> Result<Self, Box<dyn std::error::Error>> {
        //let config = Configuration::new("AC2WD/0.1").await?;
        let api = mediawiki::api::Api::new("https://www.wikidata.org/w/api.php").await?;
        let entity_container = wikibase::entity_container::EntityContainer::new();
        let entity = entity_container.load_entity(&api,id).await?;
        //let entity = wikibase::Entity::new_from_id(id,&config).await?;
        let item = match entity {
            Entity::Item(item) => item,
            _ => return Err(format!("Not an item: '{id}'").into())
        };
        Ok(Self{item,prop_text:vec![]})
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

    pub fn extract_external_ids(&self) -> Vec<ExternalId> {
        self
            .item
            .claims()
            .iter()
            .filter(|s|*s.main_snak().datatype() == SnakDataType::ExternalId)
            .filter(|s|s.main_snak().data_value().is_some())
            .map(|s|(s.property().to_string(),s.main_snak().data_value().to_owned().unwrap()))
            .filter_map(|(prop,dv)|{
                match dv.value() {
                    Value::StringValue(s) => {
                        let prop_numeric = ExternalId::prop_numeric(&prop).unwrap();
                        Some(ExternalId::new(prop_numeric,s))
                    }
                    _ => None
                }
            })
            .collect()
    }

    pub fn add_claim(&mut self, s: Statement) -> Option<Statement>{
        for s2 in self.item.claims_mut() {
            if s.main_snak()==s2.main_snak() && s.qualifiers()==s2.qualifiers() {
                if *s.main_snak().datatype() == SnakDataType::ExternalId {
                    // Don't add reference to external IDs
                    return None
                }
                let mut new_references = s.references().clone();
                let mut reference_changed = false;
                for r in s.references() {
                    if !s2.references().contains(r) {
                        new_references.push(r.to_owned());
                        reference_changed = true;
                    }
                }
                if reference_changed {
                    s2.set_references(new_references);
                    // TODO merge references
                    return Some(s2.to_owned());
                } else {
                    return None
                }
            }
        }
        self.item.add_claim(s.clone());
        Some(s)
    }

    pub fn add_prop_text(&mut self, ext_id: ExternalId) -> Option<wikibase::Statement> {
        self.prop_text.push(ext_id);
        None
    }

    pub fn get_external_ids(&self) -> Vec<ExternalId> {
        self
            .item
            .claims()
            .iter()
            .filter_map(|claim|ExternalId::from_external_id_claim(claim))
            .collect()
    }

    pub fn cleanup(&mut self) {
        self.prop_text.sort();
        self.prop_text.dedup();
    }

    fn compare_locale_string(a: &LocaleString, b: &LocaleString) -> Ordering {
        match a.language().cmp(b.language()) {
            Ordering::Equal => a.value().cmp(b.value()),
            other => other,
        }
    }

    fn merge_locale_strings(mine: &mut Vec<LocaleString>, other: &Vec<LocaleString>, diff: &mut Vec<LocaleString>) -> Vec<LocaleString> {
        let mut ret = vec![];
        let mut new_ones: Vec<LocaleString> = other
            .iter()
            .filter_map(|x|{
                match mine.iter().filter(|y|x.language()==y.language()).next() {
                    Some(y) => {
                        if x.value()!=y.value() {
                            ret.push(x.clone()); // Labels for which a language already exists, as aliases
                        }
                        None    
                    }
                    None => {
                        Some(x.clone())
                    }
                }
            })
            .collect();
        diff.append(&mut new_ones.clone());
        mine.append(&mut new_ones);
        ret
    }

    pub fn merge(&mut self, other: &MetaItem) -> MergeDiff {
        let mut diff = MergeDiff::new();
        let mut new_aliases = Self::merge_locale_strings(self.item.labels_mut(),other.item.labels(), &mut diff.labels);

        // Descriptions
        let mut new_ones: Vec<LocaleString> = other.item.descriptions()
            .iter()
            .filter_map(|x|
                match self.item.descriptions().iter().filter(|y|x.language()==y.language()).next() {
                    Some(_) => None,
                    None => Some(x.clone())
                }
            )
            .filter(|d|!self.item.labels().contains(d))
            .collect();
        diff.descriptions.append(&mut new_ones.clone());
        self.item.descriptions_mut().append(&mut new_ones);


        new_aliases.append(&mut other.item.aliases().clone());
        new_aliases.sort_by(Self::compare_locale_string);
        new_aliases.dedup();
        diff.aliases = new_aliases
            .iter()
            .filter(|a|!self.item.aliases().contains(a))
            .filter(|a|!self.item.labels().contains(a))
            .cloned()
            .collect();
        self.item.aliases_mut().append(&mut other.item.aliases().to_owned());
        self.item.aliases_mut().sort_by(Self::compare_locale_string);
        self.item.aliases_mut().dedup();

        // Sitelinks: add only
        if let Some(sitelinks) = other.item.sitelinks() {
            let mut new_ones: Vec<SiteLink> = sitelinks
            .iter()
            .filter(|x|{
                match self.item.sitelinks() {
                    Some(sl) => !sl.iter().any(|y|x.site()==y.site()),
                    None => true
                }
            })
            .cloned()
            .collect();
            if let Some(my_sitelinks) = self.item.sitelinks_mut() {
                diff.sitelinks = new_ones.clone();
                my_sitelinks.append(&mut new_ones);
            }
        }

        for claim in other.item.claims() {
            if let Some(s) = self.add_claim(claim.to_owned()) {
                diff.add_statement(s)
            }
        }

        self.prop_text.append(&mut other.prop_text.clone());
        self.prop_text.sort();
        self.prop_text.dedup();
        diff
    }

}
