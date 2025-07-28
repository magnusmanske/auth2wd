use crate::{external_id::*, item_merger::ItemMerger, merge_diff::MergeDiff};
use serde::ser::{Serialize, SerializeStruct, Serializer};
use serde_json::json;
use std::{str::FromStr, vec::Vec};
use wikibase_rest_api::{entity::Entity, prelude::*, Statement};

#[derive(Debug, Clone)]
pub struct MetaItem {
    pub item: Item,
    pub prop_text: Vec<ExternalId>,
}

impl Serialize for MetaItem {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("MetaItem", 2)?;
        let mut item = json!(self.item);
        item["type"] = json!("item");
        state.serialize_field("item", &item)?;
        state.serialize_field("prop_text", &self.prop_text)?;
        state.end()
    }
}

impl Default for MetaItem {
    fn default() -> Self {
        Self {
            item: Item::default(),
            prop_text: vec![],
        }
    }
}

impl MetaItem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_from_item(item: Item) -> Self {
        Self {
            item,
            ..Default::default()
        }
    }

    pub async fn from_entity(id: &str) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            item: Item::get(EntityId::item(id), &RestApi::wikidata()?).await?,
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
        let prop = new_claim.property().id();
        if let Some(existing_claim) = self
            .item
            .statements_mut()
            .property_mut(prop)
            .iter_mut()
            .filter(|existing_claim| existing_claim.value() == new_claim.value())
            .filter(|existing_claim| existing_claim.same_qualifiers_as(&new_claim))
            .next()
        {
            // Claim already exists, add new references unless it's an External ID
            if *new_claim.property().datatype() != Some(DataType::ExternalId) {
                let referenced_to_add: Vec<_> = new_claim
                    .references()
                    .iter()
                    .filter(|r| !Self::reference_exists(existing_claim.references(), r))
                    .cloned()
                    .collect();
                existing_claim.references_mut().extend(referenced_to_add);
            }
            return None; // Claim already exists, including references
        }

        // Claim does not exist, adding
        let mut new_claim = new_claim.clone();
        self.check_new_claim_for_dates(&mut new_claim);
        self.item.statements_mut().insert(new_claim.clone());
        Some(new_claim)
    }

    fn get_time_precision_from_statement(statement: &Statement) -> Option<TimePrecision> {
        match statement.value() {
            StatementValue::Value(StatementValueContent::Time {
                time,
                precision,
                calendarmodel,
            }) => Some(*precision),
            _ => None,
        }
    }

    /// Checks if a new claim has a more precise date than existing claims.
    fn check_new_claim_for_dates(&self, new_claim: &mut Statement) {
        let prop = new_claim.property().id();
        if prop != "P569" && prop != "P570" {
            return;
        }
        let new_claim_precision = match Self::get_time_precision_from_statement(&new_claim) {
            Some(precision) => precision,
            None => return,
        };

        let best_existing_precision = self
            .item
            .statements()
            .property(prop)
            .iter()
            .filter_map(|s| Self::get_time_precision_from_statement(s))
            .max()
            .unwrap_or(TimePrecision::BillionYears);
        if new_claim_precision < best_existing_precision {
            new_claim.set_rank(StatementRank::Deprecated);
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
            .statements()
            .statements()
            .iter()
            .flat_map(|(_, v)| v)
            .filter_map(ExternalId::from_external_id_claim)
            .collect()
    }

    pub fn cleanup(&mut self) {
        self.prop_text.sort();
        self.prop_text.dedup();
    }

    pub fn fix_images(&mut self, base_item: &MetaItem) {
        todo!()
        // // Check if base item has P18 image, remove P4765 (commons compatible image URL)
        // if base_item
        //     .item
        //     .statements()
        //     .statements()
        //     .iter()
        //     .flat_map(|(_, v)| v)
        //     .any(|c| c.property().id() == "P18")
        // {
        //     self.item
        //         .statements_mut()
        //         .statements_mut()
        //         .retain(|c| c.clone().as_property_value().property() != "P4765");
        // }
    }

    /// Fixes birth and death dates by deprecating less precise ones.
    /// <https://github.com/magnusmanske/auth2wd/issues/1>
    pub fn fix_dates(&mut self) {
        todo!()
        // for prop in ["P569", "P570"] {
        //     let mut best_precision = 0;
        //     let mut worst_precision = 255;
        //     self.item
        //         .statements()
        //         .iter()
        //         .filter(|c| c.clone().as_property_value().property() == prop)
        //         .for_each(|c| {
        //             if let Some(dv) = c.clone().as_property_value().data_value() {
        //                 if let Value::Time(t) = dv.value() {
        //                     if *t.precision() > best_precision {
        //                         best_precision = *t.precision();
        //                     }
        //                     if *t.precision() < worst_precision {
        //                         worst_precision = *t.precision();
        //                     }
        //                 }
        //             }
        //         });
        //     if best_precision <= worst_precision {
        //         continue;
        //     }
        //     self.item
        //         .statements_mut()
        //         .statements_mut()
        //         .iter_mut()
        //         .filter(|c| c.clone().as_property_value().property() == prop)
        //         .filter(|c| *c.rank() == StatementRank::Normal)
        //         .for_each(|c| {
        //             if let Some(dv) = c.clone().as_property_value().data_value() {
        //                 if let Value::Time(t) = dv.value() {
        //                     if *t.precision() < best_precision {
        //                         // Deprecate statement
        //                         c.set_rank(StatementRank::Deprecated);
        //                         // reason for deprecated rank: item/value with less precision and/or accuracy
        //                         let snak = Snak::new_item("P2241", "Q42727519");
        //                         c.add_qualifier_snak(snak);
        //                     }
        //                 }
        //             }
        //         });
        // }
    }

    // fn add_fake_statement_ids(&mut self) {
    //     self.item
    //         .statements_mut().statements_mut()
    //         .iter_mut()
    //         .filter(|c| c.id().is_none())
    //         .for_each(|c| {
    //             let fake_id = format!("tmp:{}", Uuid::new_v4());
    //             c.set_id(fake_id);
    //         });
    // }

    // pub fn clear_fake_statement_ids(&mut self) {
    //     self.item
    //         .statements_mut().statements_mut()
    //         .iter_mut()
    //         .filter(|c| match c.id() {
    //             Some(id) => id.starts_with("tmp:"),
    //             None => false,
    //         })
    //         .for_each(|c| {
    //             c.remove_id();
    //         });
    // }

    pub fn merge(&mut self, other: &MetaItem) -> MergeDiff {
        // self.add_fake_statement_ids();
        let mut im = ItemMerger::new(self.item.to_owned());
        // im.set_properties_ignore_qualifier_match(vec!["P225".to_string()]);
        let diff = im.merge(&other.item);
        self.item = im.item().clone();
        // diff.apply(&mut self.item); // TODO FIXME
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

    // #[test]
    // fn test_fix_dates() {
    //     let mut mi = MetaItem::new();
    //     let s1 = Statement::new_time(
    //         "P569",
    //         "+1650-12-00T00:00:00Z",
    //         TimePrecision::Month,
    //         GREGORIAN_CALENDAR,
    //     );
    //     let s2 = Statement::new_time(
    //         "P569",
    //         "+1650-00-00T00:00:00Z",
    //         TimePrecision::Year,
    //         GREGORIAN_CALENDAR,
    //     );
    //     let s3 = Statement::new_time(
    //         "P569",
    //         "+1650-12-29T00:00:00Z",
    //         TimePrecision::Day,
    //         GREGORIAN_CALENDAR,
    //     );
    //     mi.item.add_claim(s1);
    //     mi.item.add_claim(s3);
    //     mi.item.add_claim(s2);
    //     mi.fix_dates();
    //     assert_eq!(mi.item.statements().len(), 3);
    //     assert_eq!(*mi.item.statements()[0].rank(), StatementRank::Deprecated);
    //     assert_eq!(*mi.item.statements()[1].rank(), StatementRank::Normal);
    //     assert_eq!(*mi.item.statements()[2].rank(), StatementRank::Deprecated);
    // }
}
