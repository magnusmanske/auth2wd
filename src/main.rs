#[macro_use]
extern crate lazy_static;
extern crate nom_bibtex;

pub mod external_importer ;
pub mod meta_item ;
pub mod external_id ;
pub mod combinator ;
pub mod id_ref ;
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
use tracing;
use tracing_subscriber;
use external_importer::*;
use combinator::*;
use tower_http::cors::{Any, CorsLayer};

async fn root() -> Html<&'static str> {
    Html(r##"<h1>Auhority Control data to Wikidata item</h1>
    This API can load AC (Authority Control) data from other sources and convert them into a Wikidata item.

    <h2>Available sources</h3>
    <ul>
    <li><a href="/item/P227/118523813">GND</a> ("Charles Darwin" from Deutsche Nationalbibliothek)</li>
    <li><a href="/item/P268/11898689q">BnF</a> ("Charles Darwin" from Bibliothèque nationale de France)</li>
    <li><a href="/item/P269/026812304">IdRef</a> ("Charles Darwin" from IdRef/SUDOC)</li>
    <li><a href="/item/P950/XX990809">BNE</a> ("Charles Darwin" from Biblioteca Nacional de España)</li>
    <li><a href="/item/P1006/068364229">NB</a> ("Charles Darwin" from Nationale Thesaurus voor Auteurs ID)</li>
    </ul>
    <h2>Functions</h2>
    <ul>
    <li><a href="/item/P227/118523813">item</a>, the JSON of a new item containing the parsed data from the respective source</li>
    <li><a href="/meta_item/P1006/068364229">meta_item</a>, item plus some properties that could not be resolved automatically</li>
    <li><a href="/graph/P227/118523813">graph</a>, the internal graph representation before parsing</li>
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if false { // Combinator
        let mut combinator = Combinator::new();
        combinator.import(vec![external_id::ExternalId::new(950,"XX990809")])?;
        println!("{} items",combinator.items.len());
        return Ok(());
    }
    if false { // Single parser
        let mut parser = bnf::BNF::new("11928016k")?;
        if false {
            parser.dump_graph();
        } else {
            let item = parser.run()?;
            println!("{:?}",item);
        }
        return Ok(())
    }
    tracing_subscriber::fmt::init();

    let cors = CorsLayer::new().allow_origin(Any);

    let app = Router::new()
        .route("/", get(root))
        .route("/item/:prop/:id", get(item))
        .route("/meta_item/:prop/:id", get(meta_item))
        .route("/graph/:prop/:id", get(graph))
        .layer(cors);
    
    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}
