use anyhow::Result;
use std::time::Duration;

#[derive(Copy, Clone, Debug)]
pub struct Utility {}

impl Utility {
    pub fn get_reqwest_client() -> Result<reqwest::Client> {
        const HTTP_USER_AGENT : &str = "Mozilla/5.0 (iPad; U; CPU OS 3_2_1 like Mac OS X; en-us) AppleWebKit/531.21.10 (KHTML, like Gecko) Mobile/7B405";
        let client = reqwest::ClientBuilder::new()
            .timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::limited(10))
            .user_agent(HTTP_USER_AGENT)
            .build()?;
        Ok(client)
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
