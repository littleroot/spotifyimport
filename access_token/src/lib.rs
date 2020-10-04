use anyhow::{anyhow, Error};
use cookie::Cookie;
use serde::Deserialize;
use surf::http::Method;
use surf::url::Url;
use surf::Request;

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_13_2) \
AppleWebKit/537.36 (KHTML, like Gecko) Chrome/63.0.3239.132 Safari/537.36";

const URL: &str =
    "https://open.spotify.com/get_access_token?reason=transport&productType=web_player";

pub async fn fetch_token(sp_dc: &str, sp_key: &str) -> Result<TokenResponse, Error> {
    let cookies = vec![Cookie::new("sp_dc", sp_dc), Cookie::new("sp_key", sp_key)];
    let cookie_header = cookies
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<String>>()
        .join("; ");

    let rsp = Request::new(Method::GET, Url::parse(URL).unwrap())
        .set_header("user-agent", USER_AGENT)
        .set_header("cookie", cookie_header)
        .await;

    match rsp {
        Ok(mut r) => {
            if r.status() != 200 {
                return Err(anyhow!("bad response status: {}", r.status()));
            }
            Ok(r.body_json::<TokenResponse>().await?)
        }
        Err(e) => Err(anyhow!(e)),
    }
}

#[derive(Deserialize)]
pub struct TokenResponse {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "accessTokenExpirationTimestampMs")]
    pub expiry_ms: i64,
}
