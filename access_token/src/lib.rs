use anyhow::{anyhow, Context, Error};
use cookie::Cookie;
use reqwest::blocking::Client;
use reqwest::Method;
use serde::Deserialize;

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_13_2) \
AppleWebKit/537.36 (KHTML, like Gecko) Chrome/63.0.3239.132 Safari/537.36";

const ENDPOINT: &str =
    "https://open.spotify.com/get_access_token?reason=transport&productType=web_player";

pub fn fetch_token(sp_dc: &str, sp_key: &str) -> Result<TokenResponse, Error> {
    let cookies = vec![Cookie::new("sp_dc", sp_dc), Cookie::new("sp_key", sp_key)];
    let cookie_header = cookies
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<String>>()
        .join("; ");

    let c = Client::default();
    let rsp = c
        .request(Method::GET, ENDPOINT)
        .header("user-agent", USER_AGENT)
        .header("cookie", cookie_header)
        .send()?;

    if !rsp.status().is_success() {
        return Err(anyhow!("bad response status: {}", rsp.status()));
    }

    rsp.json().context("json deserialize")
}

#[derive(Deserialize)]
pub struct TokenResponse {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "accessTokenExpirationTimestampMs")]
    pub expiry_ms: i64,
}
