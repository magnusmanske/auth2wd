#[macro_use]
extern crate lazy_static;
extern crate nom_bibtex;

pub mod bne;
pub mod bnf;
pub mod combinator;
pub mod external_id;
pub mod external_importer;
pub mod gnd;
pub mod id_ref;
pub mod loc;
pub mod merge_diff;
pub mod meta_item;
pub mod nb;
pub mod selibr;
pub mod viaf;

use axum::{extract::Path, response::Html, routing::get, Json, Router};
use combinator::*;
use external_id::*;
use external_importer::*;
use serde_json::json;
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tower_http::{compression::CompressionLayer, trace::TraceLayer};
use wikibase::mediawiki::api::Api;

async fn root() -> Html<String> {
    let sources: Vec<String> = SUPPORTED_PROPERTIES.iter().map(|sp| sp.as_li()).collect();
    let html = r##"<h1>Auhority Control data to Wikidata item</h1>
    This API can load AC (Authority Control) data from other sources and convert them into a Wikidata item.

    <h2>Available sources</h3>
    <ul>"##.to_string() + &sources.join("\n") + r##"</ul>
    <h2>Functions</h2>
    <ul>
    <li><a href="/item/P227/118523813">item</a>, the JSON of a new item containing the parsed data from the respective source</li>
    <li><a href="/meta_item/P1006/068364229">meta_item</a>, item plus some properties that could not be resolved automatically</li>
    <li><a href="/graph/P227/118523813">graph</a>, the internal graph representation before parsing</li>
    <li><a href="/extend/Q1035">extend</a>, extract AC data from external IDs in an item, and get the payload for <tt>wbeditentity</tt></li>
    </ul>
    <hr/>
    <a href='https://github.com/magnusmanske/auth2wd'>git</a>
    "##;
    Html(html)
}

async fn item(Path((property, id)): Path<(String, String)>) -> Json<serde_json::Value> {
    let parser: Box<dyn ExternalImporter + Send + Sync> =
        match Combinator::get_parser_for_property(&property, &id).await {
            Ok(parser) => parser,
            Err(e) => return Json(json!({"status":e.to_string()})),
        };
    let mi = match parser.run().await {
        Ok(mi) => mi,
        Err(e) => return Json(json!({"status":e.to_string()})),
    };
    let mut j = json!(mi)["item"].to_owned();
    j["status"] = json!("OK");
    Json(j)
}

async fn meta_item(Path((property, id)): Path<(String, String)>) -> Json<serde_json::Value> {
    let parser: Box<dyn ExternalImporter + Send + Sync> =
        match Combinator::get_parser_for_property(&property, &id).await {
            Ok(parser) => parser,
            Err(e) => return Json(json!({"status":e.to_string()})),
        };
    let mi = match parser.run().await {
        Ok(mi) => mi,
        Err(e) => return Json(json!({"status":e.to_string()})),
    };
    let mut j = json!(mi);
    j["status"] = json!("OK");
    Json(j)
}

async fn graph(Path((property, id)): Path<(String, String)>) -> String {
    let mut parser: Box<dyn ExternalImporter> =
        match Combinator::get_parser_for_property(&property, &id).await {
            Ok(parser) => parser,
            Err(e) => return e.to_string(),
        };
    parser.get_graph_text()
}

async fn extend(Path(item): Path<String>) -> Json<serde_json::Value> {
    let mut base_item = match meta_item::MetaItem::from_entity(&item).await {
        Ok(base_item) => base_item,
        Err(e) => return Json(json!({"status":e.to_string()})),
    };
    let ext_ids: Vec<ExternalId> = base_item
        .get_external_ids()
        .iter()
        .filter(|ext_id| Combinator::has_parser_for_ext_id(ext_id))
        .cloned()
        .collect();
    let mut combinator = Combinator::new();
    if let Err(e) = combinator.import(ext_ids).await {
        return Json(json!({"status":e.to_string()}));
    }
    let other = match combinator.combine() {
        Some(other) => other,
        None => return Json(json!({"status":"No items to combine"})),
    };
    let diff = base_item.merge(&other);
    Json(json!(diff))
}

async fn supported_properties() -> Json<serde_json::Value> {
    let ret: Vec<String> = Combinator::get_supported_properties()
        .iter()
        .map(|prop| format!("P{prop}"))
        .collect();
    Json(json!(ret))
}

async fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let cors = CorsLayer::new().allow_origin(Any);

    let app = Router::new()
        .route("/", get(root))
        .route("/supported_properties", get(supported_properties))
        .route("/item/:prop/:id", get(item))
        .route("/meta_item/:prop/:id", get(meta_item))
        .route("/graph/:prop/:id", get(graph))
        .route("/extend/:item", get(extend))
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(cors);

    let port: u16 = match env::var("AC2WD_PORT") {
        Ok(port) => port.as_str().parse::<u16>().unwrap_or(8000),
        Err(_) => 8000,
    };

    let address = [0, 0, 0, 0]; // TODOO env::var("AC2WD_ADDRESS")

    let addr = SocketAddr::from((address, port));
    tracing::debug!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}

