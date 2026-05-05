use crate::external_id::*;
use crate::external_importer::*;
use crate::meta_item::*;
use crate::properties::P_VIAF;
use crate::supported_property::SUPPORTED_PROPERTIES;
use crate::viaf::VIAF;
use anyhow::{anyhow, Result};
use futures::future::join_all;
use std::collections::HashMap;
use std::collections::HashSet;
use wikimisc::merge_diff::MergeDiff;

#[derive(Debug, Clone, Default)]
pub struct Combinator {
    pub items: HashMap<String, MetaItem>,
}

impl Combinator {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get_parser_for_property(
        property: &str,
        id: &str,
    ) -> Result<Box<dyn ExternalImporter>> {
        let property = match ExternalId::prop_numeric(property) {
            Some(property) => property,
            None => return Err(anyhow!("malformed property: '{property}'")),
        };
        let ext_id = ExternalId::new(property, id);
        Self::get_parser_for_ext_id(&ext_id).await
    }

    pub fn get_supported_properties() -> Vec<usize> {
        SUPPORTED_PROPERTIES
            .iter()
            .map(|sp| sp.property())
            .collect()
    }

    pub async fn get_parser_for_ext_id(id: &ExternalId) -> Result<Box<dyn ExternalImporter>> {
        match SUPPORTED_PROPERTIES
            .iter()
            .find(|sp| sp.property() == id.property())
        {
            Some(sp) => sp.generator(id.id()).await,
            None => Err(anyhow!("unsupported property: '{}'", id.property())),
        }
    }

    pub fn has_parser_for_ext_id(id: &ExternalId) -> bool {
        SUPPORTED_PROPERTIES
            .iter()
            .any(|sp| sp.property() == id.property())
    }

    /// Builds a ready-to-use `Combinator` seeded from a `MetaItem`.
    ///
    /// Collects every external ID in `item` that has a registered parser,
    /// discovers VIAF IDs for any VIAF-mapped properties (e.g. P244 → LC),
    /// then runs `import` so all results are available for combining.
    pub async fn import_from_item(item: &MetaItem) -> Result<Self> {
        let all_ext_ids = item.get_external_ids();
        let mut ext_ids: Vec<ExternalId> = all_ext_ids
            .iter()
            .filter(|id| Self::has_parser_for_ext_id(id))
            .cloned()
            .collect();
        for viaf_id in Self::discover_viaf_ids(&all_ext_ids).await {
            if !ext_ids.contains(&viaf_id) {
                ext_ids.push(viaf_id);
            }
        }
        let mut combinator = Self::new();
        combinator.import(ext_ids).await?;
        Ok(combinator)
    }

    async fn import_get_parsers(
        &self,
        ids: &[ExternalId],
        ids_used: &mut HashSet<ExternalId>,
    ) -> Vec<Box<dyn ExternalImporter>> {
        let mut futures = vec![];
        for ext_id in ids {
            ids_used.insert(ext_id.to_owned());
            let parser = Self::get_parser_for_ext_id(ext_id);
            futures.push(parser);
        }
        let parsers = join_all(futures).await;
        let parsers: Vec<Box<dyn ExternalImporter>> = parsers
            .into_iter()
            .filter_map(|parser| parser.ok())
            .collect();
        parsers
    }

    /// For each external ID in `ids` whose property is mapped to a VIAF source
    /// key, query VIAF in parallel and return any inferred VIAF IDs as new
    /// `ExternalId`s (property = `P_VIAF`). Failures and "no match" responses
    /// are silently skipped — VIAF is treated as a best-effort enrichment.
    /// `VIAF::infer_viaf_id_for` caches results, so calling this every loop
    /// iteration does not refetch already-resolved IDs.
    ///
    /// This is also called by the `extend` endpoint on the full set of base-item
    /// external IDs (before the parser filter) so that VIAF IDs are seeded into
    /// the Combinator cycle even when the source parser (e.g. LOC for P244) is
    /// unavailable.
    pub async fn discover_viaf_ids(ids: &[ExternalId]) -> Vec<ExternalId> {
        let mut futures = Vec::new();
        for ext_id in ids {
            if ext_id.property() == P_VIAF {
                continue;
            }
            if VIAF::prop2key(ext_id.property()).is_none() {
                continue;
            }
            let prop = ext_id.property();
            let id = ext_id.id().to_string();
            futures.push(async move { VIAF::infer_viaf_id_for(prop, &id).await });
        }
        join_all(futures)
            .await
            .into_iter()
            .flatten()
            .map(|v| ExternalId::new(P_VIAF, &v))
            .collect()
    }

