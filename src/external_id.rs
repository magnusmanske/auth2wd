use std::fmt;

use regex::Regex;
use serde::{Deserialize, Serialize};
use wikibase::*;

lazy_static! {
    static ref RE_PROPERTY_NUMERIC: Regex =
        Regex::new(r#"^\s*[Pp](\d+)\s*$"#).expect("Regexp error");
    static ref RE_FROM_STRING: Regex = Regex::new(r#"^[Pp](\d+):(.+)$"#).expect("Regexp error");
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
            213 => id.replace(' ', ""), // P213 (ISNI) has no spaces
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

    pub async fn search_wikidata_single_item(&self, query: &str) -> Option<String> {
        // TODO urlencode query?
        let url = format!("https://www.wikidata.org/w/api.php?action=query&list=search&srnamespace=0&format=json&srsearch={}",&query);
        let text = reqwest::get(url).await.ok()?.text().await.ok()?;
        // let text = ureq::get(&url).call().ok()?.into_string().ok()?;
        let j: serde_json::Value = serde_json::from_str(&text).ok()?;
        if j["query"]["searchinfo"]["totalhits"].as_i64()? == 1 {
            return Some(j["query"]["search"][0]["title"].as_str()?.to_string());
        }
        None
    }

    pub async fn get_item_for_external_id_value(&self) -> Option<String> {
        let query = format!("haswbstatement:\"P{}={}\"", self.property, self.id);
        self.search_wikidata_single_item(&query).await
    }

    pub async fn get_item_for_string_external_id_value(&self, s: &str) -> Option<String> {
        let query = format!("{s} haswbstatement:\"P{}={}\"", self.property, &self.id);
        self.search_wikidata_single_item(&query).await
    }

    pub fn property(&self) -> usize {
        self.property
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let statement = Statement::new(
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
            ExternalId::from_external_id_claim(&statement)
        );

        // Test wrong value type
        let statement = Statement::new(
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
        assert_eq!(None, ExternalId::from_external_id_claim(&statement));

        // Test wrong snak type
        let statement = Statement::new(
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
        assert_eq!(None, ExternalId::from_external_id_claim(&statement));
    }

    #[tokio::test]
    async fn test_get_item_for_external_id() {
        // Test OK
        let ext_id = ExternalId::new(214, "30701597");
        assert_eq!(
            ext_id.get_item_for_external_id_value().await,
            Some("Q13520818".to_string())
        );

        // Test OK
        assert_eq!(
            ext_id.get_item_for_string_external_id_value("Magnus").await,
            Some("Q13520818".to_string())
        );

        // Test wrong string
        assert_eq!(
            ext_id
                .get_item_for_string_external_id_value("ocshs87gvdsu6gsdi7vchkuchs")
                .await,
            None
        );

        // Test wrong ID
        let ext_id = ExternalId::new(214, "3070159777777");
        assert_eq!(ext_id.get_item_for_external_id_value().await, None);

        // TODOO multiple items
    }
}
