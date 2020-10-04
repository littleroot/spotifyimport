use anyhow::*;
use reqwest::Client;
use spotifyimport::access_token;
use std::env;
use std::process;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let args = env::args().collect::<Vec<String>>();

    if args.len() != 3 {
        print_help(&args[0]);
        process::exit(2);
    }

    let c = Client::new();
    let tok = access_token::fetch(c, &args[1], &args[2]).await?;
    println!("{}", tok.access_token);
    Ok(())
}

const INSTRUCTIONS: &str = r"1. open a new incognito window in a browser at: https://accounts.spotify.com/en/login?continue=https:%2F%2Fopen.spotify.com%2F
2. open Developer Tools in your browser and select the 'Application' tab
3. login to Spotify
4. search/filter for `sp_dc` under Cookies > https://open.spotify.com
4. repeat step 4 for `sp_key`
6. close the window without logging out";

fn print_help(prog: &str) {
    eprint!("usage: {} <SP_DC> <SP_KEY>\n\n", prog);
    eprint!("To obtain SP_DC and SP_KEY:\n");
    eprint!("{}\n", INSTRUCTIONS);
}
