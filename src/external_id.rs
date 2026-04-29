use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt, sync::Arc};
use tokio::sync::Mutex;
use wikimisc::wikibase::*;

use crate::utility::Utility;

lazy_static! {
    static ref RE_PROPERTY_NUMERIC: Regex =
        Regex::new(r#"^\s*[Pp](\d+)\s*$"#).expect("Regexp error");
    static ref RE_FROM_STRING: Regex = Regex::new(r#"^[Pp](\d+):(.+)$"#).expect("Regexp error");
    static ref EXTERNAL_IDS_OK_CACHE: Arc<Mutex<HashMap<ExternalId, bool>>> =
        Arc::new(Mutex::new(HashMap::new()));
    /// In-process cache for `search_wikidata_single_item`, keyed by raw
    /// `srsearch` query string. Both hits (`Some(QID)`) and misses (`None`)
    /// are cached so the same Wikidata search isn't repeated within one run.
    /// Tests that exercise multiple mock responses for the same query can
    /// call [`ExternalId::clear_wikidata_search_cache`] between cases.
    static ref WIKIDATA_SEARCH_CACHE: Mutex<HashMap<String, Option<String>>> =
        Mutex::new(HashMap::new());
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
pub struct ExternalId {
    property: usize,
    id: String,
}

impl fmt::Display for ExternalId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "P{}:{}", self.property, self.id)
    }
}

impl ExternalId {
    pub fn new(property: usize, id: &str) -> Self {
        let id = Self::fix_property_value(property, id);
        Self { property, id }
    }

