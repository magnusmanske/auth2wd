// #![forbid(unsafe_code)]
#![warn(
    clippy::cognitive_complexity,
    clippy::dbg_macro,
    clippy::debug_assert_with_mut_call,
    clippy::doc_link_with_quotes,
    clippy::doc_markdown,
    clippy::empty_line_after_outer_attr,
    // clippy::empty_structs_with_brackets,
    clippy::float_cmp,
    clippy::float_cmp_const,
    clippy::float_equality_without_abs,
    keyword_idents,
    // clippy::missing_const_for_fn,
    missing_copy_implementations,
    missing_debug_implementations,
    // clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::mod_module_files,
    non_ascii_idents,
    noop_method_call,
    // clippy::option_if_let_else,
    // clippy::print_stderr,
    // clippy::print_stdout,
    clippy::semicolon_if_nothing_returned,
    clippy::unseparated_literal_suffix,
    clippy::shadow_unrelated,
    clippy::similar_names,
    clippy::suspicious_operation_groupings,
    unused_crate_dependencies,
    unused_extern_crates,
    unused_import_braces,
    clippy::unused_self,
    // clippy::use_debug,
    clippy::used_underscore_binding,
    clippy::useless_let_if_seq,
    // clippy::wildcard_dependencies,
    // clippy::wildcard_imports
)]

#[macro_use]
extern crate lazy_static;

pub mod bne;
pub mod bnf;
pub mod combinator;
pub mod external_id;
pub mod external_importer;
pub mod gbif_taxon;
pub mod gnd;
pub mod id_ref;
pub mod inaturalist;
pub mod isni;
pub mod loc;
pub mod merge_diff;
pub mod meta_item;
pub mod nb;
pub mod ncbi_taxonomy;
pub mod noraf;
pub mod pubchem_cid;
pub mod selibr;
pub mod supported_property;
pub mod ulan;
pub mod utility;
pub mod viaf;
pub mod worldcat;

use axum::Form;
use axum::{extract::Path, response::Html, routing::get, Json, Router};
use combinator::*;
use external_id::*;
use external_importer::*;
use meta_item::MetaItem;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::{env, fs};
use supported_property::SUPPORTED_PROPERTIES;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::{compression::CompressionLayer, trace::TraceLayer};
use wikibase_rest_api::prelude::*;

// use wikimisc::item_merger::ItemMerger;
// use wikimisc::mediawiki::api::Api;
// use wikimisc::merge_diff::MergeDiff;
// use wikimisc::wikibase::{EntityTrait, Item, Snak, Statement};

fn wrap_html(html: &str) -> String {
    let outer: String = fs::read_to_string("./html/wrapper.html").unwrap();
    outer.replace("$1$", html)
}

async fn root() -> Html<String> {
    let sources: Vec<String> = SUPPORTED_PROPERTIES.iter().map(|sp| sp.as_li()).collect();
    let mut html: String = fs::read_to_string("./html/root.html").unwrap();
    html = html.replace("$1$", &sources.join("\n"));
    Html(wrap_html(&html))
}

