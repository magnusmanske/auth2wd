use crate::external_id::*;
use serde::ser::{Serialize, SerializeStruct, Serializer};
use serde_json::json;
use std::str::FromStr;
use std::vec::Vec;
use wikimisc::item_merger::ItemMerger;
use wikimisc::merge_diff::MergeDiff;
use wikimisc::wikibase::*;

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

impl Default for MetaItem {
    fn default() -> Self {
        Self {
            item: ItemEntity::new_empty(),
            prop_text: vec![],
        }
    }
}

impl MetaItem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_from_item(item: ItemEntity) -> Self {
        Self {
            item,
            ..Default::default()
        }
    }

    pub async fn from_entity(id: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let api = mediawiki::api::Api::new("https://www.wikidata.org/w/api.php").await?;
        let entity_container = entity_container::EntityContainer::new();
        let entity = entity_container.load_entity(&api, id).await?;
        let item = match entity {
            Entity::Item(item) => item,
            _ => return Err(format!("Not an item: '{id}'").into()),
        };
        Ok(Self {
            item,
            prop_text: vec![],
        })
    }

    /// Parses a date string and returns a tuple with the time and precision.
    pub fn parse_date(&self, s: &str) -> Option<(String, u64)> {
        let date = wikimisc::date::Date::from_str(s).ok()?;
        Some((date.time().to_string(), date.precision()))
    }

    /// Checks if a reference already exists in a list of references.
    /// Uses direct equal, or the presence of any external ID from the new reference.
    /// Returns `true` if the reference exists, `false` otherwise.
    fn reference_exists(existing_references: &[Reference], new_reference: &Reference) -> bool {
        if existing_references.contains(new_reference) {
            // Easy case
            return true;
        }
        // Check if any external ID in the new reference is present in any existing reference
        let ext_ids = ItemMerger::get_external_ids_from_reference(new_reference);
        existing_references
            .iter()
            .map(ItemMerger::get_external_ids_from_reference)
            .filter(|existing_external_ids| !existing_external_ids.is_empty())
            .any(|existing_external_ids| {
                ext_ids
                    .iter()
                    .any(|ext_id| existing_external_ids.contains(ext_id))
            })
    }

    /// Adds a new claim to the item claims.
    /// If a claim with the same value and qualifiers (TBD) already exists, it will try and add any new references.
    /// Returns `Some(claim)` if the claim was added or changed, `None` otherwise.
    pub fn add_claim(&mut self, new_claim: Statement) -> Option<Statement> {
        let mut existing_claims_iter = self
            .item
            .claims_mut()
            .iter_mut()
            .filter(|existing_claim| {
                ItemMerger::is_snak_identical(new_claim.main_snak(), existing_claim.main_snak())
            })
            .filter(|existing_claim| {
                ItemMerger::are_qualifiers_identical(
                    new_claim.qualifiers(),
                    existing_claim.qualifiers(),
                )
            });
        if let Some(existing_claim) = existing_claims_iter.next() {
            // At least one claim exists, use first one
            if *new_claim.main_snak().datatype() == SnakDataType::ExternalId {
                return None; // Claim already exists, don't add reference to external IDs
            }
            let mut new_references = existing_claim.references().clone();
            let mut reference_changed = false;
            for r in new_claim.references() {
                if !Self::reference_exists(&new_references, r) {
                    new_references.push(r.to_owned());
                    reference_changed = true;
                }
            }
            if reference_changed {
                existing_claim.set_references(new_references);
                return Some(existing_claim.to_owned()); // Claim has changed (references added)
            }
            return None; // Claim already exists, including references
        }

        let mut new_claim = new_claim.clone();
        self.check_new_claim_for_dates(&mut new_claim);

        // Claim does not exist, adding
        self.item.add_claim(new_claim.clone());
        Some(new_claim)
    }

    /// Checks if a new claim has a more precise date than existing claims.
    fn check_new_claim_for_dates(&self, new_claim: &mut Statement) {
        let prop = new_claim.property();
        if prop != "P569" && prop != "P570" {
            return;
        }
        if let Some(data_value) = new_claim.main_snak().data_value() {
            let new_claim_precision = match data_value.value() {
                Value::Time(t) => *t.precision(),
                _ => return,
            };

            let best_existing_precision = self
                .item
                .claims()
                .iter()
                .filter(|c| c.property() == prop)
                .filter_map(|c| c.main_snak().data_value().to_owned())
                .filter_map(|dv| match dv.value() {
                    Value::Time(t) => Some(*t.precision()),
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            if new_claim_precision < best_existing_precision {
                new_claim.set_rank(StatementRank::Deprecated);
            }
        }
    }

    pub fn add_prop_text(&mut self, ext_id: ExternalId) -> Option<Statement> {
        if !ExternalId::do_not_use_external_url(ext_id.id()) {
            self.prop_text.push(ext_id);
        }
        None
    }

    pub fn get_external_ids(&self) -> Vec<ExternalId> {
        self.item
            .claims()
            .iter()
            .filter_map(ExternalId::from_external_id_claim)
            .collect()
    }

    pub fn cleanup(&mut self) {
        self.prop_text.sort();
        self.prop_text.dedup();
    }

    pub fn fix_images(&mut self, base_item: &MetaItem) {
        // Check if base item has P18 image, remove P4765 (commons compatible image URL)
        if base_item
            .item
            .claims()
            .iter()
            .any(|c| c.main_snak().property() == "P18")
        {
            self.item
                .claims_mut()
                .retain(|c| c.main_snak().property() != "P4765");
        }
    }

    /// Fixes birth and death dates by deprecating less precise ones.
    /// <https://github.com/magnusmanske/auth2wd/issues/1>
    pub fn fix_dates(&mut self) {
        for prop in ["P569", "P570"] {
            let mut best_precision = 0;
            let mut worst_precision = 255;
            self.item
                .claims()
                .iter()
                .filter(|c| c.main_snak().property() == prop)
                .for_each(|c| {
                    if let Some(dv) = c.main_snak().data_value() {
                        if let Value::Time(t) = dv.value() {
                            if *t.precision() > best_precision {
                                best_precision = *t.precision();
                            }
                            if *t.precision() < worst_precision {
                                worst_precision = *t.precision();
                            }
                        }
                    }
                });
            if best_precision <= worst_precision {
                continue;
            }
            self.item
                .claims_mut()
                .iter_mut()
                .filter(|c| c.main_snak().property() == prop)
                .filter(|c| *c.rank() == StatementRank::Normal)
                .for_each(|c| {
                    if let Some(dv) = c.main_snak().data_value() {
                        if let Value::Time(t) = dv.value() {
                            if *t.precision() < best_precision {
                                // Deprecate statement
                                c.set_rank(StatementRank::Deprecated);
                                // reason for deprecated rank: item/value with less precision and/or accuracy
                                let snak = Snak::new_item("P2241", "Q42727519");
                                c.add_qualifier_snak(snak);
                            }
                        }
                    }
                });
        }
    }

    pub fn merge(&mut self, other: &MetaItem) -> MergeDiff {
        let mut im = ItemMerger::new(self.item.to_owned());
        im.set_properties_ignore_qualifier_match(vec!["P225".to_string()]);
        let diff = im.merge(&other.item);
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
    fn test_parse_date() {
        let mi = MetaItem::new();
        assert_eq!(
            mi.parse_date("1987"),
            Some(("+1987-00-00T00:00:00Z".to_string(), 9))
        );
        assert_eq!(
            mi.parse_date("1987-12"),
            Some(("+1987-12-00T00:00:00Z".to_string(), 10))
        );
        assert_eq!(
            mi.parse_date("1987-12-27"),
            Some(("+1987-12-27T00:00:00Z".to_string(), 11))
        );
        assert_eq!(
            mi.parse_date("http://data.bnf.fr/date/1978"),
            Some(("+1978-00-00T00:00:00Z".to_string(), 9))
        );
    }

    #[tokio::test]
    async fn test_add_prop_text() {
        let mut mi = MetaItem::new();
        let ext_id = ExternalId::new(214, "12345");
        mi.add_prop_text(ext_id.clone());
        assert_eq!(mi.prop_text, vec![ext_id]);
    }

    #[tokio::test]
    async fn test_cleanup() {
        let mut mi = MetaItem::new();
        let ext_id1 = ExternalId::new(214, "12345");
        let ext_id2 = ExternalId::new(123, "456");
        mi.add_prop_text(ext_id1.clone());
        mi.add_prop_text(ext_id2.clone());
        mi.add_prop_text(ext_id1.clone());
        mi.cleanup();
        assert_eq!(mi.prop_text, vec![ext_id2, ext_id1]);
    }

    #[test]
    fn test_fix_dates() {
        let mut mi = MetaItem::new();
        let s1 = Statement::new_normal(
            Snak::new_time("P569", "+1650-12-00T00:00:00Z", 10),
            vec![],
            vec![],
        );
        let s2 = Statement::new_normal(
            Snak::new_time("P569", "+1650-00-00T00:00:00Z", 9),
            vec![],
            vec![],
        );
        let s3 = Statement::new_normal(
            Snak::new_time("P569", "+1650-12-29T00:00:00Z", 11),
            vec![],
            vec![],
        );
        mi.item.add_claim(s1);
        mi.item.add_claim(s3);
        mi.item.add_claim(s2);
        mi.fix_dates();
        assert_eq!(mi.item.claims().len(), 3);
        assert_eq!(*mi.item.claims()[0].rank(), StatementRank::Deprecated);
        assert_eq!(*mi.item.claims()[1].rank(), StatementRank::Normal);
        assert_eq!(*mi.item.claims()[2].rank(), StatementRank::Deprecated);
    }
}
