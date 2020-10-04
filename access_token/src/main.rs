use access_token;
use anyhow::*;
use std::env;
use std::process;

fn main() -> Result<(), anyhow::Error> {
    let args = env::args().skip(1).collect::<Vec<String>>();

    if args.len() != 2 {
        print_help();
        process::exit(2);
    }

    let tok = access_token::fetch_token(&args[0], &args[1])?;
    println!("{}", tok.access_token);

    Ok(())
}

const INSTRUCTIONS: &str = r"1. Open a new Incognito window in a browser at:
   https://accounts.spotify.com/en/login?continue=https:%2F%2Fopen.spotify.com%2F
2. Open Developer Tools in your browser
3. Login to Spotify.
4. Search/Filter for get_access_token in Developer tools under Network.
5. Under cookies for the request save the values for sp_dc and sp_key.
6. Close the window without logging out";

fn print_help() {
    eprint!("usage: accesstoken [SP_DC] [SP_KEY]\n\n");
    eprint!("To obtain SP_DC and SP_KEY follow these instructions:\n");
    eprint!("{}\n", INSTRUCTIONS);
}
