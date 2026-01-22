use crate::external_id::ExternalId;
use crate::external_importer::*;
use crate::meta_item::*;
use crate::properties::*;
use crate::utility::Utility;
use anyhow::Result;
use async_trait::async_trait;
use sophia::api::prelude::*;
use sophia::inmem::graph::FastGraph;
use sophia::xml;

#[derive(Debug)]
pub struct ULAN {
    id: String,
    graph: FastGraph,
}

#[async_trait]
impl ExternalImporter for ULAN {
    fn my_property(&self) -> usize {
        P_ULAN
    }
    fn my_stated_in(&self) -> &str {
        "Q2494649"
    }
    fn primary_language(&self) -> String {
        String::from("en")
    }
    fn get_key_url(&self, _key: &str) -> String {
        format!("http://vocab.getty.edu/ulan/{}", self.id)
    }

    fn my_id(&self) -> String {
        self.id.clone()
    }
    fn graph(&self) -> &FastGraph {
        &self.graph
    }
    fn transform_label(&self, s: &str) -> String {
        self.transform_label_last_first_name(s)
    }

    async fn run(&self) -> Result<MetaItem> {
        let mut ret = MetaItem::new();

        // let x = self.triples_iris("http://vocab.getty.edu/ontology#ulan1512_parent_of");
        // println!("x: {:?}", x);

        self.add_the_usual(&mut ret).await?;
        self.add_p31(&mut ret)?;
        self.add_children(&mut ret).await?;
        self.add_mentors(&mut ret).await?;
        self.try_rescue_prop_text(&mut ret).await?;
        ret.cleanup();
        Ok(ret)
    }
}

impl ULAN {
    pub async fn new(id: &str) -> Result<Self> {
        let rdf_url = format!("https://vocab.getty.edu/ulan/{id}.rdf");
        let client = Utility::get_reqwest_client()?;
        let resp = client.get(&rdf_url).send().await?.text().await?;
        let mut graph: FastGraph = FastGraph::new();
        let _ = xml::parser::parse_str(&resp).add_to_graph(&mut graph)?;
        Ok(Self {
            id: id.to_string(),
            graph,
        })
    }

    fn add_p31(&self, ret: &mut MetaItem) -> Result<()> {
        ret.add_claim(self.new_statement_item(P_INSTANCE_OF, "Q5"));
        Ok(())
    }

    async fn add_children(&self, ret: &mut MetaItem) -> Result<()> {
        let children = self.triples_iris("http://vocab.getty.edu/ontology#ulan1512_parent_of")?;
        for child in children {
            self.add_ulan_item(&child, P_CHILD, ret).await;
        }
        Ok(())
    }

    async fn add_mentors(&self, ret: &mut MetaItem) -> Result<()> {
        let children = self.triples_iris("http://vocab.getty.edu/ontology#ulan1102_student_of")?;
        for child in children {
            self.add_ulan_item(&child, P_STUDENT_OF, ret).await;
        }
        Ok(())
    }

    async fn add_ulan_item(&self, url: &str, property: usize, ret: &mut MetaItem) {
        if let Some(ulan_id) = url.split('/').next_back() {
            if let Some(item) = ExternalId::new(self.my_property(), ulan_id)
                .get_item_for_external_id_value()
                .await
            {
                ret.add_claim(self.new_statement_item(property, &item));
            } else {
                let _ = ret.add_prop_text(ExternalId::new(property, url));
            }
        } else {
            let _ = ret.add_prop_text(ExternalId::new(property, url));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ID: &str = "500228559";

    #[tokio::test]
    async fn test_new() {
        assert!(ULAN::new(TEST_ID).await.is_ok());
    }
}
