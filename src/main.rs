#[macro_use]
extern crate lazy_static;

pub mod external_importer ;
pub mod meta_item ;
pub mod id_ref ;


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    //let parser = IdRef::new("051626241").await?;
    let parser = id_ref::IdRef::new("026814358").await?;
    //parser.dump_graph();
    let mi = parser.run().await?;
    println!("{:?}",&mi);
    Ok(())
}


/*
https://datos.bne.es/resource/XX1553066.jsonld

http://d-nb.info/gnd/1057769584/about/marcxml
*/