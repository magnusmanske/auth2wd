use crate::external_importer::*;
use crate::meta_item::*;
use crate::properties::*;
use crate::url_override::maybe_rewrite;
use crate::utility::Utility;
use anyhow::Result;
use async_trait::async_trait;
use sophia::api::prelude::*;
use sophia::inmem::graph::FastGraph;
use sophia::xml;

#[derive(Debug)]
pub struct LOC {
    id: String,
    graph: FastGraph,
}

// unsafe impl Send for LOC {}
// unsafe impl Sync for LOC {}

#[async_trait]
impl ExternalImporter for LOC {
    fn my_property(&self) -> usize {
        P_LOC
    }
    fn my_stated_in(&self) -> &str {
        "Q13219454"
    }
    fn primary_language(&self) -> String {
        String::from("en")
    }
    fn get_key_url(&self, _key: &str) -> String {
        format!("http://id.loc.gov/authorities/names/{}", self.id)
    }

    fn my_id(&self) -> String {
        self.id.clone()
    }
    fn graph(&self) -> &FastGraph {
        &self.graph
    }
    fn transform_label(&self, s: &str) -> String {
        self.transform_label_last_first_name(s)
    }

    async fn run(&self) -> Result<MetaItem> {
        let mut ret = MetaItem::new();
        self.add_the_usual(&mut ret).await?;
        self.try_rescue_prop_text(&mut ret).await?;
        ret.cleanup();
        Ok(ret)
    }
}

impl LOC {
    pub async fn new(id: &str) -> Result<Self> {
        let rdf_url = maybe_rewrite(&format!("https://id.loc.gov/authorities/names/{id}.rdf"));
        let client = Utility::get_reqwest_client()?;
        let resp = client.get(&rdf_url).send().await?.text().await?;
        let resp = Self::sanitize_rdf(&resp);
        let mut graph: FastGraph = FastGraph::new();
        let _ = xml::parser::parse_str(&resp).add_to_graph(&mut graph)?;
        Ok(Self {
            id: id.to_string(),
            graph,
        })
    }

    /// Patch up two strict-parser violations occasionally found in real LOC
    /// payloads (both seen on n80115701):
    /// - protocol-relative URIs like `rdf:resource="//www.loc.gov/..."` get
    ///   `http:` prepended (sophia: "No scheme found in an absolute IRI").
    /// - the bare `lclang="..."` attribute (LOC's own non-standard
    ///   language tag, only on language-of-cataloging blocks we don't read)
    ///   is rewritten to `xml:lang="..."`, which sophia accepts (sophia:
    ///   "XML namespaces are required in RDF/XML").
    fn sanitize_rdf(s: &str) -> String {
        s.replace("rdf:resource=\"//", "rdf:resource=\"http://")
            .replace("rdf:about=\"//", "rdf:about=\"http://")
            .replace(" lclang=\"", " xml:lang=\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::url_override;
    use crate::viaf::VIAF;
    use serial_test::serial;
    use wikimisc::wikibase::EntityTrait;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TEST_ID: &str = "n78095637";

    async fn mock_loc() -> (MockServer, LOC) {
        let server = MockServer::start().await;
        let fixture = include_str!("../test_data/fixtures/loc_n78095637.rdf");

        Mock::given(method("GET"))
            .and(path("/authorities/names/n78095637.rdf"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
            .mount(&server)
            .await;

        url_override::register("https://id.loc.gov", server.uri());

        let loc = LOC::new(TEST_ID).await.unwrap();
        (server, loc)
    }

    #[tokio::test]
    #[serial]
    async fn test_new() {
        let (_server, _loc) = mock_loc().await;
        url_override::unregister("https://id.loc.gov");
    }

    #[test]
    fn test_sanitize_rdf_fixes_loc_quirks() {
        let input = r#"<rdf:type rdf:resource="//www.loc.gov/x"/><x rdf:about="//y"/><l lclang="en">English</l>"#;
        let out = LOC::sanitize_rdf(input);
        assert!(out.contains(r#"rdf:resource="http://www.loc.gov/x""#));
        assert!(out.contains(r#"rdf:about="http://y""#));
        assert!(out.contains(r#"xml:lang="en""#));
        assert!(!out.contains("lclang"));
    }

    /// Real LOC payloads occasionally contain RDF/XML the strict parser
    /// rejects: protocol-relative `rdf:resource="//..."` and bare `lclang="..."`
    /// attributes. `LOC::new` must sanitize both so parsing succeeds —
    /// regression test for Q139674000 (`n80115701`), whose RDF has both.
    #[tokio::test]
    #[serial]
    async fn test_new_handles_malformed_rdf() {
        let server = MockServer::start().await;
        let fixture = include_str!("../test_data/fixtures/loc_n80115701.rdf");
        Mock::given(method("GET"))
            .and(path("/authorities/names/n80115701.rdf"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
            .mount(&server)
            .await;
        url_override::register("https://id.loc.gov", server.uri());

        let _loc = LOC::new("n80115701").await.unwrap();

        url_override::unregister("https://id.loc.gov");
    }

    /// `run()` must always produce a P244 claim (from `add_own_id`) and an
    /// English description derived from the MADS authoritative label in the
    /// fixture ("Darwin, Charles, 1809-1882").
    #[tokio::test]
    #[serial]
    async fn test_run() {
        VIAF::clear_lookup_cache().await;

        // VIAF stub: empty JSON → try_viaf finds no VIAF ID (swallowed silently).
        let viaf_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/cluster-record"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&viaf_server)
            .await;
        url_override::register("https://viaf.org", viaf_server.uri());

        let (_loc_server, loc) = mock_loc().await;
        let meta = loc.run().await.unwrap();

        // add_own_id always adds the parser's external ID as the first claim.
        let has_p244 = meta.item.claims().iter().any(|c| {
            c.property() == "P244"
                && c.main_snak()
                    .data_value()
                    .as_ref()
                    .and_then(|dv| match dv.value() {
                        wikimisc::wikibase::Value::StringValue(s) => Some(s.as_str() == TEST_ID),
                        _ => None,
                    })
                    .unwrap_or(false)
        });
        assert!(has_p244, "expected P244={TEST_ID} claim in LOC run output");

        // add_description picks up madsrdf:authoritativeLabel (in DESCRIPTION_IRIS)
        // for the main authority subject → "Darwin, Charles, 1809-1882".
        let descriptions = meta.item.descriptions();
        let en_desc = descriptions
            .iter()
            .find(|ls| ls.language() == "en")
            .map(|ls| ls.value());
        assert!(
            en_desc.is_some(),
            "expected an English description in LOC run output"
        );
        assert!(
            en_desc.unwrap().contains("Darwin"),
            "English description should contain 'Darwin', got: {:?}",
            en_desc
        );

        url_override::unregister("https://viaf.org");
        url_override::unregister("https://id.loc.gov");
        VIAF::clear_lookup_cache().await;
    }
}
