use serde::{Deserialize, Serialize};
use wikibase_rest_api::{
    prelude::{StatementValue, StatementValueContent},
    DataType, Item, Reference,
};

use crate::external_id::ExternalId;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MergeDiff {}

impl MergeDiff {
    pub fn extend(&mut self, _other: &MergeDiff) {
        todo!()
        // Implement the logic to extend the current MergeDiff with another MergeDiff
    }

    pub fn apply(&self, _item: &mut Item) {
        todo!()
        // Implement the logic to apply the MergeDiff to an Item
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ItemMerger {
    item: Item,
}

impl ItemMerger {
    pub fn new(item: Item) -> Self {
        Self { item }
    }

    pub fn merge(&mut self, _new_item: &Item) -> MergeDiff {
        todo!()
    }

    pub fn item(&self) -> &Item {
        &self.item
    }

    pub fn get_external_ids_from_reference(reference: &Reference) -> Vec<ExternalId> {
        reference
            .parts()
            .iter()
            .filter(|pv| *pv.property().datatype() == Some(DataType::ExternalId))
            .map(|pv| (ExternalId::prop_numeric(pv.property().id()), pv.value()))
            .filter(|(prop, _dv)| prop.is_some())
            .map(|(prop, dv)| (prop.unwrap(), dv.to_owned()))
            .map(|(prop, dv)| (prop, dv))
            .filter_map(|(prop, value)| match value {
                StatementValue::Value(StatementValueContent::String(s)) => {
                    Some(ExternalId::new(prop, &s))
                }
                _ => None,
            })
            .collect()
    }
}