#[axum::debug_handler]
async fn item(Path((property, id)): Path<(String, String)>) -> Json<serde_json::Value> {
    let parser: Box<dyn ExternalImporter> =
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
    let parser: Box<dyn ExternalImporter> =
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
    let mut base_item = match MetaItem::from_entity(&item).await {
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
    let diff = match combinator.combine_on_base_item(&mut base_item) {
        Some(diff) => diff,
        None => return Json(json!({"status":"No items to combine"})),
    };
    base_item.fix_dates();
    // other.fix_images(&base_item);
    // let diff = base_item.merge(&other);
    Json(json!(diff))
}

#[derive(Serialize, Deserialize)]
struct MergeForm {
    base_item: String,
    new_item: String,
}

fn item_from_json_string(s: &str) -> Result<(Item, bool), String> {
    let mut item = serde_json::from_str::<Value>(s).map_err(|e| e.to_string())?;
    let has_fake_id = if item.get("id").is_none() {
        item["id"] = json!("Q0");
        true
    } else {
        false
    };
    let item = Item::from_json(item).map_err(|e| e.to_string())?;
    Ok((item, has_fake_id))
}

async fn merge(Form(params): Form<MergeForm>) -> Json<serde_json::Value> {
    let (base_item, base_item_has_fake_id) = match item_from_json_string(&params.base_item) {
        Ok(item) => item,
        Err(e) => return Json(json!({"error":e.to_string()})),
    };
    let (new_item, _) = match item_from_json_string(&params.new_item) {
        Ok(item) => item,
        Err(e) => return Json(json!({"error":e.to_string()})),
    };

    let mut im = ItemMerger::new(base_item);
    let diff = im.merge(&new_item);

    let mut j = im.item.to_json();
    if base_item_has_fake_id {
        if let Some(jo) = j.as_object_mut() {
            jo.remove("id");
        }
    }
    let j = json!({"item":j,"diff":diff});
    Json(j)
}

async fn merge_info() -> Html<String> {
    let mut base_item = Item::default();
    let mut new_item = Item::default();
    base_item.set_id(EntityId::item("Q0"));
    new_item.set_id(EntityId::item("Q0"));
    base_item
        .statements_mut()
        .insert(Statement::new_item("P31", "Q5"));
    new_item
        .statements_mut()
        .insert(Statement::new_file("P18", "Pretty_image.jpg"));

    let mut base_item = json!(base_item);
    let mut new_item = json!(new_item);
    base_item.as_object_mut().unwrap().remove("id");
    new_item.as_object_mut().unwrap().remove("id");

    let mut html: String = fs::read_to_string("./html/merge_info.html").unwrap();
    html = html.replace("$1$", &serde_json::to_string_pretty(&base_item).unwrap());
    html = html.replace("$2$", &serde_json::to_string_pretty(&new_item).unwrap());
    Html(wrap_html(&html))
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
        .route("/item/{prop}/{id}", get(item))
        .route("/meta_item/{prop}/{id}", get(meta_item))
        .route("/graph/{prop}/{id}", get(graph))
        .route("/extend/{item}", get(extend))
        .route("/merge", get(merge_info).post(merge))
        .nest_service("/images", ServeDir::new("images"))
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
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Could not create listener");
    axum::serve(listener, app)
        .await
        .expect("Could not start server");

    Ok(())
}

fn get_extid_from_argv(argv: &[String]) -> Result<ExternalId, Box<dyn std::error::Error>> {
    let property = argv.get(2).expect("USAGE: combinator PROP ID");
    let property = ExternalId::prop_numeric(property).expect("malformed property: '{property}'");
    let id = argv.get(3).expect("USAGE: combinator PROP ID");
    Ok(ExternalId::new(property, id))
}

async fn get_extend(item: &str) -> Result<MergeDiff, Box<dyn std::error::Error>> {
    let mut base_item = MetaItem::from_entity(item).await?;
    let ext_ids: Vec<ExternalId> = base_item
        .get_external_ids()
        .into_iter()
        .filter(Combinator::has_parser_for_ext_id)
        .collect();
    let mut combinator = Combinator::new();
    combinator.import(ext_ids).await?;
    let (mut other, _merge_diff) = match combinator.combine() {
        Some((other, merge_diff)) => (other, merge_diff),
        None => return Err("No items to combine".into()),
    };
    other.fix_dates();
    other.fix_images(&base_item);
    Ok(base_item.merge(&other))
}

// async fn apply_diff(
//     item: &str,
//     diff: &MergeDiff,
//     api: &mut Api,
// ) -> Result<(), Box<dyn std::error::Error>> {
//     let json_string = json!(diff).to_string();
//     if json_string == "{}" {
//         return Ok(());
//     }
//     let token = api.get_edit_token().await?;
//     let params: HashMap<String, String> = vec![
//         ("action", "wbeditentity"),
//         ("id", item),
//         ("data", &json_string),
//         ("summary", "AC2WD"),
//         ("token", &token),
//         ("bot", "1"),
//     ]
//     .into_iter()
//     .map(|(k, v)| (k.to_string(), v.to_string()))
//     .collect();
//     let j = api
//         .post_query_api_json(&params)
//         .await
//         .map_err(|e| e.to_string())?;
//     match j["error"].as_object() {
//         Some(o) => {
//             let s = format!("{o:?}");
//             Err(s.into())
//         }
//         None => Ok(()),
//     }
// }

// async fn get_wikidata_api(path: &str) -> Result<Api, Box<dyn std::error::Error>> {
//     let file = File::open(path)?;
//     let reader = BufReader::new(file);
//     let j: serde_json::Value = serde_json::from_reader(reader)?;
//     let oauth2_token = j["oauth2_token"]
//         .as_str()
//         .expect("No oauth2_token in {path}");
//     let mut api = Api::new("https://www.wikidata.org/w/api.php").await?;
//     api.set_oauth2(oauth2_token);
//     Ok(api)
// }

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let argv: Vec<String> = env::args().collect();
    match argv.get(1).map(|s| s.as_str()) {
        Some("combinator") => {
            // Combinator
            let ext_id = get_extid_from_argv(&argv)?;
            let mut combinator = Combinator::new();
            combinator.import(vec![ext_id]).await?;
            println!(
                "{} items: {:?}",
                combinator.items.len(),
                combinator.items.keys()
            );
            let (_other, diff) = combinator.combine().expect("No items to combine");
            println!(
                "{} items: {:?}",
                combinator.items.len(),
                combinator.items.keys()
            );
            //println!("{:?}",&other);
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
            println!("{item:?}");
        }
        Some("graph") => {
            // Single graph
            let ext_id = get_extid_from_argv(&argv)?;
            let mut parser = Combinator::get_parser_for_ext_id(&ext_id).await?;
            parser.dump_graph();
        }
        // Some("list") => {
        //     // List
        //     let filename = argv.get(2).expect("USAGE: list LIST_FILE [START_ROW]");
        //     let start = match argv.get(3) {
        //         Some(s) => s.parse::<usize>().unwrap(),
        //         None => 0,
        //     };
        //     let file = File::open(filename).unwrap();
        //     let reader = BufReader::new(file);
        //     let mut api = get_wikidata_api("config.json").await?;
        //     for (index, line) in reader.lines().enumerate() {
        //         if index >= start {
        //             if let Ok(item) = line {
        //                 println!("{index}: {item}");
        //                 if let Ok(diff) = get_extend(&item).await {
        //                     let _ = apply_diff(&item, &diff, &mut api).await; // Ignore result
        //                 }
        //             }
        //         }
        //     }
        // }
        Some("extend") => {
            let item = argv.get(2).expect("Item argument required");
            let diff = get_extend(item).await.unwrap();
            println!("{}", &serde_json::to_string_pretty(&diff).unwrap());
        }
        Some("merge") => {
            todo!();
        }
        _ => run_server().await?,
    }
    Ok(())
}

/*
cargo run -- combinator P950 XX990809

TODO:
P349    NDL Authority ID (Japan)
P7545	askArt person ID (has JSON in HTML source)

https://vokabular.bs.no/bibbi/nb/page/22778

*/
