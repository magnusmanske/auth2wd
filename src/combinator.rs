use std::collections::HashMap;
use crate::meta_item::*;
use crate::external_importer::*;
use crate::external_id::*;


#[derive(Debug, Clone)]
pub struct Combinator {
    pub items: HashMap<String,MetaItem>,
}

impl Combinator {
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
        }
    }

    pub fn get_parser_for_property(property: &str, id: &str) -> Result<Box<dyn ExternalImporter>,Box<dyn std::error::Error>> {
        let property = match ExternalId::prop_numeric(property) {
            Some(property) => property,
            None => return Err(format!("malformed property: '{property}'").into())
        };
        let ext_id = ExternalId::new(property,id);
        Self::get_parser_for_ext_id(&ext_id)
    }

    pub fn get_supported_properties() -> &'static [usize] {
        &[214,227,268,269,906,950,1006]
    }

    pub fn get_parser_for_ext_id(id: &ExternalId) -> Result<Box<dyn ExternalImporter>,Box<dyn std::error::Error>> {
        match id.property {
             214 => Ok(Box::new(crate::viaf::VIAF::new(&id.id)?)),
             227 => Ok(Box::new(crate::gnd::GND::new(&id.id)?)),
             268 => Ok(Box::new(crate::bnf::BNF::new(&id.id)?)),
             269 => Ok(Box::new(crate::id_ref::IdRef::new(&id.id)?)),
             906 => Ok(Box::new(crate::selibr::SELIBR::new(&id.id)?)),
             950 => Ok(Box::new(crate::bne::BNE::new(&id.id)?)),
            1006 => Ok(Box::new(crate::nb::NB::new(&id.id)?)),
            _ => Err(format!("unsupported property: '{}'",id.property).into())
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
            ids_used.push(id.to_owned());
            let parser = match Self::get_parser_for_property(&format!("P{}",id.property), &id.id) {
                Ok(parser) => parser,
                _ => continue,
            };
            let key = ExternalId::new(id.property,&parser.my_id()).to_string();
            if self.items.contains_key(&key) {
                continue;
            }
            let item = parser.run()?;
            let external_ids = item.get_external_ids();
            self.items.insert(key, item);
            for external_id in external_ids {
                if !ids_used.contains(&external_id) && !ids.contains(&external_id){
                    ids.push(external_id.to_owned());
                }
            }
        }
        Ok(())
    }

    pub fn combine(&mut self) -> Option<MetaItem> {
        while self.items.len()>1 {
            let keys: Vec<String> = self.items.keys().cloned().collect() ;
            let k1 = &keys[0];
            let k2 = &keys[1];
            let other = self.items.get(k2).unwrap().to_owned();
            let _ = self.items.get_mut(k1).unwrap().merge(&other);
            self.items.remove(k2);
        }
        self.items.iter().next().map(|(_,v)|v.to_owned())
    }
}