use anyhow::Result;
use std::time::Duration;

#[derive(Copy, Clone, Debug)]
pub struct Utility {}

impl Utility {
    pub async fn get_url(url: &str) -> Result<String> {
        let resp = reqwest::ClientBuilder::new()
            .timeout(Duration::from_secs(60))
            .build()?
            .get(url)
            .send()
            .await?
            .text()
            .await?;
        Ok(resp)
    }
}