    fn fix_property_value(property: usize, id: &str) -> String {
        match property {
            213 => id.replace(' ', ""),  // P213 (ISNI) has no spaces
            1207 => id.replace('+', ""), // P1207 (NUKAT)
            244 => id.replace('+', "").replace("%20", ""),
            1368 => id.replace("LNC10-", ""),
            8034 => id.replace("_", "/"),
            // 268 => id.chars().filter(|c| c.is_numeric()).collect(),
            // 268 => {
            //     if id.chars().all(|c| c.is_numeric()) {
            //         format!("{id}p")
            //     } else {
            //         id.to_string()
            //     }
            // }
            _ => id.to_string(),
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        let captures = RE_FROM_STRING.captures(s)?;
        let property = Self::prop_numeric(captures.get(1)?.as_str())?;
        let id = captures.get(2)?.as_str();
        Some(Self::new(property, id))
    }

    pub fn prop_numeric(prop: &str) -> Option<usize> {
        RE_PROPERTY_NUMERIC
            .replace(prop, "${1}")
            .parse::<usize>()
            .ok()
    }

    pub fn from_external_id_claim(claim: &Statement) -> Option<Self> {
        if *claim.main_snak().datatype() != SnakDataType::ExternalId {
            return None;
        }
        let prop_numeric = Self::prop_numeric(claim.property())?;
        let datavalue = (*claim.main_snak().data_value()).to_owned()?;
        let id = match datavalue.value() {
            Value::StringValue(id) => id,
            _ => return None,
        };
        // TODO change value eg P213(ISNI) from Wikidata format to external format
        Some(Self::new(prop_numeric, id))
    }

    pub async fn search_wikidata_single_item(query: &str) -> Option<String> {
        if let Some(cached) = WIKIDATA_SEARCH_CACHE.lock().await.get(query) {
            return cached.clone();
        }
        // TODO urlencode query?
        let url = format!("https://www.wikidata.org/w/api.php?action=query&list=search&srnamespace=0&format=json&srsearch={}",&query);
        // The early `?` exits do not cache: a network/parse failure is treated
        // as transient so a later call can retry. Only well-formed responses
        // (a known totalhits value) populate the cache.
        let text = Utility::get_url(&url).await.ok()?;
        let j: serde_json::Value = serde_json::from_str(&text).ok()?;
        let totalhits = j["query"]["searchinfo"]["totalhits"].as_i64()?;
        let result = if totalhits == 1 {
            j["query"]["search"][0]["title"]
                .as_str()
                .map(|s| s.to_string())
        } else {
            None
        };
        WIKIDATA_SEARCH_CACHE
            .lock()
            .await
            .insert(query.to_string(), result.clone());
        result
    }

    /// Clears the in-process Wikidata search cache. Intended for tests that
    /// exercise multiple distinct mock responses for the same query string.
    pub async fn clear_wikidata_search_cache() {
        WIKIDATA_SEARCH_CACHE.lock().await.clear();
    }

    pub async fn get_item_for_external_id_value(&self) -> Option<String> {
        let query = format!("haswbstatement:\"P{}={}\"", self.property, self.id);
        Self::search_wikidata_single_item(&query).await
    }

    pub async fn get_item_for_string_external_id_value(&self, s: &str) -> Option<String> {
        let query = format!("{s} haswbstatement:\"P{}={}\"", self.property, &self.id);
        Self::search_wikidata_single_item(&query).await
    }

    pub fn do_not_use_external_url(url: &str) -> bool {
        crate::DO_NOT_USE_EXTERNAL_URL_REGEXPS
            .iter()
            .any(|re| re.is_match(url))
    }

    /// Checks some properties (eg GND) if the external ID is valid (eg not deprecated)
    pub async fn check_if_valid(&self) -> Result<bool> {
        if let Some(is_ok) = EXTERNAL_IDS_OK_CACHE.lock().await.get(self) {
            return Ok(*is_ok);
        }
        let mut ret = true;
        let mut was_checked = false;
        if self.property == 227 {
            // GND
            was_checked = true;
            let url = format!("https://d-nb.info/gnd/{}/about/lds.rdf", self.id);
            let text = Utility::get_url(&url).await?;
            let check = format!("rdf:about=\"https://d-nb.info/gnd/{}\">", self.id);
            ret = text.contains(&check);
        }
        if was_checked {
            // No need to store the result if no check was run
            EXTERNAL_IDS_OK_CACHE.lock().await.insert(self.clone(), ret);
        }
        Ok(ret)
    }

    pub const fn property(&self) -> usize {
        self.property
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::url_override;
    use serial_test::serial;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn test_from_string() {
        let ext_id = ExternalId::from_string("P123:ABC456DEF").unwrap();
        assert_eq!(ext_id.property, 123);
        assert_eq!(ext_id.id, "ABC456DEF");
    }

    #[test]
    fn test_isni() {
        let ext_id = ExternalId::new(213, "0000 0001 2184 9233");
        assert_eq!(ext_id.id, "0000000121849233");
    }

    #[test]
    fn test_to_string() {
        let ext_id = ExternalId::new(123, "ABC456DEF");
        assert_eq!(ext_id.to_string(), "P123:ABC456DEF".to_string());
    }

    #[test]
    fn test_prop_numeric() {
        assert_eq!(ExternalId::prop_numeric("  P123  "), Some(123));
        assert_eq!(ExternalId::prop_numeric("  FOO  "), None);
    }

    #[test]
    fn test_from_external_id_claim() {
        // Test OK
        let statement1 = Statement::new(
            "statement",
            StatementRank::Normal,
            Snak::new(
                SnakDataType::ExternalId,
                "P214",
                SnakType::Value,
                Some(DataValue::new(
                    DataValueType::StringType,
                    Value::StringValue("ABCDEF".to_string()),
                )),
            ),
            vec![],
            vec![],
        );
        assert_eq!(
            ExternalId::from_string("P214:ABCDEF"),
            ExternalId::from_external_id_claim(&statement1)
        );

        // Test wrong value type
        let statement2 = Statement::new(
            "statement",
            StatementRank::Normal,
            Snak::new(
                SnakDataType::ExternalId,
                "P214",
                SnakType::Value,
                Some(DataValue::new(
                    DataValueType::StringType,
                    Value::Entity(EntityValue::new(EntityType::Item, "Q123")),
                )),
            ),
            vec![],
            vec![],
        );
        assert_eq!(None, ExternalId::from_external_id_claim(&statement2));

        // Test wrong snak type
        let statement3 = Statement::new(
            "statement",
            StatementRank::Normal,
            Snak::new(
                SnakDataType::CommonsMedia,
                "P214",
                SnakType::Value,
                Some(DataValue::new(
                    DataValueType::StringType,
                    Value::StringValue("ABCDEF".to_string()),
                )),
            ),
            vec![],
            vec![],
        );
        assert_eq!(None, ExternalId::from_external_id_claim(&statement3));
    }

    #[tokio::test]
    #[serial]
    async fn test_get_item_for_external_id() {
        ExternalId::clear_wikidata_search_cache().await;
        let server = MockServer::start().await;

        let single_result = r#"{"batchcomplete":"","query":{"searchinfo":{"totalhits":1},"search":[{"ns":0,"title":"Q13520818","pageid":15554972,"size":42000,"wordcount":0,"snippet":"","timestamp":"2024-01-01T00:00:00Z"}]}}"#;
        let no_results =
            r#"{"batchcomplete":"","query":{"searchinfo":{"totalhits":0},"search":[]}}"#;

        // Match: exact ID search  haswbstatement:"P214=30701597"
        Mock::given(method("GET"))
            .and(path("/w/api.php"))
            .and(query_param("srsearch", "haswbstatement:\"P214=30701597\""))
            .respond_with(ResponseTemplate::new(200).set_body_string(single_result))
            .mount(&server)
            .await;

        // Match: string + ID search  Magnus haswbstatement:"P214=30701597"
        Mock::given(method("GET"))
            .and(path("/w/api.php"))
            .and(query_param(
                "srsearch",
                "Magnus haswbstatement:\"P214=30701597\"",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(single_result))
            .mount(&server)
            .await;

        // Match: nonsense string + ID search → no results
        Mock::given(method("GET"))
            .and(path("/w/api.php"))
            .and(query_param(
                "srsearch",
                "ocshs87gvdsu6gsdi7vchkuchs haswbstatement:\"P214=30701597\"",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(no_results))
            .mount(&server)
            .await;

        // Match: bogus ID search → no results
        Mock::given(method("GET"))
            .and(path("/w/api.php"))
            .and(query_param(
                "srsearch",
                "haswbstatement:\"P214=3070159777777\"",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(no_results))
            .mount(&server)
            .await;

        url_override::register("https://www.wikidata.org", server.uri());

        // Test OK — exact ID lookup
        let ext_id1 = ExternalId::new(214, "30701597");
        assert_eq!(
            ext_id1.get_item_for_external_id_value().await,
            Some("Q13520818".to_string())
        );

        // Test OK — string + ID lookup
        assert_eq!(
            ext_id1
                .get_item_for_string_external_id_value("Magnus")
                .await,
            Some("Q13520818".to_string())
        );

        // Test wrong string
        assert_eq!(
            ext_id1
                .get_item_for_string_external_id_value("ocshs87gvdsu6gsdi7vchkuchs")
                .await,
            None
        );

        // Test wrong ID
        let ext_id2 = ExternalId::new(214, "3070159777777");
        assert_eq!(ext_id2.get_item_for_external_id_value().await, None);

        url_override::unregister("https://www.wikidata.org");
        ExternalId::clear_wikidata_search_cache().await;
    }

    /// Once a query has been resolved, repeating it does not hit the network.
    /// Verified by tearing the mock server down between the two calls — the
    /// second call must still return the same value.
    #[tokio::test]
    #[serial]
    async fn test_search_wikidata_single_item_caches_results() {
        ExternalId::clear_wikidata_search_cache().await;

        // ── First call: live mock returns a single hit ─────────────────────
        {
            let server = MockServer::start().await;
            let single_result = r#"{"batchcomplete":"","query":{"searchinfo":{"totalhits":1},"search":[{"ns":0,"title":"Q42","pageid":1,"size":0,"wordcount":0,"snippet":"","timestamp":"2026-01-01T00:00:00Z"}]}}"#;
            Mock::given(method("GET"))
                .and(path("/w/api.php"))
                .and(query_param(
                    "srsearch",
                    "haswbstatement:\"P227=cache-test-id\"",
                ))
                .respond_with(ResponseTemplate::new(200).set_body_string(single_result))
                .mount(&server)
                .await;
            url_override::register("https://www.wikidata.org", server.uri());

            let ext_id = ExternalId::new(227, "cache-test-id");
            assert_eq!(
                ext_id.get_item_for_external_id_value().await,
                Some("Q42".to_string())
            );

            url_override::unregister("https://www.wikidata.org");
        }

        // ── Second call: a fresh mock with no stub. A cache miss would 404
        //    and `search_wikidata_single_item` would return `None`. The
        //    cached `Some("Q42")` must win. ─────────────────────────────────
        {
            let server = MockServer::start().await;
            url_override::register("https://www.wikidata.org", server.uri());

            let ext_id = ExternalId::new(227, "cache-test-id");
            assert_eq!(
                ext_id.get_item_for_external_id_value().await,
                Some("Q42".to_string())
            );

            url_override::unregister("https://www.wikidata.org");
        }

        ExternalId::clear_wikidata_search_cache().await;
    }
}
