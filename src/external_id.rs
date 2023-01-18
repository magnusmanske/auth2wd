use regex::Regex;
use wikibase::*;

lazy_static! {
    static ref RE_PROPERTY_NUMERIC : Regex = Regex::new(r#"^\s*[Pp](\d+)\s*$"#).expect("Regexp error");
}


#[derive(Debug, Clone, PartialEq)]
pub struct ExternalId {
    pub property: usize,
    pub id: String
}

impl ExternalId {
    pub fn new(property: usize, id: &str ) -> Self {
        Self {
            property,
            id: id.to_string()
        }
    }

    pub fn prop_numeric(prop: &str) -> Option<usize> {
        RE_PROPERTY_NUMERIC.replace(prop,"${1}").parse::<usize>().ok()
    }

    pub fn from_external_id_claim(claim: &Statement) -> Option<Self> {
        if *claim.main_snak().datatype()!=SnakDataType::ExternalId {
            return None
        }
        let prop_numeric = Self::prop_numeric(&claim.property())?;
        let datavalue = (*claim.main_snak().data_value()).to_owned()?;
        let id = match datavalue.value() {
            Value::StringValue(id) => id,
            _ => return None
        };
        // TODO change value eg P213 from Wikidata format to external format
        Some(Self::new(prop_numeric,id))
    }

    pub fn search_wikidata_single_item(&self, query: &str) -> Option<String> {
        // TODO urlencode query?
        let url = format!("https://www.wikidata.org/w/api.php?action=query&list=search&srnamespace=0&format=json&srsearch={}",&query);
        //let text = reqwest::get(url).await.ok()?.text().await.ok()?;
        let text = ureq::get(&url).call().ok()?.into_string().ok()?;
        let j: serde_json::Value = serde_json::from_str(&text).ok()?;
        if j["query"]["searchinfo"]["totalhits"].as_i64()? == 1 {
            return Some(j["query"]["search"][0]["title"].as_str()?.to_string());
        }
        None
    }

    pub fn get_item_for_external_id_value(&self) -> Option<String> {
        let query = format!("haswbstatement:\"P{}={}\"",self.property,self.id);
        self.search_wikidata_single_item(&query)
    }

    pub fn get_item_for_string_external_id_value(&self, s: &str) -> Option<String> {
        let query = format!("{s} haswbstatement:\"P{}={}\"",self.property,&self.id);
        self.search_wikidata_single_item(&query)
    }
}
