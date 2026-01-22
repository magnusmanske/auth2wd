use crate::external_importer::*;
use crate::meta_item::*;
use crate::properties::*;
use crate::ExternalId;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use wikimisc::wikibase::EntityTrait;
use wikimisc::wikibase::LocaleString;

#[derive(Clone, Debug)]
pub struct NCBItaxonomy {
    id: String,
    taxon: Taxon,
}

#[async_trait]
impl ExternalImporter for NCBItaxonomy {
    fn my_property(&self) -> usize {
        P_NCBI_TAXONOMY
    }
    fn my_stated_in(&self) -> &str {
        "Q13711410"
    }
    fn primary_language(&self) -> String {
        String::from("en")
    }
    fn get_key_url(&self, _key: &str) -> String {
        format!(
            "https://www.ncbi.nlm.nih.gov/Taxonomy/Browser/wwwtax.cgi?mode=Info&id={}",
            self.id
        )
    }
    fn my_id(&self) -> String {
        self.id.clone()
    }

    async fn run(&self) -> Result<MetaItem> {
        let mut ret = MetaItem::new();
        self.add_own_id(&mut ret)?;
        let _ = self.add_parent_taxon(&mut ret).await;
        let _ = self.add_p31(&mut ret);
        let _ = self.add_taxon_name_and_labels(&mut ret);
        let _ = self.add_taxon_rank(&mut ret);
        ret.cleanup();
        Ok(ret)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
struct TaxaSet {
    #[serde(rename = "Taxon")]
    taxon: Vec<Taxon>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
struct Taxon {
    #[serde(rename = "TaxId")]
    taxid: i64,
    #[serde(rename = "ScientificName")]
    scientific_name: Option<String>,
    #[serde(rename = "ParentTaxId")]
    parent_taxid: Option<i64>,
    #[serde(rename = "Rank")]
    rank: Option<String>,
    #[serde(rename = "OtherNames")]
    other_names: OtherNames,
    #[serde(rename = "Division")]
    division: Option<String>,
    #[serde(rename = "LineageEx")]
    lineage_ex: LineageEx,
    // TODO(?): GeneticCode
    #[serde(rename = "MitoGeneticCode")]
    mito_genetic_code: Option<MitoGeneticCode>,
    #[serde(rename = "Lineage")]
    lineage: Option<String>,
    #[serde(rename = "CreateDate")]
    create_date: String,
    #[serde(rename = "UpdateDate")]
    update_date: String,
    #[serde(rename = "PubDate")]
    pub_date: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
struct MitoGeneticCode {
    #[serde(rename = "MGCId")]
    mgc_id: String,
    #[serde(rename = "MGCName")]
    mgc_name: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
struct LineageEx {
    #[serde(rename = "Taxon")]
    taxons: Vec<LineageTaxon>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
struct LineageTaxon {
    #[serde(rename = "ScientificName")]
    scientific_name: Option<String>,
    #[serde(rename = "TaxId")]
    taxid: Option<i64>,
    #[serde(rename = "Rank")]
    rank: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
struct OtherNames {
    #[serde(rename = "Name")]
    names: Vec<Name>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
struct Name {
    #[serde(rename = "ClassCDE")]
    class_cde: String,
    #[serde(rename = "DispName")]
    display_name: String,
}

impl NCBItaxonomy {
    pub async fn new(id: &str) -> Result<Self> {
        let url = format!("https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi?db=taxonomy&id={id}&format=xml");
        let resp = reqwest::get(&url).await?.text().await?;
        let taxa_set: TaxaSet = serde_xml_rs::from_str(&resp)?;
        // Use first taxon, there should only be one!
        let taxon = match taxa_set.taxon.first() {
            Some(taxon) => taxon.clone(),
            None => return Err(anyhow!("Invalid XML")),
        };
        Ok(Self {
            id: id.to_string(),
            taxon,
        })
    }

    async fn add_parent_taxon(&self, ret: &mut MetaItem) -> Option<()> {
        let parent_id = self.taxon.parent_taxid?;
        let query = format!(
            "haswbstatement:P{}={parent_id} haswbstatement:P{}=Q16521",
            P_NCBI_TAXONOMY, P_INSTANCE_OF
        );
        let item = ExternalId::search_wikidata_single_item(&query).await?;
        ret.add_claim(self.new_statement_item(P_PARENT_TAXON, &item));
        Some(())
    }

    fn add_p31(&self, ret: &mut MetaItem) -> Option<()> {
        // Taxon
        ret.add_claim(self.new_statement_item(P_INSTANCE_OF, "Q16521"));
        Some(())
    }

    fn add_taxon_name_and_labels(&self, ret: &mut MetaItem) -> Option<()> {
        let name = self.taxon.scientific_name.clone()?;
        ret.add_claim(self.new_statement_string(P_TAXON_NAME, &name));
        for lang in TAXON_LABEL_LANGUAGES {
            let label = LocaleString::new(*lang, &name);
            ret.item.labels_mut().push(label);
        }
        Some(())
    }

    fn add_taxon_rank(&self, ret: &mut MetaItem) -> Option<()> {
        let rank = self.taxon.rank.clone()?.to_lowercase();
        let item = TAXON_MAP.get(rank.as_str())?;
        ret.add_claim(self.new_statement_item(P_TAXON_RANK, item));
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ID: &str = "1747344";

    // Using all tests together to get around NCBI rate limiting
    #[tokio::test]
    async fn test_all() {
        let ncbi_taxonomy = NCBItaxonomy::new(TEST_ID).await.unwrap();
        assert_eq!(ncbi_taxonomy.my_property(), P_NCBI_TAXONOMY);
        assert_eq!(ncbi_taxonomy.my_stated_in(), "Q13711410");
        assert_eq!(ncbi_taxonomy.primary_language(), "en");
        assert_eq!(ncbi_taxonomy.my_id(), TEST_ID);
        assert_eq!(
            ncbi_taxonomy.get_key_url(TEST_ID),
            format!(
                "https://www.ncbi.nlm.nih.gov/Taxonomy/Browser/wwwtax.cgi?mode=Info&id={TEST_ID}"
            )
        );
        let new_item = ncbi_taxonomy.run().await.unwrap();
        assert_eq!(new_item.item.claims().len(), 5);
    }
}