    pub async fn import(&mut self, mut ids: Vec<ExternalId>) -> Result<()> {
        let mut ids_used: HashSet<ExternalId> = HashSet::new();
        while !ids.is_empty() {
            ids.sort();
            ids.dedup();
            // Eager VIAF discovery: for every queued source ID with a VIAF
            // mapping, look up VIAF in parallel and queue the result so the
            // VIAF parser runs alongside the source parser instead of waiting
            // for the source parser's `try_viaf` to surface a P214 claim.
            for vid in Self::discover_viaf_ids(&ids).await {
                if !ids_used.contains(&vid) && !ids.contains(&vid) {
                    ids.push(vid);
                }
            }
            ids.sort();
            ids.dedup();
            let parsers = self.import_get_parsers(&ids, &mut ids_used).await;
            ids.clear();
            let mut futures = vec![];
            let mut running_parsers = vec![];
            for parser in &parsers {
                let key = ExternalId::new(parser.my_property(), &parser.my_id()).to_string();
                if self.items.contains_key(&key) {
                    continue;
                }
                running_parsers.push(parser);
                futures.push(parser.run());
            }
            let items = join_all(futures).await;
            for (parser, item) in std::iter::zip(running_parsers, items) {
                let item = match item {
                    Ok(item) => item,
                    Err(_) => continue,
                };
                let key = ExternalId::new(parser.my_property(), &parser.my_id()).to_string();
                if self.items.contains_key(&key) {
                    continue;
                }
                let external_ids = item.get_external_ids();
                self.items.insert(key, item);
                for external_id in external_ids {
                    if !ids_used.contains(&external_id) && !ids.contains(&external_id) {
                        ids.push(external_id.to_owned());
                    }
                }
            }
        }
        Ok(())
    }

    pub fn combine(&mut self) -> Option<(MetaItem, MergeDiff)> {
        let mut merge_diff = MergeDiff::default();
        while self.items.len() > 1 {
            let mut key_iter = self.items.keys();
            let k1 = key_iter.next()?.to_owned();
            let k2 = key_iter.next()?.to_owned();
            // drop(key_iter);
            let other = self.items.remove(&k2)?;
            let diff = self.items.get_mut(&k1)?.merge(&other);
            merge_diff.extend(&diff);
        }
        // self.items
        //     .iter_mut()
        //     .for_each(|(_id, mi)| mi.clear_fake_statement_ids());
        let mut meta_item = self.items.iter().next().map(|(_, v)| v.to_owned())?;
        meta_item.cleanup();
        Some((meta_item, merge_diff))
    }

    pub fn combine_on_base_item(&mut self, base_item: &mut MetaItem) -> Option<MergeDiff> {
        let mut merge_diff = MergeDiff::default();
        if self.items.is_empty() {
            return None;
        }
        for (_id, item) in self.items.iter() {
            let diff = base_item.merge(item);
            merge_diff.extend(&diff);
        }
        Some(merge_diff)
    }
}

