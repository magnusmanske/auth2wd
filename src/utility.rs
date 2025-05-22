use anyhow::Result;
use std::time::Duration;

#[derive(Copy, Clone, Debug)]
pub struct Utility {}

impl Utility {
    pub fn get_reqwest_client() -> Result<reqwest::Client> {
        let ret = reqwest::ClientBuilder::new()
            .timeout(Duration::from_secs(60))
            .build();
        Ok(ret?)
    }

    pub async fn get_url(url: &str) -> Result<String> {
        let resp = Self::get_reqwest_client()?
            .get(url)
            .send()
            .await?
            .text()
            .await?;
        Ok(resp)
    }
}
