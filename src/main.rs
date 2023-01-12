#[macro_use]
extern crate lazy_static;
extern crate nom_bibtex;

pub mod external_importer ;
pub mod meta_item ;
pub mod id_ref ;
pub mod bne ;
pub mod gnd ;

use crate::external_importer::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    //let parser = id_ref::IdRef::new("026814358").await?;
    //let mut parser = bne::BNE::new("XX990809").await?;
    let mut parser = gnd::GND::new("118523813").await?;
    if false {
        parser.dump_graph();
    } else {
        let mi = parser.run().await?;
        println!("{:?}",&mi);    
    }
    Ok(())
}


/*


http://d-nb.info/gnd/1057769584/about/marcxml
*/