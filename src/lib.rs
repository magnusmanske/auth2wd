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
pub mod meta_item;
pub mod nb;
pub mod ncbi_taxonomy;
pub mod ndl;
pub mod noraf;
pub mod nukat;
pub mod properties;
pub mod pubchem_cid;
pub mod selibr;
pub mod supported_property;
pub mod ulan;
pub mod url_override;
pub mod utility;
pub mod viaf;
pub mod worldcat;

// Re-export items that submodules reference via `crate::` paths
pub use external_id::ExternalId;
pub use external_importer::DO_NOT_USE_EXTERNAL_URL_REGEXPS;
