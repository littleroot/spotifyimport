use anyhow::*;
use reqwest::Client;
use spotifyimport::access_token::{self, SP_DC_INSTRUCTIONS};
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
    let tok = access_token::fetch(&c, &args[1], &args[2]).await?;
    println!("{}", tok.access_token);
    Ok(())
}

fn print_help(prog: &str) {
    eprint!("usage: {} <SP_DC> <SP_KEY>\n\n", prog);
    eprint!("To obtain SP_DC and SP_KEY:\n");
    eprint!("{}\n", SP_DC_INSTRUCTIONS);
}
