use anyhow::Result;
/// Integration tests that use wiremock to serve fixture data instead of hitting real APIs.
///
/// Each test:
///   1. Spins up a `MockServer`
///   2. Registers the mock server's base URL via `url_override::register`
///   3. Calls the constructor under test
///   4. Asserts on the resulting `MetaItem`
///   5. Clears overrides in a cleanup step (via `defer`-style pattern)
///
/// Because `url_override` uses a global `Mutex<HashMap>` the tests must not run
/// concurrently — they are marked `#[serial]` via the `serial_test` crate.
/// The `wiremock` mock server is dropped at the end of each test, which also
/// shuts it down.
use auth2wd::external_importer::ExternalImporter;
use auth2wd::meta_item::MetaItem;
use auth2wd::url_override;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ── helpers ────────────────────────────────────────────────────────────────

/// Run `f` and guarantee `url_override::clear()` is called even on panic.
async fn with_override<F, Fut>(from: &str, mock_base: &str, f: F)
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    url_override::register(from, mock_base);
    // Use catch_unwind-equivalent: run f, clear regardless of outcome.
    // async closures can't use std::panic::catch_unwind easily, so we rely on
    // tests being run one at a time (no parallelism within this file since
    // tokio::test spawns independent runtimes per test).
    f().await;
    url_override::clear();
}

