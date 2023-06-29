use serde_json::json;
use serde::ser::{Serialize, Serializer, SerializeStruct};
use regex::Regex;
use std::vec::Vec;
use std::cmp::Ordering;
use wikibase::*;
use crate::external_id::*;
use crate::external_importer::ExternalImporter;
use crate::merge_diff::*;

lazy_static! {
    static ref DATES : Vec<(Regex,String,u64)> = {
        let mut vec : Vec<(Regex,String,u64)> = vec![] ;
        // NOTE: The pattern always needs to cover the whole string, so use ^$
        vec.push((Regex::new(r"^(\d{3,})$").unwrap(),"+${1}-00-00T00:00:00Z".to_string(),9));
        vec.push((Regex::new(r"^(\d{3,})-(\d{2})$").unwrap(),"+${1}-${2}-00T00:00:00Z".to_string(),10));
        vec.push((Regex::new(r"^(\d{3,})-(\d{2})-(\d{2})$").unwrap(),"+${1}-${2}-${3}T00:00:00Z".to_string(),11));
        vec.push((Regex::new(r"^https?://data.bnf.fr/date/(\d+)/?$").unwrap(),"+${1}-00-00T00:00:00Z".to_string(),9));
        vec
    };
    static ref YEAR_FIX: Regex = Regex::new(r"-\d\d-\d\dT").unwrap();
    static ref MONTH_FIX: Regex = Regex::new(r"-\d\dT").unwrap();
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
        let api = mediawiki::api::Api::new("https://www.wikidata.org/w/api.php").await?;
        let entity_container = wikibase::entity_container::EntityContainer::new();
        let entity = entity_container.load_entity(&api,id).await?;
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

    fn get_external_ids_from_reference(reference: &Reference) -> Vec<ExternalId> {
        reference
            .snaks()
            .iter()
            .filter(|snak|*snak.datatype()==SnakDataType::ExternalId)
            .map(|snak|(ExternalId::prop_numeric(snak.property()),snak.data_value()))
            .filter(|(prop,dv)|prop.is_some()&&dv.is_some())
            .map(|(prop,dv)|(prop.unwrap(),dv.to_owned().unwrap()))
            .map(|(prop,dv)|(prop,dv.value().to_owned()))
            .filter_map(|(prop,value)|match value {
                Value::StringValue(s) => Some(ExternalId::new(prop,&s)),
                _ => None
            })
            .collect()
    }

    /// Checks if a reference already exists in a list of references.
    /// Uses direct equal, or the presence of any external ID from the new reference.
    /// Returns `true` if the reference exists, `false` otherwise.
    fn reference_exists(existing_references: &Vec<Reference>, new_reference: &Reference) -> bool {
        if existing_references.contains(new_reference) { // Easy case
            return true;
        }
        // Check if any external ID in the new reference is present in any existing reference
        let ext_ids = Self::get_external_ids_from_reference(new_reference);
        existing_references
            .iter()
            .map(|reference|Self::get_external_ids_from_reference(reference))
            .filter(|existing_external_ids|!existing_external_ids.is_empty())
            .any(|existing_external_ids|ext_ids.iter().any(|ext_id|existing_external_ids.contains(ext_id)))
    }

    fn is_snak_identical(snak1: &Snak, snak2: &Snak) -> bool {
        snak1.property()==snak2.property() &&
        Self::is_data_value_identical(snak1.data_value(),snak2.data_value())
    }

    fn is_data_value_identical(dv1:&Option<DataValue>,dv2:&Option<DataValue>) -> bool {
        if let (Some(dv1),Some(dv2)) = (dv1,dv2) {
            if let (Value::Time(t1), Value::Time(t2)) = (dv1.value(),dv2.value()) {
                return Self::is_time_value_identical(t1,t2);
            }
        }
        dv1==dv2
    }

    fn is_time_value_identical(t1: &TimeValue, t2: &TimeValue) -> bool {
        if t1.precision()!=t2.precision() ||
            t1.calendarmodel()!=t1.calendarmodel() ||
            t1.before()!=t1.before() ||
            t1.after()!=t1.after() ||
            t1.timezone()!=t1.timezone()
            {
                return false
        }
        match t1.precision() {
            9 => {
                let t1s = YEAR_FIX.replace_all(t1.time(),"-00-00T");
                let t2s = YEAR_FIX.replace_all(t2.time(),"-00-00T");
                t1s==t2s
            }
            10 => {
                let t1s = MONTH_FIX.replace_all(t1.time(),"-00T");
                let t2s = MONTH_FIX.replace_all(t2.time(),"-00T");
                t1s==t2s
            }
            _ => *t1==*t2
        }
    }

    fn are_qualifiers_identical(q1: &Vec<Snak>, q2: &Vec<Snak>) -> bool {
        if q1.is_empty() && q2.is_empty() {
            return true;
        }
        if q1.len()!=q2.len() {
            return false;
        }
        let mut q1 = q1.clone();
        let mut q2= q2.clone();
        q1.sort_by(Self::compare_snak);
        q2.sort_by(Self::compare_snak);
        !q1.iter().zip(q2.iter()).any(|(snak1,snak2)|!Self::is_snak_identical(&snak1, &snak2))
    }

    /// Adds a new claim to the item claims.
    /// If a claim with the same value and qualifiers (TBD) already exists, it will try and add any new references.
    /// Returns `Some(claim)` if the claim was added or changed, `None` otherwise.
    pub fn add_claim(&mut self, new_claim: Statement) -> Option<Statement>{
        let existing_claims_iter = self
            .item
            .claims_mut()
            .iter_mut()
            .filter(|existing_claim|Self::is_snak_identical(&new_claim.main_snak(),&existing_claim.main_snak()))
            .filter(|existing_claim|Self::are_qualifiers_identical(&new_claim.qualifiers(),&existing_claim.qualifiers()));
        for existing_claim in existing_claims_iter {
            if *new_claim.main_snak().datatype() == SnakDataType::ExternalId {
                return None // Claim already exists, don't add reference to external IDs
            }
            let mut new_references = existing_claim.references().clone();
            let mut reference_changed = false;
            for r in new_claim.references() {
                if !Self::reference_exists(&new_references,&r) {
                    new_references.push(r.to_owned());
                    reference_changed = true;
                }
            }
            if reference_changed {
                existing_claim.set_references(new_references);
                return Some(existing_claim.to_owned()); // Claim has changed (references added)
            } else {
                return None // Claim already exists, including references
            }
        }
        // Claim does not exist, adding
        self.item.add_claim(new_claim.clone());
        Some(new_claim)
    }

    pub fn add_prop_text(&mut self, ext_id: ExternalId) -> Option<wikibase::Statement> {
        let ei = crate::viaf::VIAF::new("312603351").unwrap(); // Any prop/ID will do
        if !ei.do_not_use_external_url(&ext_id.id) {
            self.prop_text.push(ext_id);
        }
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

    fn compare_snak(snak1: &Snak, snak2: &Snak) -> Ordering {
        match snak1.property().cmp(snak2.property()) {
            Ordering::Equal => {
                let j1 = json!(snak1.data_value());
                let j2 = json!(snak2.data_value());
                let j1 = j1.to_string();
                let j2 = j2.to_string();
                j1.cmp(&j2)
            },
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
            .filter(|d|!self.item.aliases().contains(d))
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
            .filter(|a|!self.item.descriptions().contains(a))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_compare() {
        // Year, ignore month and day
        let t1 = TimeValue::new(0, 0, "http://www.wikidata.org/entity/Q1985727", 9, "+1650-00-00T00:00:00Z", 0);
        let t2 = TimeValue::new(0, 0, "http://www.wikidata.org/entity/Q1985727", 9, "+1650-12-29T00:00:00Z", 0);
        assert!(MetaItem::is_time_value_identical(&t1,&t1));
        assert!(MetaItem::is_time_value_identical(&t1,&t2));

        // Month, ignore day
        let t1 = TimeValue::new(0, 0, "http://www.wikidata.org/entity/Q1985727", 10, "+1650-12-00T00:00:00Z", 0);
        let t2 = TimeValue::new(0, 0, "http://www.wikidata.org/entity/Q1985727", 10, "+1650-12-29T00:00:00Z", 0);
        assert!(MetaItem::is_time_value_identical(&t1,&t1));
        assert!(MetaItem::is_time_value_identical(&t1,&t2));
    }

    #[test]
    fn test_parse_date() {
        let mi = MetaItem::new();
        assert_eq!(mi.parse_date("1987"),Some(("+1987-00-00T00:00:00Z".to_string(),9)));
        assert_eq!(mi.parse_date("1987-12"),Some(("+1987-12-00T00:00:00Z".to_string(),10)));
        assert_eq!(mi.parse_date("1987-12-27"),Some(("+1987-12-27T00:00:00Z".to_string(),11)));
        assert_eq!(mi.parse_date("http://data.bnf.fr/date/1978"),Some(("+1978-00-00T00:00:00Z".to_string(),9)));
    }

    #[test]
    fn test_add_prop_text() {
        let mut mi = MetaItem::new();
        let ext_id = ExternalId::new(214,"12345");
        mi.add_prop_text(ext_id.clone());
        assert_eq!(mi.prop_text,vec![ext_id]);
    }

    #[test]
    fn test_cleanup() {
        let mut mi = MetaItem::new();
        let ext_id1 = ExternalId::new(214,"12345");
        let ext_id2 = ExternalId::new(123,"456");
        mi.add_prop_text(ext_id1.clone());
        mi.add_prop_text(ext_id2.clone());
        mi.add_prop_text(ext_id1.clone());
        mi.cleanup();
        assert_eq!(mi.prop_text,vec![ext_id2,ext_id1]);
    }

    #[test]
    fn test_compare_locale_string() {
        let ls1 = LocaleString::new("en", "foo");
        let ls2 = LocaleString::new("en", "bar");
        let ls3 = LocaleString::new("de", "foo");
        assert_eq!(Ordering::Equal,MetaItem::compare_locale_string(&ls1,&ls1));
        assert_eq!(Ordering::Less,MetaItem::compare_locale_string(&ls2,&ls1));
        assert_eq!(Ordering::Greater,MetaItem::compare_locale_string(&ls1,&ls2));
        assert_eq!(Ordering::Greater,MetaItem::compare_locale_string(&ls1,&ls3));
    }

}