#[macro_use]
extern crate lazy_static;
extern crate nom_bibtex;

pub mod external_importer ;
pub mod merge_diff ;
pub mod meta_item ;
pub mod external_id ;
pub mod combinator ;
pub mod id_ref ;
pub mod selibr;
pub mod viaf;
pub mod bne ;
pub mod bnf ;
pub mod gnd ;
pub mod nb ;

use axum::{
    routing::get,
    Json, Router,
    response::Html,
    extract::Path
};
use serde_json::json;
use std::net::SocketAddr;
use std::env;
use tracing;
use tracing_subscriber;
use external_importer::*;
use external_id::*;
use combinator::*;
use tower_http::cors::{Any, CorsLayer};
use tower_http::{compression::CompressionLayer, trace::TraceLayer};


async fn root() -> Html<&'static str> {
    Html(r##"<h1>Auhority Control data to Wikidata item</h1>
    This API can load AC (Authority Control) data from other sources and convert them into a Wikidata item.

    <h2>Available sources</h3>
    <ul>
    <li><a href="/item/P214/27063124">VIAF</a> ("Charles Darwin" from Virtual International Authority File)</li>
    <li><a href="/item/P227/118523813">GND</a> ("Charles Darwin" from Deutsche Nationalbibliothek)</li>
    <li><a href="/item/P268/11898689q">BnF</a> ("Charles Darwin" from Bibliothèque nationale de France)</li>
    <li><a href="/item/P269/026812304">IdRef</a> ("Charles Darwin" from IdRef/SUDOC)</li>
    <li><a href="/item/P906/231727">SELIBR</a> ("Charles Darwin" from National Library of Sweden)</li>
    <li><a href="/item/P950/XX990809">BNE</a> ("Charles Darwin" from Biblioteca Nacional de España)</li>
    <li><a href="/item/P1006/068364229">NB</a> ("Charles Darwin" from Nationale Thesaurus voor Auteurs ID)</li>
    </ul>
    <h2>Functions</h2>
    <ul>
    <li><a href="/item/P227/118523813">item</a>, the JSON of a new item containing the parsed data from the respective source</li>
    <li><a href="/meta_item/P1006/068364229">meta_item</a>, item plus some properties that could not be resolved automatically</li>
    <li><a href="/graph/P227/118523813">graph</a>, the internal graph representation before parsing</li>
    <li><a href="/extend/Q1035>extend</a>, extract AC data from external IDs in an item, and get the payload for <tt>wbeditentity</tt></li>
    </ul>
    <hr/>
    <a href='https://github.com/magnusmanske/auth2wd'>git</a>
    "##)
}

async fn item(Path((property,id)): Path<(String,String)>) -> Json<serde_json::Value> {
    let parser: Box<dyn ExternalImporter> = match Combinator::get_parser_for_property(&property, &id) {
        Ok(parser) => parser,
        Err(e) => return Json(json!({"status":e.to_string()}))
    };
    let mi = match parser.run() {
        Ok(mi) => mi,
        Err(e) => return Json(json!({"status":e.to_string()}))
    };
    let mut j = json!(mi)["item"].to_owned();
    j["status"] = json!("OK");
    Json(j)
}

async fn meta_item(Path((property,id)): Path<(String,String)>) -> Json<serde_json::Value> {
    let parser: Box<dyn ExternalImporter> = match Combinator::get_parser_for_property(&property, &id) {
        Ok(parser) => parser,
        Err(e) => return Json(json!({"status":e.to_string()}))
    };
    let mi = match parser.run() {
        Ok(mi) => mi,
        Err(e) => return Json(json!({"status":e.to_string()}))
    };
    let mut j = json!(mi);
    j["status"] = json!("OK");
    Json(j)
}

async fn graph(Path((property,id)): Path<(String,String)>) -> String {
    let mut parser: Box<dyn ExternalImporter> = match Combinator::get_parser_for_property(&property, &id) {
        Ok(parser) => parser,
        Err(e) => return e.to_string()
    };
    parser.get_graph_text()
}

async fn extend(Path(item): Path<String>) -> Json<serde_json::Value> {
    let mut base_item = match meta_item::MetaItem::from_entity(&item).await {
        Ok(base_item) => base_item,
        Err(e) => return Json(json!({"status":e.to_string()}))
    };
    let ext_ids: Vec<ExternalId> = base_item
        .extract_external_ids()
        .iter()
        .filter(|ext_id|Combinator::get_parser_for_ext_id(ext_id).ok().is_some())
        .cloned()
        .collect();
    let mut combinator = Combinator::new();
    if let Err(e) = combinator.import(ext_ids) {
        return Json(json!({"status":e.to_string()}))
    }
    let other = match combinator.combine() {
        Some(other) => other,
        None => return Json(json!({"status":"No items to combine"}))
    };
    let diff = base_item.merge(&other);
    Json(json!(diff))
}

async fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let cors = CorsLayer::new().allow_origin(Any);

    let app = Router::new()
        .route("/", get(root))
        .route("/item/:prop/:id", get(item))
        .route("/meta_item/:prop/:id", get(meta_item))
        .route("/graph/:prop/:id", get(graph))
        .route("/extend/:item", get(extend))
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(cors);
    
    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}

fn get_extid_from_argv(argv: &Vec<String>) -> Result<ExternalId, Box<dyn std::error::Error>> {
    let property = argv.get(2).expect("USAGE: combinator PROP ID");
    let property = ExternalId::prop_numeric(&property).expect("malformed property: '{property}'");
    let id = argv.get(3).expect("USAGE: combinator PROP ID");
    Ok(ExternalId::new(property,&id))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let argv: Vec<String> = env::args().collect();
    match argv.get(1).map(|s|s.as_str()) {
        Some("combinator") => { // Combinator
            let mut base_item = meta_item::MetaItem::from_entity("Q1035").await?;
            //println!("{:?}",&base_item);
            let ext_id = get_extid_from_argv(&argv)?;
            let mut combinator = Combinator::new();
            combinator.import(vec![ext_id])?;
            println!("{} items: {:?}",combinator.items.len(),combinator.items.keys());
            let other = combinator.combine().expect("No items to combine");
            println!("{} items: {:?}",combinator.items.len(),combinator.items.keys());
            //println!("{:?}",&other);
            let diff = base_item.merge(&other);
            //println!("{:?}",&diff);
            println!("Altered: {}, added: {}",diff.altered_statements.len(),diff.added_statements.len());
            let payload = json!(diff);
            println!("{}",&serde_json::to_string_pretty(&payload).unwrap());
        }
        Some("parser") => { // Single parser
            let ext_id = get_extid_from_argv(&argv)?;
            let parser = Combinator::get_parser_for_ext_id(&ext_id)?;
            let item = parser.run()?;
            println!("{:?}",item);
        }
        Some("graph") => { // Single graph
            let ext_id = get_extid_from_argv(&argv)?;
            let mut parser = Combinator::get_parser_for_ext_id(&ext_id)?;
            parser.dump_graph();
        }
        _ => run_server().await?
    }
    Ok(())
}

/*
cargo run -- combinator P950 XX990809

TODO:
P244
P213
*/