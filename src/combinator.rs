use crate::meta_item::*;
use crate::external_importer::*;
use crate::external_id::*;


#[derive(Debug, Clone)]
pub struct Combinator {
    pub items: Vec<MetaItem>,
}

impl Combinator {
    pub fn new() -> Self {
        Self {
            items: vec![],
        }
    }

    pub fn get_parser_for_property(property: &str, id: &str) -> Result<Box<dyn ExternalImporter>,Box<dyn std::error::Error>> {
        match property.to_ascii_uppercase().as_str() {
            "P227" => Ok(Box::new(crate::gnd::GND::new(&id)?)),
            "P268" => Ok(Box::new(crate::bnf::BNF::new(&id)?)),
            "P269" => Ok(Box::new(crate::id_ref::IdRef::new(&id)?)),
            "P950" => Ok(Box::new(crate::bne::BNE::new(&id)?)),
            "P1006" => Ok(Box::new(crate::nb::NB::new(&id)?)),
            _ => Err(format!("unknown property: '{property}'").into())
        }
    }

    pub fn import(&mut self, ids: Vec<ExternalId>) -> Result<(),Box<dyn std::error::Error>> {
        let mut ids_used: Vec<ExternalId> = vec![];
        let mut ids = ids.to_owned();
        while !ids.is_empty() {
            let id = match ids.pop() {
                Some(id) => id,
                None => break,
            };
            println!("{:?}",&id);
            ids_used.push(id.to_owned());
            let parser = match Self::get_parser_for_property(&format!("P{}",id.property), &id.id) {
                Ok(parser) => parser,
                _ => continue,
            };
            let item = parser.run()?;
            let external_ids = item.get_external_ids();
            self.items.push(item);
            for external_id in external_ids {
                if !ids_used.contains(&external_id) && !ids.contains(&external_id){
                    ids.push(external_id.to_owned());
                }
            }
        }
        Ok(())
    }
}