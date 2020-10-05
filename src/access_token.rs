// Code adapated from https://github.com/enriquegh/spotify-webplayer-token
//
// MIT License
//
// Copyright (c) 2018 Enrique Gonzalez
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use anyhow::{anyhow, Error};
use cookie::Cookie;
use reqwest;
use serde::Deserialize;

pub const SP_DC_INSTRUCTIONS: &str = r"1. open a new incognito window in a browser at: https://accounts.spotify.com/en/login?continue=https:%2F%2Fopen.spotify.com%2F
2. open Developer Tools in your browser and select the 'Application' tab
3. login to Spotify
4. search/filter for `sp_dc` under Cookies > https://open.spotify.com
4. repeat step 4 for `sp_key`
6. close the window without logging out";

pub async fn fetch(c: reqwest::Client, sp_dc: &str, sp_key: &str) -> Result<TokenResponse, Error> {
    const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_13_2) \
AppleWebKit/537.36 (KHTML, like Gecko) Chrome/63.0.3239.132 Safari/537.36";
    const URL: &str =
        "https://open.spotify.com/get_access_token?reason=transport&productType=web_player";

    let cookies = vec![Cookie::new("sp_dc", sp_dc), Cookie::new("sp_key", sp_key)];
    let cookie_header = cookies
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<String>>()
        .join("; ");

    let req = c
        .get(URL)
        .header("user-agent", USER_AGENT)
        .header("cookie", cookie_header)
        .build()
        .unwrap();
    let rsp = c.execute(req).await;

    match rsp {
        Ok(r) => {
            if !r.status().is_success() {
                return Err(anyhow!("bad response status: {}", r.status()));
            }
            Ok(r.json::<TokenResponse>().await?)
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
