use serde::{Deserialize, Serialize};
use wikibase_rest_api::Item;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Copy)]
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
