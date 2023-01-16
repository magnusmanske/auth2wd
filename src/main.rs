#[macro_use]
extern crate lazy_static;
extern crate nom_bibtex;

pub mod external_importer ;
pub mod meta_item ;
pub mod id_ref ;
pub mod bne ;
pub mod gnd ;

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

async fn root() -> Html<&'static str> {
    Html(r##"<h1>Auhority Control data to Wikidata item</h1>
    This API can load AC (Authority Control) data from other sources and convert them into a Wikidata item.

    <h2>Examples</h3>
    <ul>
    <li><a href="/item/P227/118523813">GND</a> ("Charles Darwin" from Deutsche Nationalbibliothek)</li>
    <li><a href="/item/P269/026812304">IdRef</a> ("Charles Darwin" from IdRef/SUDOC)</li>
    <li><a href="/item/P950/XX990809">BNE</a> ("Charles Darwin" from Biblioteca Nacional de España)</li>
    </ul>
    <hr/>
    <a href='https://github.com/magnusmanske/auth2wd'>git</a>
    "##)
}

async fn item(Path((property,id)): Path<(String,String)>) -> Json<serde_json::Value> {
    let parser: Box<dyn ExternalImporter> = match property.to_ascii_uppercase().as_str() {
        "P227" => Box::new(
            match gnd::GND::new(&id) {
                Ok(r) => r,
                Err(e) => return Json(json!({"status":e.to_string()}))
            }
        ),
        "P269" => Box::new(
            match id_ref::IdRef::new(&id) {
                Ok(r) => r,
                Err(e) => return Json(json!({"status":e.to_string()}))
            }
        ),
        "P950" => Box::new(
            match bne::BNE::new(&id) {
                Ok(r) => r,
                Err(e) => return Json(json!({"status":e.to_string()}))
            }
        ),
        _ => return Json(json!({"status":format!("unknown property: '{property}'")}))
    };
    let mi = match parser.run() {
        Ok(mi) => mi,
        _ => return Json(json!({"status":"error parsing"}))
    };
    let mut j = json!(mi)["item"].to_owned();
    j["status"] = json!("OK");
    Json(j)
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/", get(root))
        .route("/item/:prop/:id", get(item))
        ;

    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}
