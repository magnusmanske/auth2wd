use crate::url_override::maybe_rewrite;
use anyhow::Result;
use std::time::Duration;

const HTTP_USER_AGENT: &str =
    "Mozilla/5.0 (compatible; auth2wd/0.1; +https://github.com/magnusmanske/auth2wd)";

#[derive(Copy, Clone, Debug)]
pub struct Utility {}

impl Utility {
    pub fn get_reqwest_client() -> Result<reqwest::Client> {
        Ok(reqwest::ClientBuilder::new()
            .timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::limited(10))
            .user_agent(HTTP_USER_AGENT)
            .build()?)
    }

    pub async fn get_url(url: &str) -> Result<String> {
        let url = maybe_rewrite(url);
        let resp = Self::get_reqwest_client()?
            .get(&url)
            .send()
            .await?
            .text()
            .await?;
        Ok(resp)
    }
}