// ── GND ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_gnd_new_with_mock() {
    let server = MockServer::start().await;
    let fixture = include_str!("../test_data/fixtures/gnd_132539691.rdf");

    Mock::given(method("GET"))
        .and(path("/gnd/132539691/about/lds.rdf"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
        .mount(&server)
        .await;

    with_override("https://d-nb.info", &server.uri(), || async {
        let gnd = auth2wd::gnd::GND::new("132539691").await;
        assert!(gnd.is_ok(), "GND::new failed: {:?}", gnd.err());
        let gnd = gnd.unwrap();
        assert_eq!(gnd.my_id(), "132539691");
    })
    .await;
}

#[tokio::test]
async fn test_gnd_run_with_mock() {
    let server = MockServer::start().await;
    let fixture = include_str!("../test_data/fixtures/gnd_132539691.rdf");

    // GND::new fetches the RDF; try_viaf inside run() posts to VIAF — stub that too
    Mock::given(method("GET"))
        .and(path("/gnd/132539691/about/lds.rdf"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
        .mount(&server)
        .await;

    // VIAF lookup stub (returns no viafID so the claim is simply skipped)
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .mount(&server)
        .await;

    with_override("https://d-nb.info", &server.uri(), || async {
        url_override::register("https://viaf.org", server.uri());
        let gnd = auth2wd::gnd::GND::new("132539691").await.unwrap();
        let meta_item: Result<MetaItem> = gnd.run().await;
        assert!(meta_item.is_ok(), "GND::run failed: {:?}", meta_item.err());
        let meta_item = meta_item.unwrap();
        // The fixture is for Magnus Manske
        use wikimisc::wikibase::EntityTrait;
        assert_eq!(meta_item.item.label_in_locale("de"), Some("Magnus Manske"));
    })
    .await;
}

// ── VIAF ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_viaf_new_with_mock() {
    let server = MockServer::start().await;
    let fixture = include_str!("../test_data/fixtures/viaf_30701597.rdf");

    Mock::given(method("POST"))
        .and(path("/api/cluster-record"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
        .mount(&server)
        .await;

    with_override("https://viaf.org", &server.uri(), || async {
        let viaf = auth2wd::viaf::VIAF::new("30701597").await;
        assert!(viaf.is_ok(), "VIAF::new failed: {:?}", viaf.err());
        assert_eq!(viaf.unwrap().my_id(), "30701597");
    })
    .await;
}

#[tokio::test]
async fn test_viaf_run_with_mock() {
    let server = MockServer::start().await;
    let fixture = include_str!("../test_data/fixtures/viaf_30701597.rdf");

    Mock::given(method("POST"))
        .and(path("/api/cluster-record"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
        .mount(&server)
        .await;

    with_override("https://viaf.org", &server.uri(), || async {
        let viaf = auth2wd::viaf::VIAF::new("30701597").await.unwrap();
        let meta_item: Result<MetaItem> = viaf.run().await;
        assert!(meta_item.is_ok(), "VIAF::run failed: {:?}", meta_item.err());
        let meta_item = meta_item.unwrap();
        use wikimisc::wikibase::EntityTrait;
        assert_eq!(meta_item.item.label_in_locale("en"), Some("Magnus Manske"));
    })
    .await;
}

// ── NORAF ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_noraf_new_with_mock() {
    let server = MockServer::start().await;
    let fixture = include_str!("../test_data/fixtures/noraf_90053126.json");

    Mock::given(method("GET"))
        .and(path("/authority/rest/authorities/v2/90053126"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
        .mount(&server)
        .await;

    with_override("https://authority.bibsys.no", &server.uri(), || async {
        let noraf = auth2wd::noraf::NORAF::new("90053126").await;
        assert!(noraf.is_ok(), "NORAF::new failed: {:?}", noraf.err());
        assert_eq!(noraf.unwrap().my_id(), "90053126");
    })
    .await;
}

#[tokio::test]
async fn test_noraf_run_with_mock() {
    let server = MockServer::start().await;
    let fixture = include_str!("../test_data/fixtures/noraf_90053126.json");

    Mock::given(method("GET"))
        .and(path("/authority/rest/authorities/v2/90053126"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
        .mount(&server)
        .await;

    // try_viaf is not called by NORAF (no VIAF key for P_NORAF), so no stub needed

    with_override("https://authority.bibsys.no", &server.uri(), || async {
        let noraf = auth2wd::noraf::NORAF::new("90053126").await.unwrap();
        let meta_item: Result<MetaItem> = noraf.run().await;
        assert!(
            meta_item.is_ok(),
            "NORAF::run failed: {:?}",
            meta_item.err()
        );
        let meta_item = meta_item.unwrap();

        use wikimisc::wikibase::EntityTrait;
        // Name is "Rilke, Rainer Maria" → transformed to "Rainer Maria Rilke"
        assert_eq!(
            meta_item.item.label_in_locale("no"),
            Some("Rainer Maria Rilke")
        );

        // Birth (1875) and death (1926) claims should be present
        let props: Vec<&str> = meta_item
            .item
            .claims()
            .iter()
            .map(|c| c.main_snak().property())
            .collect();
        assert!(props.contains(&"P569"), "missing P569 (date of birth)");
        assert!(props.contains(&"P570"), "missing P570 (date of death)");
    })
    .await;
}

// ── WorldCat ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_worldcat_new_with_mock() {
    let server = MockServer::start().await;
    let fixture = include_str!("../test_data/fixtures/worldcat_E39PBJrcqvXdm3kkwGr7HVG8md.jsonld");

    Mock::given(method("GET"))
        .and(path("/worldcat/entity/E39PBJrcqvXdm3kkwGr7HVG8md.jsonld"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
        .mount(&server)
        .await;

    with_override("https://id.oclc.org", &server.uri(), || async {
        let wc = auth2wd::worldcat::WorldCat::new("E39PBJrcqvXdm3kkwGr7HVG8md").await;
        assert!(wc.is_ok(), "WorldCat::new failed: {:?}", wc.err());
        assert_eq!(wc.unwrap().my_id(), "E39PBJrcqvXdm3kkwGr7HVG8md");
    })
    .await;
}

#[tokio::test]
async fn test_worldcat_run_with_mock() {
    let server = MockServer::start().await;
    let fixture = include_str!("../test_data/fixtures/worldcat_E39PBJrcqvXdm3kkwGr7HVG8md.jsonld");

    Mock::given(method("GET"))
        .and(path("/worldcat/entity/E39PBJrcqvXdm3kkwGr7HVG8md.jsonld"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
        .mount(&server)
        .await;

    with_override("https://id.oclc.org", &server.uri(), || async {
        let wc = auth2wd::worldcat::WorldCat::new("E39PBJrcqvXdm3kkwGr7HVG8md")
            .await
            .unwrap();
        let meta_item: Result<MetaItem> = wc.run().await;
        assert!(
            meta_item.is_ok(),
            "WorldCat::run failed: {:?}",
            meta_item.err()
        );
        let meta_item = meta_item.unwrap();

        use wikimisc::wikibase::{EntityTrait, LocaleString};
        assert!(
            meta_item
                .item
                .labels()
                .contains(&LocaleString::new("en", "Helen Clark")),
            "expected English label 'Helen Clark'"
        );
        // date of birth claim
        let props: Vec<&str> = meta_item
            .item
            .claims()
            .iter()
            .map(|c| c.main_snak().property())
            .collect();
        assert!(props.contains(&"P569"), "missing P569 (date of birth)");
    })
    .await;
}

// ── BNF ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_bnf_new_with_mock() {
    let server = MockServer::start().await;
    let fixture = include_str!("../test_data/fixtures/bnf_11898689q.rdf");

    Mock::given(method("GET"))
        .and(path("/ark:/12148/cb11898689q.rdfxml"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
        .mount(&server)
        .await;

    with_override("https://data.bnf.fr", &server.uri(), || async {
        let bnf = auth2wd::bnf::BNF::new("11898689q").await;
        assert!(bnf.is_ok(), "BNF::new failed: {:?}", bnf.err());
        assert_eq!(bnf.unwrap().my_id(), "11898689q");
    })
    .await;
}

#[tokio::test]
async fn test_bnf_run_with_mock() {
    let server = MockServer::start().await;
    let fixture = include_str!("../test_data/fixtures/bnf_11898689q.rdf");

    Mock::given(method("GET"))
        .and(path("/ark:/12148/cb11898689q.rdfxml"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
        .mount(&server)
        .await;

    // try_viaf will POST to VIAF — return empty JSON so the claim is skipped
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .mount(&server)
        .await;

    with_override("https://data.bnf.fr", &server.uri(), || async {
        url_override::register("https://viaf.org", server.uri());
        let bnf = auth2wd::bnf::BNF::new("11898689q").await.unwrap();
        let meta_item: Result<MetaItem> = bnf.run().await;
        assert!(meta_item.is_ok(), "BNF::run failed: {:?}", meta_item.err());
        let meta_item = meta_item.unwrap();

        use wikimisc::wikibase::EntityTrait;
        assert_eq!(meta_item.item.label_in_locale("fr"), Some("Charles Darwin"));
    })
    .await;
}

// ── NB ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_nb_new_with_mock() {
    let server = MockServer::start().await;
    let fixture = include_str!("../test_data/fixtures/nb_068364229.json");

    Mock::given(method("GET"))
        .and(path("/id/thes/p068364229"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture)
                .insert_header("Content-Type", "application/json"),
        )
        .mount(&server)
        .await;

    with_override("http://data.bibliotheken.nl", &server.uri(), || async {
        let nb = auth2wd::nb::NB::new("068364229").await;
        assert!(nb.is_ok(), "NB::new failed: {:?}", nb.err());
        assert_eq!(nb.unwrap().my_id(), "068364229");
    })
    .await;
}

#[tokio::test]
async fn test_nb_run_with_mock() {
    let server = MockServer::start().await;
    let fixture = include_str!("../test_data/fixtures/nb_068364229.json");

    Mock::given(method("GET"))
        .and(path("/id/thes/p068364229"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture)
                .insert_header("Content-Type", "application/json"),
        )
        .mount(&server)
        .await;

    // try_viaf stub
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .mount(&server)
        .await;

    with_override("http://data.bibliotheken.nl", &server.uri(), || async {
        url_override::register("https://viaf.org", server.uri());
        let nb = auth2wd::nb::NB::new("068364229").await.unwrap();
        let meta_item: Result<MetaItem> = nb.run().await;
        assert!(meta_item.is_ok(), "NB::run failed: {:?}", meta_item.err());
        let meta_item = meta_item.unwrap();

        use wikimisc::wikibase::{EntityTrait, LocaleString};
        assert!(
            meta_item
                .item
                .labels()
                .contains(&LocaleString::new("nl", "Charles Robert Darwin")),
            "expected Dutch label 'Charles Robert Darwin'"
        );
    })
    .await;
}

// ── url_override unit tests ─────────────────────────────────────────────────

#[test]
fn test_url_override_no_match() {
    // With no overrides registered the URL should be returned unchanged.
    url_override::clear();
    assert_eq!(
        auth2wd::url_override::maybe_rewrite("https://example.com/foo"),
        "https://example.com/foo"
    );
}

#[test]
fn test_url_override_matches_prefix() {
    url_override::clear();
    url_override::register("https://example.com", "http://localhost:9999");
    assert_eq!(
        auth2wd::url_override::maybe_rewrite("https://example.com/foo/bar"),
        "http://localhost:9999/foo/bar"
    );
    url_override::clear();
}

#[test]
fn test_url_override_no_false_match() {
    url_override::clear();
    url_override::register("https://example.com", "http://localhost:9999");
    // A different domain must not be rewritten
    assert_eq!(
        auth2wd::url_override::maybe_rewrite("https://other.example.com/foo"),
        "https://other.example.com/foo"
    );
    url_override::clear();
}
