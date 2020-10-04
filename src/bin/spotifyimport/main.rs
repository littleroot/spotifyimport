use anyhow::{anyhow, Error};
use log::*;
use logosaurus::{self, Logger, L_SHORT_FILE, L_STD};
use std::process;

#[tokio::main]
async fn main() {
    let logger = Logger::builder()
        .set_prefix("spotifyimport: ")
        .set_flags(L_STD | L_SHORT_FILE)
        .build();
    logosaurus::init(logger).unwrap();

    if let Err(e) = run() {
        error!("{}", e);
        process::exit(1);
    }
}

fn run() -> Result<(), Error> {
    Err(anyhow!("not implemented"))
}
