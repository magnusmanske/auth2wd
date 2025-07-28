use std::{cmp::Ordering, collections::HashSet};

use crate::{external_id::ExternalId, merge_diff::MergeDiff};
use serde::Serialize;
use wikibase_rest_api::{
    prelude::{PropertyValue, StatementValue, StatementValueContent},
    DataType, Item, Reference, Statement,
};
use wikimisc::wikibase::LocaleString;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ItemMerger {
    item: Item,
}

impl ItemMerger {
    pub fn new(item: Item) -> Self {
        Self { item }
    }

    pub fn item(&self) -> &Item {
        &self.item
    }

    pub fn merge(&mut self, _new_item: &Item) -> MergeDiff {
        todo!()
    }

    fn merge_qualifiers(
        _new_qualifiers: &Vec<PropertyValue>,
        _existing_qualifiers: &Vec<PropertyValue>,
    ) -> Vec<PropertyValue> {
        todo!()
    }

    pub fn get_external_ids_from_reference(reference: &Reference) -> Vec<ExternalId> {
        reference
            .parts()
            .iter()
            .filter(|pv| *pv.property().datatype() == Some(DataType::ExternalId))
            .map(|pv| (ExternalId::prop_numeric(pv.property().id()), pv.value()))
            .filter_map(|(prop, dv)| Some((prop?, dv.to_owned())))
            .filter_map(|(prop, value)| match value {
                StatementValue::Value(StatementValueContent::String(s)) => {
                    Some(ExternalId::new(prop, &s))
                }
                _ => None,
            })
            .collect()
    }

    pub fn get_reference_urls_from_reference(reference: &Reference) -> Vec<String> {
        reference
            .parts()
            .iter()
            .filter(|pv| *pv.property().datatype() == Some(DataType::Url))
            .map(|pv| pv.value().to_owned())
            .filter_map(|sv| match sv {
                StatementValue::Value(StatementValueContent::String(s)) => Some(s.to_owned()),
                _ => None,
            })
            .collect()
    }

    // Checks if a reference already exists in a list of references.
    // Uses direct equal, or the presence of any external ID from the new reference.
    // Returns `true` if the reference exists, `false` otherwise.
    // fn reference_exists(existing_references: &[Reference], new_reference: &Reference) -> bool {
    //     todo!()
    // }

    // pub fn is_snak_identical(snak1: &PropertyValue, snak2: &PropertyValue) -> bool {
    //     todo!()
    // }

    // fn is_data_value_identical(dv1: &Option<StatementValue>, dv2: &Option<StatementValue>) -> bool {
    //     todo!()
    // }

    // pub fn is_time_value_identical(t1: &StatementValueContent, t2: &StatementValueContent) -> bool {
    //     todo!()
    // }

    // pub fn are_qualifiers_identical(q1: &[PropertyValue], q2: &[PropertyValue]) -> bool {
    //     if q1.len() != q2.len() {
    //         return false;
    //     }
    //     q1.iter().any(|q| !q2.contains(q))
    // }

    // pub fn check_new_claim_for_dates(&self, new_claim: &mut Statement) {
    //     todo!()
    // }

    // pub fn compare_locale_string(a: &LocaleString, b: &LocaleString) -> Ordering {
    //     todo!()
    // }

    // fn compare_snak(snak1: &PropertyValue, snak2: &PropertyValue) -> Ordering {
    //     todo!()
    // }

    // fn merge_locale_strings(
    //     mine: &mut Vec<LocaleString>,
    //     other: &[LocaleString],
    //     diff: &mut Vec<LocaleString>,
    // ) -> Vec<LocaleString> {
    //     todo!()
    // }

    // pub fn set_properties_ignore_qualifier_match(
    //     &mut self,
    //     properties_ignore_qualifier_match: Vec<String>,
    // ) {
    //     todo!()
    // }
}

#[cfg(test)]
mod tests {
    // use super::*;

    // #[test]
    // fn test_add_claim_p225_both_with_qualifiers() {
    //     let mut base_item = ItemEntity::new_empty();
    //     let mut statement = Statement::new_normal(
    //         Snak::new_string("P225", "foo bar"),
    //         vec![Snak::new_item("P31", "Q5")],
    //         vec![],
    //     );
    //     statement.set_id("Blah");
    //     base_item.add_claim(statement);

    //     let mut new_item = ItemEntity::new_empty();
    //     new_item.add_claim(Statement::new_normal(
    //         Snak::new_string("P225", "foo bar"),
    //         vec![Snak::new_item("P31", "Q1")],
    //         vec![],
    //     ));

    //     let mut im = ItemMerger::new(base_item);
    //     im.set_properties_ignore_qualifier_match(vec!["P225".to_string()]);
    //     let diff = im.merge(&new_item);
    //     assert!(!diff.altered_statements.is_empty());
    //     assert_eq!(diff.altered_statements["Blah"].qualifiers().len(), 2);
    // }

    // #[test]
    // fn test_reference_exists_by_external_ids() {
    //     let reference1 = Reference::new(vec![Snak::new_external_id("P214", "12345")]);
    //     let reference2 = Reference::new(vec![Snak::new_external_id("P214", "12346")]);
    //     let references = vec![reference1.to_owned()];
    //     assert!(ItemMerger::reference_exists(&references, &reference1));
    //     assert!(!ItemMerger::reference_exists(&references, &reference2));
    // }

    // #[test]
    // fn test_reference_exists_by_reference_urls() {
    //     let reference1 = Reference::new(vec![Snak::new_url("P854", "http://foo.bar")]);
    //     let reference2 = Reference::new(vec![Snak::new_url("P854", "http://foo.bars")]);
    //     let references = vec![reference1.to_owned()];
    //     assert!(ItemMerger::reference_exists(&references, &reference1));
    //     assert!(!ItemMerger::reference_exists(&references, &reference2));
    // }
}