fn get_extid_from_argv(argv: &[String]) -> Result<ExternalId, Box<dyn std::error::Error>> {
    let property = argv.get(2).expect("USAGE: combinator PROP ID");
    let property = ExternalId::prop_numeric(property).expect("malformed property: '{property}'");
    let id = argv.get(3).expect("USAGE: combinator PROP ID");
    Ok(ExternalId::new(property, id))
}

async fn get_extend(item: &str) -> Result<merge_diff::MergeDiff, Box<dyn std::error::Error>> {
    let mut base_item = meta_item::MetaItem::from_entity(item).await?;
    let ext_ids: Vec<ExternalId> = base_item
        .get_external_ids()
        .iter()
        .filter(|ext_id| Combinator::has_parser_for_ext_id(ext_id))
        .cloned()
        .collect();
    let mut combinator = Combinator::new();
    combinator.import(ext_ids).await?;
    let other = match combinator.combine() {
        Some(other) => other,
        None => return Err("No items to combine".into()),
    };
    Ok(base_item.merge(&other))
}

async fn apply_diff(
    item: &str,
    diff: &merge_diff::MergeDiff,
    api: &mut Api,
) -> Result<(), Box<dyn std::error::Error>> {
    let json_string = json!(diff).to_string();
    // println!("{item}: {json_string}");
    if json_string == "{}" {
        return Ok(());
    }
    let token = api.get_edit_token().await?;
    let params: HashMap<String, String> = vec![
        ("action", "wbeditentity"),
        ("id", item),
        ("data", &json_string),
        ("summary", "AC2WD"),
        ("token", &token),
        ("bot", "1"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let j = api
        .post_query_api_json(&params)
        .await
        .map_err(|e| e.to_string())?;
    match j["error"].as_object() {
        Some(o) => {
            let s = format!("{o:?}");
            Err(s.into())
        }
        None => Ok(()),
    }
}

async fn get_wikidata_api(path: &str) -> Result<Api, Box<dyn std::error::Error>> {
    let file = File::open(path).map_err(|e| format!("{:?}", e))?;
    let reader = BufReader::new(file);
    let j: serde_json::Value = serde_json::from_reader(reader).map_err(|e| format!("{:?}", e))?;
    let oauth2_token = j["oauth2_token"]
        .as_str()
        .expect("No oauth2_token in {path}");
    let mut api = Api::new("https://www.wikidata.org/w/api.php").await?;
    api.set_oauth2(oauth2_token);
    Ok(api)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let argv: Vec<String> = env::args().collect();
    match argv.get(1).map(|s| s.as_str()) {
        Some("combinator") => {
            // Combinator
            let mut base_item = meta_item::MetaItem::from_entity("Q1035").await?;
            //println!("{:?}",&base_item);
            let ext_id = get_extid_from_argv(&argv)?;
            let mut combinator = Combinator::new();
            combinator.import(vec![ext_id]).await?;
            println!(
                "{} items: {:?}",
                combinator.items.len(),
                combinator.items.keys()
            );
            let other = combinator.combine().expect("No items to combine");
            println!(
                "{} items: {:?}",
                combinator.items.len(),
                combinator.items.keys()
            );
            //println!("{:?}",&other);
            let diff = base_item.merge(&other);
            //println!("{:?}",&diff);
            println!(
                "Altered: {}, added: {}",
                diff.altered_statements.len(),
                diff.added_statements.len()
            );
            let payload = json!(diff);
            println!("{}", &serde_json::to_string_pretty(&payload).unwrap());
        }
        Some("parser") => {
            // Single parser
            let ext_id = get_extid_from_argv(&argv)?;
            let parser = Combinator::get_parser_for_ext_id(&ext_id).await?;
            let item = parser.run().await?;
            println!("{:?}", item);
        }
        Some("graph") => {
            // Single graph
            let ext_id = get_extid_from_argv(&argv)?;
            let mut parser = Combinator::get_parser_for_ext_id(&ext_id).await?;
            parser.dump_graph();
        }
        Some("list") => {
            // List
            let filename = argv.get(2).expect("USAGE: list LIST_FILE [START_ROW]");
            let start = match argv.get(3) {
                Some(s) => s.parse::<usize>().unwrap(),
                None => 0,
            };
            let file = File::open(filename).unwrap();
            let reader = BufReader::new(file);
            let mut api = get_wikidata_api("config.json").await?;
            for (index, line) in reader.lines().enumerate() {
                if index >= start {
                    if let Ok(item) = line {
                        println!("{index}: {item}");
                        if let Ok(diff) = get_extend(&item).await {
                            let _ = apply_diff(&item, &diff, &mut api).await; // Ignore result
                        }
                    }
                }
            }
        }
        Some("extend") => {
            let item = argv.get(2).expect("Item argument required");
            let diff = get_extend(item).await.unwrap();
            println!("{}", &serde_json::to_string_pretty(&diff).unwrap());
        }
        _ => run_server().await?,
    }
    Ok(())
}

/*
cargo run -- combinator P950 XX990809

TODO:
P244
P7859
P213
P349
P1015

*/