#[cfg(test)]
mod tests {
    use crate::properties::{P_GND, P_INATURALIST_TAXON, P_LOC, P_ULAN, P_VIAF};
    use crate::url_override;
    use serde_json::Value;
    use serial_test::serial;
    use wikimisc::wikibase::{
        DataValue, DataValueType, EntityTrait, ItemEntity, Snak, SnakDataType, SnakType,
        Statement, StatementRank, Value as WbValue,
    };
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    /// Properties without a VIAF source-key mapping (and P_VIAF itself) are
    /// skipped; properties with a mapping are queried in parallel; the
    /// resulting VIAF IDs are returned as `P_VIAF` external IDs.
    #[tokio::test]
    #[serial]
    async fn test_discover_viaf_ids() {
        VIAF::clear_lookup_cache().await;

        let server = MockServer::start().await;
        // Stub: any cluster-record POST returns the JPG fixture (viafID 27063124).
        let fixture = include_str!("../test_data/fixtures/viaf_lookup_jpg_500228559.json");
        Mock::given(method("POST"))
            .and(path("/api/cluster-record"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
            .mount(&server)
            .await;
        url_override::register("https://viaf.org", server.uri());

        let inputs = vec![
            ExternalId::new(P_ULAN, "500228559"),       // mapped via JPG
            ExternalId::new(P_GND, "discover-test-id"), // mapped via DNB
            ExternalId::new(P_INATURALIST_TAXON, "1"),  // unmapped → skipped
            ExternalId::new(P_VIAF, "27063124"),        // VIAF → skipped
        ];
        let discovered = Combinator::discover_viaf_ids(&inputs).await;

        // Only the two mapped inputs trigger lookups; both resolve to the
        // same fixture VIAF ID, and the dedup happens later in `import`.
        assert_eq!(discovered.len(), 2);
        for ext_id in &discovered {
            assert_eq!(ext_id.property(), P_VIAF);
            assert_eq!(ext_id.id(), "27063124");
        }

        url_override::unregister("https://viaf.org");
        VIAF::clear_lookup_cache().await;
    }

    /// A P244 (LOC) ext_id is mapped via "LC" in KEY2PROP, so
    /// `discover_viaf_ids` must return the corresponding VIAF ID.
    /// This mirrors the path taken by the `extend` endpoint when the base
    /// item contains a P244 value but no P214.
    #[tokio::test]
    #[serial]
    async fn test_discover_viaf_ids_for_p244() {
        use crate::properties::P_LOC;

        VIAF::clear_lookup_cache().await;

        let server = MockServer::start().await;
        let fixture = include_str!("../test_data/fixtures/viaf_lookup_lc_n78095637.json");
        Mock::given(method("POST"))
            .and(path("/api/cluster-record"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
            .mount(&server)
            .await;
        url_override::register("https://viaf.org", server.uri());

        let inputs = vec![
            ExternalId::new(P_LOC, "n78095637"),
            ExternalId::new(P_VIAF, "30701597"), // already a VIAF ID → skipped
        ];
        let discovered = Combinator::discover_viaf_ids(&inputs).await;

        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].property(), P_VIAF);
        assert_eq!(discovered[0].id(), "30701597");

        url_override::unregister("https://viaf.org");
        VIAF::clear_lookup_cache().await;
    }

    // ── has_parser_for_ext_id ────────────────────────────────────────────────

    #[test]
    fn test_has_parser_for_ext_id_supported() {
        assert!(Combinator::has_parser_for_ext_id(&ExternalId::new(P_VIAF, "123")));
        assert!(Combinator::has_parser_for_ext_id(&ExternalId::new(P_LOC, "n123")));
        assert!(Combinator::has_parser_for_ext_id(&ExternalId::new(P_GND, "456")));
    }

    #[test]
    fn test_has_parser_for_ext_id_unsupported() {
        // Property 99999 is not in SUPPORTED_PROPERTIES
        assert!(!Combinator::has_parser_for_ext_id(&ExternalId::new(99999, "x")));
    }

    // ── combine_on_base_item ─────────────────────────────────────────────────

    #[test]
    fn test_combine_on_base_item_empty_returns_none() {
        let mut combinator = Combinator::new();
        let mut base = MetaItem::new_from_item(ItemEntity::new_empty());
        assert!(combinator.combine_on_base_item(&mut base).is_none());
    }

    #[test]
    fn test_combine_on_base_item_merges_into_base() {
        let mut combinator = Combinator::new();
        let s1 = include_str!("../test_data/item1.json");
        let j1: Value = serde_json::from_str(s1).unwrap();
        let i1 = ItemEntity::new_from_json(&j1).unwrap();
        let claim_count = i1.claims().len();
        combinator
            .items
            .insert("imported".to_string(), MetaItem::new_from_item(i1));

        let mut base = MetaItem::new_from_item(ItemEntity::new_empty());
        let diff = combinator.combine_on_base_item(&mut base);

        assert!(diff.is_some());
        assert_eq!(
            base.item.claims().len(),
            claim_count,
            "all imported claims should be merged into the base item"
        );
    }

    // ── import_from_item ─────────────────────────────────────────────────────

    /// A MetaItem with no external-ID claims yields an empty Combinator.
    #[tokio::test]
    #[serial]
    async fn test_import_from_item_no_external_ids() {
        let base = MetaItem::new_from_item(ItemEntity::new_empty());
        let combinator = Combinator::import_from_item(&base).await.unwrap();
        assert!(combinator.items.is_empty());
    }

    /// A MetaItem that only has an item-type claim (not an ExternalId snak)
    /// should produce an empty Combinator — non-externalid snaks are invisible
    /// to `get_external_ids`.
    #[tokio::test]
    #[serial]
    async fn test_import_from_item_item_snak_ignored() {
        let mut item = ItemEntity::new_empty();
        item.set_id("Q0".to_string());
        item.add_claim(Statement::new(
            "statement",
            StatementRank::Normal,
            Snak::new(
                SnakDataType::WikibaseItem,
                "P31",
                SnakType::Value,
                Some(DataValue::new(
                    DataValueType::EntityId,
                    WbValue::Entity(wikimisc::wikibase::EntityValue::new(
                        wikimisc::wikibase::EntityType::Item,
                        "Q5",
                    )),
                )),
            ),
            vec![],
            vec![],
        ));
        let base = MetaItem::new_from_item(item);
        let combinator = Combinator::import_from_item(&base).await.unwrap();
        assert!(combinator.items.is_empty());
    }

    /// When the base MetaItem has a P244 (LOC) claim, `import_from_item`
    /// should run the LOC parser and store the result keyed by "P244:<id>".
    #[tokio::test]
    #[serial]
    async fn test_import_from_item_with_p244_imports_loc() {
        VIAF::clear_lookup_cache().await;

        // VIAF returns empty JSON so discover_viaf_ids finds no VIAF ID and
        // the VIAF parser silently fails — this keeps the test focused on LOC.
        let viaf_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/cluster-record"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&viaf_server)
            .await;
        url_override::register("https://viaf.org", viaf_server.uri());

        let loc_server = MockServer::start().await;
        let loc_fixture = include_str!("../test_data/fixtures/loc_n78095637.rdf");
        Mock::given(method("GET"))
            .and(path("/authorities/names/n78095637.rdf"))
            .respond_with(ResponseTemplate::new(200).set_body_string(loc_fixture))
            .mount(&loc_server)
            .await;
        url_override::register("https://id.loc.gov", loc_server.uri());

        // Build a MetaItem that carries a single P244 external-ID claim.
        let mut item = ItemEntity::new_empty();
        item.set_id("Q0".to_string());
        item.add_claim(Statement::new(
            "statement",
            StatementRank::Normal,
            Snak::new(
                SnakDataType::ExternalId,
                "P244",
                SnakType::Value,
                Some(DataValue::new(
                    DataValueType::StringType,
                    WbValue::StringValue("n78095637".to_string()),
                )),
            ),
            vec![],
            vec![],
        ));
        let base = MetaItem::new_from_item(item);

        let combinator = Combinator::import_from_item(&base).await.unwrap();

        assert!(
            combinator.items.contains_key("P244:n78095637"),
            "expected LOC item keyed by P244:n78095637, got: {:?}",
            combinator.items.keys().collect::<Vec<_>>()
        );

        url_override::unregister("https://viaf.org");
        url_override::unregister("https://id.loc.gov");
        VIAF::clear_lookup_cache().await;
    }

    #[test]
    fn test_combine() {
        // this test does not work correctly ... yet!
        let mut combinator = Combinator::new();

        let s1 = include_str!("../test_data/item1.json");
        let j1: Value = serde_json::from_str(s1).unwrap();
        let i1 = ItemEntity::new_from_json(&j1).unwrap();
        let mi1 = MetaItem::new_from_item(i1);

        let s2 = include_str!("../test_data/item2.json");
        let j2: Value = serde_json::from_str(s2).unwrap();
        let i2 = ItemEntity::new_from_json(&j2).unwrap();
        let mi2 = MetaItem::new_from_item(i2);

        combinator.items.insert("Q1".to_string(), mi1.to_owned());
        combinator.items.insert("Q2".to_string(), mi2.to_owned());
        combinator.items.insert("Q3".to_string(), mi2.to_owned());
        let (res_item1, _res_diff1) = combinator.combine().unwrap();

        combinator.items.insert("Q2".to_string(), mi2.to_owned());
        combinator.items.insert("Q1".to_string(), mi1.to_owned());
        combinator.items.insert("Q3".to_string(), mi1.to_owned());
        let (res_item2, _res_diff2) = combinator.combine().unwrap();

        assert_eq!(res_item1.item.claims().len(), res_item2.item.claims().len());
    }
}
